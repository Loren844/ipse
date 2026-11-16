//! The access log (`docs/spec/rights.md`, section 5): one line per call an AI made,
//! saying which categories it read and how many results it got. Never the arguments'
//! text, never the content returned.
//!
//! Lines go to `.ipse/access/<YYYY-MM>.jsonl`, by month in the vault's time zone.
//! Months older than [`KEEP_MONTHS`] are deleted when a new month starts.

use std::collections::{BTreeSet, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use jiff::Zoned;
use serde::{Deserialize, Serialize};

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

/// What one AI read on one day, from the access log.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AccessDay {
    /// The day, `YYYY-MM-DD`, in the vault's time zone.
    pub day: String,
    /// The AI's id.
    pub client: String,
    /// Calls it made.
    pub calls: u64,
    /// Results it got, in all.
    pub results: u64,
    /// Each category it read, and in how many calls.
    pub categories: std::collections::BTreeMap<String, u64>,
}

/// What each AI read, day by day, from `from` (`YYYY-MM-DD`, included) on: most
/// recent day first, then by AI. Only what the log keeps: the categories and counts,
/// never what was asked nor what was returned.
///
/// # Errors
///
/// [`Error::Io`] when the log's folder or one of its months exists but cannot be read.
/// A line that is not valid is skipped.
pub fn access_days(root: &Path, from: &str) -> Result<Vec<AccessDay>> {
    #[derive(Deserialize)]
    struct Call {
        at: String,
        client: String,
        #[serde(default)]
        categories: Vec<String>,
        #[serde(default)]
        results: u64,
    }
    let first = from.get(..7).and_then(month_index).unwrap_or(0);
    let mut days: HashMap<(String, String), AccessDay> = HashMap::new();
    let mut line = String::new();
    for (month, path) in months(root)? {
        if month < first {
            break;
        }
        let mut reader = BufReader::new(File::open(&path).map_err(|e| read_error(&path, e))?);
        loop {
            line.clear();
            if reader
                .read_line(&mut line)
                .map_err(|e| read_error(&path, e))?
                == 0
            {
                break;
            }
            let Ok(call) = serde_json::from_str::<Call>(&line) else {
                continue;
            };
            let Some(day) = call.at.get(..10).filter(|d| *d >= from) else {
                continue;
            };
            let entry = days
                .entry((day.to_owned(), call.client.clone()))
                .or_insert_with(|| AccessDay {
                    day: day.to_owned(),
                    client: call.client,
                    ..AccessDay::default()
                });
            entry.calls += 1;
            entry.results += call.results;
            for category in call.categories {
                *entry.categories.entry(category).or_default() += 1;
            }
        }
    }
    let mut days: Vec<AccessDay> = days.into_values().collect();
    days.sort_unstable_by(|a, b| b.day.cmp(&a.day).then_with(|| a.client.cmp(&b.client)));
    Ok(days)
}

/// The months of the access log of the vault in `root`, newest first.
fn months(root: &Path) -> Result<Vec<(i32, PathBuf)>> {
    let dir = root.join(ACCESS_DIR);
    let mut months: Vec<(i32, PathBuf)> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name();
                let month = month_index(name.to_str()?.strip_suffix(".jsonl")?)?;
                Some((month, e.path()))
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => return Err(read_error(&dir, source)),
    };
    months.sort_unstable_by_key(|m| std::cmp::Reverse(m.0));
    Ok(months)
}

fn read_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        action: "read",
        path: path.to_owned(),
        source,
    }
}

/// When each of the AIs `ids` last called a tool, from the access log, as written
/// there (RFC 3339 in the vault's time zone). An AI with no call in the months kept is
/// left out. Months are read newest first, line by line, and reading stops once every
/// AI is found.
///
/// # Errors
///
/// [`Error::Io`] when the log's folder or one of its months exists but cannot be read.
/// A line that is not valid is skipped.
pub fn last_access(root: &Path, ids: &[&str]) -> Result<HashMap<String, String>> {
    #[derive(Deserialize)]
    struct Call<'a> {
        #[serde(borrow)]
        at: std::borrow::Cow<'a, str>,
        #[serde(borrow)]
        client: std::borrow::Cow<'a, str>,
    }
    let io = read_error;
    let months = months(root)?;
    let mut found: HashMap<String, String> = HashMap::with_capacity(ids.len());
    let mut line = String::new();
    for (_, path) in months {
        if found.len() == ids.len() {
            break;
        }
        // Lines are in the order of the calls: the last one of an AI in the newest
        // month that has it is its last call.
        let mut month: HashMap<&str, String> = HashMap::new();
        let mut reader = BufReader::new(File::open(&path).map_err(|e| io(&path, e))?);
        loop {
            line.clear();
            if reader.read_line(&mut line).map_err(|e| io(&path, e))? == 0 {
                break;
            }
            let Ok(call) = serde_json::from_str::<Call<'_>>(&line) else {
                continue;
            };
            if let Some(id) = ids.iter().find(|id| **id == call.client)
                && !found.contains_key(*id)
            {
                month.insert(id, call.at.into_owned());
            }
        }
        found.extend(month.into_iter().map(|(id, at)| (id.to_owned(), at)));
    }
    Ok(found)
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

    #[test]
    fn the_last_access_of_each_ai_is_its_last_call() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(ACCESS_DIR);
        assert!(
            last_access(dir.path(), &["claude"]).unwrap().is_empty(),
            "no log yet"
        );
        std::fs::create_dir_all(&folder).unwrap();
        let call = |at: &str, client: &str| {
            format!(
                "{{\"at\":\"{at}\",\"client\":\"{client}\",\"tool\":\"now\",\"categories\":[],\"results\":0}}\n"
            )
        };
        std::fs::write(
            folder.join("2026-10.jsonl"),
            call("2026-10-02T10:00:00+02:00", "claude")
                + &call("2026-10-30T10:00:00+01:00", "lm-studio"),
        )
        .unwrap();
        std::fs::write(
            folder.join("2026-11.jsonl"),
            call("2026-11-01T09:00:00+01:00", "claude")
                + "not json\n"
                + &call("2026-11-12T20:41:03+01:00", "claude"),
        )
        .unwrap();
        let last = last_access(dir.path(), &["claude", "lm-studio", "ollama"]).unwrap();
        assert_eq!(last["claude"], "2026-11-12T20:41:03+01:00");
        assert_eq!(last["lm-studio"], "2026-10-30T10:00:00+01:00");
        assert!(!last.contains_key("ollama"));
    }

    #[test]
    fn accesses_are_summed_per_day_and_per_ai() {
        let dir = tempfile::tempdir().unwrap();
        let log = AccessLog::new(dir.path());
        let people: BTreeSet<String> = ["people".to_owned()].into();
        let both: BTreeSet<String> = ["agenda".to_owned(), "people".to_owned()].into();
        for (when, client, categories, results) in [
            (
                "2026-10-31T23:00:00+01:00[Europe/Paris]",
                "claude",
                &people,
                1,
            ),
            (
                "2026-11-12T09:00:00+01:00[Europe/Paris]",
                "claude",
                &people,
                3,
            ),
            (
                "2026-11-12T20:00:00+01:00[Europe/Paris]",
                "claude",
                &both,
                7,
            ),
            (
                "2026-11-12T21:00:00+01:00[Europe/Paris]",
                "lm-studio",
                &people,
                0,
            ),
            (
                "2026-11-13T08:00:00+01:00[Europe/Paris]",
                "claude",
                &BTreeSet::new(),
                0,
            ),
        ] {
            log.record(&at(when), client, "search", categories, results)
                .unwrap();
        }
        let days = access_days(dir.path(), "2026-11-01").unwrap();
        let summary: Vec<(&str, &str, u64, u64)> = days
            .iter()
            .map(|d| (d.day.as_str(), d.client.as_str(), d.calls, d.results))
            .collect();
        assert_eq!(
            summary,
            [
                ("2026-11-13", "claude", 1, 0),
                ("2026-11-12", "claude", 2, 10),
                ("2026-11-12", "lm-studio", 1, 0),
            ]
        );
        assert_eq!(days[1].categories["people"], 2);
        assert_eq!(days[1].categories["agenda"], 1);
        assert_eq!(access_days(dir.path(), "2026-10-01").unwrap().len(), 4);
    }
}
