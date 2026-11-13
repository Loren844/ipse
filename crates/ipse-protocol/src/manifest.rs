//! A connector's manifest (`docs/spec/connectors.md`, section 2).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Mode;

/// Text in several languages, by language code. A plain string means English.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Localized {
    /// The same text in every language.
    Plain(String),
    /// One text per language code: `en`, `fr`.
    ByLanguage(BTreeMap<String, String>),
}

impl Localized {
    /// The text in `language`, else in English, else any text there is.
    #[must_use]
    pub fn get(&self, language: &str) -> &str {
        match self {
            Self::Plain(text) => text,
            Self::ByLanguage(texts) => texts
                .get(language)
                .or_else(|| texts.get("en"))
                .or_else(|| texts.values().next())
                .map_or("", String::as_str),
        }
    }
}

/// Which files a connector may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAccess {
    /// No file.
    None,
    /// Only files the person picks.
    Picked,
}

/// What a connector needs, shown to the person before it is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    /// Hosts it may contact; empty for none.
    #[serde(default)]
    pub network: Vec<String>,
    /// Files it may read.
    pub files: FileAccess,
    /// Secrets the core keeps in the keychain for it.
    #[serde(default)]
    pub secrets: Vec<String>,
}

/// Settings of the `sync` mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSettings {
    /// How often to sync by default, like `15m`.
    pub default_every: String,
}

/// Everything the core needs to know about a connector before starting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Protocol version it speaks.
    pub protocol: u32,
    /// Unique identifier, `[a-z0-9-]+`.
    pub id: String,
    /// Name shown to the person.
    pub name: Localized,
    /// One sentence shown to the person: what it brings.
    pub summary: Localized,
    /// The connector's version.
    pub version: String,
    /// How to start it.
    pub command: Vec<String>,
    /// Modes it supports.
    pub modes: Vec<Mode>,
    /// Decision 0008: 1 official API, 2 device, 3 relay, 4 file export.
    pub rung: u8,
    /// Record kinds it may send.
    pub kinds: Vec<String>,
    /// Default rights category of what it produces.
    pub category: String,
    /// What it needs.
    pub access: Access,
    /// Settings of the `sync` mode, when supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncSettings>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_text() {
        let plain = Localized::Plain("Agenda".into());
        assert_eq!(plain.get("fr"), "Agenda");
        let many = Localized::ByLanguage(BTreeMap::from([
            ("en".into(), "Contacts".into()),
            ("fr".into(), "Contacts (fr)".into()),
        ]));
        assert_eq!(many.get("fr"), "Contacts (fr)");
        assert_eq!(many.get("de"), "Contacts");
    }

    #[test]
    fn json_round_trip() {
        let manifest = Manifest {
            protocol: 1,
            id: "contacts".into(),
            name: Localized::Plain("Contacts".into()),
            summary: Localized::Plain("x".into()),
            version: "0.1.0".into(),
            command: vec!["ipse".into(), "connector".into(), "contacts".into()],
            modes: vec![Mode::Import],
            rung: 4,
            kinds: vec!["contact.card".into()],
            category: "people".into(),
            access: Access {
                network: vec![],
                files: FileAccess::Picked,
                secrets: vec![],
            },
            sync: None,
        };
        let json = serde_json::to_string(&manifest).unwrap();
        assert_eq!(serde_json::from_str::<Manifest>(&json).unwrap(), manifest);
    }
}
