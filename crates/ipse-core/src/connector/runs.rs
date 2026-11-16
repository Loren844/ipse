//! What each import and sync did, and what connectors wrote to their standard error
//! (`docs/spec/connectors.md`, section 4), for the person to read in the interface.
//!
//! Runs go to `.ipse/connectors/runs.jsonl`, one line each: when, which source, how it
//! ended (as a code, never the connector's own words), and counts. Never a record's
//! content. A connector's standard error goes to `.ipse/connectors/<instance>/stderr.log`,
//! its last [`DIAGNOSTIC_LINES`] lines; connectors must not log personal data there.

use std::path::{Path, PathBuf};

use ipse_protocol::{ErrorCode, NeedKind};
use serde::{Deserialize, Serialize};

use super::host::{Outcome, Report, StopReason};
use crate::journal::is_source_name;
use crate::materialize::{EventsSummary, PeopleSummary};
use crate::vault::write_atomic;
use crate::{Error, Result};

/// The connectors' folder of runs and diagnostics, relative to the vault's folder.
pub const CONNECTORS_DIR: &str = ".ipse/connectors";

/// Lines of a connector's standard error kept per instance.
pub const DIAGNOSTIC_LINES: usize = 1_000;

/// Runs kept in the log; older ones are dropped when it grows past twice as many.
pub const KEEP_RUNS: usize = 500;

/// One import or sync, as the log keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    /// When it ran, RFC 3339 in the vault's time zone.
    pub at: String,
    /// The instance: `contacts`, `calendar-2`.
    pub instance: String,
    /// `import` or `sync`.
    pub mode: String,
    /// `done`, `failed`, `needs` or `stopped`.
    pub outcome: String,
    /// Why, for one that did not end `done`: the connector's error code (`auth`,
    /// `unavailable`…), what it needs (`reauth`, `file`, `config`), or why the core
    /// stopped it (`no_ready`, `broke_protocol`, `ended`, `silent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Records written to the journal.
    #[serde(default)]
    pub written: u64,
    /// Records rejected as malformed.
    #[serde(default)]
    pub rejected: u64,
    /// Items the source no longer holds, recorded as removed.
    #[serde(default)]
    pub removed: u64,
    /// Files created.
    #[serde(default)]
    pub added: u64,
    /// Files updated.
    #[serde(default)]
    pub updated: u64,
    /// Items already up to date.
    #[serde(default)]
    pub unchanged: u64,
    /// Events removed before they happened, marked cancelled.
    #[serde(default)]
    pub cancelled: u64,
    /// Records that could not become a file, ignored.
    #[serde(default)]
    pub skipped: u64,
    /// Files the person had changed, left as they are; each conflict waits as a
    /// proposal.
    #[serde(default)]
    pub conflicts: u64,
    /// Tasks made for the person's AI.
    #[serde(default)]
    pub tasks: u64,
    /// Incomplete journal lines, left by a crash, cut off before writing. Their content
    /// is never kept.
    #[serde(default)]
    pub repaired: u64,
}

impl RunRecord {
    /// A run of `instance` in `mode` at `at`, as `report` says it went, and how many
    /// journal lines it repaired.
    #[must_use]
    pub fn new(at: String, instance: &str, mode: &str, report: &Report, repaired: u64) -> Self {
        let (outcome, code) = match &report.outcome {
            Outcome::Done(_) => ("done", None),
            Outcome::Failed { code, .. } => ("failed", Some(error_code(*code))),
            Outcome::Needs { what, .. } => ("needs", Some(need_code(*what))),
            Outcome::Stopped(reason) => ("stopped", Some(stop_code(*reason))),
        };
        Self {
            at,
            instance: instance.to_owned(),
            mode: mode.to_owned(),
            outcome: outcome.to_owned(),
            code: code.map(str::to_owned),
            written: report.written,
            rejected: report.rejected,
            removed: report.removed,
            repaired,
            ..Self::default()
        }
    }

    /// Adds what the run changed in people files.
    pub fn people(&mut self, summary: &PeopleSummary) {
        self.added = summary.added;
        self.updated = summary.updated;
        self.unchanged = summary.unchanged;
        self.skipped = summary.skipped;
        self.conflicts = summary.conflicts;
    }

    /// Adds what the run changed in event files.
    pub fn events(&mut self, summary: &EventsSummary) {
        self.added = summary.added;
        self.updated = summary.updated;
        self.unchanged = summary.unchanged;
        self.cancelled = summary.cancelled;
        self.skipped = summary.skipped;
        self.conflicts = summary.conflicts;
    }
}

fn error_code(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::RateLimited => "rate_limited",
        ErrorCode::Unavailable => "unavailable",
        ErrorCode::Auth => "auth",
        ErrorCode::BadInput => "bad_input",
        ErrorCode::Unsupported => "unsupported",
        ErrorCode::UnsupportedProtocol => "unsupported_protocol",
        ErrorCode::Internal => "internal",
    }
}

fn need_code(what: NeedKind) -> &'static str {
    match what {
        NeedKind::Reauth => "reauth",
        NeedKind::File => "file",
        NeedKind::Config => "config",
    }
}

fn stop_code(reason: StopReason) -> &'static str {
    match reason {
        StopReason::NoReady => "no_ready",
        StopReason::BrokeProtocol(_) => "broke_protocol",
        StopReason::Ended => "ended",
        StopReason::Silent => "silent",
    }
}

fn runs_path(root: &Path) -> PathBuf {
    root.join(CONNECTORS_DIR).join("runs.jsonl")
}

fn io(action: &'static str, path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        action,
        path: path.to_owned(),
        source,
    }
}

/// Reads a file of the connectors' folder; none is empty.
fn read_or_empty(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(io("read", path, source)),
    }
}

/// Adds `run` to the log of the vault in `root`, keeping the last [`KEEP_RUNS`] once
/// it holds twice as many.
///
/// # Errors
///
/// [`Error::Io`] when the log cannot be read or written.
pub fn record_run(root: &Path, run: &RunRecord) -> Result<()> {
    let path = runs_path(root);
    let line = serde_json::to_string(run)
        .map_err(|_| Error::Refused("A run could not be logged.".to_owned()))?;
    let mut text = read_or_empty(&path)?;
    text.push_str(&line);
    text.push('\n');
    let count = text.lines().count();
    if count > 2 * KEEP_RUNS {
        let keep: Vec<&str> = text.lines().skip(count - KEEP_RUNS).collect();
        text = keep.join("\n") + "\n";
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
    }
    write_atomic(&path, text.as_bytes())
}

/// The last `limit` runs of the vault in `root`, most recent first. A line that is not
/// valid is skipped.
///
/// # Errors
///
/// [`Error::Io`] when the log exists but cannot be read.
pub fn runs(root: &Path, limit: usize) -> Result<Vec<RunRecord>> {
    let text = read_or_empty(&runs_path(root))?;
    Ok(text
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str(line).ok())
        .take(limit)
        .collect())
}

fn diagnostics_path(root: &Path, instance: &str) -> Result<PathBuf> {
    if !is_source_name(instance) {
        return Err(Error::Refused(format!("There is no source {instance}.")));
    }
    Ok(root.join(CONNECTORS_DIR).join(instance).join("stderr.log"))
}

/// Adds `lines`, what a session of `instance` wrote to its standard error, to that
/// instance's diagnostics in the vault in `root`, keeping the last [`DIAGNOSTIC_LINES`].
///
/// # Errors
///
/// [`Error::Refused`] for a name that is not an instance's; [`Error::Io`] when the
/// file cannot be read or written.
pub fn save_diagnostics(root: &Path, instance: &str, lines: &[String]) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let path = diagnostics_path(root, instance)?;
    let before = read_or_empty(&path)?;
    let all: Vec<&str> = before
        .lines()
        .chain(lines.iter().flat_map(|l| l.lines()))
        .collect();
    let keep = &all[all.len().saturating_sub(DIAGNOSTIC_LINES)..];
    let mut text = keep.join("\n");
    text.push('\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io("create", parent, e))?;
    }
    write_atomic(&path, text.as_bytes())
}

/// The diagnostics kept for `instance` in the vault in `root`, oldest first.
///
/// # Errors
///
/// [`Error::Refused`] for a name that is not an instance's; [`Error::Io`] when the
/// file exists but cannot be read.
pub fn diagnostics(root: &Path, instance: &str) -> Result<Vec<String>> {
    let text = read_or_empty(&diagnostics_path(root, instance)?)?;
    Ok(text.lines().map(str::to_owned).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_keep_the_last_thousand_lines() {
        let dir = tempfile::tempdir().unwrap();
        let first: Vec<String> = (0..700).map(|i| format!("line {i}")).collect();
        let second: Vec<String> = (700..1_500).map(|i| format!("line {i}")).collect();
        save_diagnostics(dir.path(), "contacts", &first).unwrap();
        save_diagnostics(dir.path(), "contacts", &second).unwrap();
        save_diagnostics(dir.path(), "contacts", &[]).unwrap();
        let kept = diagnostics(dir.path(), "contacts").unwrap();
        assert_eq!(kept.len(), DIAGNOSTIC_LINES);
        assert_eq!(kept[0], "line 500");
        assert_eq!(kept[999], "line 1499");
        assert!(diagnostics(dir.path(), "calendar").unwrap().is_empty());
        assert!(diagnostics(dir.path(), "../clients").is_err());
    }

    #[test]
    fn runs_are_listed_newest_first_and_kept_bounded() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..=(2 * KEEP_RUNS) {
            let run = RunRecord {
                at: format!("2026-11-16T00:00:{:02}+01:00", i % 60),
                instance: "contacts".to_owned(),
                mode: "import".to_owned(),
                outcome: "done".to_owned(),
                added: i as u64,
                ..RunRecord::default()
            };
            record_run(dir.path(), &run).unwrap();
        }
        let all = runs(dir.path(), usize::MAX).unwrap();
        assert_eq!(all.len(), KEEP_RUNS);
        assert_eq!(all[0].added, 2 * KEEP_RUNS as u64, "newest first");
        assert_eq!(runs(dir.path(), 3).unwrap().len(), 3);
    }
}
