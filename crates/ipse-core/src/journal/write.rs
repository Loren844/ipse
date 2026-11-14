//! Appending records to the journal (`docs/spec/journal.md`, sections 5 and 6).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use ipse_protocol::{Op, Record};
use jiff::Timestamp;
use jiff::tz::Offset;
use serde_json::Value;

use super::canonical::data_hash;
use super::is_source_name;
use super::latest::Table;
use super::read::Reader;
use super::record::{JournalRecord, RECORD_VERSION};
use super::segment;
use crate::vault::write_atomic;
use crate::{Error, Result};

/// The journal of one vault, at `<vault>/journal`.
///
/// The latest record of each item is kept in a table per source, read on the first
/// write and saved when the journal is dropped ([`Journal::save_index`] saves it
/// earlier).
#[derive(Debug)]
pub struct Journal {
    vault_root: PathBuf,
    root: PathBuf,
    ids: ulid::Generator,
    tables: HashMap<String, Table>,
    /// Sources whose past months were sealed by this journal.
    sealed: HashSet<String>,
    recovered: u64,
}

impl Drop for Journal {
    fn drop(&mut self) {
        self.save_index();
    }
}

impl Journal {
    /// The journal of the vault in `vault_root`. Nothing is read or created yet.
    #[must_use]
    pub fn new(vault_root: &Path) -> Self {
        Self {
            vault_root: vault_root.to_owned(),
            root: vault_root.join("journal"),
            ids: ulid::Generator::new(),
            tables: HashMap::new(),
            sealed: HashSet::new(),
            recovered: 0,
        }
    }

    /// Saves the tables of latest records read or changed so far. A failure only means
    /// reading the journal again next time.
    pub fn save_index(&mut self) {
        for (source, table) in &mut self.tables {
            table.save(&self.vault_root, source);
        }
    }

    /// How many incomplete last lines, left by a crash in the middle of a write, this
    /// journal cut off. Nothing of those lines is kept or shown.
    #[must_use]
    pub fn recovered(&self) -> u64 {
        self.recovered
    }

    /// The folder of `source`'s segments.
    #[must_use]
    pub fn source_dir(&self, source: &str) -> PathBuf {
        self.root.join(source)
    }

    /// Appends a batch of records from `source`, each to the segment of its month, and
    /// flushes them to disk before returning. Then, if given, saves `cursor` as the
    /// point `source` resumes from. Returns the records as written.
    ///
    /// An upsert whose data is the same as the item's latest record is skipped: a full
    /// re-sync writes only what changed. The first write of a source in a new month
    /// seals its past months (section 5); a sealed month that does not match its
    /// checksum is left as it is.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] for an invalid source name or record (nothing is written);
    /// [`Error::Io`] when a segment or the cursor cannot be written.
    pub fn append(
        &mut self,
        source: &str,
        records: &[Record],
        cursor: Option<&Value>,
    ) -> Result<Vec<JournalRecord>> {
        if !is_source_name(source) {
            return Err(Error::Refused(
                "A source name uses lowercase letters, digits and dashes.".to_owned(),
            ));
        }
        for (i, record) in records.iter().enumerate() {
            record
                .validate()
                .map_err(|rule| Error::Refused(format!("Record {} is invalid: {rule}.", i + 1)))?;
        }

        if !self.tables.contains_key(source) {
            let table = Table::load(&self.vault_root, source, &self.source_dir(source))?;
            self.tables.insert(source.to_owned(), table);
        }
        if self.sealed.insert(source.to_owned()) {
            self.seal_past_months(source)?;
        }

        let ingested_at = Timestamp::now();
        let mut written = Vec::with_capacity(records.len());
        // Segment month → lines, in order. A batch usually fits in one segment.
        let mut segments: BTreeMap<(i16, i8), Vec<u8>> = BTreeMap::new();
        for record in records {
            let hash = match (record.op, &record.data) {
                (Op::Upsert, Some(data)) => Some(data_hash(data)),
                _ => None,
            };
            if let (Some(hash), Some(external_id), Some(table)) =
                (&hash, &record.external_id, self.tables.get(source))
                && table.unchanged(&record.kind, external_id, hash)
            {
                continue;
            }
            let entry = JournalRecord {
                v: RECORD_VERSION,
                id: self.next_id(),
                source: source.to_owned(),
                kind: record.kind.clone(),
                op: record.op,
                external_id: record.external_id.clone(),
                observed_at: record.observed_at,
                ingested_at,
                hash,
                data: record.data.clone(),
            };
            if let Some(table) = self.tables.get_mut(source) {
                table.note(
                    &entry.kind,
                    entry.external_id.as_deref(),
                    entry.op,
                    &entry.id,
                    entry.hash.as_deref(),
                );
            }
            let lines = segments.entry(month_of(entry.observed_at)).or_default();
            serde_json::to_writer(&mut *lines, &entry)
                .map_err(|_| Error::Refused("A record could not be encoded as JSON.".to_owned()))?;
            lines.push(b'\n');
            written.push(entry);
        }

        for ((year, month), lines) in &segments {
            let path = self.segment_path(source, *year, *month);
            if segment::recover(&path)? {
                self.recovered += 1;
            }
            self.append_lines(&path, lines)?;
        }

        if let Some(cursor) = cursor {
            let path = self.source_dir(source).join("cursor.json");
            let text = serde_json::to_vec(cursor).map_err(|_| {
                Error::Refused("The connector's cursor could not be saved.".to_owned())
            })?;
            write_atomic(&path, &text)?;
        }
        Ok(written)
    }

    /// The point `source` resumes from, as saved by its last batch.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the cursor file exists but cannot be read; [`Error::Invalid`]
    /// when it is not JSON.
    pub fn cursor(&self, source: &str) -> Result<Option<Value>> {
        let path = self.source_dir(source).join("cursor.json");
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| Error::Invalid {
                    path,
                    line: None,
                    problem: "the cursor is not valid JSON; delete it to sync this source again from the start".to_owned(),
                }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::Io {
                action: "read",
                path,
                source,
            }),
        }
    }

    /// Reads `source`'s records in the order they were written. `kinds` limits the
    /// records returned; other kinds are skipped and counted.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the source's folder cannot be listed.
    pub fn read(&self, source: &str, kinds: Option<&[&str]>) -> Result<Reader> {
        Reader::open(&self.source_dir(source), kinds, None)
    }

    /// As [`Journal::read`], only the records observed from `from` (included) to `to`
    /// (excluded). Segments of other months are not opened. A time-series batch counts
    /// by its first point (section 4).
    ///
    /// # Errors
    ///
    /// As [`Journal::read`].
    pub fn read_between(
        &self,
        source: &str,
        kinds: Option<&[&str]>,
        from: Timestamp,
        to: Timestamp,
    ) -> Result<Reader> {
        Reader::open(&self.source_dir(source), kinds, Some((from, to)))
    }

    /// Seals the open segments of `source` older than this month. A month whose earlier
    /// sealed file does not match its checksum is left open, never overwritten.
    fn seal_past_months(&self, source: &str) -> Result<()> {
        let this_month = month_of(Timestamp::now());
        let segments = segment::list(&self.source_dir(source))?;
        for open in segments
            .iter()
            .filter(|s| !s.sealed && s.month < this_month)
        {
            let earlier = segments.iter().find(|s| s.sealed && s.month == open.month);
            segment::seal(open, earlier)?;
        }
        Ok(())
    }

    fn segment_path(&self, source: &str, year: i16, month: i8) -> PathBuf {
        self.source_dir(source)
            .join(format!("{year:04}"))
            .join(format!("{year:04}-{month:02}.jsonl"))
    }

    fn append_lines(&self, path: &Path, lines: &[u8]) -> Result<()> {
        let io = |action, source| Error::Io {
            action,
            path: path.to_owned(),
            source,
        };
        let dir = path.parent().unwrap_or(&self.root);
        let created = !path.exists();
        if created {
            std::fs::create_dir_all(dir).map_err(|e| io("create the folder of", e))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| io("open", e))?;
        file.write_all(lines).map_err(|e| io("write", e))?;
        file.sync_data().map_err(|e| io("write", e))?;
        #[cfg(unix)]
        if created {
            std::fs::File::open(dir)
                .and_then(|d| d.sync_all())
                .map_err(|e| io("write", e))?;
        }
        Ok(())
    }

    fn next_id(&mut self) -> String {
        let id = self
            .ids
            .generate()
            .unwrap_or_else(|_| ulid::Ulid::generate());
        id.to_string().to_ascii_lowercase()
    }
}

/// Year and month of `ts` in UTC: the segment a record goes in.
fn month_of(ts: Timestamp) -> (i16, i8) {
    let dt = Offset::UTC.to_datetime(ts);
    (dt.year(), dt.month())
}
