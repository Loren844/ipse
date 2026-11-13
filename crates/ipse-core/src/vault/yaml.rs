//! A small YAML tree built from parser events, keeping each scalar's original text.
//!
//! YAML resolves `0612345678` to a number and drops its leading zero. Ipse decides how
//! to read each value from the key it belongs to (a phone number is text), so the tree
//! keeps scalars as written and leaves interpretation to the caller.

use std::borrow::Cow;

use saphyr_parser::{Event, Parser, ScalarStyle, ScanError, Span, StrInput};

/// Deepest nesting accepted, so that a hostile file cannot exhaust the stack.
const MAX_DEPTH: usize = 32;

pub(crate) const NOT_VALID: &str =
    "the front matter is not valid YAML here: check quotes, colons and indentation";
const ANCHORS: &str = "anchors and aliases (`&` and `*`) are not supported in the front matter";
const TOO_DEEP: &str = "the front matter is nested too deeply";
const MANY_DOCUMENTS: &str = "the front matter holds more than one YAML document";

/// One node of the tree. `line` is 1-based, relative to the parsed text.
#[derive(Debug)]
pub(crate) enum Node<'a> {
    Scalar {
        text: Cow<'a, str>,
        style: ScalarStyle,
        line: u32,
    },
    Seq {
        items: Vec<Node<'a>>,
        line: u32,
    },
    Map {
        entries: Vec<(Node<'a>, Node<'a>)>,
        line: u32,
    },
}

impl Node<'_> {
    pub(crate) fn line(&self) -> u32 {
        match self {
            Self::Scalar { line, .. } | Self::Seq { line, .. } | Self::Map { line, .. } => *line,
        }
    }

    /// True for an empty value, `~` or `null` written without quotes.
    pub(crate) fn is_null(&self) -> bool {
        matches!(
            self,
            Self::Scalar { text, style: ScalarStyle::Plain, .. }
                if matches!(text.as_ref(), "" | "~" | "null" | "Null" | "NULL")
        )
    }

    /// The scalar's text, whatever its quoting style.
    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Scalar { text, .. } => Some(text),
            _ => None,
        }
    }
}

/// A YAML problem, with its 1-based line when known. Never holds the file's content.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct YamlError {
    pub(crate) line: Option<u32>,
    pub(crate) problem: &'static str,
}

/// Parses one YAML document. Returns `None` for an empty document.
pub(crate) fn parse(text: &str) -> Result<Option<Node<'_>>, YamlError> {
    let mut parser = Parser::new_from_str(text);
    let mut root = None;
    let mut documents = 0_u32;
    while let Some(next) = parser.next_event() {
        let (event, span) = next.map_err(scan_error)?;
        match event {
            Event::StreamEnd => break,
            Event::StreamStart | Event::DocumentEnd | Event::Nothing => {}
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(YamlError {
                        line: Some(line_of(span)),
                        problem: MANY_DOCUMENTS,
                    });
                }
            }
            event => root = Some(node(event, span, &mut parser, 0)?),
        }
    }
    Ok(root)
}

fn node<'a>(
    event: Event<'a>,
    span: Span,
    parser: &mut Parser<'a, StrInput<'a>>,
    depth: usize,
) -> Result<Node<'a>, YamlError> {
    let line = line_of(span);
    let at = |problem| YamlError {
        line: Some(line),
        problem,
    };
    if depth > MAX_DEPTH {
        return Err(at(TOO_DEEP));
    }
    match event {
        Event::Scalar(text, style, anchor, _) => {
            if anchor != 0 {
                return Err(at(ANCHORS));
            }
            Ok(Node::Scalar { text, style, line })
        }
        Event::SequenceStart(anchor, _) => {
            if anchor != 0 {
                return Err(at(ANCHORS));
            }
            let mut items = Vec::new();
            loop {
                let (event, span) = next(parser)?;
                if matches!(event, Event::SequenceEnd) {
                    break;
                }
                items.push(node(event, span, parser, depth + 1)?);
            }
            Ok(Node::Seq { items, line })
        }
        Event::MappingStart(anchor, _) => {
            if anchor != 0 {
                return Err(at(ANCHORS));
            }
            let mut entries = Vec::new();
            loop {
                let (event, span) = next(parser)?;
                if matches!(event, Event::MappingEnd) {
                    break;
                }
                let key = node(event, span, parser, depth + 1)?;
                let (event, span) = next(parser)?;
                let value = node(event, span, parser, depth + 1)?;
                entries.push((key, value));
            }
            Ok(Node::Map { entries, line })
        }
        Event::Alias(_) => Err(at(ANCHORS)),
        _ => Err(at(NOT_VALID)),
    }
}

fn next<'a>(parser: &mut Parser<'a, StrInput<'a>>) -> Result<(Event<'a>, Span), YamlError> {
    match parser.next_event() {
        Some(result) => result.map_err(scan_error),
        None => Err(YamlError {
            line: None,
            problem: NOT_VALID,
        }),
    }
}

/// The scanner's own message may quote the file, so only its position is kept.
#[allow(clippy::needless_pass_by_value)] // used as `map_err(scan_error)`
fn scan_error(err: ScanError) -> YamlError {
    YamlError {
        line: Some(to_u32(err.marker().line())),
        problem: NOT_VALID,
    }
}

fn line_of(span: Span) -> u32 {
    to_u32(span.start.line())
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(text: &str) -> Node<'_> {
        parse(text).unwrap().unwrap()
    }

    #[test]
    fn keeps_scalars_as_written() {
        let node = root("phone: 0612345678\nok: true\n");
        let Node::Map { entries, .. } = node else {
            panic!("not a map")
        };
        assert_eq!(entries[0].1.as_text(), Some("0612345678"));
        assert_eq!(entries[1].1.as_text(), Some("true"));
    }

    #[test]
    fn records_lines() {
        let Node::Map { entries, .. } = root("a: 1\n\nb:\n  - x\n") else {
            panic!("not a map")
        };
        assert_eq!(entries[0].0.line(), 1);
        assert_eq!(entries[1].0.line(), 3);
    }

    #[test]
    fn null_values() {
        let Node::Map { entries, .. } = root("a:\nb: ~\nc: \"\"\n") else {
            panic!("not a map")
        };
        assert!(entries[0].1.is_null());
        assert!(entries[1].1.is_null());
        assert!(!entries[2].1.is_null(), "a quoted empty string is a value");
    }

    #[test]
    fn empty_document() {
        assert!(parse("").unwrap().is_none());
    }

    #[test]
    fn invalid_yaml_gives_a_line_and_no_content() {
        let err = parse("a: 1\nb: [unclosed\n").unwrap_err();
        assert!(err.line.is_some());
        assert_eq!(err.problem, NOT_VALID);
    }

    #[test]
    fn anchors_are_refused() {
        assert_eq!(parse("a: &x 1\nb: *x\n").unwrap_err().problem, ANCHORS);
    }

    #[test]
    fn deep_nesting_is_refused_without_overflow() {
        let text = format!("a: {}{}\n", "[".repeat(10_000), "]".repeat(10_000));
        assert!(parse(&text).is_err());
        let text = format!("a: {}{}\n", "[".repeat(40), "]".repeat(40));
        assert_eq!(parse(&text).unwrap_err().problem, TOO_DEEP);
    }

    #[test]
    fn two_documents_are_refused() {
        assert_eq!(
            parse("a: 1\n...\n---\nb: 2\n").unwrap_err().problem,
            MANY_DOCUMENTS
        );
    }
}
