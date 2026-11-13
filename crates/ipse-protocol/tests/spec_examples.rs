//! Every JSON example of `docs/spec/connectors.md` must parse into this crate's types
//! and serialize back to the same JSON. This keeps the spec and the code in step.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use ipse_protocol::{ConnectorMessage, CoreMessage, from_line, to_line};
use serde_json::Value;

const SPEC: &str = include_str!("../../../docs/spec/connectors.md");

const FROM_CORE: [&str; 4] = ["hello", "start", "ack", "bye"];
const FROM_CONNECTOR: [&str; 7] = [
    "ready", "batch", "progress", "need", "done", "error", "alive",
];

/// The JSON objects found in the spec's JSON code blocks, one per line.
fn spec_examples() -> Vec<Value> {
    let mut examples = Vec::new();
    let mut in_json = false;
    for line in SPEC.lines() {
        match line.trim() {
            "```json" => in_json = true,
            "```" => in_json = false,
            text if in_json && text.starts_with('{') => {
                examples.push(serde_json::from_str(text).expect("spec example is valid JSON"));
            }
            _ => {}
        }
    }
    examples
}

fn round_trip<M>(example: &Value) -> Value
where
    M: serde::Serialize + for<'a> serde::Deserialize<'a>,
{
    let line = format!("{example}\n");
    let message: M = from_line(line.as_bytes()).expect("example parses");
    let encoded = to_line(&message).expect("message encodes");
    assert_eq!(encoded.last(), Some(&b'\n'));
    serde_json::from_slice(&encoded).expect("encoded line is JSON")
}

#[test]
fn every_spec_example_round_trips() {
    let examples = spec_examples();
    let mut seen = Vec::new();

    for example in &examples {
        let kind = example["type"].as_str().expect("example has a type");
        let back = if FROM_CORE.contains(&kind) {
            round_trip::<CoreMessage>(example)
        } else if FROM_CONNECTOR.contains(&kind) {
            round_trip::<ConnectorMessage>(example)
        } else {
            panic!("spec example of unknown type {kind}");
        };
        assert_eq!(&back, example, "{kind} changed in a round trip");
        seen.push(kind.to_owned());
    }

    for kind in FROM_CORE.iter().chain(FROM_CONNECTOR.iter()) {
        if *kind != "alive" {
            assert!(
                seen.iter().any(|s| s == kind),
                "the spec has no {kind} example"
            );
        }
    }
}

#[test]
fn unknown_fields_are_ignored() {
    let line = br#"{"type":"bye","added_in_protocol_2":true}"#;
    assert_eq!(from_line::<CoreMessage>(line).unwrap(), CoreMessage::Bye);
}

#[test]
fn unknown_type_is_an_error() {
    let line = br#"{"type":"teleport"}"#;
    assert!(from_line::<ConnectorMessage>(line).is_err());
}

#[test]
fn accepts_crlf_line_endings() {
    let line = b"{\"type\":\"alive\"}\r\n";
    assert_eq!(
        from_line::<ConnectorMessage>(line).unwrap(),
        ConnectorMessage::Alive
    );
}

#[test]
fn secrets_never_show_in_debug_output() {
    let line = br#"{"type":"hello","protocol":1,"instance":"calendar","mode":"sync","language":"fr","time_zone":"Europe/Paris","cursor":null,"secrets":{"oauth":"ya29.secret"}}"#;
    let message: CoreMessage = from_line(line).unwrap();
    assert!(!format!("{message:?}").contains("ya29.secret"));
}
