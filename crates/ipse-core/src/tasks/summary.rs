//! `summary` tasks: a past day with events to summarize, then each week, month and
//! year once the summaries inside it exist (`docs/spec/vault.md`, section 7).
//!
//! Days are looked at over the last [`WINDOW_DAYS`] days, so that a new memory gets
//! its recent days and then one a day. What each period held when it was asked is
//! remembered in `.ipse/tasks/summaries.json`: a period is asked again only when what
//! it holds changed (a new event, a summary inside it rewritten). The summary itself
//! goes in the task's items, so that the AI keeps the person's words when updating it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jiff::ToSpan;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    FOLDER, Item, ItemRef, MAX_ITEMS, Task, TaskDetail, TaskKind, TaskStatus, Tasks, save_json,
};
use crate::Result;
use crate::search::SearchIndex;
use crate::summaries::{self, Period, Span};
use crate::vault::body::{Section, section_text};
use crate::vault::{LinkOrText, MemoryFile, Vault};

/// Past days looked at for day summaries.
pub const WINDOW_DAYS: i64 = 30;

/// The longest text given for one item; longer ones are cut.
const MAX_TEXT: usize = 4_000;

/// What each period held when a task last asked for it.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    format: u32,
    /// The first day summaries were asked for: longer periods count from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    since: Option<Date>,
    /// Period key (`day:2026-11-14`) → BLAKE3 of what it held.
    periods: BTreeMap<String, String>,
    /// Period key → its task, while not done: reading only these is enough to know
    /// what waits, however many tasks there are.
    #[serde(default)]
    waiting: BTreeMap<String, String>,
}

fn seen_path(vault: &Vault) -> PathBuf {
    vault.root().join(FOLDER).join("summaries.json")
}

/// Makes the summary tasks due on `today`; returns how many.
pub(super) fn plan(tasks: &Tasks<'_>, index: &SearchIndex, today: Date) -> Result<usize> {
    let vault = tasks.vault;
    let root = vault.root();
    let path = seen_path(vault);
    let mut seen: Seen = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let before = serde_json::to_vec(&seen).unwrap_or_default();
    let window = today.checked_sub(WINDOW_DAYS.days()).unwrap_or(today);
    let since = *seen.since.get_or_insert(window);
    // Periods asked and not answered yet, those asked now included.
    seen.waiting.retain(|_, id| {
        tasks
            .get(id)
            .is_ok_and(|t| matches!(t.status, TaskStatus::Open | TaskStatus::Proposed { .. }))
    });
    let mut pending: BTreeSet<String> = seen.waiting.keys().cloned().collect();

    let mut made = 0;
    let first = since.max(window);
    // One pass over the events, by day: every period below looks them up.
    let by_day = summaries::events_by_day(index, since.min(first), today);
    let holds = |span: Span| by_day.range(span.start..span.end()).next().is_some();
    let mut day = first;
    let mut longer: BTreeSet<Span> = BTreeSet::new();
    while day < today {
        let span = Span::holding(Period::Day, day);
        if let Some(events) = by_day.get(&day) {
            let hash = hash_files(vault, events);
            if seen.periods.get(&span.key()) != Some(&hash) && !pending.contains(&span.key()) {
                let items = day_items(vault, index, span, events);
                let id = save_task(tasks, span, items, events.len())?;
                seen.periods.insert(span.key(), hash);
                seen.waiting.insert(span.key(), id);
                pending.insert(span.key());
                made += 1;
            }
        }
        for period in [Period::Week, Period::Month, Period::Year] {
            longer.insert(Span::holding(period, day));
        }
        day = day.checked_add(1.day()).unwrap_or(today);
    }

    // Longer periods, shortest first: a month can be asked once its days are.
    for span in longer.into_iter().filter(|s| s.end() <= today) {
        let Some(shorter) = span.period.shorter() else {
            continue;
        };
        let mut sources: Vec<String> = Vec::new();
        let mut ready = true;
        for part in span.parts(shorter) {
            if part.end() <= since {
                continue;
            }
            let file = part.file();
            if root.join(&file).is_file() {
                sources.push(file);
                continue;
            }
            // A part with something to summarize and no summary yet: wait for it,
            // unless the AI found nothing worth keeping in it (asked, then closed).
            let asked = seen.periods.contains_key(&part.key());
            if pending.contains(&part.key()) || (holds(part) && !asked && part.start >= first) {
                ready = false;
                break;
            }
        }
        if !ready || sources.is_empty() || pending.contains(&span.key()) {
            continue;
        }
        let hash = hash_files(vault, &sources);
        if seen.periods.get(&span.key()) == Some(&hash) {
            continue;
        }
        let mut items: Vec<ItemRef> = sources.iter().map(|f| item(vault, f)).collect();
        let current = span.file();
        if root.join(&current).is_file() {
            items.push(item(vault, &current));
        }
        items.truncate(MAX_ITEMS);
        let id = save_task(tasks, span, items, sources.len())?;
        seen.periods.insert(span.key(), hash);
        seen.waiting.insert(span.key(), id);
        pending.insert(span.key());
        made += 1;
    }

    seen.format = super::FORMAT;
    // Written only when it changed: an AI's every call plans.
    if serde_json::to_vec(&seen).unwrap_or_default() != before {
        save_json(&path, &seen)?;
    }
    Ok(made)
}

/// The items of a day: its events, the people and places they link to, and its
/// current summary if there is one.
fn day_items(vault: &Vault, index: &SearchIndex, span: Span, events: &[String]) -> Vec<ItemRef> {
    let mut items: Vec<ItemRef> = events.iter().map(|f| item(vault, f)).collect();
    let mut linked: BTreeSet<String> = BTreeSet::new();
    for file in events {
        let Ok(found) = MemoryFile::read(vault.root().join(file)) else {
            continue;
        };
        let Some(event) = found.event() else {
            continue;
        };
        for link in event.participants.iter().chain(event.place.as_ref()) {
            if let LinkOrText::Link(link) = link
                && let Some(entity) = index.linked(&link.target, |_| true)
            {
                linked.insert(slashed(entity.path));
            }
        }
    }
    items.extend(linked.iter().map(|f| item(vault, f)));
    let current = span.file();
    if vault.root().join(&current).is_file() {
        items.push(item(vault, &current));
    }
    items.truncate(MAX_ITEMS);
    items
}

fn slashed(path: &std::path::Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn item(vault: &Vault, file: &str) -> ItemRef {
    let found = MemoryFile::read(vault.root().join(file)).ok();
    ItemRef {
        file: file.to_owned(),
        category: found.as_ref().map_or_else(
            || "summaries".to_owned(),
            |f| f.common().category.as_str().to_owned(),
        ),
        hash: hash_files(vault, std::slice::from_ref(&file.to_owned())),
    }
}

/// BLAKE3 of the files' paths and contents, in order.
fn hash_files(vault: &Vault, files: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(file.as_bytes());
        hasher.update(&[0]);
        if let Ok(bytes) = std::fs::read(vault.root().join(file)) {
            hasher.update(&bytes);
        }
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Saves the task of `span`; returns its id.
fn save_task(tasks: &Tasks<'_>, span: Span, items: Vec<ItemRef>, count: usize) -> Result<String> {
    let mut categories: Vec<String> = items.iter().map(|i| i.category.clone()).collect();
    categories.push("summaries".to_owned());
    categories.sort();
    categories.dedup();
    let what = match span.period {
        Period::Day if count == 1 => "1 event".to_owned(),
        Period::Day => format!("{count} events"),
        Period::Year if count == 1 => "1 month summary".to_owned(),
        Period::Year => format!("{count} month summaries"),
        _ if count == 1 => "1 day summary".to_owned(),
        _ => format!("{count} day summaries"),
    };
    let task = Task {
        format: super::FORMAT,
        id: crate::vault::Id::generate().to_string(),
        kind: TaskKind::Summary,
        created: tasks.now(),
        description: format!(
            "Summarize the {} {} from its {what}.",
            span.period.as_str(),
            span.label()
        ),
        categories,
        items,
        status: TaskStatus::Open,
        comment: None,
        period: Some(span.key()),
    };
    tasks.save(&task)?;
    Ok(task.id)
}

/// The task in full, its items read now.
pub(super) fn detail(vault: &Vault, task: Task) -> TaskDetail {
    let span = task.period.as_deref().and_then(Span::from_key);
    let items: Vec<Item> = task
        .items
        .iter()
        .map(|item| {
            let found = MemoryFile::read(vault.root().join(&item.file)).ok();
            let file_type = found
                .as_ref()
                .map_or("", |f| f.common().file_type.as_str())
                .to_owned();
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
                file_type,
                category: found.as_ref().map_or(item.category.clone(), |f| {
                    f.common().category.as_str().to_owned()
                }),
                text: found.as_ref().and_then(text_of).map(|t| cut(&t)),
                details: found.as_ref().and_then(details_of),
            }
        })
        .collect();
    let language = vault.settings().language();
    let period = span.map_or(Period::Day, |s| s.period);
    let (expected, text) = summaries::guidance(period, language);
    let example = json!({
        "task": task.id,
        "reason": match language {
            crate::vault::Language::Fr => "Résumé écrit à partir des événements de la période.",
            _ => "Summary written from the period's events.",
        },
        "changes": [{
            "kind": "write_summary",
            "period": period.as_str(),
            "start": span.map_or_else(String::new, |s| s.start.to_string()),
            "text": text,
        }]
    });
    TaskDetail {
        expected,
        example: serde_json::to_string_pretty(&example).unwrap_or_default(),
        items,
        task,
    }
}

/// What the AI reads of a person, a place or a summary: its facts, or its text. An
/// event is given as [`details_of`].
fn text_of(file: &MemoryFile) -> Option<String> {
    if file.event().is_some() {
        return None;
    }
    if file.entity().is_some() {
        let facts: Vec<String> = file.sections().facts.into_iter().map(|f| f.text).collect();
        return (!facts.is_empty()).then(|| facts.join("\n"));
    }
    section_text(file.body(), Section::Summary)
}

/// An event's fields, each in its own key.
fn details_of(file: &MemoryFile) -> Option<Value> {
    let event = file.event()?;
    let shown = |link: &LinkOrText| match link {
        LinkOrText::Link(l) => l.display.clone().unwrap_or_else(|| l.target.clone()),
        LinkOrText::Text(t) => t.clone(),
    };
    let mut details = serde_json::Map::new();
    details.insert("title".into(), Value::String(event.title.clone()));
    details.insert("start".into(), Value::String(event.start.to_string()));
    if let Some(end) = &event.end {
        details.insert("end".into(), Value::String(end.to_string()));
    }
    if let Some(place) = &event.place {
        details.insert("place".into(), Value::String(shown(place)));
    }
    if !event.participants.is_empty() {
        let with: Vec<Value> = event
            .participants
            .iter()
            .map(|p| Value::String(shown(p)))
            .collect();
        details.insert("with".into(), Value::Array(with));
    }
    if let Some(status) = &event.status {
        details.insert("status".into(), Value::String(status.as_str().to_owned()));
    }
    if let Some(organizer) = &event.organizer {
        details.insert("invited_by".into(), Value::String(shown(organizer)));
    }
    if let Some(notes) = file.sections().notes {
        details.insert("notes".into(), Value::String(cut(&notes)));
    }
    Some(Value::Object(details))
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
