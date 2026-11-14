//! The owner's own contact card → relations of the owner's file.
//!
//! Address books often know who is who: the owner's card lists related people
//! ("mother: Marie Dupont"), and contacts are often saved under a nickname ("Maman").
//! Ipse writes those links as relations of the owner's file, in the vault's language:
//! `- mère : [[maman]]`.
//!
//! - The owner's card is the one card whose email or phone is also on the owner's file;
//!   it is never guessed from a name.
//! - A related person is linked only when exactly one person file has their name, or one
//!   of their other names, or holds the card they point to.
//! - The person always wins: a relation Ipse added is remembered in
//!   [`ADDED_FILE`], so that one the person removed is not added again.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::entity_files;
use crate::vault::{
    Change, DateTime, FileType, Language, MemoryFile, Relation, Vault, WikiLink, body, write_atomic,
};
use crate::{Error, Result};

/// The relations Ipse added to the owner's file, so that one the person removed is not
/// added again. Relative to the vault's folder.
pub const ADDED_FILE: &str = ".ipse/added-relations.toml";

/// Someone a card names as related.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Related {
    name: Option<String>,
    uid: Option<String>,
    kind: Option<String>,
}

impl Related {
    /// The `related` list of a `contact.card` record's data.
    pub(crate) fn list(data: &Value) -> Vec<Self> {
        let Some(items) = data.get("related").and_then(Value::as_array) else {
            return Vec::new();
        };
        let text = |item: &Value, key: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        items
            .iter()
            .map(|item| Self {
                name: text(item, "name"),
                uid: text(item, "uid"),
                kind: text(item, "kind"),
            })
            .filter(|r| r.name.is_some() || r.uid.is_some())
            .collect()
    }
}

/// What Ipse needs from a card to find the owner's and follow its links.
#[derive(Debug, Clone)]
pub(crate) struct OwnerCandidate {
    pub(crate) emails: Vec<String>,
    pub(crate) phones: Vec<String>,
    pub(crate) related: Vec<Related>,
}

/// One person file, as relations need it.
struct Person {
    stem: String,
    id: String,
    owner: bool,
}

/// Adds to the owner's file the relations their own card names (`related`), and
/// returns how many.
///
/// # Errors
///
/// Vault errors: the owner's file cannot be read or written, or [`ADDED_FILE`] cannot
/// be read or saved.
pub(crate) fn add_owner_relations(
    vault: &Vault,
    instance: &str,
    related: &[Related],
    change: &mut Change<'_>,
) -> Result<u64> {
    if related.is_empty() {
        return Ok(0);
    }
    let root = vault.root();
    let mut owner_file = MemoryFile::read(vault.owner_file())?;

    let (people, by_name, by_source) = people(root, instance)?;
    let language = vault.settings().language();
    let current = owner_file.sections();
    let mut added = Added::read(root)?;
    let mut new = Vec::new();
    for related in related {
        let matches: Vec<usize> = match (&related.uid, &related.name) {
            (Some(uid), _) => by_source
                .get(&uid_key(uid))
                .into_iter()
                .flatten()
                .copied()
                .collect(),
            (None, Some(name)) => by_name.get(&name_key(name)).cloned().unwrap_or_default(),
            (None, None) => Vec::new(),
        };
        let [only] = matches[..] else {
            continue;
        };
        let person = &people[only];
        if person.owner {
            continue;
        }
        let key = format!("{} {}", related.kind.as_deref().unwrap_or("-"), person.id);
        let linked = current
            .relations
            .iter()
            .chain(&new)
            .any(|r| r.link.target == person.stem);
        if linked || added.contains(&key) {
            continue;
        }
        let Some(link) = WikiLink::parse(&format!("[[{}]]", person.stem)) else {
            continue;
        };
        new.push(Relation {
            word: word(related.kind.as_deref(), language),
            link,
        });
        added.insert(key);
    }
    if new.is_empty() {
        return Ok(0);
    }
    let text = body::add_relations(owner_file.body(), &new, language);
    owner_file.set_body(text);
    owner_file.common_mut().updated = DateTime::now(vault.settings().time_zone());
    owner_file.write()?;
    change.touched(vault.owner_file());
    added.save()?;
    Ok(new.len() as u64)
}

/// The position of the one card whose email or phone is on the owner's file. Two such
/// cards: none.
pub(crate) fn owner_card(
    cards: &[OwnerCandidate],
    emails: &[String],
    phones: &[String],
) -> Option<usize> {
    let emails: BTreeSet<String> = emails.iter().map(|e| e.trim().to_lowercase()).collect();
    let phones: BTreeSet<String> = phones.iter().filter_map(|p| phone_key(p)).collect();
    if emails.is_empty() && phones.is_empty() {
        return None;
    }
    let mut found = cards.iter().enumerate().filter(|(_, card)| {
        card.emails
            .iter()
            .any(|e| emails.contains(&e.trim().to_lowercase()))
            || card
                .phones
                .iter()
                .filter_map(|p| phone_key(p))
                .any(|p| phones.contains(&p))
    });
    let (i, _) = found.next()?;
    found.next().is_none().then_some(i)
}

/// The people files, and indexes by name and other name, and by the cards of
/// `instance` they come from: a card points to cards of its own address book.
#[allow(clippy::type_complexity)] // three views of one list, built in one pass
fn people(
    root: &Path,
    instance: &str,
) -> Result<(
    Vec<Person>,
    HashMap<String, Vec<usize>>,
    HashMap<String, Vec<usize>>,
)> {
    let mut people = Vec::new();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_source: HashMap<String, Vec<usize>> = HashMap::new();
    for path in entity_files(&root.join(FileType::Person.folder()), false)? {
        let Ok(file) = MemoryFile::read(&path) else {
            continue;
        };
        let (Some(entity), Some(stem)) = (file.entity(), path.file_stem()) else {
            continue;
        };
        let i = people.len();
        let names: BTreeSet<String> = entity
            .name
            .iter()
            .chain(&entity.aliases)
            .map(|n| name_key(n))
            .filter(|n| !n.is_empty())
            .collect();
        for name in names {
            by_name.entry(name).or_default().push(i);
        }
        for source in &file.common().sources {
            if let Some((from, id)) = source.split_once(':')
                && from == instance
            {
                by_source.entry(uid_key(id)).or_default().push(i);
            }
        }
        people.push(Person {
            stem: stem.to_string_lossy().into_owned(),
            id: file.common().id.to_string(),
            owner: entity.owner,
        });
    }
    Ok((people, by_name, by_source))
}

/// A name compared without case or extra spaces.
pub(crate) fn name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A card's `UID`, with or without `urn:uuid:`, without case.
fn uid_key(uid: &str) -> String {
    let uid = uid.trim();
    let bare = uid
        .get(..9)
        .filter(|scheme| scheme.eq_ignore_ascii_case("urn:uuid:"))
        .map_or(uid, |_| &uid[9..]);
    bare.to_lowercase()
}

/// The last 9 digits of a phone number, so that `+33 6 98 76 54 32` and
/// `06 98 76 54 32` match. Shorter numbers are not compared.
pub(crate) fn phone_key(phone: &str) -> Option<String> {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 9).then(|| digits[digits.len() - 9..].to_owned())
}

/// The relation word for `kind`, in `language`. A word the person typed is kept.
fn word(kind: Option<&str>, language: Language) -> String {
    let fr = language == Language::Fr;
    let Some(kind) = kind else {
        return if fr { "proche" } else { "related" }.to_owned();
    };
    let (en, fr_word) = match kind.to_lowercase().as_str() {
        "mother" => ("mother", "mère"),
        "father" => ("father", "père"),
        "parent" => ("parent", "parent"),
        "brother" => ("brother", "frère"),
        "sister" => ("sister", "sœur"),
        "sibling" => ("sibling", "frère ou sœur"),
        "child" => ("child", "enfant"),
        "son" => ("son", "fils"),
        "daughter" => ("daughter", "fille"),
        "spouse" => ("spouse", "conjoint"),
        "husband" => ("husband", "mari"),
        "wife" => ("wife", "épouse"),
        "partner" => ("partner", "partenaire"),
        "friend" => ("friend", "ami"),
        "colleague" => ("colleague", "collègue"),
        "manager" => ("manager", "responsable"),
        "assistant" => ("assistant", "assistant"),
        "neighbor" => ("neighbor", "voisin"),
        "acquaintance" => ("acquaintance", "connaissance"),
        "kin" | "relative" => ("relative", "famille"),
        "domestic partner" => ("domestic partner", "partenaire"),
        "referred by" => ("referred by", "recommandé par"),
        "grandmother" => ("grandmother", "grand-mère"),
        "grandfather" => ("grandfather", "grand-père"),
        "grandparent" => ("grandparent", "grand-parent"),
        "grandchild" => ("grandchild", "petit-enfant"),
        "aunt" => ("aunt", "tante"),
        "uncle" => ("uncle", "oncle"),
        "cousin" => ("cousin", "cousin"),
        "niece" => ("niece", "nièce"),
        "nephew" => ("nephew", "neveu"),
        "emergency" => ("emergency contact", "contact d'urgence"),
        "other" | "contact" => ("related", "proche"),
        _ => return kind.to_owned(),
    };
    if fr { fr_word } else { en }.to_owned()
}

/// The relations Ipse added to the owner's file.
struct Added {
    path: PathBuf,
    keys: BTreeSet<String>,
    changed: bool,
}

#[derive(Deserialize)]
struct AddedFile {
    #[serde(default)]
    added: BTreeSet<String>,
}

impl Added {
    fn read(root: &Path) -> Result<Self> {
        let path = root.join(ADDED_FILE);
        let keys = match std::fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str::<AddedFile>(&text)
                    .map_err(|_| Error::Invalid {
                        path: path.clone(),
                        line: None,
                        problem: "this file is Ipse's own record of the relations it added; delete it and import again".to_owned(),
                    })?
                    .added
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeSet::new(),
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path,
                    source,
                });
            }
        };
        Ok(Self {
            path,
            keys,
            changed: false,
        })
    }

    fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    fn insert(&mut self, key: String) {
        self.changed |= self.keys.insert(key);
    }

    fn save(&self) -> Result<()> {
        if !self.changed {
            return Ok(());
        }
        let mut text = String::from(
            "# The relations Ipse added to your own file from your contact card: one you remove is not added again.\nadded = [\n",
        );
        for key in &self.keys {
            let _ = writeln!(
                text,
                "  {},",
                serde_json::to_string(key).unwrap_or_default()
            );
        }
        text.push_str("]\n");
        write_atomic(&self.path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, emails: &[&str], phones: &[&str]) -> OwnerCandidate {
        OwnerCandidate {
            related: vec![Related {
                name: Some(id.into()),
                uid: None,
                kind: None,
            }],
            emails: emails.iter().map(|s| (*s).to_owned()).collect(),
            phones: phones.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn the_owners_card_by_email_or_phone_never_by_name() {
        let cards = [
            card("a", &["ana@example.org"], &[]),
            card("me", &["Camille@Example.org"], &[]),
            card("c", &[], &["+33 6 98 76 54 32"]),
        ];
        let id = |emails: &[&str], phones: &[&str]| {
            let emails: Vec<String> = emails.iter().map(|s| (*s).to_owned()).collect();
            let phones: Vec<String> = phones.iter().map(|s| (*s).to_owned()).collect();
            owner_card(&cards, &emails, &phones).and_then(|i| cards[i].related[0].name.clone())
        };
        assert_eq!(id(&["camille@example.org"], &[]), Some("me".into()));
        assert_eq!(id(&[], &["06 98 76 54 32"]), Some("c".into()));
        assert_eq!(id(&[], &[]), None, "nothing to compare: no owner card");
        assert_eq!(
            id(&["camille@example.org"], &["0698765432"]),
            None,
            "two cards match: none is chosen"
        );
        assert_eq!(id(&[], &["12 34"]), None, "short numbers are not compared");
    }

    #[test]
    fn keys() {
        assert_eq!(name_key("  Marie   DUPONT "), "marie dupont");
        assert_eq!(uid_key("URN:UUID:7D1E-ab"), "7d1e-ab");
        assert_eq!(uid_key("card-1"), "card-1");
    }

    #[test]
    fn words_in_the_vaults_language() {
        assert_eq!(word(Some("mother"), Language::Fr), "mère");
        assert_eq!(word(Some("mother"), Language::En), "mother");
        assert_eq!(word(Some("Parrain"), Language::En), "Parrain");
        assert_eq!(word(None, Language::Fr), "proche");
        assert_eq!(word(Some("kin"), Language::En), "relative");
        assert_eq!(
            word(Some("Brother"), Language::Fr),
            "frère",
            "a typed English label"
        );
    }

    #[test]
    fn related_from_data() {
        let data = serde_json::json!({"related": [
            {"name": "Marie Dupont", "kind": "mother"},
            {"uid": "urn:uuid:1"},
            {"kind": "friend"},
        ]});
        let list = Related::list(&data);
        assert_eq!(
            list.len(),
            2,
            "an entry with neither name nor uid is skipped"
        );
        assert_eq!(list[1].uid.as_deref(), Some("urn:uuid:1"));
    }
}
