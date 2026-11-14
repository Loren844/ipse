//! Content lines, shared by vCard (RFC 6350) and iCalendar (RFC 5545) files: physical
//! lines folded with a leading space, `NAME;PARAM=value:value`, and text escapes.

use std::borrow::Cow;
use std::io::BufRead;

/// The logical lines of a file, read one at a time.
pub(crate) struct Lines<R> {
    input: R,
    raw: Vec<u8>,
    /// The logical line being unfolded.
    pending: Option<String>,
    /// A logical line to read again before the input.
    requeued: Option<String>,
}

impl<R: BufRead> Lines<R> {
    pub(crate) fn new(input: R) -> Self {
        Self {
            input,
            raw: Vec::with_capacity(1024),
            pending: None,
            requeued: None,
        }
    }

    /// The next logical line: physical lines joined where a line starts with a space
    /// or a tab (folding). A line that is not UTF-8 is read as Windows-1252, as older
    /// Windows and Outlook files are written; never fatal.
    ///
    /// # Errors
    ///
    /// When the input cannot be read.
    pub(crate) fn next_line(&mut self) -> std::io::Result<Option<String>> {
        if let Some(line) = self.requeued.take() {
            return Ok(Some(line));
        }
        loop {
            self.raw.clear();
            if self.input.read_until(b'\n', &mut self.raw)? == 0 {
                return Ok(self.pending.take());
            }
            let text = match std::str::from_utf8(&self.raw) {
                Ok(text) => Cow::Borrowed(text),
                Err(_) => Cow::Owned(windows_1252(&self.raw)),
            };
            let text = text.trim_end_matches(['\n', '\r']);
            if let Some(rest) = text.strip_prefix([' ', '\t'])
                && let Some(pending) = &mut self.pending
            {
                pending.push_str(rest);
                continue;
            }
            if let Some(previous) = self.pending.replace(text.to_owned()) {
                return Ok(Some(previous));
            }
        }
    }

    /// Gives `line` back, to be returned by the next call.
    pub(crate) fn requeue(&mut self, line: String) {
        self.requeued = Some(line);
    }
}

/// One content line: `[group.]NAME[;param=value…]:value`, borrowed from the line.
pub(crate) struct Property<'a> {
    /// The group before the name (`item1` in `item1.X-ABLABEL`), which ties properties
    /// together.
    pub(crate) group: Option<&'a str>,
    /// Upper case, without the group.
    pub(crate) name: Cow<'a, str>,
    /// Values without their quotes. Compare names with [`Property::param`].
    params: Vec<(&'a str, &'a str)>,
    pub(crate) value: &'a str,
}

impl<'a> Property<'a> {
    /// Splits a logical line; `None` when it has no `:` or no name.
    pub(crate) fn parse(line: &'a str) -> Option<Self> {
        let colon = unquoted(line, ':').next()?;
        let (head, value) = (&line[..colon], &line[colon + 1..]);
        let mut parts = split_unquoted(head, ';');
        let full = parts.next()?;
        let (group, name) = match full.rsplit_once('.') {
            Some((group, name)) => (Some(group.trim()).filter(|g| !g.is_empty()), name.trim()),
            None => (None, full.trim()),
        };
        if name.is_empty() {
            return None;
        }
        let name = if name.bytes().any(|b| b.is_ascii_lowercase()) {
            Cow::Owned(name.to_ascii_uppercase())
        } else {
            Cow::Borrowed(name)
        };
        let params = parts
            .map(|p| match p.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim().trim_matches('"')),
                None => ("TYPE", p.trim()),
            })
            .collect();
        Some(Self {
            group,
            name,
            params,
            value,
        })
    }

    /// The same property with another value, such as its value once decoded.
    pub(crate) fn with_value<'b>(&'b self, value: &'b str) -> Property<'b> {
        Property {
            group: self.group,
            name: Cow::Borrowed(self.name.as_ref()),
            params: self.params.clone(),
            value,
        }
    }

    /// True when a parameter is `value`, as `ENCODING=QUOTED-PRINTABLE` or, in vCard
    /// 2.1, a bare `QUOTED-PRINTABLE`; ignoring case.
    pub(crate) fn has_param_value(&self, value: &str) -> bool {
        self.params
            .iter()
            .any(|(_, v)| v.eq_ignore_ascii_case(value))
    }

    /// The value of the parameter `key` (upper case), if present; names are compared
    /// ignoring case.
    pub(crate) fn param(&self, key: &str) -> Option<&'a str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| *v)
    }
}

/// Byte positions of `separator` outside double quotes.
fn unquoted(text: &str, separator: char) -> impl Iterator<Item = usize> + '_ {
    let mut quoted = false;
    text.char_indices().filter_map(move |(i, c)| {
        if c == '"' {
            quoted = !quoted;
            None
        } else if c == separator && !quoted {
            Some(i)
        } else {
            None
        }
    })
}

/// `text` split on `separator` outside double quotes.
fn split_unquoted(text: &str, separator: char) -> impl Iterator<Item = &str> + '_ {
    let mut start = 0;
    let mut cuts = unquoted(text, separator);
    let mut finished = false;
    std::iter::from_fn(move || {
        if finished {
            return None;
        }
        if let Some(at) = cuts.next() {
            let part = &text[start..at];
            start = at + separator.len_utf8();
            Some(part)
        } else {
            finished = true;
            Some(&text[start..])
        }
    })
}

/// `bytes` read as Windows-1252, which is also ISO-8859-1 for every printable letter.
pub(crate) fn windows_1252(bytes: &[u8]) -> String {
    /// The characters of bytes 0x80 to 0x9F, where Windows-1252 and ISO-8859-1 differ.
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9f => HIGH[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect()
}

/// The bytes of a quoted-printable value (RFC 2045): `=C3=A9` is two bytes, a final
/// `=` joined the next line and is dropped. A malformed `=` is kept as it is.
pub(crate) fn quoted_printable(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok());
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
            if i + 1 == bytes.len() {
                break;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// An unescaped, trimmed value; `None` when empty.
pub(crate) fn text(value: &str) -> Option<String> {
    non_empty(&unescape(value))
}

/// A trimmed copy; `None` when empty.
pub(crate) fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// `\n`, `\N`, `\,`, `\;` and `\\` decoded.
pub(crate) fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Splits on `separator` where it is not escaped, then unescapes each part.
pub(crate) fn split_unescaped(value: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            current.push(c);
            if let Some(next) = chars.next() {
                current.push(next);
            }
        } else if c == separator {
            parts.push(unescape(&current));
            current.clear();
        } else {
            current.push(c);
        }
    }
    parts.push(unescape(&current));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_lines_are_joined() {
        let mut lines = Lines::new("A:1\r\nB:long\r\n  line\r\n\tend\nC:3".as_bytes());
        let mut all = Vec::new();
        while let Some(line) = lines.next_line().unwrap() {
            all.push(line);
        }
        assert_eq!(
            all,
            ["A:1", "B:long lineend", "C:3"],
            "one space or tab removed"
        );
    }

    #[test]
    fn quoted_parameters_may_hold_separators() {
        let p = Property::parse(
            "ATTENDEE;CN=\"Dupont; Marie: RH\";ROLE=REQ-PARTICIPANT:mailto:marie@example.org",
        )
        .unwrap();
        assert_eq!(p.name, "ATTENDEE");
        assert_eq!(p.param("CN"), Some("Dupont; Marie: RH"));
        assert_eq!(p.param("ROLE"), Some("REQ-PARTICIPANT"));
        assert_eq!(p.value, "mailto:marie@example.org");
    }

    #[test]
    fn groups_are_kept_apart_and_names_upper_cased() {
        let p = Property::parse("item1.email;type=INTERNET:a@example.org").unwrap();
        assert_eq!(
            (p.group, p.name.as_ref(), p.param("TYPE")),
            (Some("item1"), "EMAIL", Some("INTERNET"))
        );
        assert_eq!(Property::parse("FN:Ana").unwrap().group, None);
        assert!(Property::parse("no colon").is_none());
        assert!(Property::parse(":value").is_none());
    }

    #[test]
    fn windows_files_are_read() {
        let mut lines = Lines::new(&b"FN:Caf\xe9 \x80 l\x92\xe9t\xe9\r\nNOTE:ok\r\n"[..]);
        assert_eq!(
            lines.next_line().unwrap().as_deref(),
            Some("FN:Café € l’été")
        );
        assert_eq!(lines.next_line().unwrap().as_deref(), Some("NOTE:ok"));
    }

    #[test]
    fn quoted_printable_values() {
        assert_eq!(quoted_printable("L=C3=A9a"), "Léa".as_bytes());
        assert_eq!(quoted_printable("caf=E9"), b"caf\xe9");
        assert_eq!(
            quoted_printable("a=\nb"),
            b"a=\nb",
            "only a final = is a soft break"
        );
        assert_eq!(quoted_printable("fin="), b"fin");
        assert_eq!(quoted_printable("x=ZZ"), b"x=ZZ");
    }

    #[test]
    fn escapes() {
        assert_eq!(unescape(r"a\, b\; c\\ d\ne"), "a, b; c\\ d\ne");
        assert_eq!(split_unescaped(r"a\,b,c", ','), ["a,b", "c"]);
        assert_eq!(text("  "), None);
    }
}
