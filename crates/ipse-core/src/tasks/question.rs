//! `question` tasks: people known only by a first name or a nickname ("Jo plombier",
//! "Maman"), whom only the person can name. Their AI asks them in conversation, or the
//! person answers in the review inbox; either way the answer completes the file and
//! the old name stays as another name.
//!
//! A person is asked about once, a few at a time, never more than [`PER_DAY`] a day,
//! so that questions never feel like a chore. What was asked is remembered in
//! `.ipse/tasks/questions.json`, by the file's id.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jiff::ToSpan;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{FOLDER, Item, ItemRef, Task, TaskDetail, TaskKind, TaskStatus, Tasks, save_json};
use crate::search::SearchIndex;
use crate::slug::to_file_stem;
use crate::vault::body::{self, Relation};
use crate::vault::{DateTime, FileType, Id, Language, MemoryFile, Source, Vault, WikiLink};
use crate::{Error, Result};

/// Questions waiting at once, at most.
pub const OPEN: usize = 3;

/// New questions in one day, at most.
pub const PER_DAY: usize = 3;

/// Words that name a relative rather than a person, in every supported language.
const FAMILY: [&str; 34] = [
    "maman",
    "papa",
    "mamie",
    "mamy",
    "papi",
    "papy",
    "meme",
    "pepe",
    "tonton",
    "tata",
    "tatie",
    "grand-mere",
    "grand-pere",
    "belle-mere",
    "beau-pere",
    "cheri",
    "cherie",
    "mon-amour",
    "ma-femme",
    "mon-mari",
    "frerot",
    "soeurette",
    "mom",
    "mum",
    "mommy",
    "dad",
    "daddy",
    "grandma",
    "grandpa",
    "granny",
    "auntie",
    "uncle",
    "honey",
    "babe",
];

/// The first words of the family words made of two.
const FAMILY_FIRST: [&str; 5] = ["mon", "ma", "grand", "belle", "beau"];

/// What was asked, and when.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    format: u32,
    /// The ids of the people asked about.
    #[serde(default)]
    asked: BTreeSet<String>,
    /// The question tasks not answered yet: counted without reading every task.
    #[serde(default)]
    open: Vec<String>,
    /// The day a pass over everyone found nobody left to ask, and how many files the
    /// index had then: until either changes, the pass is not made again. Questions are
    /// never urgent; a rename is seen the next day.
    #[serde(default)]
    nobody: Option<(Date, usize)>,
    /// How many questions were made each day.
    #[serde(default)]
    days: BTreeMap<Date, usize>,
}

fn seen_path(vault: &Vault) -> PathBuf {
    vault.root().join(FOLDER).join("questions.json")
}

/// True when `name` says who someone is only by a first name, a nickname or a family
/// word: fewer than two words that start with a capital letter, or a family word.
#[must_use]
pub fn is_partial(name: &str) -> bool {
    let mut words = 0;
    let mut capitalized = 0;
    let mut first = "";
    for word in name.split_whitespace() {
        if words == 0 {
            first = word;
        }
        words += 1;
        if word.chars().next().is_some_and(char::is_uppercase) {
            capitalized += 1;
        }
    }
    if capitalized < 2 {
        return true;
    }
    // A family word of two words ("Mon Amour", "Grand Mère"); a name of one word was
    // caught above. Folded only then: most names are not.
    words == 2
        && FAMILY_FIRST.iter().any(|w| w.eq_ignore_ascii_case(first))
        && FAMILY.contains(&to_file_stem(name).unwrap_or_default().as_str())
}

fn read_seen(vault: &Vault) -> Seen {
    std::fs::read(seen_path(vault))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The ids of the question tasks waiting for an answer, oldest first. Only those
/// questions are read, not every task.
///
/// # Errors
///
/// None today: a task that cannot be read is no longer waiting.
pub fn open(vault: &Vault) -> Result<Vec<Task>> {
    let tasks = Tasks::new(vault);
    Ok(read_seen(vault)
        .open
        .iter()
        .filter_map(|id| tasks.get(id).ok())
        .filter(|t| t.kind == TaskKind::Question && t.status == TaskStatus::Open)
        .collect())
}

/// Makes the question tasks due on `today`; returns how many.
pub(super) fn plan(tasks: &Tasks<'_>, index: &SearchIndex, today: Date) -> Result<usize> {
    let vault = tasks.vault;
    let mut seen = read_seen(vault);
    let was_open = seen.open.len();
    seen.open.retain(|id| {
        tasks
            .get(id)
            .is_ok_and(|t| t.kind == TaskKind::Question && t.status == TaskStatus::Open)
    });
    let today_count = seen.days.get(&today).copied().unwrap_or(0);
    let room = OPEN
        .saturating_sub(seen.open.len())
        .min(PER_DAY.saturating_sub(today_count));
    let mut made = 0;
    let checked = room == 0 || seen.nobody == Some((today, index.ids()));
    if !checked {
        made = ask(tasks, index, &mut seen, room)?;
        if made == 0 {
            seen.nobody = Some((today, index.ids()));
        }
    }
    if made > 0 {
        *seen.days.entry(today).or_default() += made;
        // Only recent days matter for the daily limit.
        let week = today.saturating_sub(7.days());
        seen.days.retain(|day, _| *day >= week);
    }
    if !checked || seen.open.len() != was_open {
        seen.format = super::FORMAT;
        save_json(&seen_path(vault), &seen)?;
    }
    Ok(made)
}

/// Makes up to `room` questions, about the people not asked yet, in the order they
/// were added to the memory.
fn ask(tasks: &Tasks<'_>, index: &SearchIndex, seen: &mut Seen, room: usize) -> Result<usize> {
    let vault = tasks.vault;
    let owner = vault.owner_file().strip_prefix(vault.root()).ok();
    // One pass, without sorting every name: few people are known by a first name.
    let mut due: Vec<_> = index
        .each_entity(FileType::Person)
        .filter(|p| {
            is_partial(p.name)
                && Some(p.path) != owner
                && !p.aliases.iter().any(|a| !is_partial(a))
                && !seen.asked.contains(p.id.as_str())
        })
        .collect();
    // Ids are ordered by time.
    due.sort_unstable_by_key(|p| p.id.as_str());
    let mut made = 0;
    for person in due.into_iter().take(room) {
        let file = slashed(person.path);
        let id = person.id.to_string();
        let task = Task {
            format: super::FORMAT,
            id: Id::generate().to_string(),
            kind: TaskKind::Question,
            created: tasks.now(),
            description: "Ask the person who someone known only by a first name or a nickname is."
                .to_owned(),
            categories: vec![person.category.as_str().to_owned()],
            items: vec![ItemRef {
                file,
                category: person.category.as_str().to_owned(),
                hash: blake3::hash(person.name.as_bytes()).to_hex().to_string(),
            }],
            status: TaskStatus::Open,
            comment: None,
            period: None,
        };
        seen.open.push(task.id.clone());
        tasks.save(&task)?;
        seen.asked.insert(id);
        made += 1;
    }
    Ok(made)
}

fn slashed(path: &std::path::Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// What is known of someone, for the person to recognize them: their name, phones,
/// emails, and the events they attended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    /// The file, relative to the vault, with `/`.
    pub file: String,
    /// Its id.
    pub id: String,
    /// The name as saved.
    pub name: String,
    /// Phone numbers.
    pub phones: Vec<String>,
    /// Email addresses.
    pub emails: Vec<String>,
    /// The events they attended, most recent first: title and day.
    pub events: Vec<(String, String)>,
}

/// Characters in the word saying how the person knows someone, at most.
pub const MAX_WORD: usize = 60;

/// Events listed in what is known, at most.
const KNOWN_EVENTS: usize = 5;

/// What is known of the person a question task is about; `None` when the file is
/// gone.
#[must_use]
pub fn known(vault: &Vault, index: &SearchIndex, task: &Task) -> Option<Known> {
    let item = task.items.first()?;
    let file = MemoryFile::read(vault.root().join(&item.file)).ok()?;
    let entity = file.entity()?;
    let stem = item
        .file
        .rsplit('/')
        .next()?
        .trim_end_matches(".md")
        .to_owned();
    let mut events: Vec<(String, String)> = index
        .events(|_| true, |e| e.participants.contains(&stem))
        .into_iter()
        .rev()
        .take(KNOWN_EVENTS)
        .filter_map(|e| {
            let event = MemoryFile::read(vault.root().join(e.path)).ok()?;
            Some((event.event()?.title.clone(), e.start.date().to_string()))
        })
        .collect();
    events.truncate(KNOWN_EVENTS);
    Some(Known {
        file: item.file.clone(),
        id: file.common().id.to_string(),
        name: entity.name.clone().unwrap_or_default(),
        phones: entity.phones.clone(),
        emails: entity.emails.clone(),
        events,
    })
}

/// The task in full, for the person's AI.
pub(super) fn detail(vault: &Vault, task: Task) -> TaskDetail {
    let index = SearchIndex::load(vault.root()).ok();
    let known = index.as_ref().and_then(|index| known(vault, index, &task));
    let items: Vec<Item> = known
        .iter()
        .map(|k| {
            let events: Vec<_> = k
                .events
                .iter()
                .map(|(title, day)| json!({ "title": title, "day": day }))
                .collect();
            Item {
                file: k.file.clone(),
                name: k.name.clone(),
                file_type: "person".to_owned(),
                category: task.items[0].category.clone(),
                text: None,
                details: Some(json!({
                    "name": k.name,
                    "phones": k.phones,
                    "emails": k.emails,
                    "events": events,
                })),
            }
        })
        .collect();
    let language = vault.settings().language();
    let owner = vault
        .owner_file()
        .strip_prefix(vault.root())
        .map(slashed)
        .unwrap_or_default();
    let file = items.first().map_or("people/jo.md", |i| i.file.as_str());
    let (name, word, reason) = match language {
        Language::Fr => ("Jo Lambert", "plombier", "Vous m'avez dit qui c'est."),
        _ => ("Jo Lambert", "plumber", "You told me who this is."),
    };
    let stem = file
        .rsplit('/')
        .next()
        .unwrap_or(file)
        .trim_end_matches(".md");
    let example = json!({
        "task": task.id,
        "reason": reason,
        "changes": [
            {"kind": "set_key", "file": file, "key": "name", "from": items.first().map(|i| i.name.clone()), "to": name},
            {"kind": "add_relation", "file": owner, "word": word, "target": stem},
        ]
    });
    let language_name = match language {
        Language::Fr => "French",
        _ => "English",
    };
    let expected = format!(
        "The person saved someone under a first name or a nickname only. When it fits the \
         conversation, ask them, in {language_name} and in one short question, who this is, \
         using what the item says (phone, email, events) to help them remember. If they \
         answer, propose `set_key` `name` with the full name (the old name stays as another \
         name), and, if they say how they know them, `add_relation` on their own file ({owner}) \
         with that word and this person's file name, when the example shows one: otherwise \
         you may not change their file, so say it in the reason for them to add. If they do \
         not know or do not want to say, send this task with no changes: it will not be \
         asked again."
    );
    TaskDetail {
        expected,
        example: serde_json::to_string_pretty(&example).unwrap_or_default(),
        items,
        task,
    }
}

/// The person answers a question in the interface: the file gets `name`, the old one
/// stays as another name, and `word`, when given, links it from the person's own file
/// (`plombier : [[jo]]`). One change of the history by the person; the task is done.
///
/// # Errors
///
/// [`Error::Refused`] when the task is not an open question or `name` is empty;
/// errors writing the files or the history.
pub fn answer(vault: &Vault, task_id: &str, name: &str, word: Option<&str>) -> Result<()> {
    let tasks = Tasks::new(vault);
    let task = tasks.get(task_id)?;
    if task.kind != TaskKind::Question || task.status != TaskStatus::Open {
        return Err(Error::Refused(format!(
            "Task {} is not a question waiting for an answer.",
            task.id
        )));
    }
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(Error::Refused(
            "Give a name of 1 to 200 characters.".to_owned(),
        ));
    }
    let word = word.map(str::trim).filter(|w| !w.is_empty());
    if word.is_some_and(|w| w.chars().count() > MAX_WORD) {
        return Err(Error::Refused(format!(
            "Say how you know them in at most {MAX_WORD} characters."
        )));
    }
    let Some(item) = task.items.first() else {
        return Err(Error::Refused("This question has no person.".to_owned()));
    };
    let history = vault.history()?;
    let path = vault.root().join(&item.file);
    let mut file = MemoryFile::read(&path)?;
    let now = DateTime::now(vault.settings().time_zone());
    let mut change = history.change(Source::Person);
    if let Some(entity) = file.entity_mut()
        && let Some(old) = entity.name.replace(name.to_owned())
        && old != name
        && !entity.aliases.contains(&old)
    {
        entity.aliases.push(old);
    }
    file.common_mut().updated = now;
    file.write()?;
    change.touched(&path);
    if let Some(word) = word {
        let stem = item
            .file
            .rsplit('/')
            .next()
            .unwrap_or(&item.file)
            .trim_end_matches(".md");
        let owner_path = vault.owner_file().to_owned();
        let mut owner = MemoryFile::read(&owner_path)?;
        let relation = Relation {
            word: word.to_owned(),
            link: WikiLink {
                target: stem.to_owned(),
                display: None,
            },
        };
        let known = owner
            .sections()
            .relations
            .iter()
            .any(|r| r.word == relation.word && r.link.target == relation.link.target);
        if !known {
            let body = body::add_relations(owner.body(), &[relation], vault.settings().language());
            owner.set_body(body);
            owner.common_mut().updated = now;
            owner.write()?;
            change.touched(&owner_path);
        }
    }
    change.commit("Answer a question")?;
    tasks.dismiss(&task.id)
}

/// The person skips a question: it is not asked again.
///
/// # Errors
///
/// [`Error::Refused`] when the task is not an open question.
pub fn skip(vault: &Vault, task_id: &str) -> Result<()> {
    let tasks = Tasks::new(vault);
    let task = tasks.get(task_id)?;
    if task.kind != TaskKind::Question {
        return Err(Error::Refused(format!(
            "Task {} is not a question.",
            task.id
        )));
    }
    tasks.dismiss(&task.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_names() {
        for name in [
            "Jo plombier",
            "Léa",
            "Maman",
            "mamie",
            "Grand-mère",
            "Grand Mère",
            "Mon Amour",
            "Dad",
            "jean pierre",
        ] {
            assert!(is_partial(name), "{name}");
        }
        for name in ["Marie Dupont", "Jo Lambert", "Dr Martin", "Élodie Martin"] {
            assert!(!is_partial(name), "{name}");
        }
    }
}
