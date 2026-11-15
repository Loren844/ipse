//! `extract` tasks: notes an import brought (a contact's notes, an event's description)
//! that only a model can turn into facts and relations.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{FOLDER, Item, ItemRef, Task, TaskDetail, TaskKind, TaskStatus, Tasks, save_json};
use crate::Result;
use crate::vault::{FileType, Id, Language, MemoryFile, Vault};

/// Items per task: enough to be worth a call, few enough to read well.
const PER_TASK: usize = 20;

/// The longest text given for one item; longer notes are cut.
const MAX_TEXT: usize = 4_000;

/// What the notes of each file were when a task last covered them.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    format: u32,
    /// File → BLAKE3 of its notes.
    notes: BTreeMap<String, String>,
}

fn seen_path(vault: &Vault) -> PathBuf {
    vault.root().join(FOLDER).join("extract.json")
}

/// Makes tasks for the notes of `files` that are new or changed.
pub(super) fn make(tasks: &Tasks<'_>, files: &[PathBuf]) -> Result<usize> {
    let vault = tasks.vault;
    let path = seen_path(vault);
    let mut seen: Seen = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut items = Vec::new();
    for file in files {
        let Some(rel) = relative(vault, file) else {
            continue;
        };
        let Ok(found) = MemoryFile::read(file) else {
            continue;
        };
        let file_type = found.common().file_type;
        if !(file_type.is_entity() || file_type == FileType::Event) {
            continue;
        }
        let Some(notes) = found.sections().notes else {
            continue;
        };
        let hash = blake3::hash(notes.as_bytes()).to_hex().to_string();
        if seen.notes.get(&rel) == Some(&hash) {
            continue;
        }
        seen.notes.insert(rel.clone(), hash.clone());
        items.push(ItemRef {
            file: rel,
            category: found.common().category.as_str().to_owned(),
            hash,
        });
    }
    if items.is_empty() {
        return Ok(0);
    }
    let mut made = 0;
    for chunk in items.chunks(PER_TASK) {
        let mut categories: Vec<String> = chunk.iter().map(|i| i.category.clone()).collect();
        categories.sort();
        categories.dedup();
        let task = Task {
            format: super::FORMAT,
            id: Id::generate().to_string(),
            kind: TaskKind::Extract,
            created: tasks.now(),
            description: if chunk.len() == 1 {
                "Read the notes of 1 file and propose the facts and relations they hold.".to_owned()
            } else {
                format!(
                    "Read the notes of {} files and propose the facts and relations they hold.",
                    chunk.len()
                )
            },
            categories,
            items: chunk.to_vec(),
            status: TaskStatus::Open,
            comment: None,
        };
        tasks.save(&task)?;
        made += 1;
    }
    seen.format = super::FORMAT;
    save_json(&path, &seen)?;
    Ok(made)
}

/// The task in full, its notes read now.
pub(super) fn detail(vault: &Vault, task: Task) -> TaskDetail {
    let items: Vec<Item> = task
        .items
        .iter()
        .map(|item| {
            let found = MemoryFile::read(vault.root().join(&item.file)).ok();
            let name = found
                .as_ref()
                .and_then(|f| {
                    f.entity()
                        .and_then(|e| e.name.clone())
                        .or_else(|| f.event().map(|e| e.title.clone()))
                })
                .unwrap_or_else(|| stem(&item.file));
            Item {
                file: item.file.clone(),
                name,
                file_type: found
                    .as_ref()
                    .map_or("", |f| f.common().file_type.as_str())
                    .to_owned(),
                category: found.as_ref().map_or(item.category.clone(), |f| {
                    f.common().category.as_str().to_owned()
                }),
                text: found.and_then(|f| f.sections().notes).map(|t| cut(&t)),
            }
        })
        .collect();
    let language = vault.settings().language();
    let example_file = items
        .first()
        .map_or("people/marie-dupont.md", |i| i.file.as_str());
    let (fact, word, target, reason) = match language {
        Language::Fr => (
            "Infirmière à l'hôpital de Lyon.",
            "sœur",
            "lea-dupont",
            "Lu dans les notes de la fiche.",
        ),
        _ => (
            "Works as a nurse at the Lyon hospital.",
            "sister",
            "lea-dupont",
            "Read in the file's notes.",
        ),
    };
    let example = json!({
        "task": task.id,
        "reason": reason,
        "changes": [
            {"kind": "add_fact", "file": example_file, "text": fact},
            {"kind": "add_relation", "file": example_file, "word": word, "target": target},
        ]
    });
    TaskDetail {
        expected: expected(language),
        example: serde_json::to_string_pretty(&example).unwrap_or_default(),
        items,
        task,
    }
}

fn expected(language: Language) -> String {
    let language = match language {
        Language::Fr => "French",
        _ => "English",
    };
    format!(
        "Read each item's notes. For each lasting fact they state about that person, place or event \
         (work, family, tastes, where they live, important dates), propose `add_fact` on that file: one short \
         sentence, in {language}, that stands on its own. For each link to another person or place already in \
         the memory, propose `add_relation` with the relation word and that file's name (find it with `search`). \
         Leave out what is uncertain, passing, or already a fact of the file. Make one proposal for the whole \
         task, with `task` set to this task's id and a short reason the person will read."
    )
}

fn relative(vault: &Vault, path: &std::path::Path) -> Option<String> {
    let rel = path.strip_prefix(vault.root()).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn stem(file: &str) -> String {
    file.rsplit('/')
        .next()
        .unwrap_or(file)
        .trim_end_matches(".md")
        .to_owned()
}

/// `text`, cut at [`MAX_TEXT`] characters.
fn cut(text: &str) -> String {
    if text.chars().count() <= MAX_TEXT {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(MAX_TEXT).collect();
    out.push('…');
    out
}
