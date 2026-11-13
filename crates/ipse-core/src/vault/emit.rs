//! Writing the keys Ipse owns as YAML lines.
//!
//! Values are written plain when that is unambiguous, quoted otherwise, so that any YAML
//! reader (Obsidian included) reads back exactly the text Ipse meant: a phone number
//! `0612345678` is quoted, or a YAML reader would turn it into a number.

use std::fmt::Write as _;

use super::fields::{Common, EntityFields};

/// A value Ipse owns, ready to be written.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Out {
    /// Text, quoted when needed.
    Text(String),
    /// A validated date or date-time, always written plain like the spec's examples.
    Date(String),
    Bool(bool),
    List(Vec<String>),
    Pair(f64, f64),
}

/// The keys Ipse owns for this file, in the order a new file lists them. `None` means
/// the key is absent.
pub(crate) fn canonical(
    common: &Common,
    entity: Option<&EntityFields>,
) -> Vec<(&'static str, Option<Out>)> {
    let mut keys = vec![
        ("id", Some(Out::Text(common.id.to_string()))),
        (
            "type",
            Some(Out::Text(common.file_type.as_str().to_owned())),
        ),
        ("category", Some(Out::Text(common.category.to_string()))),
        ("created", Some(Out::Date(common.created.to_string()))),
        ("updated", Some(Out::Date(common.updated.to_string()))),
        ("sources", list(&common.sources)),
    ];
    if let Some(e) = entity {
        keys.extend([
            ("name", e.name.clone().map(Out::Text)),
            ("aliases", list(&e.aliases)),
            ("owner", e.owner.then_some(Out::Bool(true))),
            (
                "birthday",
                e.birthday.map(|b| {
                    if b.year().is_some() {
                        Out::Date(b.to_string())
                    } else {
                        Out::Text(b.to_string())
                    }
                }),
            ),
            ("emails", list(&e.emails)),
            ("phones", list(&e.phones)),
            ("address", e.address.clone().map(Out::Text)),
            ("geo", e.geo.map(|g| Out::Pair(g.lat(), g.lon()))),
            (
                "located_in",
                e.located_in.as_ref().map(|l| Out::Text(l.to_string())),
            ),
        ]);
    }
    keys
}

fn list(items: &[String]) -> Option<Out> {
    (!items.is_empty()).then(|| Out::List(items.to_vec()))
}

/// One entry, `key: value` and its lines, ending with `\n`.
pub(crate) fn entry(key: &str, value: &Out) -> String {
    match value {
        Out::Text(text) => format!("{key}: {}\n", scalar(text)),
        Out::Date(text) => format!("{key}: {text}\n"),
        Out::Bool(b) => format!("{key}: {b}\n"),
        Out::Pair(a, b) => format!("{key}: [{a}, {b}]\n"),
        Out::List(items) if items.is_empty() => format!("{key}: []\n"),
        Out::List(items) => {
            let mut out = format!("{key}:\n");
            for item in items {
                out.push_str("  - ");
                out.push_str(&scalar(item));
                out.push('\n');
            }
            out
        }
    }
}

/// `text` as a YAML scalar: plain when unambiguous, double-quoted otherwise.
pub(crate) fn scalar(text: &str) -> String {
    if plain_ok(text) {
        text.to_owned()
    } else {
        quoted(text)
    }
}

fn plain_ok(text: &str) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    text.trim() == text
        && !"-?:,[]{}#&*!|>'\"%@`".contains(first)
        && !text.chars().any(needs_escape)
        && !text.contains([',', '[', ']', '{', '}'])
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':')
        && !resolves_to_non_string(text)
}

fn needs_escape(c: char) -> bool {
    c.is_control() || matches!(c, '\u{feff}' | '\u{2028}' | '\u{2029}')
}

fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if needs_escape(c) => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// True when a YAML 1.1 or 1.2 reader could read `text`, written plain, as something
/// other than text: a boolean, null, a number or a date. Errs on the side of quoting.
fn resolves_to_non_string(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "~" | "null"
            | "true"
            | "false"
            | "yes"
            | "no"
            | "on"
            | "off"
            | "y"
            | "n"
            | ".inf"
            | "+.inf"
            | "-.inf"
            | ".nan"
    ) {
        return true;
    }
    let body = lower.strip_prefix(['+', '-']).unwrap_or(&lower);
    let radix = |rest: &str, ok: fn(char) -> bool| !rest.is_empty() && rest.chars().all(ok);
    if let Some(rest) = body.strip_prefix("0x") {
        return radix(rest, |c| c.is_ascii_hexdigit() || c == '_');
    }
    if let Some(rest) = body.strip_prefix("0o") {
        return radix(rest, |c| matches!(c, '0'..='7' | '_'));
    }
    if let Some(rest) = body.strip_prefix("0b") {
        return radix(rest, |c| matches!(c, '0' | '1' | '_'));
    }
    looks_numeric(body) || looks_like_date(text)
}

/// Digits with `_` and `:` separators, an optional fraction and an optional exponent.
fn looks_numeric(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    let mut take_digits = |i: &mut usize, separators: bool| {
        while let Some(&b) = bytes.get(*i) {
            if b.is_ascii_digit() {
                digits += 1;
            } else if !(separators && matches!(b, b'_' | b':')) {
                break;
            }
            *i += 1;
        }
    };
    take_digits(&mut i, true);
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        take_digits(&mut i, true);
    }
    if digits == 0 {
        return false;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == bytes.len()
}

/// Starts like a YAML 1.1 timestamp: four digits, a dash, a digit.
fn looks_like_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 6
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::yaml::{Node, parse};

    /// Writes `text` as a value and reads it back with the YAML parser.
    fn round_trip(text: &str) -> String {
        let line = format!("k: {}\n", scalar(text));
        let Some(Node::Map { entries, .. }) = parse(&line).unwrap() else {
            panic!("not a map: {line}")
        };
        entries[0].1.as_text().unwrap().to_owned()
    }

    #[test]
    fn plain_when_unambiguous() {
        for text in [
            "Marie Dupont",
            "people",
            "state-log",
            "01jcx8m2p4q6r8t0v2w4y6z8a0",
            "marie@example.org",
            "Café de l'Œuvre",
            "contacts:abc",
        ] {
            assert_eq!(scalar(text), text, "{text:?} should stay plain");
        }
    }

    #[test]
    fn quoted_when_a_reader_could_misread_it() {
        for text in [
            "0612345678",
            "+33612345678",
            "1e5",
            "12:30",
            "0x1F",
            "true",
            "No",
            "null",
            "~",
            "",
            " padded",
            "- dash",
            "a: b",
            "a #b",
            "[[lyon]]",
            "1965-03-12",
            "--03-12",
            "line\nbreak",
            "\"quoted\"",
            "tab\there",
            "bell\u{7}",
        ] {
            assert_ne!(scalar(text), text, "{text:?} should be quoted");
            assert_eq!(round_trip(text), text, "{text:?} must read back unchanged");
        }
    }

    #[test]
    fn plain_values_read_back_unchanged() {
        for text in [
            "Marie Dupont",
            "Café",
            "x:y",
            "a-b-c",
            "Maman ❤️",
            "back\\slash",
        ] {
            assert_eq!(round_trip(text), text);
        }
    }

    #[test]
    fn list_entry() {
        let out = Out::List(vec!["a@example.org".into(), "0612345678".into()]);
        assert_eq!(
            entry("emails", &out),
            "emails:\n  - a@example.org\n  - \"0612345678\"\n"
        );
        assert_eq!(entry("emails", &Out::List(vec![])), "emails: []\n");
    }

    #[test]
    fn pair_and_bool_entries() {
        assert_eq!(
            entry("geo", &Out::Pair(45.7578, 4.832)),
            "geo: [45.7578, 4.832]\n"
        );
        assert_eq!(entry("owner", &Out::Bool(true)), "owner: true\n");
    }
}
