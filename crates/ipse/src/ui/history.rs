//! The history screen: what changed in the memory and who changed it, what each AI
//! read, what each import and sync brought (decision 0005). Undoing a change makes a
//! new change; the history is never rewritten.

use ipse_core::connector::{RunRecord, diagnostics, runs};
use ipse_core::proposals::{Author, Proposals, Status};
use ipse_core::rights::{Clients, Gateway, access_days};
use ipse_core::vault::{DateTime, Source};
use ipse_core::{Error, Result};
use jiff::ToSpan;
use serde::Serialize;

use super::api::LinkView;

/// Changes on one page of the history.
const PAGE: usize = 30;
/// Files named per change; the others are counted.
const FILES_SHOWN: usize = 6;
/// Days of accesses shown.
const ACCESS_DAYS: i32 = 30;
/// Runs shown.
const RUNS: usize = 50;

/// Who made a change, for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ByView {
    /// `person`, `ipse`, `source` (an import or a sync), `proposal` (one the person
    /// accepted) or `unknown` (a change made with git, outside Ipse).
    kind: String,
    /// The source's instance (`contacts`), or the author of the proposal: the AI's
    /// name, or the source's instance when `author` is `source`.
    name: Option<String>,
    /// For a proposal: `ai`, `source` or `ipse`.
    author: Option<String>,
}

/// One change of the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ChangeLine {
    /// Its id, to undo it.
    id: String,
    /// When, RFC 3339 in the vault's time zone.
    at: String,
    by: ByView,
    /// Its summary as Ipse wrote it, in English: the interface words the known ones.
    summary: String,
    /// The files it touched, the first few.
    files: Vec<LinkView>,
    /// How many more it touched.
    more_files: usize,
}

/// One page of the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct HistoryView {
    changes: Vec<ChangeLine>,
    /// There are older changes.
    more: bool,
}

/// One category an AI read on one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct CategoryRead {
    name: String,
    /// In how many calls.
    calls: u32,
}

/// What one AI read on one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AccessDayView {
    /// `YYYY-MM-DD`.
    day: String,
    /// The AI's name; its id when it was disconnected since.
    ai: String,
    calls: u32,
    results: u32,
    categories: Vec<CategoryRead>,
}

/// What the AIs read, day by day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AccessesView {
    /// The first day shown, `YYYY-MM-DD`.
    from: String,
    days: Vec<AccessDayView>,
}

/// One import or sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct RunView {
    at: String,
    instance: String,
    /// `import` or `sync`.
    mode: String,
    /// `done`, `failed`, `needs` or `stopped`.
    outcome: String,
    /// Why, for one that did not end `done`.
    code: Option<String>,
    added: u32,
    updated: u32,
    unchanged: u32,
    cancelled: u32,
    removed: u32,
    skipped: u32,
    rejected: u32,
    conflicts: u32,
    tasks: u32,
    /// Journal lines repaired after a crash, without their content.
    repaired: u32,
}

/// The imports and syncs, most recent first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct RunsView {
    runs: Vec<RunView>,
    /// The instances that have diagnostics kept.
    diagnosed: Vec<String>,
}

/// A connector's last diagnostic lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct DiagnosticsView {
    instance: String,
    lines: Vec<String>,
}

/// What an undo did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct UndoneView {
    /// False when the files already were as before the change.
    changed: bool,
}

/// What `GET /api/history?offset=` answers.
///
/// # Errors
///
/// Errors reading the history, the proposals or the connected AIs.
pub(crate) fn changes(gateway: &mut Gateway, offset: usize) -> Result<HistoryView> {
    let history = gateway.vault().history()?;
    let mut entries = history.page(offset, PAGE + 1)?;
    let more = entries.len() > PAGE;
    entries.truncate(PAGE);
    let time_zone = gateway.settings().time_zone().clone();
    let names = ai_names(gateway);
    let proposals = Proposals::new(gateway.vault());
    let authors: Vec<(String, ByView)> = entries
        .iter()
        .filter_map(|e| match &e.source {
            Some(Source::Proposal(id)) => Some(id.clone()),
            _ => None,
        })
        .map(|id| {
            let by = proposals.get(&id).ok().map(|p| p.by);
            (id, proposal_by(by.as_ref(), &names))
        })
        .collect();
    let owner = gateway.fresh_owner()?;
    let changes = entries
        .into_iter()
        .map(|entry| {
            let by = match &entry.source {
                Some(Source::Person) => by("person", None, None),
                Some(Source::Ipse) => by("ipse", None, None),
                Some(Source::Connector(instance)) => by("source", Some(instance.clone()), None),
                Some(Source::Proposal(id)) => authors
                    .iter()
                    .find(|(p, _)| p == id)
                    .map_or_else(|| proposal_by(None, &names), |(_, by)| by.clone()),
                None => by("unknown", None, None),
            };
            let at = jiff::Timestamp::from_second(entry.at)
                .map(|t| DateTime::at(t, &time_zone).to_string())
                .unwrap_or_default();
            // Memory files only: the settings and `.gitignore` mean nothing here.
            let memory: Vec<&String> = entry
                .files
                .iter()
                .filter(|f| {
                    !f.starts_with('.')
                        && std::path::Path::new(f)
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                })
                .collect();
            let more_files = memory.len().saturating_sub(FILES_SHOWN);
            let files = memory
                .into_iter()
                .take(FILES_SHOWN)
                .map(|file| {
                    owner
                        .at(file)
                        .map_or_else(|| LinkView::text(&display_stem(file)), LinkView::from)
                })
                .collect();
            ChangeLine {
                id: entry.id,
                at,
                by,
                summary: entry.summary,
                files,
                more_files,
            }
        })
        .collect();
    Ok(HistoryView { changes, more })
}

fn by(kind: &str, name: Option<String>, author: Option<&str>) -> ByView {
    ByView {
        kind: kind.to_owned(),
        name,
        author: author.map(str::to_owned),
    }
}

fn proposal_by(author: Option<&Author>, names: &[(String, String)]) -> ByView {
    match author {
        Some(Author::Client(id)) => by(
            "proposal",
            Some(
                names
                    .iter()
                    .find(|(client, _)| client == id)
                    .map_or_else(|| id.clone(), |(_, name)| name.clone()),
            ),
            Some("ai"),
        ),
        Some(Author::Connector(instance)) => by("proposal", Some(instance.clone()), Some("source")),
        Some(Author::Ipse) | None => by("proposal", None, Some("ipse")),
    }
}

/// A file's name on disk without its folder and `.md`, for a file the index no longer
/// holds.
fn display_stem(file: &str) -> String {
    file.rsplit('/')
        .next()
        .unwrap_or(file)
        .trim_end_matches(".md")
        .to_owned()
}

/// The connected AIs' names, by client id.
fn ai_names(gateway: &Gateway) -> Vec<(String, String)> {
    Clients::load(gateway.vault().root())
        .map(|clients| {
            clients
                .list()
                .iter()
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// What `GET /api/accesses` answers: the last 30 days.
///
/// # Errors
///
/// Errors reading the access log.
pub(crate) fn accesses(gateway: &mut Gateway) -> Result<AccessesView> {
    let today = jiff::Zoned::now()
        .with_time_zone(gateway.settings().time_zone().clone())
        .date();
    let from = today
        .checked_sub((ACCESS_DAYS - 1).days())
        .unwrap_or(today)
        .to_string();
    let names = ai_names(gateway);
    let days = access_days(gateway.vault().root(), &from)?
        .into_iter()
        .map(|day| AccessDayView {
            ai: names
                .iter()
                .find(|(id, _)| *id == day.client)
                .map_or_else(|| day.client.clone(), |(_, name)| name.clone()),
            day: day.day,
            calls: count(day.calls),
            results: count(day.results),
            categories: day
                .categories
                .into_iter()
                .map(|(name, calls)| CategoryRead {
                    name,
                    calls: count(calls),
                })
                .collect(),
        })
        .collect();
    Ok(AccessesView { from, days })
}

/// What `GET /api/runs` answers.
///
/// # Errors
///
/// Errors reading the log of runs.
pub(crate) fn runs_view(gateway: &mut Gateway) -> Result<RunsView> {
    let root = gateway.vault().root();
    let runs: Vec<RunView> = runs(root, RUNS)?.into_iter().map(run_view).collect();
    let mut diagnosed: Vec<String> = Vec::new();
    for run in &runs {
        if !diagnosed.contains(&run.instance) && !diagnostics(root, &run.instance)?.is_empty() {
            diagnosed.push(run.instance.clone());
        }
    }
    Ok(RunsView { runs, diagnosed })
}

/// A count for the interface, where numbers are JavaScript's: saturated at
/// `u32::MAX`, far beyond any real count.
fn count(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn run_view(run: RunRecord) -> RunView {
    RunView {
        at: run.at,
        instance: run.instance,
        mode: run.mode,
        outcome: run.outcome,
        code: run.code,
        added: count(run.added),
        updated: count(run.updated),
        unchanged: count(run.unchanged),
        cancelled: count(run.cancelled),
        removed: count(run.removed),
        skipped: count(run.skipped),
        rejected: count(run.rejected),
        conflicts: count(run.conflicts),
        tasks: count(run.tasks),
        repaired: count(run.repaired),
    }
}

/// What `GET /api/runs/<instance>/diagnostics` answers; `None` for a name that is not
/// an instance's.
///
/// # Errors
///
/// Errors reading the file.
pub(crate) fn diagnostics_view(
    gateway: &mut Gateway,
    instance: &str,
) -> Result<Option<DiagnosticsView>> {
    match diagnostics(gateway.vault().root(), instance) {
        Ok(lines) => Ok(Some(DiagnosticsView {
            instance: instance.to_owned(),
            lines,
        })),
        Err(Error::Refused(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `POST /api/history/<id>/undo`: undoes a change as a new change by the person. An
/// accepted proposal is undone as such, so that it shows as undone too.
///
/// # Errors
///
/// [`Error::Refused`] when the change is unknown or one of its files changed since;
/// errors writing the files or the history.
pub(crate) fn undo(gateway: &mut Gateway, id: &str) -> Result<UndoneView> {
    let proposals = Proposals::new(gateway.vault());
    let accepted = proposals.list()?.into_iter().find(
        |p| matches!(&p.status, Status::Accepted { change: Some(change), .. } if change == id),
    );
    if let Some(proposal) = accepted {
        let undone = proposals.undo(&proposal.id)?;
        return Ok(UndoneView {
            changed: matches!(
                undone.status,
                Status::Undone {
                    change: Some(_),
                    ..
                }
            ),
        });
    }
    let history = gateway.vault().history()?;
    let entry = history.undo(id, Source::Person, "Undo a change")?;
    Ok(UndoneView {
        changed: entry.is_some(),
    })
}
