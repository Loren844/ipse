//! One line of the journal (`docs/spec/journal.md`, section 3).

use ipse_protocol::Op;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the record format this Ipse writes and reads.
pub const RECORD_VERSION: u32 = 1;

/// A record as stored in the journal: what the connector sent, plus the fields the
/// core adds when writing it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalRecord {
    /// Record format version.
    pub v: u32,
    /// Lowercase ULID assigned when written; sorts by write time.
    pub id: String,
    /// The connector instance: `calendar`, `contacts-2`.
    pub source: String,
    /// What the data is: `calendar.event`, `contact.card`.
    pub kind: String,
    /// What the record says about the item.
    pub op: Op,
    /// The item's identifier at the source, for `upsert` and `delete`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// When the fact was true at the source.
    pub observed_at: Timestamp,
    /// When Ipse wrote the record.
    pub ingested_at: Timestamp,
    /// Hash of `data`, for `upsert`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// The connector's payload, as sent. Absent for `delete`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = include_str!("../../../../docs/spec/journal.md");
    const FIELDS: [&str; 10] = [
        "v",
        "id",
        "source",
        "kind",
        "op",
        "external_id",
        "observed_at",
        "ingested_at",
        "hash",
        "data",
    ];

    /// Top-level fields in the order they appear on the line.
    fn field_order(line: &str) -> Vec<&'static str> {
        let mut found: Vec<(usize, &'static str)> = FIELDS
            .iter()
            .filter_map(|f| line.find(&format!(",\"{f}\":")).map(|i| (i, *f)))
            .collect();
        if line.starts_with("{\"v\":") {
            found.push((0, "v"));
        }
        found.sort_unstable();
        found.into_iter().map(|(_, f)| f).collect()
    }

    #[test]
    fn written_like_the_spec_examples() {
        let examples: Vec<&str> = SPEC
            .lines()
            .filter(|l| l.starts_with("{\"v\":1,"))
            .collect();
        assert_eq!(examples.len(), 2, "the spec shows two records");

        let record = JournalRecord {
            v: 1,
            id: "01jcxb7k2m4p6r8t0v2w4y6z8a".into(),
            source: "calendar".into(),
            kind: "calendar.event".into(),
            op: Op::Upsert,
            external_id: Some("5f2a9c1e@example.org".into()),
            observed_at: "2026-11-12T18:30:00Z".parse().unwrap(),
            ingested_at: "2026-11-12T18:31:04Z".parse().unwrap(),
            hash: Some("b3:00".into()),
            data: Some(serde_json::json!({"title": "Dîner"})),
        };
        let line = serde_json::to_string(&record).unwrap();
        assert_eq!(field_order(&line), field_order(examples[0]));
        assert!(line.contains(r#""observed_at":"2026-11-12T18:30:00Z""#));
        assert_eq!(
            serde_json::from_str::<JournalRecord>(&line).unwrap(),
            record
        );

        for example in examples {
            let value: Value = serde_json::from_str(example).unwrap();
            for required in [
                "v",
                "id",
                "source",
                "kind",
                "op",
                "observed_at",
                "ingested_at",
                "data",
            ] {
                assert!(
                    value.get(required).is_some(),
                    "spec example lacks {required}"
                );
            }
        }
    }
}
