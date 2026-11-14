//! Streaming a source's records back (`docs/spec/journal.md`, section 7).

use std::io::BufRead;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::Deserialize;

use super::record::{JournalRecord, RECORD_VERSION};
use super::segment::{self, Segment};
use crate::{Error, Result};

/// Reads one source's records, segment by segment and line by line, without loading a
/// segment in memory; sealed segments are decompressed as they are read, once their
/// checksum is checked. Records of an unknown version or kind, and lines that are not a
/// record, are skipped and counted; a sealed segment that does not match its checksum
/// is skipped and listed ([`Reader::damaged`]).
pub struct Reader {
    segments: std::vec::IntoIter<Segment>,
    current: Option<Box<dyn BufRead + Send>>,
    line: String,
    kinds: Option<Vec<String>>,
    /// Only records observed in this period: `from` included, `to` excluded.
    between: Option<(Timestamp, Timestamp)>,
    skipped_unknown: u64,
    skipped_malformed: u64,
    damaged: Vec<PathBuf>,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("skipped_unknown", &self.skipped_unknown)
            .field("skipped_malformed", &self.skipped_malformed)
            .field("damaged", &self.damaged)
            .finish_non_exhaustive()
    }
}

/// Just enough of a record to tell an unknown version from a broken line.
#[derive(Deserialize)]
struct Head {
    v: u32,
}

impl Reader {
    pub(super) fn open(
        dir: &Path,
        kinds: Option<&[&str]>,
        between: Option<(Timestamp, Timestamp)>,
    ) -> Result<Self> {
        let mut segments = segment::list(dir)?;
        if let Some((from, to)) = between {
            // A segment holds the records observed in its month, in UTC.
            let month = |t: Timestamp| {
                let dt = jiff::tz::Offset::UTC.to_datetime(t);
                (dt.year(), dt.month())
            };
            let (first, last) = (month(from), month(to));
            segments.retain(|s| s.month >= first && s.month <= last);
        }
        Ok(Self {
            segments: segments.into_iter(),
            current: None,
            line: String::with_capacity(1024),
            kinds: kinds.map(|k| k.iter().map(|s| (*s).to_owned()).collect()),
            between,
            skipped_unknown: 0,
            skipped_malformed: 0,
            damaged: Vec::new(),
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

    /// Sealed segments skipped because they do not match their checksum: kept as they
    /// are, to be restored from a backup.
    #[must_use]
    pub fn damaged(&self) -> &[PathBuf] {
        &self.damaged
    }

    /// True when `record` was observed in the period read, if any.
    fn in_period(&self, record: &JournalRecord) -> bool {
        self.between
            .is_none_or(|(from, to)| record.observed_at >= from && record.observed_at < to)
    }
}

impl Iterator for Reader {
    type Item = Result<JournalRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current.is_none() {
                let segment = self.segments.next()?;
                if segment.sealed && !segment::verify(&segment) {
                    self.damaged.push(segment.path);
                    continue;
                }
                match segment::open(&segment) {
                    Ok(reader) => self.current = Some(reader),
                    Err(source) => {
                        return Some(Err(Error::Io {
                            action: "read",
                            path: segment.path,
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
                    if self
                        .kinds
                        .as_ref()
                        .is_some_and(|k| !k.contains(&record.kind))
                    {
                        self.skipped_unknown += 1;
                    } else if self.in_period(&record) {
                        return Some(Ok(record));
                    }
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
