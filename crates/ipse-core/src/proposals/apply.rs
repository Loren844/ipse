//! Applying a proposal's changes to the files as they are now: what they would change
//! (the preview the person reviews), and writing them.

use std::collections::HashMap;

use serde_json::Value;

use super::Proposal;
use super::change::{Change, fact_id, memory_path, relative};
use super::key;
use super::merge;
use crate::search::SearchIndex;
use crate::summaries::{self, Span};
use crate::vault::body::{self, Body, Fact, Relation};
use crate::vault::{
    Category, Change as HistoryChange, Common, DateTime, EntityFields, EventFields, EventTime,
    FileType, Language, MemoryFile, Vault, WikiLink, sync_files,
};
use crate::{Error, Result};

/// What a proposal would change, file by file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The files it touches, in the order of its changes.
    pub files: Vec<FileDiff>,
    /// Why it no longer applies, naming the file but not its content; `None` when it
    /// does.
    pub stale: Option<String>,
}

/// The changes to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// The file, relative to the vault, with `/`.
    pub file: String,
    /// Its name or title, after the changes.
    pub name: String,
    /// Its type: `person`, `event`…
    pub file_type: String,
    /// Its category.
    pub category: String,
    /// True when the proposal creates it.
    pub created: bool,
    /// What changes in it. Fact identifiers are never part of it.
    pub items: Vec<DiffItem>,
}

/// One thing that changes: a key, a fact, a relation, the notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffItem {
    /// What it is.
    pub what: What,
    /// Its text before; `None` when added.
    pub before: Option<String>,
    /// Its text after; `None` when removed.
    pub after: Option<String>,
}

/// The part of a file a [`DiffItem`] is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum What {
    /// A front-matter key, by name: `phones`.
    Key(String),
    /// A fact.
    Fact,
    /// A relation, as `word: target`.
    Relation,
    /// The notes.
    Notes,
    /// A summary's text.
    Summary,
    /// Another file merged into this one: its name before, this one's after.
    Merged,
    /// Links in other files redirected to this one: how many files, after.
    Links,
}

/// A file as the proposal leaves it.
pub(super) struct Pending {
    file: MemoryFile,
    created: bool,
    changed: bool,
    /// Removed by a merge, not written.
    removed: bool,
}

impl Pending {
    /// An existing file, changed.
    fn changed(file: MemoryFile) -> Self {
        Self {
            file,
            created: false,
            changed: true,
            removed: false,
        }
    }
}

impl Pending {
    /// `file`, to remove.
    pub(super) fn removal(file: MemoryFile) -> Self {
        Self {
            removed: true,
            ..Self::changed(file)
        }
    }
}

/// The diff of a file written with another and not listed on its own.
fn unlisted() -> FileDiff {
    FileDiff {
        file: String::new(),
        name: String::new(),
        file_type: String::new(),
        category: String::new(),
        created: false,
        items: Vec::new(),
    }
}

/// The changes applied in memory, not yet written.
pub(super) struct Plan {
    pub(super) preview: Preview,
    pub(super) files: Vec<Pending>,
}

/// Applies `proposal`'s changes, in order, to the files as they are on disk now,
/// without writing anything. A change that no longer fits makes the plan stale; the
/// others are still shown.
pub(super) fn plan(vault: &Vault, proposal: &Proposal) -> Result<Plan> {
    plan_with(vault, proposal, true)
}

/// As [`plan`]; without `links`, a merge does not look for the links to redirect,
/// which reads every file: to check a proposal when it is submitted.
pub(super) fn plan_with(vault: &Vault, proposal: &Proposal, links: bool) -> Result<Plan> {
    let source = format!("proposal:{}", proposal.id);
    plan_changes(vault, &proposal.changes, Some(&source), links)
}

/// As [`plan_with`], for `changes` whose new files get `source` as their only source;
/// none for the person's own files.
#[allow(clippy::too_many_lines)] // one branch per kind of change
pub(super) fn plan_changes(
    vault: &Vault,
    changes: &[Change],
    source: Option<&str>,
    links: bool,
) -> Result<Plan> {
    let root = vault.root();
    let language = vault.settings().language();
    let mut files: Vec<Pending> = Vec::new();
    let mut diffs: Vec<FileDiff> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut stale: Option<String> = None;
    // Read once, for a proposal that writes summaries.
    let mut search: Option<SearchIndex> = None;

    for change in changes {
        let Some(rel) = change.file().map(str::to_owned) else {
            return Err(Error::Refused(
                "A new file of this proposal has no path.".to_owned(),
            ));
        };
        if let Change::Merge { keep, remove, .. } = change {
            match merge::plan(vault, keep, remove, links)? {
                Err(reason) => {
                    stale.get_or_insert(reason);
                }
                Ok(merged) => {
                    index.insert(rel.clone(), files.len());
                    diffs.push(merged.diff);
                    files.push(Pending::changed(merged.keep));
                    // Files whose links are redirected, and the one removed, are
                    // written with the kept one but not listed: the kept one's diff
                    // says so.
                    for file in merged.redirected {
                        let at = relative(root, file.path());
                        // Already changed by this proposal: redirect in that version.
                        if let Some(&before) = index.get(&at) {
                            let pending = &mut files[before];
                            if pending.removed {
                                continue;
                            }
                            let text = pending.file.render()?;
                            if let Some(new) = merge::redirect(&text, &merged.from, &merged.to) {
                                pending.file = MemoryFile::parse(pending.file.path(), &new)?;
                                pending.changed = true;
                            }
                            continue;
                        }
                        index.insert(at, files.len());
                        diffs.push(unlisted());
                        files.push(Pending::changed(file));
                    }
                    let removed = MemoryFile::read(&merged.removed)?;
                    index.insert(relative(root, &merged.removed), files.len());
                    diffs.push(unlisted());
                    files.push(Pending {
                        removed: true,
                        ..Pending::changed(removed)
                    });
                }
            }
            continue;
        }
        if matches!(change, Change::WriteSummary { .. }) {
            match summary_step(vault, source, &rel, change, &mut search)? {
                SummaryStep::Stale(reason) => {
                    stale.get_or_insert(reason);
                }
                SummaryStep::Same => {}
                SummaryStep::Write(planned) => {
                    let (diff, pending) = *planned;
                    index.insert(rel.clone(), files.len());
                    diffs.push(diff);
                    files.push(pending);
                }
            }
            continue;
        }
        let path = memory_path(root, &rel)?;
        let step = if matches!(change, Change::Create { .. }) {
            if path.exists() || index.contains_key(&rel) {
                Step::Stale(format!("{rel} was created meanwhile."))
            } else {
                let (file, items) = create(vault, source, &path, change)?;
                index.insert(rel.clone(), files.len());
                diffs.push(file_diff(&rel, &file, true, items));
                files.push(Pending {
                    file,
                    created: true,
                    changed: true,
                    removed: false,
                });
                Step::Done(None)
            }
        } else if let Some(&at) = index.get(&rel) {
            edit(&mut files[at].file, change, &rel, language)?.at(at)
        } else if !path.is_file() {
            Step::Stale(format!("{rel} no longer exists."))
        } else if let Ok(file) = MemoryFile::read(&path) {
            let at = files.len();
            index.insert(rel.clone(), at);
            diffs.push(file_diff(&rel, &file, false, Vec::new()));
            files.push(Pending {
                file,
                created: false,
                changed: false,
                removed: false,
            });
            edit(&mut files[at].file, change, &rel, language)?.at(at)
        } else {
            Step::Stale(format!(
                "{rel} cannot be read; fix it first (`ipse check` says how)."
            ))
        };
        match step {
            Step::Stale(reason) => {
                stale.get_or_insert(reason);
            }
            Step::Changed(at, item) => {
                let pending = &mut files[at];
                pending.changed = true;
                diffs[at].name = display_name(&rel, &pending.file);
                diffs[at].items.push(item);
            }
            Step::Done(_) => {}
        }
    }

    // A file the proposal leaves as it is changes nothing.
    let mut kept = Vec::with_capacity(files.len());
    let mut shown_files = Vec::with_capacity(diffs.len());
    for (pending, diff) in files.into_iter().zip(diffs) {
        if pending.changed {
            kept.push(pending);
            if !diff.file.is_empty() {
                shown_files.push(diff);
            }
        }
    }
    Ok(Plan {
        preview: Preview {
            files: shown_files,
            stale,
        },
        files: kept,
    })
}

/// What one change did.
enum Step {
    /// Applied; what it changed, if anything.
    Done(Option<DiffItem>),
    /// Applied to the file at this position of the plan.
    Changed(usize, DiffItem),
    /// Not applied: why.
    Stale(String),
}

impl Step {
    /// This step, for the file at `at`.
    fn at(self, at: usize) -> Self {
        match self {
            Self::Done(Some(item)) => Self::Changed(at, item),
            other => other,
        }
    }
}

/// What a `write_summary` change does to the files as they are now.
enum SummaryStep {
    /// The summary changed since the proposal was made.
    Stale(String),
    /// It already says so.
    Same,
    /// What changes in the file, and the file to write.
    Write(Box<(FileDiff, Pending)>),
}

/// Plans one `write_summary` change; `search` is read once, for the first.
fn summary_step(
    vault: &Vault,
    source: Option<&str>,
    rel: &str,
    change: &Change,
    search: &mut Option<SearchIndex>,
) -> Result<SummaryStep> {
    let Change::WriteSummary {
        period,
        start,
        from,
        text,
        ..
    } = change
    else {
        return Ok(SummaryStep::Same);
    };
    let root = vault.root();
    let span = Span::parse(period, start)?;
    let path = root.join(rel);
    let now = summaries::text_of(&path);
    if now != *from {
        return Ok(SummaryStep::Stale(format!(
            "The summary {} changed since the proposal was made.",
            span.label()
        )));
    }
    let text = text.trim();
    if now.as_deref() == Some(text) {
        return Ok(SummaryStep::Same);
    }
    let found = match search {
        Some(found) => found,
        None => search.insert(SearchIndex::load(root)?),
    };
    let events = summaries::events_in(found, span);
    let file = summaries::write_file(
        vault,
        &path,
        span,
        text,
        &events,
        source.unwrap_or("person"),
    )?;
    let created = !path.is_file();
    let item = DiffItem {
        what: What::Summary,
        before: now,
        after: Some(text.to_owned()),
    };
    let diff = file_diff(rel, &file, created, vec![item]);
    Ok(SummaryStep::Write(Box::new((
        diff,
        Pending {
            file,
            created,
            changed: true,
            removed: false,
        },
    ))))
}

/// The file a `create` change makes, and what it holds.
fn create(
    vault: &Vault,
    source: Option<&str>,
    path: &std::path::Path,
    change: &Change,
) -> Result<(MemoryFile, Vec<DiffItem>)> {
    let Change::Create {
        file_type,
        category,
        keys,
        facts,
        relations,
        notes,
        ..
    } = change
    else {
        unreachable!("only called for create");
    };
    let file = new_file(
        vault,
        source,
        path,
        file_type,
        category.as_deref(),
        keys,
        facts,
        relations,
        notes.as_deref(),
    )?;
    let mut items: Vec<DiffItem> = Vec::new();
    for key in keys.keys() {
        if let Some(value) = key::get(&file, key).and_then(|v| shown(&v)) {
            items.push(DiffItem {
                what: What::Key(key.clone()),
                before: None,
                after: Some(value),
            });
        }
    }
    let sections = file.sections();
    items.extend(
        sections
            .facts
            .iter()
            .map(|f| added(What::Fact, f.text.clone())),
    );
    items.extend(
        sections
            .relations
            .iter()
            .map(|r| added(What::Relation, relation_text(r))),
    );
    if let Some(notes) = sections.notes {
        items.push(added(What::Notes, notes));
    }
    Ok((file, items))
}

/// Applies one change other than `create` to `file`.
#[allow(clippy::too_many_lines)] // one arm per kind of change
fn edit(file: &mut MemoryFile, change: &Change, rel: &str, language: Language) -> Result<Step> {
    let removed = || {
        Step::Stale(format!(
            "A fact of {rel} was removed since the proposal was made."
        ))
    };
    let changed = || {
        Step::Stale(format!(
            "A fact of {rel} changed since the proposal was made."
        ))
    };
    let item = match change {
        Change::Create { .. } | Change::WriteSummary { .. } | Change::Merge { .. } => {
            unreachable!("not an edit")
        }
        Change::SetKey { key, from, to, .. } => {
            let Some(now) = key::get(file, key) else {
                return Err(Error::Refused(format!("`{key}` cannot be set on {rel}.")));
            };
            if key::normalized(key, &now) != key::normalized(key, from) {
                return Ok(Step::Stale(format!(
                    "`{key}` of {rel} changed since the proposal was made."
                )));
            }
            key::set(file, key, to).map_err(|e| Error::Refused(format!("{e}.")))?;
            let after = key::get(file, key).unwrap_or(Value::Null);
            (key::normalized(key, &now) != key::normalized(key, &after)).then(|| DiffItem {
                what: What::Key(key.clone()),
                before: shown(&now),
                after: shown(&after),
            })
        }
        Change::AddFact { text, .. } => {
            let text = one_line(text);
            if file.sections().facts.iter().any(|f| f.text == text) {
                None
            } else {
                let new = body::add_facts(file.body(), std::slice::from_ref(&text), language);
                file.set_body(new);
                Some(added(What::Fact, text))
            }
        }
        Change::EditFact { fact, from, to, .. } => {
            let id = fact_id(fact)?;
            let Some(text) = current_fact(file, &id) else {
                return Ok(removed());
            };
            if text != one_line(from) {
                return Ok(changed());
            }
            let to = one_line(to);
            if text == to {
                None
            } else {
                if let Some(new) = body::edit_fact(file.body(), &id, &to) {
                    file.set_body(new);
                }
                Some(DiffItem {
                    what: What::Fact,
                    before: Some(text),
                    after: Some(to),
                })
            }
        }
        Change::RemoveFact {
            fact, text: was, ..
        } => {
            let id = fact_id(fact)?;
            let Some(text) = current_fact(file, &id) else {
                return Ok(removed());
            };
            if text != one_line(was) {
                return Ok(changed());
            }
            if let Some(new) = body::remove_fact(file.body(), &id) {
                file.set_body(new);
            }
            Some(DiffItem {
                what: What::Fact,
                before: Some(text),
                after: None,
            })
        }
        Change::AddRelation {
            word,
            target,
            display,
            ..
        } => {
            let relation = Relation {
                word: one_line(word),
                link: WikiLink {
                    target: target.clone(),
                    display: display.clone(),
                },
            };
            let exists = file
                .sections()
                .relations
                .iter()
                .any(|r| r.word == relation.word && r.link.target == relation.link.target);
            if exists {
                None
            } else {
                let new =
                    body::add_relations(file.body(), std::slice::from_ref(&relation), language);
                file.set_body(new);
                Some(added(What::Relation, relation_text(&relation)))
            }
        }
        Change::SetNotes { from, to, .. } => {
            let now = file.sections().notes;
            let clean = |t: &Option<String>| {
                t.as_deref()
                    .map(|t| t.trim_matches('\n').trim_end())
                    .filter(|t| !t.trim().is_empty())
                    .map(str::to_owned)
            };
            if clean(&now) != clean(from) {
                return Ok(Step::Stale(format!(
                    "The notes of {rel} changed since the change was made."
                )));
            }
            let to = clean(to);
            if to == clean(&now) {
                None
            } else {
                let new = body::replace_section(
                    file.body(),
                    body::Section::Notes,
                    to.as_deref().unwrap_or_default(),
                    language,
                );
                file.set_body(new);
                Some(DiffItem {
                    what: What::Notes,
                    before: clean(&now),
                    after: to,
                })
            }
        }
        Change::RemoveRelation { word, target, .. } => {
            let Some(new) = body::remove_relation(file.body(), word, target) else {
                return Ok(Step::Stale(format!(
                    "A relation of {rel} was removed since the proposal was made."
                )));
            };
            file.set_body(new);
            Some(DiffItem {
                what: What::Relation,
                before: Some(format!("{word}: {target}")),
                after: None,
            })
        }
    };
    Ok(Step::Done(item))
}

/// Writes the files of a plan, durably, and marks them as part of `change`.
pub(super) fn write(
    vault: &Vault,
    files: Vec<Pending>,
    change: &mut HistoryChange<'_>,
) -> Result<()> {
    let now = DateTime::now(vault.settings().time_zone());
    let mut written = Vec::with_capacity(files.len());
    for mut pending in files {
        if pending.removed {
            let path = pending.file.path();
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(Error::Io {
                        action: "remove",
                        path: path.to_owned(),
                        source,
                    });
                }
            }
            change.touched(path);
            continue;
        }
        if !pending.created {
            pending.file.common_mut().updated = now;
        }
        if pending.file.write_unsynced()? {
            written.push(pending.file.path().to_owned());
        }
        change.touched(pending.file.path());
    }
    sync_files(&written)
}

#[allow(clippy::too_many_arguments)]
fn new_file(
    vault: &Vault,
    source: Option<&str>,
    path: &std::path::Path,
    file_type: &str,
    category: Option<&str>,
    keys: &std::collections::BTreeMap<String, Value>,
    facts: &[String],
    relations: &[super::NewRelation],
    notes: Option<&str>,
) -> Result<MemoryFile> {
    let refused = |text: String| Error::Refused(text);
    let kind = FileType::parse(file_type)
        .ok_or_else(|| refused(format!("`{file_type}` is not a type of file.")))?;
    let mut common = Common::new(kind, vault.settings().time_zone())
        .ok_or_else(|| refused(format!("A proposal cannot create a {file_type}.")))?;
    if let Some(category) = category {
        common.category = Category::new(category)
            .ok_or_else(|| refused("`category` must be a category name.".to_owned()))?;
    }
    // A file without sources is the person's.
    common.sources = source.map(str::to_owned).into_iter().collect();
    let body = body::render(
        &Body {
            facts: facts
                .iter()
                .map(|f| Fact {
                    text: one_line(f),
                    id: None,
                })
                .collect(),
            relations: relations
                .iter()
                .map(|r| Relation {
                    word: one_line(&r.word),
                    link: WikiLink {
                        target: r.target.clone(),
                        display: r.display.clone(),
                    },
                })
                .collect(),
            notes: notes
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned),
        },
        vault.settings().language(),
    );
    let mut file = if kind == FileType::Event {
        let title = keys
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let start = keys
            .get("start")
            .and_then(Value::as_str)
            .and_then(EventTime::parse)
            .ok_or_else(|| refused("A new event needs `title` and `start`.".to_owned()))?;
        MemoryFile::new_event(path, common, EventFields::new(title, start), body)
    } else {
        MemoryFile::new(path, common, Some(EntityFields::default()), body)
    };
    for (key, value) in keys {
        key::set(&mut file, key, value).map_err(|e| refused(format!("{e}.")))?;
    }
    Ok(file)
}

fn file_diff(rel: &str, file: &MemoryFile, created: bool, items: Vec<DiffItem>) -> FileDiff {
    FileDiff {
        file: rel.to_owned(),
        name: display_name(rel, file),
        file_type: file.common().file_type.as_str().to_owned(),
        category: file.common().category.as_str().to_owned(),
        created,
        items,
    }
}

fn display_name(rel: &str, file: &MemoryFile) -> String {
    file.entity()
        .and_then(|e| e.name.clone())
        .or_else(|| file.event().map(|e| e.title.clone()))
        .unwrap_or_else(|| {
            rel.rsplit('/')
                .next()
                .unwrap_or(rel)
                .trim_end_matches(".md")
                .to_owned()
        })
}

fn current_fact(file: &MemoryFile, id: &crate::vault::FactId) -> Option<String> {
    file.sections()
        .facts
        .into_iter()
        .find(|f| f.id.as_ref() == Some(id))
        .map(|f| f.text)
}

fn added(what: What, text: String) -> DiffItem {
    DiffItem {
        what,
        before: None,
        after: Some(text),
    }
}

fn relation_text(relation: &Relation) -> String {
    format!(
        "{}: {}",
        relation.word,
        relation
            .link
            .display
            .as_deref()
            .unwrap_or(&relation.link.target)
    )
}

/// A key's value as text: lists joined; `None` for no value.
fn shown(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) if !items.is_empty() => Some(
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        _ => None,
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What `proposal` says it changes, as written when it was made, without the files as
/// they are now: for a decided proposal, whose changes are already in the files or
/// were never applied. The names come from the files when they still exist.
pub(super) fn described(vault: &Vault, proposal: &Proposal) -> Preview {
    let mut files: Vec<FileDiff> = Vec::new();
    for change in &proposal.changes {
        let rel = change.file().unwrap_or_default().to_owned();
        let (items, created) = described_items(change);
        if let Some(diff) = files.iter_mut().find(|d| !rel.is_empty() && d.file == rel) {
            diff.items.extend(items);
            continue;
        }
        let existing = (!rel.is_empty())
            .then(|| memory_path(vault.root(), &rel).ok())
            .flatten()
            .and_then(|path| MemoryFile::read(path).ok());
        let (name, file_type, category) = match (change, &existing) {
            (
                Change::Create {
                    file_type,
                    category,
                    keys,
                    ..
                },
                _,
            ) => (
                keys.get("name")
                    .or_else(|| keys.get("title"))
                    .and_then(Value::as_str)
                    .map_or_else(|| display_stem(&rel), str::to_owned),
                file_type.clone(),
                category.clone().unwrap_or_default(),
            ),
            (Change::WriteSummary { .. }, _) => (
                display_stem(&rel),
                "summary".to_owned(),
                "summaries".to_owned(),
            ),
            (_, Some(file)) => (
                display_name(&rel, file),
                file.common().file_type.as_str().to_owned(),
                file.common().category.as_str().to_owned(),
            ),
            (_, None) => (display_stem(&rel), String::new(), String::new()),
        };
        files.push(FileDiff {
            file: rel,
            name,
            file_type,
            category,
            created,
            items,
        });
    }
    Preview { files, stale: None }
}

fn display_stem(rel: &str) -> String {
    rel.rsplit('/')
        .next()
        .unwrap_or(rel)
        .trim_end_matches(".md")
        .to_owned()
}

/// One change as written, in plain words; and whether it creates its file.
fn described_items(change: &Change) -> (Vec<DiffItem>, bool) {
    let relation = |word: &str, target: &str, display: Option<&str>| {
        format!("{word}: {}", display.unwrap_or(target))
    };
    match change {
        Change::Create {
            keys,
            facts,
            relations,
            notes,
            ..
        } => {
            let mut items: Vec<DiffItem> = keys
                .iter()
                .filter_map(|(key, value)| shown(value).map(|v| added(What::Key(key.clone()), v)))
                .collect();
            items.extend(facts.iter().map(|f| added(What::Fact, one_line(f))));
            items.extend(relations.iter().map(|r| {
                added(
                    What::Relation,
                    relation(&r.word, &r.target, r.display.as_deref()),
                )
            }));
            if let Some(notes) = notes {
                items.push(added(What::Notes, notes.clone()));
            }
            (items, true)
        }
        Change::SetKey { key, from, to, .. } => (
            vec![DiffItem {
                what: What::Key(key.clone()),
                before: shown(from),
                after: shown(to),
            }],
            false,
        ),
        Change::AddFact { text, .. } => (vec![added(What::Fact, one_line(text))], false),
        Change::EditFact { from, to, .. } => (
            vec![DiffItem {
                what: What::Fact,
                before: Some(one_line(from)),
                after: Some(one_line(to)),
            }],
            false,
        ),
        Change::RemoveFact { text, .. } => (
            vec![DiffItem {
                what: What::Fact,
                before: Some(one_line(text)),
                after: None,
            }],
            false,
        ),
        Change::AddRelation {
            word,
            target,
            display,
            ..
        } => (
            vec![added(
                What::Relation,
                relation(word, target, display.as_deref()),
            )],
            false,
        ),
        Change::RemoveRelation { word, target, .. } => (
            vec![DiffItem {
                what: What::Relation,
                before: Some(relation(word, target, None)),
                after: None,
            }],
            false,
        ),
        Change::SetNotes { from, to, .. } => (
            vec![DiffItem {
                what: What::Notes,
                before: from.clone(),
                after: to.clone(),
            }],
            false,
        ),
        Change::Merge { remove, .. } => (
            vec![DiffItem {
                what: What::Merged,
                before: Some(display_stem(remove)),
                after: None,
            }],
            false,
        ),
        Change::WriteSummary { from, text, .. } => (
            vec![DiffItem {
                what: What::Summary,
                before: from.clone(),
                after: Some(text.trim().to_owned()),
            }],
            from.is_none(),
        ),
    }
}
