//! Records a connector sends inside a batch.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a record says about an item (`docs/spec/journal.md`, section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    /// The item exists at the source with this content.
    Upsert,
    /// The item no longer exists at the source.
    Delete,
    /// A one-off observation with no identity, such as a song being played.
    Observe,
}

/// One record as a connector sends it.
///
/// The core adds the journal fields a connector cannot know (`v`, `id`, `source`,
/// `ingested_at`, `hash`) before writing it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// What the data is, `<domain>.<type>`: `calendar.event`, `contact.card`.
    pub kind: String,
    /// What the record says about the item.
    pub op: Op,
    /// The item's identifier at the source. Required for `upsert` and `delete`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// When the fact was true at the source.
    pub observed_at: Timestamp,
    /// The connector's payload. Absent for `delete`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Record {
    /// Checks the rules a record must follow whatever its kind.
    ///
    /// # Errors
    ///
    /// Returns a short description of the first broken rule: an empty kind, a missing
    /// `external_id` for `upsert` or `delete`, or missing `data` for `upsert` or
    /// `observe`.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.kind.is_empty() {
            return Err("`kind` is empty");
        }
        match self.op {
            Op::Upsert | Op::Delete if self.external_id.is_none() => {
                Err("`external_id` is required for `upsert` and `delete`")
            }
            Op::Upsert | Op::Observe if self.data.is_none() => {
                Err("`data` is required for `upsert` and `observe`")
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(op: Op, external_id: Option<&str>, data: Option<Value>) -> Record {
        Record {
            kind: "calendar.event".into(),
            op,
            external_id: external_id.map(Into::into),
            observed_at: "2026-11-12T18:30:00Z".parse().unwrap(),
            data,
        }
    }

    #[test]
    fn valid_records() {
        let data = Some(serde_json::json!({"title": "x"}));
        assert!(
            record(Op::Upsert, Some("a"), data.clone())
                .validate()
                .is_ok()
        );
        assert!(record(Op::Delete, Some("a"), None).validate().is_ok());
        assert!(record(Op::Observe, None, data).validate().is_ok());
    }

    #[test]
    fn upsert_needs_external_id_and_data() {
        let data = Some(serde_json::json!({}));
        assert!(record(Op::Upsert, None, data).validate().is_err());
        assert!(record(Op::Upsert, Some("a"), None).validate().is_err());
    }

    #[test]
    fn delete_needs_external_id() {
        assert!(record(Op::Delete, None, None).validate().is_err());
    }
}
