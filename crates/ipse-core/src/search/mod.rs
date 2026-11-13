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
use std::path::{Path, PathBuf};

use crate::vault::{Category, FileType, Id, MemoryFile};
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

/// The vault's entities, ready to be searched.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    documents: Vec<Document>,
    skipped: u64,
}

impl SearchIndex {
    /// Reads every entity file of the vault in `root`. Files that cannot be read are
    /// skipped and counted.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when an entity folder exists but cannot be listed.
    pub fn load(root: &Path) -> Result<Self> {
        let mut index = Self::default();
        for file_type in FileType::ALL.into_iter().filter(|t| t.is_entity()) {
            let dir = root.join(file_type.folder());
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
                let path = entry.path();
                let is_note = path.extension().is_some_and(|e| e == "md")
                    && !entry.file_name().to_string_lossy().starts_with('.');
                if !is_note {
                    continue;
                }
                match MemoryFile::read(&path)
                    .ok()
                    .and_then(|f| document(root, &path, &f))
                {
                    Some(document) => index.documents.push(document),
                    None => index.skipped += 1,
                }
            }
        }
        Ok(index)
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
        self.documents
            .iter()
            .find(|d| allowed(&d.category) && &d.id == id)
            .map(|d| d.path.as_path())
    }

    /// The categories of the entities whose file stem is `stem`, ignoring case. Several
    /// folders may hold the same stem.
    #[must_use]
    pub fn categories_of(&self, stem: &str) -> Vec<&Category> {
        self.documents
            .iter()
            .filter(|d| {
                d.path
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(stem))
            })
            .map(|d| &d.category)
            .collect()
    }

    /// The entities whose name or alias is `name`, ignoring case and accents, among the
    /// allowed categories.
    #[must_use]
    pub fn named(&self, name: &str, allowed: impl Fn(&Category) -> bool) -> Vec<&Path> {
        let wanted: Vec<String> = fold(name).split_whitespace().map(str::to_owned).collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        self.documents
            .iter()
            .filter(|d| allowed(&d.category))
            .filter(|d| d.name.words == wanted || d.aliases.iter().any(|a| a.words == wanted))
            .map(|d| d.path.as_path())
            .collect()
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
        self.search_with(query, limit, allowed, str::to_owned)
    }

    /// As [`SearchIndex::search`], but texts holding `[[links]]` are first passed
    /// through `rewrite` (which leaves out lines linking to hidden files) and matched as
    /// rewritten. Such a line can then neither match nor show in an excerpt.
    #[must_use]
    pub fn search_with(
        &self,
        query: &str,
        limit: usize,
        allowed: impl Fn(&Category) -> bool,
        rewrite: impl Fn(&str) -> String,
    ) -> Vec<Hit> {
        let folded = fold(query);
        let tokens: Vec<&str> = folded.split_whitespace().collect();
        if tokens.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<Hit> = self
            .documents
            .iter()
            .filter(|d| allowed(&d.category))
            .filter_map(|d| score(d, &tokens, &rewrite))
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
        hits.truncate(limit.clamp(1, MAX_LIMIT));
        hits
    }
}

fn document(root: &Path, path: &Path, file: &MemoryFile) -> Option<Document> {
    let common = file.common();
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
    })
}

/// A field as matched: as loaded, or rewritten when it holds links.
fn view<'f>(field: &'f Field, rewrite: &dyn Fn(&str) -> String) -> Cow<'f, Field> {
    if field.text.contains("[[") {
        Cow::Owned(Field::new(rewrite(&field.text)))
    } else {
        Cow::Borrowed(field)
    }
}

/// A hit when every token matches somewhere; its score and best excerpt otherwise.
fn score(document: &Document, tokens: &[&str], rewrite: &dyn Fn(&str) -> String) -> Option<Hit> {
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
