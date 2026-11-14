//! Searching the vault by words: entities, events and summaries (architecture layer 4).
//!
//! The index lists, for each word of the vault, the files that hold it. Words are
//! folded (case and accents) and brought back to the singular in the vault's language
//! ([`fold::stem`]), the same way for the files and for what is searched.
//!
//! - It is saved in `.ipse/index/`, which is derived and disposable: deleting it only
//!   means the next start reads every file again.
//! - When opened, it compares each file's size and date with what it saved, and reads
//!   again only the files that changed ([`SearchIndex::update`]).
//! - Rights are applied inside the query: a file outside the allowed categories is
//!   never scored, so it can neither be returned nor change the order of the results.

mod document;
mod fold;

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::vault::{
    Category, EventTime, FileType, Id, Language, MemoryFile, write_atomic_unsynced,
};
use crate::{Error, Result};
use document::{Document, Field, Stored};

pub use fold::{fold, stem, words};

/// Results returned when no limit is given.
pub const DEFAULT_LIMIT: usize = 10;

/// Most results one search returns.
pub const MAX_LIMIT: usize = 50;

/// The saved index, relative to the vault's folder.
pub const INDEX_FILE: &str = ".ipse/index/search.bin";

/// The summaries' folder, and the periods with one folder per year inside it.
const SUMMARIES: &str = "summaries";
const SUMMARY_PERIODS: [&str; 3] = ["day", "week", "month"];

/// Version of the saved index; another version is read again from the files.
const INDEX_FORMAT: u32 = 1;

/// Longest excerpt, in characters.
const EXCERPT_CHARS: usize = 160;

/// Weights of a match, by field and by kind: a whole word, or the start of one.
const NAME: (u32, u32) = (100, 80);
const ALIAS: (u32, u32) = (90, 70);
const FACT: (u32, u32) = (30, 20);
const RELATION: (u32, u32) = (30, 20);
const NOTES: (u32, u32) = (10, 5);

/// An event of the index, for listing events by period.
#[derive(Debug, Clone, Copy)]
pub struct EventRef<'i> {
    /// Its file, relative to the vault's folder.
    pub path: &'i Path,
    /// When it starts.
    pub start: &'i EventTime,
    /// When it ends, exclusive, if known.
    pub end: Option<&'i EventTime>,
    /// File stems of the participants that are links.
    pub participants: &'i [String],
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The entity's identifier.
    pub id: Id,
    /// What the entity is.
    pub file_type: FileType,
    /// Its rights category.
    pub category: Category,
    /// Its file, relative to the vault's folder.
    pub path: PathBuf,
    /// The entity's name.
    pub name: String,
    /// The text that matched best, shortened.
    pub excerpt: String,
    /// Higher is better.
    pub score: u32,
}

/// The results of one search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Results {
    /// The best matches, best first.
    pub hits: Vec<Hit>,
    /// True when more entities matched than the limit allowed.
    pub more: bool,
}

/// A file's size and modification time, to tell whether it changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
struct Stamp {
    len: u64,
    secs: u64,
    nanos: u32,
}

impl Stamp {
    fn of(entry: &std::fs::DirEntry) -> Self {
        let Ok(meta) = entry.metadata() else {
            return Self::default();
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .unwrap_or_default();
        Self {
            len: meta.len(),
            secs: modified.as_secs(),
            nanos: modified.subsec_nanos(),
        }
    }
}

/// One file of the vault, as the index knows it.
#[derive(Debug, Clone)]
struct Slot {
    /// `people/marie-dupont.md`.
    path: String,
    stamp: Stamp,
    /// `None` for a file that could not be read: it is not searchable.
    document: Option<Document>,
}

/// The vault's entities, events and summaries, ready to be searched.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    language: Language,
    /// One slot per file; `None` once removed. Slots are never reused before the index
    /// is compacted, so the lists of words stay sorted by pushing.
    slots: Vec<Option<Slot>>,
    removed: usize,
    by_path: HashMap<String, usize>,
    /// Each word, and the slots holding it, sorted. Ordered, to find the words that
    /// start with what is typed.
    terms: BTreeMap<String, Vec<u32>>,
    /// Slots by file stem in ASCII lower case, to follow links: several folders may
    /// hold the same stem.
    by_stem: HashMap<String, Vec<usize>>,
    /// Slots by id.
    by_id: HashMap<Id, usize>,
    /// Slots by name and by other name, as matched (words joined by a space).
    by_name: HashMap<String, Vec<usize>>,
    /// The slots holding an event, in order.
    events: Vec<usize>,
    skipped: u64,
    files: Fingerprint,
    folders: u64,
}

/// The saved index.
#[derive(Serialize, Deserialize)]
struct Saved {
    format: u32,
    language: String,
    files: Vec<SavedFile>,
}

#[derive(Serialize, Deserialize)]
struct SavedFile {
    path: String,
    stamp: Stamp,
    document: Option<Stored>,
}

/// A cheap summary of the files' names, sizes and modification times, to tell whether
/// the vault changed since it was read without reading it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Fingerprint {
    count: u64,
    sum: u64,
}

impl Fingerprint {
    /// Adds one file. Files are summed, so the order they are listed in does not matter.
    fn add(&mut self, folder: &str, entry: &std::fs::DirEntry, stamp: Stamp) {
        let mut hasher = DefaultHasher::new();
        folder.hash(&mut hasher);
        entry.file_name().hash(&mut hasher);
        stamp.hash(&mut hasher);
        self.count += 1;
        self.sum = self.sum.wrapping_add(hasher.finish());
    }
}

impl Hash for Stamp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.len, self.secs, self.nanos).hash(state);
    }
}

/// A summary of the folders' modification times: adding, removing or renaming a file
/// (an atomic write included) changes it; editing a file in place does not. Runs before
/// every request, so it leaves out the summaries' folders: Ipse writes summaries
/// itself, and the full check ([`SearchIndex::changed`], every 2 seconds) finds them.
fn folders_fingerprint(root: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    let (mut folders, parents) = folders(root, false);
    // The parents, where a new year's folder appears.
    folders.extend(parents);
    for folder in folders {
        folder.hash(&mut hasher);
        std::fs::metadata(root.join(&folder))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .hash(&mut hasher);
    }
    hasher.finish()
}

/// The folders holding searchable files (`people`, `events/2026`, with `summaries`
/// also `summaries/day/2026`…), and the folders that hold one folder per year.
fn folders(root: &Path, summaries: bool) -> (Vec<String>, Vec<String>) {
    let mut folders: Vec<String> = FileType::ALL
        .into_iter()
        .filter(|t| t.is_entity())
        .map(|t| t.folder().to_owned())
        .collect();
    let mut parents = vec![FileType::Event.folder().to_owned()];
    if summaries {
        folders.push(format!("{SUMMARIES}/year"));
        parents.extend(SUMMARY_PERIODS.map(|p| format!("{SUMMARIES}/{p}")));
    }
    for parent in &parents {
        let mut years: Vec<String> = subfolders(&root.join(parent))
            .into_iter()
            .map(|name| format!("{parent}/{name}"))
            .collect();
        years.sort();
        folders.extend(years);
    }
    (folders, parents)
}

/// The names of the folders in `dir`, hidden ones aside; none when it cannot be read.
fn subfolders(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect()
}

/// The searchable files of the vault in `root`, by folder. A missing folder has none.
fn searchable_files(root: &Path, mut each: impl FnMut(&str, &std::fs::DirEntry)) -> Result<()> {
    for folder in &folders(root, true).0 {
        let folder = folder.as_str();
        let dir = root.join(folder);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path: dir,
                    source,
                });
            }
        };
        for entry in entries.flatten() {
            let is_note = entry.path().extension().is_some_and(|e| e == "md")
                && !entry.file_name().to_string_lossy().starts_with('.');
            if is_note {
                each(folder, &entry);
            }
        }
    }
    Ok(())
}

/// The vault's language, from its settings; English when they cannot be read.
fn language_of(root: &Path) -> Language {
    crate::vault::Settings::read(&root.join(crate::vault::SETTINGS_FILE))
        .map(|s| s.language())
        .unwrap_or_default()
}

impl SearchIndex {
    /// The index of the vault in `root`: the saved one brought up to date, or, when
    /// there is none or it is from another version, one read from every file. The
    /// result is saved again when it changed; a failure to save only means more work
    /// next time. Files that cannot be read are skipped and counted.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a folder exists but cannot be listed.
    pub fn load(root: &Path) -> Result<Self> {
        let language = language_of(root);
        let mut index = Self::read_saved(root, language).unwrap_or_else(|| Self {
            language,
            ..Self::default()
        });
        let changed = index.update(root)?;
        if changed {
            let _ = index.save(root);
        }
        Ok(index)
    }

    /// The index of the vault in `root`, read from every file, ignoring the saved one.
    ///
    /// # Errors
    ///
    /// As [`SearchIndex::load`].
    pub fn build(root: &Path) -> Result<Self> {
        let mut index = Self {
            language: language_of(root),
            ..Self::default()
        };
        index.update(root)?;
        Ok(index)
    }

    /// Brings the index up to date with the vault in `root`: reads again the files whose
    /// size or date changed and the new ones, and forgets those removed. Returns
    /// whether anything changed.
    ///
    /// # Errors
    ///
    /// As [`SearchIndex::load`].
    pub fn update(&mut self, root: &Path) -> Result<bool> {
        // Noted before listing, so that a change made while listing shows next time.
        let folders = folders_fingerprint(root);
        let mut fingerprint = Fingerprint::default();
        let mut seen = vec![false; self.slots.len()];
        let mut to_read: Vec<(String, PathBuf, Stamp)> = Vec::new();
        searchable_files(root, |folder, entry| {
            let stamp = Stamp::of(entry);
            fingerprint.add(folder, entry, stamp);
            let path = format!("{folder}/{}", entry.file_name().to_string_lossy());
            match self.by_path.get(&path) {
                Some(&slot) if self.slots[slot].as_ref().is_some_and(|s| s.stamp == stamp) => {
                    seen[slot] = true;
                }
                _ => to_read.push((path, entry.path(), stamp)),
            }
        })?;

        // Removed and changed files alike: a changed one is read again below.
        let mut changed = false;
        for (slot, seen) in seen.into_iter().enumerate() {
            if !seen && self.slots[slot].is_some() {
                self.remove(slot);
                changed = true;
            }
        }
        for (path, full, stamp) in to_read {
            let document = MemoryFile::read(&full)
                .ok()
                .and_then(|f| Document::of(root, &full, &f, self.language));
            self.insert(path, stamp, document);
            changed = true;
        }
        self.files = fingerprint;
        self.folders = folders;
        if self.removed > 64 && self.removed > self.slots.len() / 2 {
            self.compact();
        }
        Ok(changed)
    }

    /// Saves the index in [`INDEX_FILE`], to be opened quickly next time.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file cannot be written.
    pub fn save(&self, root: &Path) -> Result<()> {
        let saved = Saved {
            format: INDEX_FORMAT,
            language: self.language.to_string(),
            files: self
                .slots
                .iter()
                .flatten()
                .map(|s| SavedFile {
                    path: s.path.clone(),
                    stamp: s.stamp,
                    document: s.document.as_ref().map(Document::to_stored),
                })
                .collect(),
        };
        let bytes = postcard::to_stdvec(&saved)
            .map_err(|_| Error::Refused("The search index could not be saved.".to_owned()))?;
        write_atomic_unsynced(&root.join(INDEX_FILE), &bytes)
    }

    /// The saved index, if there is one of this version and language.
    fn read_saved(root: &Path, language: Language) -> Option<Self> {
        let bytes = std::fs::read(root.join(INDEX_FILE)).ok()?;
        let saved: Saved = postcard::from_bytes(&bytes).ok()?;
        if saved.format != INDEX_FORMAT || saved.language != language.to_string() {
            return None;
        }
        let mut index = Self {
            language,
            ..Self::default()
        };
        index.slots.reserve(saved.files.len());
        for file in saved.files {
            let document = match file.document {
                None => None,
                Some(stored) => Some(Document::from_stored(stored, language)?),
            };
            index.insert(file.path, file.stamp, document);
        }
        Some(index)
    }

    fn insert(&mut self, path: String, stamp: Stamp, document: Option<Document>) {
        let slot = self.slots.len();
        let Ok(id) = u32::try_from(slot) else {
            return;
        };
        if let Some(d) = &document {
            for term in d.terms() {
                self.terms.entry(term.to_owned()).or_default().push(id);
            }
            if let Some(stem) = d.path.file_stem() {
                let stem = stem.to_string_lossy().to_ascii_lowercase();
                self.by_stem.entry(stem).or_default().push(slot);
            }
            self.by_id.insert(d.id.clone(), slot);
            for name in d.names() {
                self.by_name.entry(name).or_default().push(slot);
            }
            if d.event.is_some() {
                self.events.push(slot);
            }
        } else {
            self.skipped += 1;
        }
        self.by_path.insert(path.clone(), slot);
        self.slots.push(Some(Slot {
            path,
            stamp,
            document,
        }));
    }

    fn remove(&mut self, slot: usize) {
        let Some(old) = self.slots.get_mut(slot).and_then(Option::take) else {
            return;
        };
        self.removed += 1;
        self.by_path.remove(&old.path);
        let Some(d) = old.document else {
            self.skipped = self.skipped.saturating_sub(1);
            return;
        };
        let id = u32::try_from(slot).unwrap_or(u32::MAX);
        for term in d.terms() {
            if let Some(list) = self.terms.get_mut(term) {
                if let Ok(at) = list.binary_search(&id) {
                    list.remove(at);
                }
                if list.is_empty() {
                    self.terms.remove(term);
                }
            }
        }
        if let Some(stem) = d.path.file_stem() {
            let stem = stem.to_string_lossy().to_ascii_lowercase();
            if let Some(list) = self.by_stem.get_mut(&stem) {
                list.retain(|&s| s != slot);
                if list.is_empty() {
                    self.by_stem.remove(&stem);
                }
            }
        }
        if self.by_id.get(&d.id) == Some(&slot) {
            self.by_id.remove(&d.id);
        }
        for name in d.names() {
            if let Some(list) = self.by_name.get_mut(&name) {
                list.retain(|&s| s != slot);
                if list.is_empty() {
                    self.by_name.remove(&name);
                }
            }
        }
        if let Ok(at) = self.events.binary_search(&slot) {
            self.events.remove(at);
        }
    }

    /// Rebuilds the lists without the removed slots.
    fn compact(&mut self) {
        let slots = std::mem::take(&mut self.slots);
        let mut fresh = Self {
            language: self.language,
            files: self.files,
            folders: self.folders,
            ..Self::default()
        };
        fresh.slots.reserve(slots.len() - self.removed);
        for slot in slots.into_iter().flatten() {
            fresh.insert(slot.path, slot.stamp, slot.document);
        }
        *self = fresh;
    }

    fn documents(&self) -> impl Iterator<Item = &Document> {
        self.slots
            .iter()
            .flatten()
            .filter_map(|s| s.document.as_ref())
    }

    fn document(&self, slot: usize) -> Option<&Document> {
        self.slots.get(slot)?.as_ref()?.document.as_ref()
    }

    /// True when a file was added, removed or renamed in a folder of entities or events
    /// since this index was read. Much cheaper than [`SearchIndex::changed`]: only the
    /// folders are looked at, not the files; a file edited in place, and summaries, are
    /// not seen.
    #[must_use]
    pub fn folders_changed(&self, root: &Path) -> bool {
        folders_fingerprint(root) != self.folders
    }

    /// True when a searchable file of the vault in `root` was added, removed, renamed or
    /// modified since this index was read. Lists the folders and looks at each file's
    /// size and date: no file is read.
    ///
    /// # Errors
    ///
    /// As [`SearchIndex::load`].
    pub fn changed(&self, root: &Path) -> Result<bool> {
        let mut now = Fingerprint::default();
        searchable_files(root, |folder, entry| {
            now.add(folder, entry, Stamp::of(entry));
        })?;
        Ok(now != self.files)
    }

    /// How many files are searchable.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents().count()
    }

    /// True when the vault has nothing searchable.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents().next().is_none()
    }

    /// Files that could not be read.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// The path, relative to the vault, of the entity with this id, if its category is
    /// allowed.
    #[must_use]
    pub fn path_of(&self, id: &Id, allowed: impl Fn(&Category) -> bool) -> Option<&Path> {
        let d = self.document(*self.by_id.get(id)?)?;
        allowed(&d.category).then_some(d.path.as_path())
    }

    /// The categories of the entities whose file stem is `stem`, ignoring case. Several
    /// folders may hold the same stem.
    #[must_use]
    pub fn categories_of(&self, stem: &str) -> Vec<&Category> {
        self.by_stem
            .get(&stem.to_ascii_lowercase())
            .map(|found| {
                found
                    .iter()
                    .filter_map(|&i| self.document(i))
                    .map(|d| &d.category)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The entities whose name or alias is `name`, ignoring case and accents, or whose
    /// file stem is `name`, as in a `[[link]]`, among the allowed categories.
    #[must_use]
    pub fn named(&self, name: &str, allowed: impl Fn(&Category) -> bool) -> Vec<&Path> {
        let wanted = words(name, self.language);
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut slots: Vec<usize> = self
            .by_name
            .get(&wanted.join(" "))
            .into_iter()
            .chain(self.by_stem.get(&name.trim().to_ascii_lowercase()))
            .flatten()
            .copied()
            .collect();
        slots.sort_unstable();
        slots.dedup();
        slots
            .into_iter()
            .filter_map(|slot| self.document(slot))
            .filter(|d| allowed(&d.category))
            .map(|d| d.path.as_path())
            .collect()
    }

    /// The events of the allowed categories that `keep` accepts, ordered by start.
    #[must_use]
    pub fn events(
        &self,
        allowed: impl Fn(&Category) -> bool,
        keep: impl Fn(&EventRef<'_>) -> bool,
    ) -> Vec<EventRef<'_>> {
        let mut events: Vec<EventRef<'_>> = self
            .events
            .iter()
            .filter_map(|&slot| self.document(slot))
            .filter(|d| allowed(&d.category))
            .filter_map(|d| {
                let span = d.event.as_ref()?;
                Some(EventRef {
                    path: &d.path,
                    start: &span.start,
                    end: span.end.as_ref(),
                    participants: &span.participants,
                })
            })
            .filter(|e| keep(e))
            .collect();
        events.sort_by_key(|e| sort_key(e.start));
        events
    }

    /// The files matching every word of `query`, best first, among the allowed
    /// categories only. `limit` is clamped to 1–50.
    #[must_use]
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        allowed: impl Fn(&Category) -> bool,
    ) -> Vec<Hit> {
        self.search_with(query, limit, allowed, |_| None).hits
    }

    /// As [`SearchIndex::search`], saying whether results were cut, but texts holding
    /// `[[links]]` are first passed through `rewrite` (which leaves out lines linking
    /// to hidden files) and matched as rewritten. Such a line can then neither match
    /// nor show in an excerpt.
    #[must_use]
    pub fn search_with(
        &self,
        query: &str,
        limit: usize,
        allowed: impl Fn(&Category) -> bool,
        rewrite: impl Fn(&str) -> Option<String>,
    ) -> Results {
        let tokens = words(query, self.language);
        if tokens.is_empty() {
            return Results::default();
        }
        let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let mut hits: Vec<Hit> = self
            .candidates(&tokens)
            .into_iter()
            .filter_map(|slot| self.document(slot as usize))
            .filter(|d| allowed(&d.category))
            .filter_map(|d| score(d, &tokens, &rewrite, self.language))
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
        let limit = limit.clamp(1, MAX_LIMIT);
        let more = hits.len() > limit;
        hits.truncate(limit);
        Results { hits, more }
    }

    /// The slots holding, for every token, a word that starts with it: the only ones
    /// that can match.
    fn candidates(&self, tokens: &[&str]) -> Vec<u32> {
        let mut sets: Vec<Vec<u32>> = tokens
            .iter()
            .map(|token| {
                let mut slots: Vec<u32> = self
                    .terms
                    .range::<str, _>((
                        std::ops::Bound::Included(*token),
                        std::ops::Bound::Unbounded,
                    ))
                    .take_while(|(term, _)| term.starts_with(token))
                    .flat_map(|(_, slots)| slots.iter().copied())
                    .collect();
                slots.sort_unstable();
                slots.dedup();
                slots
            })
            .collect();
        sets.sort_by_key(Vec::len);
        let mut sets = sets.into_iter();
        let Some(mut found) = sets.next() else {
            return Vec::new();
        };
        for set in sets {
            found.retain(|slot| set.binary_search(slot).is_ok());
            if found.is_empty() {
                break;
            }
        }
        found
    }
}

/// Orders days and moments together: a day counts from its first second in UTC.
fn sort_key(time: &EventTime) -> jiff::Timestamp {
    match time {
        EventTime::Day(day) => day
            .to_zoned(jiff::tz::TimeZone::UTC)
            .map_or(jiff::Timestamp::MIN, |z| z.timestamp()),
        EventTime::At(at) => at.timestamp(),
    }
}

/// A field as matched: as read, or rewritten when it links to something hidden.
fn view<'f>(
    field: &'f Field,
    rewrite: &dyn Fn(&str) -> Option<String>,
    language: Language,
) -> Cow<'f, Field> {
    if field.text.contains("[[")
        && let Some(text) = rewrite(&field.text)
    {
        return Cow::Owned(Field::new(text, language));
    }
    Cow::Borrowed(field)
}

/// A hit when every token matches somewhere; its score and best excerpt otherwise.
fn score(
    document: &Document,
    tokens: &[&str],
    rewrite: &dyn Fn(&str) -> Option<String>,
    language: Language,
) -> Option<Hit> {
    let name = view(&document.name, rewrite, language);
    let fields: Vec<(Cow<'_, Field>, (u32, u32))> = document
        .aliases
        .iter()
        .map(|f| (view(f, rewrite, language), ALIAS))
        .chain(
            document
                .facts
                .iter()
                .map(|f| (view(f, rewrite, language), FACT)),
        )
        .chain(
            document
                .relations
                .iter()
                .map(|f| (view(f, rewrite, language), RELATION)),
        )
        .chain(
            document
                .notes
                .iter()
                .map(|f| (view(f, rewrite, language), NOTES)),
        )
        .collect();
    let mut total = 0;
    let mut excerpt: Option<(u32, String)> = None;
    for token in tokens {
        let mut best: (u32, Option<&Field>) = (name.score(token, NAME), None);
        let candidates = fields.iter().map(|(f, w)| (f.as_ref(), *w));
        for (field, weights) in candidates {
            let s = field.score(token, weights);
            if s > best.0 {
                best = (s, Some(field));
            }
        }
        if best.0 == 0 {
            return None;
        }
        total += best.0;
        let text = best.1.map_or(&name.text, |f| &f.text);
        if excerpt.as_ref().is_none_or(|(s, _)| best.0 > *s) {
            excerpt = Some((best.0, shorten(text, token)));
        }
    }
    Some(Hit {
        id: document.id.clone(),
        file_type: document.file_type,
        category: document.category.clone(),
        path: document.path.clone(),
        name: document.name.text.clone(),
        excerpt: excerpt.map(|(_, e)| e).unwrap_or_default(),
        score: total,
    })
}

/// At most [`EXCERPT_CHARS`] characters of `text`, around the first word starting with
/// `token` when the text is longer.
fn shorten(text: &str, token: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= EXCERPT_CHARS {
        return text.to_owned();
    }
    let at = text
        .split_whitespace()
        .scan(0, |chars_before, word| {
            let start = *chars_before;
            *chars_before += word.chars().count() + 1;
            Some((start, word))
        })
        .find(|(_, word)| fold(word).split_whitespace().any(|w| w.starts_with(token)))
        .map_or(0, |(start, _)| start);
    let begin = at
        .saturating_sub(EXCERPT_CHARS / 3)
        .min(chars.len() - EXCERPT_CHARS);
    let end = begin + EXCERPT_CHARS;
    let mut out = String::new();
    if begin > 0 {
        out.push('…');
    }
    out.extend(&chars[begin..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}
