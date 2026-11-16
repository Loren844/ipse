//! Merging two files the person said are the same (decision 0011): one is kept, the
//! other's names, emails, phones, sources, facts, relations and notes are brought into
//! it without repeating what it already holds, every link to the other is redirected
//! to it, and the other is removed, all in one change of the history.

use std::path::Path;

use super::apply::{DiffItem, FileDiff, What};
use crate::duplicates::phone_key;
use crate::slug::to_file_stem;
use crate::vault::body::{self, Section, replace_section, section_text};
use crate::vault::{EntityFields, EventFields, FileType, Language, MemoryFile, Vault};
use crate::{Error, Result};

/// What merging two files makes.
pub(super) struct Merged {
    /// The kept file, with what the other brought.
    pub(super) keep: MemoryFile,
    /// The other files whose links now point to the kept one.
    pub(super) redirected: Vec<MemoryFile>,
    /// The removed file's path.
    pub(super) removed: std::path::PathBuf,
    /// Links to this stem…
    pub(super) from: String,
    /// …now point to this one.
    pub(super) to: String,
    /// What changes, for the person to read.
    pub(super) diff: FileDiff,
}

/// Plans merging `remove` into `keep` (relative to the vault). `Err(reason)` inside
/// when it no longer applies: a file went away. Without `links`, the links to
/// redirect are not looked for: that reads every file.
pub(super) fn plan(
    vault: &Vault,
    keep_rel: &str,
    remove_rel: &str,
    links: bool,
) -> Result<std::result::Result<Merged, String>> {
    let root = vault.root();
    let (keep_path, remove_path) = (root.join(keep_rel), root.join(remove_rel));
    if !keep_path.is_file() {
        return Ok(Err(format!("{keep_rel} no longer exists.")));
    }
    if !remove_path.is_file() {
        return Ok(Err(format!("{remove_rel} no longer exists.")));
    }
    let mut keep = MemoryFile::read(&keep_path)?;
    let other = MemoryFile::read(&remove_path)?;
    if other.entity().is_some_and(|e| e.owner) {
        return Err(Error::Refused(
            "Your own file is never merged into another one.".to_owned(),
        ));
    }
    let keep_name = name_of(&keep, keep_rel);
    let other_name = name_of(&other, remove_rel);
    let mut items = vec![DiffItem {
        what: What::Merged,
        before: Some(other_name.clone()),
        after: Some(keep_name.clone()),
    }];

    let mut sources = keep.common().sources.clone();
    for source in &other.common().sources {
        if !sources.contains(source) {
            sources.push(source.clone());
        }
    }
    keep.common_mut().sources = sources;

    if let (Some(from), Some(into)) = (other.entity().cloned(), keep.entity_mut()) {
        merge_entity(&from, into, &mut items);
    }
    if let (Some(from), Some(into)) = (other.event().cloned(), keep.event_mut()) {
        merge_event(&from, into, &mut items);
    }

    let keep_stem = stem(keep_rel);
    let body = merge_body(
        keep.body(),
        &other,
        &keep_stem,
        vault.settings().language(),
        &mut items,
    );
    keep.set_body(body);

    let redirected = if links {
        redirect_all(root, &stem(remove_rel), &keep_stem, &[keep_rel, remove_rel])
    } else {
        Vec::new()
    };
    if !redirected.is_empty() {
        items.push(DiffItem {
            what: What::Links,
            before: None,
            after: Some(redirected.len().to_string()),
        });
    }
    let diff = FileDiff {
        file: keep_rel.to_owned(),
        name: keep_name,
        file_type: keep.common().file_type.as_str().to_owned(),
        category: keep.common().category.as_str().to_owned(),
        created: false,
        items,
    };
    Ok(Ok(Merged {
        keep,
        redirected,
        removed: remove_path,
        from: stem(remove_rel),
        to: keep_stem,
        diff,
    }))
}

/// Brings `from`'s names, emails, phones, birthday and address into `into`, without
/// repeating what it holds.
fn merge_entity(from: &EntityFields, into: &mut EntityFields, items: &mut Vec<DiffItem>) {
    let folded = |t: &str| to_file_stem(t).unwrap_or_default();
    let mut names: Vec<String> = into.name.iter().cloned().collect();
    names.extend(into.aliases.iter().cloned());
    for alias in from.name.iter().chain(from.aliases.iter()) {
        if !names.iter().any(|n| folded(n) == folded(alias)) {
            names.push(alias.clone());
            into.aliases.push(alias.clone());
            items.push(added("aliases", alias));
        }
    }
    for email in &from.emails {
        if !into.emails.iter().any(|e| e.eq_ignore_ascii_case(email)) {
            into.emails.push(email.clone());
            items.push(added("emails", email));
        }
    }
    for phone in &from.phones {
        let key = phone_key(phone);
        let known = into
            .phones
            .iter()
            .any(|p| p == phone || (key.is_some() && phone_key(p) == key));
        if !known {
            into.phones.push(phone.clone());
            items.push(added("phones", phone));
        }
    }
    if into.birthday.is_none()
        && let Some(birthday) = from.birthday
    {
        into.birthday = Some(birthday);
        items.push(added("birthday", &birthday.to_string()));
    }
    if into.address.is_none()
        && let Some(address) = from.address.clone()
    {
        items.push(added("address", &address));
        into.address = Some(address);
    }
}

/// Brings `from`'s participants, and its place when `into` has none.
fn merge_event(from: &EventFields, into: &mut EventFields, items: &mut Vec<DiffItem>) {
    for person in &from.participants {
        if !into.participants.contains(person) {
            into.participants.push(person.clone());
            items.push(added("participants", &person.to_string()));
        }
    }
    if into.place.is_none()
        && let Some(place) = from.place.clone()
    {
        items.push(added("place", &place.to_string()));
        into.place = Some(place);
    }
}

/// `body` with `other`'s facts and relations it lacks, and `other`'s notes after its
/// own: nothing the person wrote is lost.
fn merge_body(
    body_text: &str,
    other: &MemoryFile,
    keep_stem: &str,
    language: Language,
    items: &mut Vec<DiffItem>,
) -> String {
    let have = body::parse(body_text);
    let theirs = other.sections();
    let facts: Vec<String> = theirs
        .facts
        .iter()
        .map(|f| f.text.clone())
        .filter(|t| !have.facts.iter().any(|f| &f.text == t))
        .collect();
    let relations: Vec<body::Relation> = theirs
        .relations
        .into_iter()
        .filter(|r| r.link.target != keep_stem)
        .filter(|r| {
            !have
                .relations
                .iter()
                .any(|h| h.word == r.word && h.link.target == r.link.target)
        })
        .collect();
    let mut text = body_text.to_owned();
    if !facts.is_empty() {
        text = body::add_facts(&text, &facts, language);
        items.extend(facts.iter().map(|f| DiffItem {
            what: What::Fact,
            before: None,
            after: Some(f.clone()),
        }));
    }
    if !relations.is_empty() {
        text = body::add_relations(&text, &relations, language);
        items.extend(relations.iter().map(|r| DiffItem {
            what: What::Relation,
            before: None,
            after: Some(format!("{}: {}", r.word, r.link.target)),
        }));
    }
    if let Some(notes) = section_text(other.body(), Section::Notes) {
        let mine = section_text(&text, Section::Notes).unwrap_or_default();
        if !mine.contains(notes.trim()) {
            let joined = if mine.is_empty() {
                notes.clone()
            } else {
                format!("{mine}\n\n{notes}")
            };
            text = replace_section(&text, Section::Notes, &joined, language);
            items.push(DiffItem {
                what: What::Notes,
                before: None,
                after: Some(notes),
            });
        }
    }
    text
}

fn added(key: &str, value: &str) -> DiffItem {
    DiffItem {
        what: What::Key(key.to_owned()),
        before: None,
        after: Some(value.to_owned()),
    }
}

fn name_of(file: &MemoryFile, rel: &str) -> String {
    file.entity()
        .and_then(|e| e.name.clone())
        .or_else(|| file.event().map(|e| e.title.clone()))
        .unwrap_or_else(|| stem(rel))
}

fn stem(rel: &str) -> String {
    rel.rsplit('/')
        .next()
        .unwrap_or(rel)
        .trim_end_matches(".md")
        .to_owned()
}

/// Every memory file but `skip` whose links to `from` now point to `to`.
fn redirect_all(root: &Path, from: &str, to: &str, skip: &[&str]) -> Vec<MemoryFile> {
    let needle = format!("[[{}", from.to_lowercase());
    let mut changed = Vec::new();
    let mut folders: Vec<String> = FileType::ALL
        .iter()
        .map(|t| t.folder().to_owned())
        .collect();
    folders.sort();
    folders.dedup();
    for folder in folders {
        let mut stack = vec![root.join(folder)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "md")
                    || skip.iter().any(|s| path == root.join(s))
                {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if !text.to_lowercase().contains(&needle) {
                    continue;
                }
                if let Some(new) = redirect(&text, from, to)
                    && let Ok(file) = MemoryFile::parse(&path, &new)
                {
                    changed.push(file);
                }
            }
        }
    }
    changed
}

/// `text` with every `[[from]]` and `[[from|…]]` (case aside) pointing to `to`;
/// `None` when it has none.
pub(super) fn redirect(text: &str, from: &str, to: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut any = false;
    while let Some(at) = rest.find("[[") {
        out.push_str(&rest[..at + 2]);
        rest = &rest[at + 2..];
        let end = rest.find(['|', ']']).unwrap_or(rest.len());
        let target = &rest[..end];
        if target.trim().eq_ignore_ascii_case(from) && !target.contains('\n') {
            out.push_str(to);
            rest = &rest[end..];
            any = true;
        }
    }
    out.push_str(rest);
    any.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_redirected_case_aside() {
        let text = "- sœur : [[Marie-Dupont-2]]\n- ami : [[marc]]\n- [[marie-dupont-2|Maman]]\n";
        assert_eq!(
            redirect(text, "marie-dupont-2", "marie-dupont").as_deref(),
            Some("- sœur : [[marie-dupont]]\n- ami : [[marc]]\n- [[marie-dupont|Maman]]\n")
        );
        assert_eq!(redirect("[[marie-dupont-22]]", "marie-dupont-2", "x"), None);
        assert_eq!(redirect("no link", "a", "b"), None);
    }
}
