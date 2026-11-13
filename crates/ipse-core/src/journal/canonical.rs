//! Canonical JSON and the hash of a record's `data` (`docs/spec/journal.md`, section 3).
//!
//! Object keys are sorted at every level and no space is written, so two payloads that
//! differ only by key order or formatting have the same hash. Sorting is done here, not
//! left to `serde_json`'s map type, whose order depends on crate features.

use serde_json::Value;

/// `b3:` followed by the BLAKE3 hash, in lowercase hex, of the canonical JSON of `data`.
#[must_use]
pub fn data_hash(data: &Value) -> String {
    let mut bytes = Vec::with_capacity(256);
    write_canonical(data, &mut bytes);
    format!("b3:{}", blake3::hash(&bytes).to_hex())
}

fn write_canonical(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push(b'{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_scalar(&Value::String(key.clone()), out);
                out.push(b':');
                if let Some(item) = map.get(key) {
                    write_canonical(item, out);
                }
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_canonical(item, out);
            }
            out.push(b']');
        }
        scalar => write_scalar(scalar, out),
    }
}

fn write_scalar(value: &Value, out: &mut Vec<u8>) {
    // Writing a JSON scalar to a `Vec` cannot fail.
    let _ = serde_json::to_writer(out, value);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn key_order_and_spacing_do_not_matter() {
        let a: Value = serde_json::from_str(r#"{"b":1,"a":{"y":[1,2],"x":"é"}}"#).unwrap();
        let b: Value =
            serde_json::from_str(r#"{ "a": { "x": "é", "y": [1, 2] }, "b": 1 }"#).unwrap();
        assert_eq!(data_hash(&a), data_hash(&b));
    }

    #[test]
    fn content_matters() {
        assert_ne!(data_hash(&json!({"a": 1})), data_hash(&json!({"a": 2})));
        assert_ne!(data_hash(&json!([1, 2])), data_hash(&json!([2, 1])));
    }

    #[test]
    fn canonical_form() {
        let mut out = Vec::new();
        write_canonical(&json!({"b": [true, null], "a": "x\"y"}), &mut out);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            r#"{"a":"x\"y","b":[true,null]}"#
        );
        assert!(data_hash(&json!({})).starts_with("b3:"));
        assert_eq!(data_hash(&json!({})).len(), 3 + 64);
    }
}
