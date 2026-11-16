//! `classify` tasks: events no one gave a kind yet (issue #131), for the person's AI to
//! say what each is (a meal, a trip, an appointment…) from its title, times, place and
//! people. The answer is a proposal of `set_key kind`, which the person accepts.
//!
//! Events are asked about once, [`PER_TASK`] at a time, those nearest today first, in
//! tasks of one category each (an AI that may not read a category never blocks the
//! others). At most [`OPEN`] tasks wait at once and [`PER_DAY`] are made a day. What was
//! asked is remembered in `.ipse/tasks/classify.json`, by the event's id.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jiff::ToSpan;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{FOLDER, Item, ItemRef, Task, TaskDetail, TaskKind, TaskStatus, Tasks, save_json};
use crate::Result;
use crate::search::SearchIndex;
use crate::vault::{EventKind, EventTime, Id, Language, MemoryFile, Vault};

/// Events in one task.
pub const PER_TASK: usize = 30;

/// Tasks waiting at once, at most.
pub const OPEN: usize = 2;

/// New tasks in one day, at most.
pub const PER_DAY: usize = 3;

/// The longest notes given for one event: enough to tell what it is.
const MAX_NOTES: usize = 500;

/// What was asked, and when.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Seen {
    format: u32,
    /// The ids of the events asked about.
    #[serde(default)]
    asked: BTreeSet<String>,
    /// The tasks not done yet: counted without reading every task.
    #[serde(default)]
    open: Vec<String>,
    /// How many tasks were made each day.
    #[serde(default)]
    days: BTreeMap<Date, usize>,
    /// The day a pass found no event left to ask about, and how many files the index
    /// had then: until either changes, the pass is not made again.
    #[serde(default)]
    nobody: Option<(Date, usize)>,
}

fn seen_path(vault: &Vault) -> PathBuf {
    vault.root().join(FOLDER).join("classify.json")
}

/// Makes the classify tasks due on `today`; returns how many.
pub(super) fn plan(tasks: &Tasks<'_>, index: &SearchIndex, today: Date) -> Result<usize> {
    let vault = tasks.vault;
    let path = seen_path(vault);
    let mut seen: Seen = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let was_open = seen.open.len();
    seen.open.retain(|id| {
        tasks.get(id).is_ok_and(|t| {
            t.kind == TaskKind::Classify
                && matches!(t.status, TaskStatus::Open | TaskStatus::Proposed { .. })
        })
    });
    let made_today = seen.days.get(&today).copied().unwrap_or(0);
    let room = OPEN
        .saturating_sub(seen.open.len())
        .min(PER_DAY.saturating_sub(made_today));
    let mut made = 0;
    let checked = room == 0 || seen.nobody == Some((today, index.ids()));
    if !checked {
        made = ask(tasks, index, &mut seen, room, today)?;
        if made == 0 {
            seen.nobody = Some((today, index.ids()));
        }
    }
    if made > 0 {
        *seen.days.entry(today).or_default() += made;
        let week = today.saturating_sub(7.days());
        seen.days.retain(|day, _| *day >= week);
    }
    if !checked || seen.open.len() != was_open {
        seen.format = super::FORMAT;
        save_json(&path, &seen)?;
    }
    Ok(made)
}

/// How far an event is from `today`, in seconds, for the order of the questions.
fn distance(start: &EventTime, today: Date) -> i64 {
    let noon = today
        .at(12, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map_or(0, |z| z.timestamp().as_second());
    let at = match start {
        EventTime::Day(day) => day
            .at(12, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_or(0, |z| z.timestamp().as_second()),
        EventTime::At(at) => at.timestamp().as_second(),
    };
    (at - noon).abs()
}

/// Makes up to `room` tasks, of the events not asked about yet, nearest today first.
fn ask(
    tasks: &Tasks<'_>,
    index: &SearchIndex,
    seen: &mut Seen,
    room: usize,
    today: Date,
) -> Result<usize> {
    let mut due: Vec<_> = index
        .events_without_kind()
        .filter(|(id, ..)| !seen.asked.contains(id.as_str()))
        .map(|(id, path, category, start)| (distance(start, today), id, path, category))
        .collect();
    if due.is_empty() {
        return Ok(0);
    }
    due.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    // The nearest events decide which categories are asked about first.
    let mut by_category: Vec<(String, Vec<ItemRef>, Vec<String>)> = Vec::new();
    for (_, id, path, category) in due {
        let at = if let Some(at) = by_category
            .iter()
            .position(|(c, ..)| c == category.as_str())
        {
            at
        } else {
            by_category.push((category.as_str().to_owned(), Vec::new(), Vec::new()));
            by_category.len() - 1
        };
        let (_, items, ids) = &mut by_category[at];
        if items.len() < PER_TASK {
            let file = slashed(path);
            items.push(ItemRef {
                hash: blake3::hash(file.as_bytes()).to_hex().to_string(),
                file,
                category: category.as_str().to_owned(),
            });
            ids.push(id.to_string());
        }
    }
    let mut made = 0;
    for (category, items, ids) in by_category.into_iter().take(room) {
        let count = items.len();
        let task = Task {
            format: super::FORMAT,
            id: Id::generate().to_string(),
            kind: TaskKind::Classify,
            created: tasks.now(),
            description: if count == 1 {
                "Say what kind of event 1 event is.".to_owned()
            } else {
                format!("Say what kind of event each of {count} events is.")
            },
            categories: vec![category],
            items,
            status: TaskStatus::Open,
            comment: None,
            period: None,
        };
        tasks.save(&task)?;
        seen.asked.extend(ids);
        seen.open.push(task.id);
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

/// The task in full, its events read now. An event given a kind since is left out.
pub(super) fn detail(vault: &Vault, task: Task) -> TaskDetail {
    let items: Vec<Item> = task
        .items
        .iter()
        .filter_map(|item| {
            let found = MemoryFile::read(vault.root().join(&item.file)).ok()?;
            let event = found.event()?;
            if event.kind.is_some() {
                return None;
            }
            let mut details = super::summary::details_of(&found)?;
            if let Some(Value::String(notes)) = details.get_mut("notes")
                && notes.chars().count() > MAX_NOTES
            {
                let mut cut: String = notes.chars().take(MAX_NOTES).collect();
                cut.push('…');
                *notes = cut;
            }
            Some(Item {
                file: item.file.clone(),
                name: event.title.clone(),
                file_type: "event".to_owned(),
                category: found.common().category.as_str().to_owned(),
                text: None,
                details: Some(details),
            })
        })
        .collect();
    let file = items
        .first()
        .map_or("events/2026/2026-11-14-diner.md", |i| i.file.as_str());
    let example = json!({
        "task": task.id,
        "reason": match vault.settings().language() {
            Language::Fr => "D'après le titre, le lieu et les personnes de chaque événement.",
            _ => "From each event's title, place and people.",
        },
        "changes": [
            {"kind": "set_key", "file": file, "key": "kind", "from": null, "to": "meal"}
        ]
    });
    TaskDetail {
        expected: expected(),
        example: serde_json::to_string_pretty(&example).unwrap_or_default(),
        items,
        task,
    }
}

fn expected() -> String {
    let kinds = EventKind::ALL
        .iter()
        .map(|k| format!("`{}` ({})", k.as_str(), meaning(*k)))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "For each event, say what kind it is from its title, times, place, people and notes: \
         propose `set_key` with `key` \"kind\", `from` null, and one of: {kinds}. Use `other` when \
         none fits, and leave out an event you cannot tell. Make one proposal for the whole task, \
         with `task` set to this task's id and a short reason the person will read."
    )
}

/// What a kind covers, for the AI.
fn meaning(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Meal => "lunch, dinner, a coffee",
        EventKind::Celebration => "a birthday, a party, a wedding",
        EventKind::Outing => "drinks, a walk, a visit, without a meal at its heart",
        EventKind::Culture => "a concert, a film, a play, a museum",
        EventKind::Sport => "sport played or watched",
        EventKind::Trip => "a stay away from home, holidays",
        EventKind::Travel => "a flight, a train, a long drive",
        EventKind::Work => "a meeting, a conference",
        EventKind::Study => "a class, an exam, a training",
        EventKind::Appointment => "the bank, the hairdresser, a repair, papers",
        EventKind::Health => "a doctor, a dentist, a therapist",
        EventKind::Other => "none of these",
    }
}
