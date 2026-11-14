//! The access log (`docs/spec/rights.md`, section 5): one line per call an AI made,
//! saying which categories it read and how many results it got. Never the arguments'
//! text, never the content returned.
//!
//! Lines go to `.ipse/access/<YYYY-MM>.jsonl`, by month in the vault's time zone.
//! Months older than [`KEEP_MONTHS`] are deleted when a new month starts.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use jiff::Zoned;
use serde::Serialize;

use crate::vault::DateTime;
use crate::{Error, Result};

/// The access log's folder, relative to the vault's folder.
pub const ACCESS_DIR: &str = ".ipse/access";

/// How many months the access log keeps.
pub const KEEP_MONTHS: i32 = 12;

/// One call, as the log keeps it.
#[derive(Serialize)]
struct Line<'a> {
    at: String,
    client: &'a str,
    tool: &'a str,
    categories: &'a BTreeSet<String>,
    results: usize,
}

/// The access log of one vault. The month's file stays open between calls.
#[derive(Debug)]
pub(crate) struct AccessLog {
    dir: PathBuf,
    open: Mutex<Option<(String, File)>>,
}

impl AccessLog {
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            dir: root.join(ACCESS_DIR),
            open: Mutex::new(None),
        }
    }

    /// Appends one call made at `at`.
    pub(crate) fn record(
        &self,
        at: &Zoned,
        client: &str,
        tool: &str,
        categories: &BTreeSet<String>,
        results: usize,
    ) -> Result<()> {
        let line = Line {
            at: DateTime::at(at.timestamp(), at.time_zone()).to_string(),
            client,
            tool,
            categories,
            results,
        };
        let mut bytes = serde_json::to_vec(&line)
            .map_err(|_| Error::Refused("An access could not be logged.".to_owned()))?;
        bytes.push(b'\n');

        let month = at.strftime("%Y-%m").to_string();
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if open.as_ref().is_none_or(|(m, _)| *m != month) {
            std::fs::create_dir_all(&self.dir).map_err(|source| self.io("create", source))?;
            let path = self.dir.join(format!("{month}.jsonl"));
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|source| Error::Io {
                    action: "open",
                    path,
                    source,
                })?;
            self.prune(at);
            *open = Some((month, file));
        }
        if let Some((_, file)) = open.as_mut() {
            file.write_all(&bytes)
                .map_err(|source| self.io("write", source))?;
        }
        Ok(())
    }

    /// Deletes the months older than [`KEEP_MONTHS`] before `now`'s. A file that
    /// cannot be deleted is tried again next month.
    fn prune(&self, now: &Zoned) {
        let current = i32::from(now.year()) * 12 + i32::from(now.month());
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(month) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".jsonl"))
                .and_then(month_index)
            else {
                continue;
            };
            if current - month >= KEEP_MONTHS {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    fn io(&self, action: &'static str, source: std::io::Error) -> Error {
        Error::Io {
            action,
            path: self.dir.clone(),
            source,
        }
    }
}

/// `2026-11` as a count of months.
fn month_index(text: &str) -> Option<i32> {
    let (year, month) = text.split_once('-')?;
    let (year, month): (i32, i32) = (year.parse().ok()?, month.parse().ok()?);
    (year.to_string().len() == 4 && (1..=12).contains(&month)).then_some(year * 12 + month)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Zoned {
        text.parse().unwrap()
    }

    #[test]
    fn one_line_per_call_by_month() {
        let dir = tempfile::tempdir().unwrap();
        let log = AccessLog::new(dir.path());
        let categories: BTreeSet<String> = ["agenda".to_owned()].into();
        log.record(
            &at("2026-11-12T20:41:03+01:00[Europe/Paris]"),
            "claude-desktop",
            "events",
            &categories,
            7,
        )
        .unwrap();
        log.record(
            &at("2026-12-01T00:00:01+01:00[Europe/Paris]"),
            "claude-desktop",
            "now",
            &BTreeSet::new(),
            0,
        )
        .unwrap();
        let november =
            std::fs::read_to_string(dir.path().join(".ipse/access/2026-11.jsonl")).unwrap();
        assert_eq!(
            november,
            "{\"at\":\"2026-11-12T20:41:03+01:00\",\"client\":\"claude-desktop\",\"tool\":\"events\",\"categories\":[\"agenda\"],\"results\":7}\n"
        );
        assert!(dir.path().join(".ipse/access/2026-12.jsonl").is_file());
    }

    #[test]
    fn months_older_than_a_year_are_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(ACCESS_DIR);
        std::fs::create_dir_all(&folder).unwrap();
        for month in ["2025-10", "2025-11", "2025-12", "notes"] {
            std::fs::write(folder.join(format!("{month}.jsonl")), "").unwrap();
        }
        AccessLog::new(dir.path())
            .record(
                &at("2026-11-14T10:00:00+01:00[Europe/Paris]"),
                "claude",
                "now",
                &BTreeSet::new(),
                0,
            )
            .unwrap();
        let mut left: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["2025-12.jsonl", "2026-11.jsonl", "notes.jsonl"],
            "12 months kept; files Ipse did not name are left alone"
        );
    }
}
