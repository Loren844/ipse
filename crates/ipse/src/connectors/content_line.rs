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
    /// or a tab (folding). Invalid UTF-8 is replaced, never fatal.
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
            let text = String::from_utf8_lossy(&self.raw);
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
        let name = parts.next()?.rsplit('.').next()?.trim();
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
            name,
            params,
            value,
        })
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
    fn groups_are_dropped_and_names_upper_cased() {
        let p = Property::parse("item1.email;type=INTERNET:a@example.org").unwrap();
        assert_eq!(
            (p.name.as_ref(), p.param("TYPE")),
            ("EMAIL", Some("INTERNET"))
        );
        assert!(Property::parse("no colon").is_none());
        assert!(Property::parse(":value").is_none());
    }

    #[test]
    fn escapes() {
        assert_eq!(unescape(r"a\, b\; c\\ d\ne"), "a, b; c\\ d\ne");
        assert_eq!(split_unescaped(r"a\,b,c", ','), ["a,b", "c"]);
        assert_eq!(text("  "), None);
    }
}
