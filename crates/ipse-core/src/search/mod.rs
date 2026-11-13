//! Searching the vault's entities by words.
//!
//! This first version reads the entity files once and searches them in memory, which
//! is enough for vaults of a few thousand people and places. A persistent index comes
//! later (#16) behind the same interface.
//!
//! Rights are applied before anything else: a file outside the allowed categories is
//! never looked at by the scoring, so it can neither be returned nor change the order
//! of the results.

mod fold;

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::vault::{Category, EventFields, EventTime, FileType, Id, MemoryFile};
use crate::{Error, Result};

pub use fold::fold;

/// Results returned when no limit is given.
pub const DEFAULT_LIMIT: usize = 10;

/// Most results one search returns.
pub const MAX_LIMIT: usize = 50;

/// Longest excerpt, in characters.
const EXCERPT_CHARS: usize = 160;

/// Weights of a match, by field and by kind: a whole word, or the start of one.
const NAME: (u32, u32) = (100, 80);
const ALIAS: (u32, u32) = (90, 70);
const FACT: (u32, u32) = (30, 20);
const NOTES: (u32, u32) = (10, 5);

/// One searchable entity.
#[derive(Debug, Clone)]
struct Document {
    id: Id,
    file_type: FileType,
    category: Category,
    path: PathBuf,
    name: Field,
    aliases: Vec<Field>,
    facts: Vec<Field>,
    notes: Option<Field>,
    /// For an event: when, and who.
    event: Option<EventSpan>,
}

/// When an event happens and the people it links to.
#[derive(Debug, Clone)]
struct EventSpan {
    start: EventTime,
    end: Option<EventTime>,
    /// File stems of the participants that are links.
    participants: Vec<String>,
}

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

/// A text and its folded words, computed once when the vault is read.
#[derive(Debug, Clone)]
struct Field {
    text: String,
    words: Vec<String>,
}

impl Field {
    fn new(text: String) -> Self {
        let words = fold(&text).split_whitespace().map(str::to_owned).collect();
        Self { text, words }
    }

    /// How well `token` matches: a whole word, the start of a word, or not at all.
    fn score(&self, token: &str, (whole, start): (u32, u32)) -> u32 {
        let mut best = 0;
        for word in &self.words {
            if word == token {
                return whole;
            }
            if word.starts_with(token) {
                best = start;
            }
        }
        best
    }
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

/// The vault's entities, ready to be searched.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    documents: Vec<Document>,
    /// Documents by file stem in ASCII lower case, to follow links: several folders may
    /// hold the same stem.
    by_stem: HashMap<String, Vec<usize>>,
    /// Documents by id.
    by_id: HashMap<Id, usize>,
    skipped: u64,
    files: Fingerprint,
    folders: u64,
}

/// A cheap summary of the entity files' names, sizes and modification times, to tell
/// whether the vault changed since it was read without reading it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Fingerprint {
    count: u64,
    sum: u64,
}

impl Fingerprint {
    /// Adds one file. Files are summed, so the order they are listed in does not matter.
    fn add(&mut self, folder: &str, entry: &std::fs::DirEntry) {
        let mut hasher = DefaultHasher::new();
        folder.hash(&mut hasher);
        entry.file_name().hash(&mut hasher);
        if let Ok(meta) = entry.metadata() {
            meta.len().hash(&mut hasher);
            meta.modified()
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .hash(&mut hasher);
        }
        self.count += 1;
        self.sum = self.sum.wrapping_add(hasher.finish());
    }
}

/// A summary of the folders' modification times: adding, removing or renaming a file
/// (an atomic write included) changes it; editing a file in place does not.
fn folders_fingerprint(root: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut folders = folders(root);
    folders.push(FileType::Event.folder().to_owned());
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

/// The folders holding entities and events: `people`, `events/2026`…
fn folders(root: &Path) -> Vec<String> {
    let mut folders: Vec<String> = FileType::ALL
        .into_iter()
        .filter(|t| t.is_entity())
        .map(|t| t.folder().to_owned())
        .collect();
    // Events are one folder per year.
    if let Ok(years) = std::fs::read_dir(root.join(FileType::Event.folder())) {
        let mut years: Vec<String> = years
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .map(|name| format!("{}/{name}", FileType::Event.folder()))
            .collect();
        years.sort();
        folders.extend(years);
    }
    folders
}

/// The entity and event files of the vault in `root`, by folder: `people`,
/// `events/2026`… A missing folder has none.
fn entity_files(root: &Path, mut each: impl FnMut(&str, &std::fs::DirEntry)) -> Result<()> {
    for folder in &folders(root) {
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

impl SearchIndex {
    /// Reads every entity file of the vault in `root`. Files that cannot be read are
    /// skipped and counted.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when an entity folder exists but cannot be listed.
    pub fn load(root: &Path) -> Result<Self> {
        let mut index = Self {
            folders: folders_fingerprint(root),
            ..Self::default()
        };
        entity_files(root, |folder, entry| {
            // Noted before reading, so that a change made while reading shows next time.
            index.files.add(folder, entry);
            let path = entry.path();
            match MemoryFile::read(&path)
                .ok()
                .and_then(|f| document(root, &path, &f))
            {
                Some(document) => index.documents.push(document),
                None => index.skipped += 1,
            }
        })?;
        index.by_stem.reserve(index.documents.len());
        index.by_id.reserve(index.documents.len());
        for (i, d) in index.documents.iter().enumerate() {
            if let Some(stem) = d.path.file_stem() {
                let stem = stem.to_string_lossy().to_ascii_lowercase();
                index.by_stem.entry(stem).or_default().push(i);
            }
            index.by_id.insert(d.id.clone(), i);
        }
        Ok(index)
    }

    /// True when a file was added, removed or renamed in an entity or event folder since
    /// this index was read. Much cheaper than [`SearchIndex::changed`]: only the
    /// folders are looked at, not the files; a file edited in place is not seen.
    #[must_use]
    pub fn folders_changed(&self, root: &Path) -> bool {
        folders_fingerprint(root) != self.folders
    }

    /// True when an entity or event file of the vault in `root` was added, removed,
    /// renamed or modified since this index was read. Lists the folders and looks at
    /// each file's size and date: no file is read.
    ///
    /// # Errors
    ///
    /// As [`SearchIndex::load`].
    pub fn changed(&self, root: &Path) -> Result<bool> {
        let mut now = Fingerprint::default();
        entity_files(root, |folder, entry| now.add(folder, entry))?;
        Ok(now != self.files)
    }

    /// How many entities are searchable.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// True when the vault has no searchable entity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Files that could not be read when loading.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// The path, relative to the vault, of the entity with this id, if its category is
    /// allowed.
    #[must_use]
    pub fn path_of(&self, id: &Id, allowed: impl Fn(&Category) -> bool) -> Option<&Path> {
        let d = &self.documents[*self.by_id.get(id)?];
        allowed(&d.category).then_some(d.path.as_path())
    }

    /// The categories of the entities whose file stem is `stem`, ignoring case. Several
    /// folders may hold the same stem.
    #[must_use]
    pub fn categories_of(&self, stem: &str) -> Vec<&Category> {
        self.by_stem
            .get(&stem.to_ascii_lowercase())
            .map(|found| found.iter().map(|&i| &self.documents[i].category).collect())
            .unwrap_or_default()
    }

    /// The entities whose name or alias is `name`, ignoring case and accents, or whose
    /// file stem is `name`, as in a `[[link]]`, among the allowed categories.
    #[must_use]
    pub fn named(&self, name: &str, allowed: impl Fn(&Category) -> bool) -> Vec<&Path> {
        let wanted: Vec<String> = fold(name).split_whitespace().map(str::to_owned).collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let stem = name.trim();
        self.documents
            .iter()
            .filter(|d| allowed(&d.category))
            .filter(|d| {
                d.name.words == wanted
                    || d.aliases.iter().any(|a| a.words == wanted)
                    || d.path
                        .file_stem()
                        .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(stem))
            })
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
            .documents
            .iter()
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

    /// The entities matching every word of `query`, best first, among the allowed
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

    /// As [`SearchIndex::search`], saying whether results were cut, but texts holding `[[links]]` are first passed
    /// through `rewrite` (which leaves out lines linking to hidden files) and matched as
    /// rewritten. Such a line can then neither match nor show in an excerpt.
    #[must_use]
    pub fn search_with(
        &self,
        query: &str,
        limit: usize,
        allowed: impl Fn(&Category) -> bool,
        rewrite: impl Fn(&str) -> Option<String>,
    ) -> Results {
        let folded = fold(query);
        let tokens: Vec<&str> = folded.split_whitespace().collect();
        if tokens.is_empty() {
            return Results::default();
        }
        let mut hits: Vec<Hit> = self
            .documents
            .iter()
            .filter(|d| allowed(&d.category))
            .filter_map(|d| score(d, &tokens, &rewrite))
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
        let limit = limit.clamp(1, MAX_LIMIT);
        let more = hits.len() > limit;
        hits.truncate(limit);
        Results { hits, more }
    }
}

fn document(root: &Path, path: &Path, file: &MemoryFile) -> Option<Document> {
    let common = file.common();
    if let Some(event) = file.event() {
        return Some(event_document(root, path, file, event));
    }
    let entity = file.entity()?;
    let sections = file.sections();
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    Some(Document {
        id: common.id.clone(),
        file_type: common.file_type,
        category: common.category.clone(),
        path: path.strip_prefix(root).unwrap_or(path).to_owned(),
        name: Field::new(entity.name.clone().unwrap_or(stem)),
        aliases: entity.aliases.iter().cloned().map(Field::new).collect(),
        facts: sections
            .facts
            .into_iter()
            .map(|f| Field::new(f.text))
            .collect(),
        notes: sections.notes.map(Field::new),
        event: None,
    })
}

/// An event as a document: its title as name; its place and participants searchable
/// like facts, so that "dinner Julie" finds it.
fn event_document(root: &Path, path: &Path, file: &MemoryFile, event: &EventFields) -> Document {
    let common = file.common();
    let facts = event
        .place
        .iter()
        .chain(&event.participants)
        .map(|p| Field::new(p.to_string()))
        .collect();
    Document {
        id: common.id.clone(),
        file_type: common.file_type,
        category: common.category.clone(),
        path: path.strip_prefix(root).unwrap_or(path).to_owned(),
        name: Field::new(event.title.clone()),
        aliases: Vec::new(),
        facts,
        notes: file.sections().notes.map(Field::new),
        event: Some(EventSpan {
            start: event.start,
            end: event.end,
            participants: event
                .participants
                .iter()
                .filter_map(|p| p.link().map(|l| l.target.clone()))
                .collect(),
        }),
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

/// A field as matched: as loaded, or rewritten when it links to something hidden.
fn view<'f>(field: &'f Field, rewrite: &dyn Fn(&str) -> Option<String>) -> Cow<'f, Field> {
    if field.text.contains("[[")
        && let Some(text) = rewrite(&field.text)
    {
        return Cow::Owned(Field::new(text));
    }
    Cow::Borrowed(field)
}

/// A hit when every token matches somewhere; its score and best excerpt otherwise.
fn score(
    document: &Document,
    tokens: &[&str],
    rewrite: &dyn Fn(&str) -> Option<String>,
) -> Option<Hit> {
    let name = view(&document.name, rewrite);
    let fields: Vec<(Cow<'_, Field>, (u32, u32))> = document
        .aliases
        .iter()
        .map(|f| (view(f, rewrite), ALIAS))
        .chain(document.facts.iter().map(|f| (view(f, rewrite), FACT)))
        .chain(document.notes.iter().map(|f| (view(f, rewrite), NOTES)))
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
