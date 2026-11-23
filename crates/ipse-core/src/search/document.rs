//! What the index keeps of one file, and the form it is saved in.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::fold::words;
use crate::vault::body::{Section, section_text};
use crate::vault::{
    Category, EventFields, EventKind, EventTime, FileType, Id, Language, LinkOrText, MemoryFile,
    WikiLink,
};

/// A text and its words as matched, computed once when the file is read.
#[derive(Debug, Clone)]
pub(super) struct Field {
    pub(super) text: String,
    pub(super) words: Vec<String>,
}

impl Field {
    pub(super) fn new(text: String, language: Language) -> Self {
        let words = words(&text, language);
        Self { text, words }
    }

    /// How well `token` matches: a whole word, the start of a word, or not at all.
    pub(super) fn score(&self, token: &str, (whole, start): (u32, u32)) -> u32 {
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

/// One searchable file: an entity, an event or a summary.
#[derive(Debug, Clone)]
pub(super) struct Document {
    pub(super) id: Id,
    pub(super) file_type: FileType,
    pub(super) category: Category,
    /// Relative to the vault's folder.
    pub(super) path: PathBuf,
    pub(super) name: Field,
    pub(super) aliases: Vec<Field>,
    /// Facts; for an event, its place and participants.
    pub(super) facts: Vec<Field>,
    /// Relations, as `word : [[target]]`.
    pub(super) relations: Vec<Field>,
    /// Notes; for a summary, its text.
    pub(super) notes: Option<Field>,
    /// For an event: when, and who.
    pub(super) event: Option<EventSpan>,
    /// The files it links to, read once from its relations, or an event's place and
    /// people: for the graph.
    pub(super) links: Vec<DocLink>,
}

/// One link of a document to another file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DocLink {
    /// The linked file's stem, in lower case.
    pub(super) target: String,
    /// The relation's word; `None` for an event's place or people.
    pub(super) word: Option<String>,
}

/// The links of a document: `word : [[target]]` relations, and an event's place and
/// people, which its facts hold as `[[target]]`.
fn links_of(relations: &[String], facts: &[String], event: bool) -> Vec<DocLink> {
    let mut links = Vec::with_capacity(relations.len() + if event { facts.len() } else { 0 });
    for relation in relations {
        let Some((word, link)) = relation.split_once(" : ") else {
            continue;
        };
        if let Some(link) = WikiLink::parse(link.trim()) {
            links.push(DocLink {
                target: link.target.to_ascii_lowercase(),
                word: Some(word.trim().to_owned()),
            });
        }
    }
    if event {
        for fact in facts {
            if let Some(link) = WikiLink::parse(fact.trim()) {
                links.push(DocLink {
                    target: link.target.to_ascii_lowercase(),
                    word: None,
                });
            }
        }
    }
    links
}

/// When an event happens and the people it links to.
#[derive(Debug, Clone)]
pub(super) struct EventSpan {
    pub(super) start: EventTime,
    pub(super) end: Option<EventTime>,
    /// File stems of the participants that are links.
    pub(super) participants: Vec<String>,
    /// Its kind, when someone said: `meal`, `trip`…
    pub(super) kind: Option<EventKind>,
    /// The file stem of its place, when it is a link.
    pub(super) place: Option<String>,
    /// For an occurrence of a repeating calendar event: a short hash of the calendar's
    /// `UID`, the same for every occurrence.
    pub(super) series: Option<String>,
}

impl Document {
    /// The name and other names as matched, words joined by a space, once each.
    pub(super) fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::iter::once(&self.name)
            .chain(&self.aliases)
            .map(|f| f.words.join(" "))
            .filter(|n| !n.is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Every word of the document, once each, for the index's lists of words.
    pub(super) fn terms(&self) -> Vec<&str> {
        let mut terms: Vec<&str> = std::iter::once(&self.name)
            .chain(&self.aliases)
            .chain(&self.facts)
            .chain(&self.relations)
            .chain(&self.notes)
            .flat_map(|f| f.words.iter().map(String::as_str))
            .collect();
        terms.sort_unstable();
        terms.dedup();
        terms
    }

    /// The document of `file` at `path`; `None` for a file Ipse does not search (a
    /// state log, a file without name).
    pub(super) fn of(
        root: &Path,
        path: &Path,
        file: &MemoryFile,
        language: Language,
    ) -> Option<Self> {
        let stored = Stored::of(root, path, file)?;
        Self::from_stored(stored, language)
    }

    /// A document from its saved form; `None` when it no longer reads (a format from
    /// another version).
    pub(super) fn from_stored(s: Stored, language: Language) -> Option<Self> {
        let field = |t: String| Field::new(t, language);
        let event = match s.event {
            None => None,
            Some(e) => Some(EventSpan {
                start: EventTime::parse(&e.start)?,
                end: match e.end {
                    None => None,
                    Some(end) => Some(EventTime::parse(&end)?),
                },
                participants: e.participants,
                kind: e.kind.as_deref().and_then(EventKind::parse),
                place: e.place,
                series: e.series,
            }),
        };
        let links = links_of(&s.relations, &s.facts, event.is_some());
        Some(Self {
            links,
            id: Id::parse(&s.id)?,
            file_type: FileType::parse(&s.file_type)?,
            category: Category::new(&s.category)?,
            path: PathBuf::from(s.path),
            name: field(s.name),
            aliases: s.aliases.into_iter().map(field).collect(),
            facts: s.facts.into_iter().map(field).collect(),
            relations: s.relations.into_iter().map(field).collect(),
            notes: s.notes.map(field),
            event,
        })
    }

    /// The saved form of this document.
    pub(super) fn to_stored(&self) -> Stored {
        let text = |f: &Field| f.text.clone();
        Stored {
            id: self.id.to_string(),
            file_type: self.file_type.as_str().to_owned(),
            category: self.category.as_str().to_owned(),
            path: self.path.to_string_lossy().replace('\\', "/"),
            name: self.name.text.clone(),
            aliases: self.aliases.iter().map(text).collect(),
            facts: self.facts.iter().map(text).collect(),
            relations: self.relations.iter().map(text).collect(),
            notes: self.notes.as_ref().map(text),
            event: self.event.as_ref().map(|e| StoredEvent {
                start: e.start.to_string(),
                end: e.end.as_ref().map(ToString::to_string),
                participants: e.participants.clone(),
                kind: e.kind.map(|k| k.as_str().to_owned()),
                place: e.place.clone(),
                series: e.series.clone(),
            }),
        }
    }
}

/// A document as saved in `.ipse/index/`: its texts, not their words, which are
/// computed again when the index is opened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Stored {
    id: String,
    file_type: String,
    category: String,
    path: String,
    name: String,
    aliases: Vec<String>,
    facts: Vec<String>,
    relations: Vec<String>,
    notes: Option<String>,
    event: Option<StoredEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredEvent {
    start: String,
    end: Option<String>,
    participants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    place: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    series: Option<String>,
}

impl Stored {
    /// What the index keeps of `file`.
    fn of(root: &Path, path: &Path, file: &MemoryFile) -> Option<Self> {
        let common = file.common();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path.file_stem()?.to_string_lossy().into_owned();
        let base = |name: String| Self {
            id: common.id.to_string(),
            file_type: common.file_type.as_str().to_owned(),
            category: common.category.as_str().to_owned(),
            path: relative.clone(),
            name,
            aliases: Vec::new(),
            facts: Vec::new(),
            relations: Vec::new(),
            notes: None,
            event: None,
        };
        if let Some(event) = file.event() {
            return Some(Self::event(base(event.title.clone()), file, event));
        }
        if common.file_type == FileType::Summary {
            return Some(Self {
                notes: section_text(file.body(), Section::Summary),
                ..base(stem)
            });
        }
        let entity = file.entity()?;
        let sections = file.sections();
        Some(Self {
            aliases: entity.aliases.clone(),
            facts: sections.facts.into_iter().map(|f| f.text).collect(),
            relations: sections
                .relations
                .iter()
                .map(|r| format!("{} : {}", r.word, r.link))
                .collect(),
            notes: sections.notes,
            ..base(entity.name.clone().unwrap_or(stem))
        })
    }

    /// An event: its title as name, its place and participants searchable like facts,
    /// so that "dîner Julie" finds it.
    fn event(base: Self, file: &MemoryFile, event: &EventFields) -> Self {
        Self {
            facts: event
                .place
                .iter()
                .chain(&event.participants)
                .map(ToString::to_string)
                .collect(),
            notes: file.sections().notes,
            event: Some(StoredEvent {
                start: event.start.to_string(),
                end: event.end.as_ref().map(ToString::to_string),
                participants: event
                    .participants
                    .iter()
                    .filter_map(|p| p.link().map(|l| l.target.clone()))
                    .collect(),
                kind: event.kind.map(|k| k.as_str().to_owned()),
                place: event
                    .place
                    .as_ref()
                    .and_then(LinkOrText::link)
                    .map(|l| l.target.clone()),
                series: series_of(&file.common().sources),
            }),
            ..base
        }
    }
}

/// The series of an occurrence imported from a calendar: its source reads
/// `calendar:<UID>#<when>`; a short hash of the `UID`, so that the index keeps no
/// identifier from elsewhere.
fn series_of(sources: &[String]) -> Option<String> {
    sources.iter().find_map(|source| {
        let (connector, id) = source.split_once(':')?;
        if connector != "calendar" && !connector.starts_with("calendar-") {
            return None;
        }
        let (uid, _) = id.rsplit_once('#')?;
        let hash = blake3::hash(uid.as_bytes()).to_hex();
        Some(hash[..16].to_owned())
    })
}
