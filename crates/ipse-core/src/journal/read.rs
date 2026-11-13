//! Streaming a source's records back (`docs/spec/journal.md`, section 7).

use std::fs::File;
use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::record::{JournalRecord, RECORD_VERSION};
use crate::{Error, Result};

/// Reads one source's records, segment by segment and line by line, without loading a
/// segment in memory. Records of an unknown version or kind, and lines that are not a
/// record, are skipped and counted.
#[derive(Debug)]
pub struct Reader {
    segments: std::vec::IntoIter<PathBuf>,
    current: Option<BufReader<File>>,
    line: String,
    kinds: Option<Vec<String>>,
    skipped_unknown: u64,
    skipped_malformed: u64,
}

/// Just enough of a record to tell an unknown version from a broken line.
#[derive(Deserialize)]
struct Head {
    v: u32,
}

impl Reader {
    pub(super) fn open(dir: &Path, kinds: Option<&[&str]>) -> Result<Self> {
        let mut segments = Vec::new();
        list_segments(dir, &mut segments)?;
        segments.sort();
        Ok(Self {
            segments: segments.into_iter(),
            current: None,
            line: String::with_capacity(1024),
            kinds: kinds.map(|k| k.iter().map(|s| (*s).to_owned()).collect()),
            skipped_unknown: 0,
            skipped_malformed: 0,
        })
    }

    /// Records skipped because of an unknown version or kind.
    #[must_use]
    pub fn skipped_unknown(&self) -> u64 {
        self.skipped_unknown
    }

    /// Lines skipped because they are not a record.
    #[must_use]
    pub fn skipped_malformed(&self) -> u64 {
        self.skipped_malformed
    }

    fn wanted(&self, kind: &str) -> bool {
        self.kinds
            .as_ref()
            .is_none_or(|kinds| kinds.iter().any(|k| k == kind))
    }
}

impl Iterator for Reader {
    type Item = Result<JournalRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current.is_none() {
                let path = self.segments.next()?;
                match File::open(&path) {
                    Ok(file) => self.current = Some(BufReader::with_capacity(64 * 1024, file)),
                    Err(source) => {
                        return Some(Err(Error::Io {
                            action: "read",
                            path,
                            source,
                        }));
                    }
                }
            }
            let reader = self.current.as_mut()?;
            self.line.clear();
            match reader.read_line(&mut self.line) {
                Ok(0) => {
                    self.current = None;
                    continue;
                }
                Ok(_) => {}
                Err(_) => {
                    // An unreadable line (not UTF-8, for example): skip the rest of it.
                    self.skipped_malformed += 1;
                    continue;
                }
            }
            let line = self.line.trim_end();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<JournalRecord>(line) {
                Ok(record) if record.v == RECORD_VERSION => {
                    if self.wanted(&record.kind) {
                        return Some(Ok(record));
                    }
                    self.skipped_unknown += 1;
                }
                Ok(_) => self.skipped_unknown += 1,
                Err(_) => match serde_json::from_str::<Head>(line) {
                    Ok(head) if head.v != RECORD_VERSION => self.skipped_unknown += 1,
                    _ => self.skipped_malformed += 1,
                },
            }
        }
    }
}

/// Every `<YYYY>/<YYYY-MM>.jsonl` under `dir`.
fn list_segments(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let years = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(Error::Io {
                action: "read",
                path: dir.to_owned(),
                source,
            });
        }
    };
    for year in years.flatten() {
        if !year.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(months) = std::fs::read_dir(year.path()) else {
            continue;
        };
        for month in months.flatten() {
            let path = month.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                out.push(path);
            }
        }
    }
    Ok(())
}
