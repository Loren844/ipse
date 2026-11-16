//! The front-matter keys a proposal may set, read and written as JSON values: a string,
//! a list of strings, or `null` for none.

use serde_json::Value;

use crate::vault::{Birthday, EventStatus, EventTime, LinkOrText, MemoryFile, WikiLink};

/// The keys a proposal may set on an entity.
pub(super) const ENTITY_KEYS: [&str; 7] = [
    "name",
    "aliases",
    "birthday",
    "emails",
    "phones",
    "address",
    "located_in",
];

/// The keys a proposal may set on an event.
pub(super) const EVENT_KEYS: [&str; 6] =
    ["title", "start", "end", "place", "participants", "status"];

/// Keys whose value is a list.
const LISTS: [&str; 4] = ["aliases", "emails", "phones", "participants"];

/// `value` in the form keys are compared in: a list key's `null` is an empty list.
pub(super) fn normalized(key: &str, value: &Value) -> Value {
    if LISTS.contains(&key) && value.is_null() {
        Value::Array(Vec::new())
    } else {
        value.clone()
    }
}

/// The current value of `key` in `file`; `None` when the file has no such key.
pub(crate) fn get(file: &MemoryFile, key: &str) -> Option<Value> {
    let text = |t: Option<String>| t.map_or(Value::Null, Value::String);
    let list = |l: &[String]| Value::Array(l.iter().cloned().map(Value::String).collect());
    if let Some(e) = file.entity() {
        return Some(match key {
            "name" => text(e.name.clone()),
            "aliases" => list(&e.aliases),
            "birthday" => text(e.birthday.map(|b| b.to_string())),
            "emails" => list(&e.emails),
            "phones" => list(&e.phones),
            "address" => text(e.address.clone()),
            "located_in" => text(e.located_in.as_ref().map(ToString::to_string)),
            _ => return None,
        });
    }
    let e = file.event()?;
    Some(match key {
        "title" => Value::String(e.title.clone()),
        "start" => Value::String(e.start.to_string()),
        "end" => text(e.end.map(|t| t.to_string())),
        "place" => text(e.place.as_ref().map(ToString::to_string)),
        "participants" => Value::Array(
            e.participants
                .iter()
                .map(|p| Value::String(p.to_string()))
                .collect(),
        ),
        "status" => text(e.status.map(|s| s.as_str().to_owned())),
        _ => return None,
    })
}

/// Sets `key` of `file` to `value`. The error says what is wrong, without the value.
pub(super) fn set(file: &mut MemoryFile, key: &str, value: &Value) -> Result<(), String> {
    if let Some(e) = file.entity_mut() {
        match key {
            "name" => {
                // The old name stays as another name: the person may still say it.
                let new = optional_text(key, value)?;
                if let (Some(old), Some(new)) = (&e.name, &new)
                    && old != new
                    && !e.aliases.contains(old)
                {
                    e.aliases.push(old.clone());
                }
                e.name = new;
            }
            "aliases" => e.aliases = list(key, value)?,
            "birthday" => {
                e.birthday = optional(
                    key,
                    value,
                    Birthday::parse,
                    "a date: 1990-04-02, or --04-02 without the year",
                )?;
            }
            "emails" => e.emails = list(key, value)?,
            "phones" => e.phones = list(key, value)?,
            "address" => e.address = optional_text(key, value)?,
            "located_in" => {
                e.located_in = optional(key, value, WikiLink::parse, "a link: [[file-name]]")?;
            }
            _ => return Err(format!("`{key}` cannot be set on this file")),
        }
        return Ok(());
    }
    let Some(e) = file.event_mut() else {
        return Err(format!("`{key}` cannot be set on this file"));
    };
    match key {
        "title" => {
            e.title =
                optional_text(key, value)?.ok_or_else(|| format!("`{key}` cannot be empty"))?;
        }
        "start" => {
            e.start = optional(key, value, EventTime::parse, DATE_OR_TIME)?
                .ok_or_else(|| format!("`{key}` cannot be empty"))?;
        }
        "end" => e.end = optional(key, value, EventTime::parse, DATE_OR_TIME)?,
        "place" => e.place = optional_text(key, value)?.map(|t| LinkOrText::parse(&t)),
        "participants" => {
            e.participants = list(key, value)?
                .iter()
                .map(|p| LinkOrText::parse(p))
                .collect();
        }
        "status" => {
            e.status = optional(key, value, EventStatus::parse, "planned, done or cancelled")?;
        }
        _ => return Err(format!("`{key}` cannot be set on this file")),
    }
    Ok(())
}

const DATE_OR_TIME: &str =
    "a day (2026-11-14) or a date and time with its offset (2026-11-14T20:00:00+01:00)";

fn optional_text(key: &str, value: &Value) -> Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(s) if s.trim().is_empty() => Ok(None),
        Value::String(s) if s.contains('\n') => Err(format!("`{key}` must be on one line")),
        Value::String(s) => Ok(Some(s.trim().to_owned())),
        _ => Err(format!("`{key}` must be a string or null")),
    }
}

fn optional<T>(
    key: &str,
    value: &Value,
    parse: impl Fn(&str) -> Option<T>,
    form: &str,
) -> Result<Option<T>, String> {
    match optional_text(key, value)? {
        None => Ok(None),
        Some(text) => parse(&text)
            .map(Some)
            .ok_or_else(|| format!("`{key}` must be {form}")),
    }
}

fn list(key: &str, value: &Value) -> Result<Vec<String>, String> {
    let items = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Array(items) => items,
        _ => return Err(format!("`{key}` must be a list of strings")),
    };
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::String(s) if s.contains('\n') => {
                return Err(format!("each item of `{key}` must be on one line"));
            }
            Value::String(s) if !s.trim().is_empty() => {
                let s = s.trim().to_owned();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            Value::String(_) => {}
            _ => return Err(format!("`{key}` must be a list of strings")),
        }
    }
    Ok(out)
}
