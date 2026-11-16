//! Possible duplicates (decision 0011): people who may be the same person, events
//! copied into two calendars. Nothing is ever merged silently: each candidate becomes a
//! `merge` proposal by Ipse, saying why, that the person accepts (merge) or refuses
//! ("these are two people"). A refusal is remembered in `.ipse/not-duplicates.toml` and
//! never asked again.
//!
//! People are matched through maps of their emails and phone numbers, never by
//! comparing every pair: same email is "very likely", same phone "likely"; the same
//! name only adds weight to another clue. Events match on the same title and start
//! in two different calendars, and are proposed grouped per pair of calendars.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::proposals::{Author, Change, Proposals, Status};
use crate::slug::to_file_stem;
use crate::vault::{FileType, Language, MemoryFile, Vault, write_atomic};
use crate::{Error, Result};

/// Where the pairs the person said are different are kept, relative to the vault.
pub const NOT_DUPLICATES_FILE: &str = ".ipse/not-duplicates.toml";

/// More files than this sharing one email or phone are not one person (a company's
/// switchboard, `info@`): the clue is ignored.
const MAX_SHARING: usize = 8;

/// Merges in one proposal for events, at most.
const MAX_GROUPED: usize = 100;

/// One possible duplicate: the file to keep, the one to merge into it, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The file kept: the owner's, or the older one.
    pub keep: String,
    /// The file merged into it.
    pub remove: String,
    /// Why: `email`, `phone`, `name`, `title_start`.
    pub clues: Vec<String>,
}

/// One person file, as detection reads it.
struct Person {
    file: String,
    created: String,
    owner: bool,
    name: Option<String>,
}

/// A phone number in the form numbers are compared in: its last 9 digits, so that
/// `+33 6 12 34 56 78`, `0033612345678` and `06 12 34 56 78` match. `None` when it
/// holds fewer than 9 digits.
#[must_use]
pub fn phone_key(text: &str) -> Option<String> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    (digits.len() >= 9).then(|| digits[digits.len() - 9..].to_owned())
}

/// The memory files of `folder`, relative to the vault with `/`, recursively.
fn files_in(root: &Path, folder: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut stack = vec![root.join(folder)];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path: dir,
                    source,
                });
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md")
                && let Ok(rel) = path.strip_prefix(root)
            {
                found.push(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The people who may be the same, each pair once, keeping the owner's or the older
/// file. A name alone is never enough.
///
/// # Errors
///
/// [`Error::Io`] when the people folder cannot be read. Files that cannot be read are
/// skipped.
pub fn people(root: &Path) -> Result<Vec<Candidate>> {
    let mut people: Vec<Person> = Vec::new();
    let mut by_email: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_phone: HashMap<String, Vec<usize>> = HashMap::new();
    for file in files_in(root, FileType::Person.folder())? {
        let Ok(found) = MemoryFile::read(root.join(&file)) else {
            continue;
        };
        let Some(entity) = found.entity() else {
            continue;
        };
        let at = people.len();
        let mut emails: BTreeSet<String> = BTreeSet::new();
        for email in &entity.emails {
            emails.insert(email.trim().to_lowercase());
        }
        for email in emails {
            by_email.entry(email).or_default().push(at);
        }
        let phones: BTreeSet<String> = entity.phones.iter().filter_map(|p| phone_key(p)).collect();
        for phone in phones {
            by_phone.entry(phone).or_default().push(at);
        }
        people.push(Person {
            file,
            created: found.common().created.to_string(),
            owner: entity.owner,
            name: entity.name.as_deref().and_then(to_file_stem),
        });
    }
    let mut pairs: BTreeMap<(usize, usize), BTreeSet<&'static str>> = BTreeMap::new();
    for (clue, map) in [("email", &by_email), ("phone", &by_phone)] {
        for group in map.values() {
            if group.len() < 2 || group.len() > MAX_SHARING {
                continue;
            }
            for (i, &a) in group.iter().enumerate() {
                for &b in &group[i + 1..] {
                    pairs.entry((a.min(b), a.max(b))).or_default().insert(clue);
                }
            }
        }
    }
    let mut found = Vec::with_capacity(pairs.len());
    for ((a, b), mut clues) in pairs {
        let (a, b) = (&people[a], &people[b]);
        if a.name.is_some() && a.name == b.name {
            clues.insert("name");
        }
        // The owner's file is kept; between two others, the older one.
        let (keep, remove) =
            if a.owner || (!b.owner && (&a.created, &a.file) <= (&b.created, &b.file)) {
                (a, b)
            } else {
                (b, a)
            };
        if remove.owner {
            continue;
        }
        found.push(Candidate {
            keep: keep.file.clone(),
            remove: remove.file.clone(),
            clues: order(&clues),
        });
    }
    Ok(found)
}

/// Clues from the strongest.
fn order(clues: &BTreeSet<&'static str>) -> Vec<String> {
    ["email", "phone", "name", "title_start"]
        .into_iter()
        .filter(|c| clues.contains(c))
        .map(str::to_owned)
        .collect()
}

/// Events with the same title and start in two different calendars, grouped per pair
/// of calendars: the key names the two sources, `calendar` and `calendar-2`.
///
/// # Errors
///
/// [`Error::Io`] when the events folder cannot be read. Files that cannot be read are
/// skipped.
pub fn events(root: &Path) -> Result<BTreeMap<(String, String), Vec<Candidate>>> {
    struct Event {
        file: String,
        created: String,
        source: String,
    }
    let mut by_key: HashMap<(String, String), Vec<Event>> = HashMap::new();
    for file in files_in(root, FileType::Event.folder())? {
        let Ok(found) = MemoryFile::read(root.join(&file)) else {
            continue;
        };
        let Some(event) = found.event() else {
            continue;
        };
        let Some(title) = to_file_stem(&event.title) else {
            continue;
        };
        let source = found
            .common()
            .sources
            .first()
            .map_or("person", |s| s.split(':').next().unwrap_or(s))
            .to_owned();
        by_key
            .entry((title, event.start.to_string()))
            .or_default()
            .push(Event {
                file,
                created: found.common().created.to_string(),
                source,
            });
    }
    let mut groups: BTreeMap<(String, String), Vec<Candidate>> = BTreeMap::new();
    for mut same in by_key.into_values() {
        if same.len() < 2 || same.len() > MAX_SHARING {
            continue;
        }
        same.sort_by(|a, b| (&a.created, &a.file).cmp(&(&b.created, &b.file)));
        let keep = &same[0];
        // One copy at a time per kept event: a third one comes at the next look.
        if let Some(other) = same[1..].iter().find(|o| o.source != keep.source) {
            // The same calendar may hold two events alike: only copies across two are.
            let pair = if keep.source <= other.source {
                (keep.source.clone(), other.source.clone())
            } else {
                (other.source.clone(), keep.source.clone())
            };
            groups.entry(pair).or_default().push(Candidate {
                keep: keep.file.clone(),
                remove: other.file.clone(),
                clues: vec!["title_start".to_owned()],
            });
        }
    }
    for group in groups.values_mut() {
        group.sort_by(|a, b| a.keep.cmp(&b.keep));
    }
    Ok(groups)
}

/// The pairs of files the person said are different, by id.
#[derive(Debug, Default, Serialize, Deserialize)]
struct NotDuplicates {
    #[serde(default)]
    pairs: Vec<[String; 2]>,
}

fn read_not_duplicates(root: &Path) -> NotDuplicates {
    std::fs::read_to_string(root.join(NOT_DUPLICATES_FILE))
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn pair(a: &str, b: &str) -> [String; 2] {
    if a <= b {
        [a.to_owned(), b.to_owned()]
    } else {
        [b.to_owned(), a.to_owned()]
    }
}

/// The id of the file at `file`, or the path itself when it cannot be read.
fn id_of(root: &Path, file: &str) -> String {
    MemoryFile::read(root.join(file))
        .map_or_else(|_| file.to_owned(), |f| f.common().id.to_string())
}

/// Remembers that the files `a` and `b` (relative to the vault) are not the same: they
/// are never proposed again.
///
/// # Errors
///
/// [`Error::Io`] when the file cannot be written.
pub fn remember_different(root: &Path, a: &str, b: &str) -> Result<()> {
    let mut known = read_not_duplicates(root);
    let new = pair(&id_of(root, a), &id_of(root, b));
    if known.pairs.contains(&new) {
        return Ok(());
    }
    known.pairs.push(new);
    let mut text = String::from(
        "# Files the person said are not the same, by id: Ipse never proposes to merge them.\npairs = [\n",
    );
    for [a, b] in &known.pairs {
        let _ = writeln!(text, "  [\"{a}\", \"{b}\"],");
    }
    text.push_str("]\n");
    write_atomic(&root.join(NOT_DUPLICATES_FILE), text.as_bytes())
}

/// Finds the possible duplicates of the vault and makes a proposal by Ipse for each
/// new one: one per pair of people, one per pair of calendars for events. Pairs the
/// person said are different, and those already waiting, are left out. Returns how
/// many proposals it made.
///
/// # Errors
///
/// [`Error::Io`] when a folder cannot be read or a proposal cannot be saved.
pub fn propose(vault: &Vault) -> Result<usize> {
    let root = vault.root();
    let different: BTreeSet<[String; 2]> = read_not_duplicates(root).pairs.into_iter().collect();
    let proposals = Proposals::new(vault);
    // Pairs already proposed and not decided: never asked twice.
    let mut asked: BTreeSet<[String; 2]> = BTreeSet::new();
    for proposal in proposals.list()? {
        if !matches!(proposal.status, Status::Waiting | Status::Stale { .. }) {
            continue;
        }
        for change in &proposal.changes {
            if let Change::Merge { keep, remove, .. } = change {
                asked.insert(pair(keep, remove));
            }
        }
    }
    let new = |c: &Candidate| {
        !asked.contains(&pair(&c.keep, &c.remove))
            && !different.contains(&pair(&id_of(root, &c.keep), &id_of(root, &c.remove)))
    };
    let language = vault.settings().language();
    let mut made = 0;
    for candidate in people(root)?.into_iter().filter(|c| new(c)) {
        let reason = people_reason(&candidate.clues, language);
        if submit(&proposals, &reason, vec![candidate])? {
            made += 1;
        }
    }
    for ((a, b), group) in events(root)? {
        let group: Vec<Candidate> = group.into_iter().filter(|c| new(c)).collect();
        for chunk in group.chunks(MAX_GROUPED) {
            let reason = events_reason(chunk.len(), &a, &b, language);
            if submit(&proposals, &reason, chunk.to_vec())? {
                made += 1;
            }
        }
    }
    Ok(made)
}

/// Submits one merge proposal; a pair that no longer applies is skipped.
fn submit(proposals: &Proposals<'_>, reason: &str, candidates: Vec<Candidate>) -> Result<bool> {
    let changes = candidates
        .into_iter()
        .map(|c| Change::Merge {
            keep: c.keep,
            remove: c.remove,
            clues: c.clues,
        })
        .collect();
    match proposals.submit(Author::Ipse, reason, None, changes) {
        Ok(_) => Ok(true),
        // A file changed or went away meanwhile: asked again at the next look.
        Err(Error::Refused(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

fn people_reason(clues: &[String], language: Language) -> String {
    let has = |c: &str| clues.iter().any(|x| x == c);
    let name = has("name");
    let (email, phone) = (has("email"), has("phone"));
    if language == Language::Fr {
        let why = match (email, phone) {
            (true, true) => "Même adresse email et même numéro de téléphone",
            (true, false) => "Même adresse email",
            _ => "Même numéro de téléphone",
        };
        let likely = if email {
            "très probablement"
        } else {
            "probablement"
        };
        let name = if name { ", et même nom" } else { "" };
        format!("{why}{name} : {likely} la même personne.")
    } else {
        let why = match (email, phone) {
            (true, true) => "Same email address and same phone number",
            (true, false) => "Same email address",
            _ => "Same phone number",
        };
        let likely = if email { "very likely" } else { "likely" };
        let name = if name { ", and same name" } else { "" };
        format!("{why}{name}: {likely} the same person.")
    }
}

fn events_reason(count: usize, a: &str, b: &str, language: Language) -> String {
    match (language, count) {
        (Language::Fr, 1) => {
            format!("Un événement semble copié entre « {a} » et « {b} » : même titre, même heure.")
        }
        (Language::Fr, n) => format!(
            "{n} événements semblent copiés entre « {a} » et « {b} » : même titre, même heure."
        ),
        (_, 1) => {
            format!("An event looks copied between \"{a}\" and \"{b}\": same title, same time.")
        }
        (_, n) => {
            format!("{n} events look copied between \"{a}\" and \"{b}\": same title, same time.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_compare_on_their_last_nine_digits() {
        let key = phone_key("+33 6 12 34 56 78");
        assert_eq!(key.as_deref(), Some("612345678"));
        assert_eq!(phone_key("0033612345678"), key);
        assert_eq!(phone_key("06.12.34.56.78"), key);
        assert_eq!(phone_key("112"), None);
    }
}
