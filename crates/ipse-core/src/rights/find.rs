//! A search as the person reads it (issue #258): results grouped by kind, each saying
//! what it is, the occurrences of a repeating event as one result.

use std::collections::HashMap;

use super::Access;
use super::browse::LinkOut;
use crate::search::{Hit, Series, timestamp_of};
use crate::vault::{FileType, LinkOrText, WikiLink};

/// Most results one group returns in full; the group's count says how many there are.
pub const MAX_PER_KIND: usize = 50;

/// The results of one search, by kind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Found {
    /// The kinds that matched, the one with the best match first. Within a kind,
    /// results that match as well come with what the owner's file names first.
    pub groups: Vec<FoundGroup>,
    /// How many results in all, a series counting once.
    pub total: usize,
}

/// The results of one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundGroup {
    /// What they are.
    pub file_type: FileType,
    /// How many matched, a series counting once; more than `results` holds when cut.
    pub count: usize,
    /// Events newest first, the others best first.
    pub results: Vec<FoundOut>,
}

/// One result, with what tells it from the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundOut {
    /// What was found; for a series, its most recent occurrence found.
    pub hit: Hit,
    /// Closed to AIs unless the person opens its category.
    pub sensitive: bool,
    /// For an event: when it starts, `YYYY-MM-DD` for a whole day, otherwise RFC 3339
    /// in the vault's time zone.
    pub start: Option<String>,
    /// For an event: when it ends, exclusive.
    pub end: Option<String>,
    /// For an event: its place.
    pub place: Option<LinkOut>,
    /// For an event: its people.
    pub people: Vec<LinkOut>,
    /// What it is to the owner, in the owner's words: `sister`, `works at`.
    pub relation: Option<String>,
    /// For a person: the organization their file links to.
    pub organization: Option<LinkOut>,
    /// For an event that repeats: its series, and its occurrences found.
    pub series: Option<SeriesFound>,
}

/// A repeating event, found once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesFound {
    /// The series, counted in the whole memory this client may read.
    pub series: Series,
    /// The occurrences that matched, newest first: id and start as in
    /// [`FoundOut::start`].
    pub occurrences: Vec<(String, String)>,
}

/// One result before it is told apart: a hit, or the occurrences of one series.
struct Row {
    hit: Hit,
    /// For an event: its start, to order events.
    start: Option<jiff::Timestamp>,
    /// The occurrences of its series that matched, newest first, itself included.
    occurrences: Vec<(Hit, jiff::Timestamp)>,
    series: Option<Series>,
}

impl Access<'_> {
    /// The vault's language, which searches match words in.
    #[must_use]
    pub fn language(&self) -> crate::vault::Language {
        self.gateway.index.language()
    }

    /// The files matching `query` this client may read, grouped by kind (see
    /// [`Found`]). `per_kind` cuts each group to that many, at most
    /// [`MAX_PER_KIND`]; the counts stay whole.
    #[must_use]
    pub fn find(&self, query: &str, per_kind: usize) -> Found {
        let index = &self.gateway.index;
        let hits: Vec<Hit> = index
            .search_all(
                query,
                |c| self.reads(c.as_str()),
                |text| self.hides(text).then(|| self.strip(text)),
            )
            .into_iter()
            .filter(|h| h.file_type != FileType::StateLog)
            .collect();
        if hits.is_empty() {
            return Found::default();
        }
        let series = if hits.iter().any(|h| h.file_type == FileType::Event) {
            index.series(|c| self.reads(c.as_str()))
        } else {
            HashMap::new()
        };
        let per_kind = per_kind.clamp(1, MAX_PER_KIND);

        // Kinds in the order of their best hit; within one, hits best first.
        let mut kinds: Vec<(FileType, Vec<Row>)> = Vec::new();
        let mut series_rows: HashMap<&str, (usize, usize)> = HashMap::new();
        for hit in hits {
            let at = if let Some(at) = kinds.iter().position(|(t, _)| *t == hit.file_type) {
                at
            } else {
                kinds.push((hit.file_type, Vec::new()));
                kinds.len() - 1
            };
            let start = index
                .context(&hit.id)
                .and_then(|c| c.start)
                .map(timestamp_of);
            let found = series.get(&hit.id);
            if let (Some(s), Some(start)) = (found, start) {
                if let Some(&(k, r)) = series_rows.get(s.key.as_str()) {
                    kinds[k].1[r].occurrences.push((hit, start));
                    continue;
                }
                series_rows.insert(s.key.as_str(), (at, kinds[at].1.len()));
            }
            kinds[at].1.push(Row {
                occurrences: match (found, start) {
                    (Some(_), Some(start)) => vec![(hit.clone(), start)],
                    _ => Vec::new(),
                },
                hit,
                start,
                series: found.cloned(),
            });
        }

        let owner = self.owner_relations();
        let mut found = Found::default();
        for (file_type, mut rows) in kinds {
            // Among results that match as well, what the owner named first: their
            // sister before a stranger with the same first name.
            if !owner.is_empty() {
                rows.sort_by_key(|row| {
                    let stem = row
                        .hit
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_ascii_lowercase());
                    let named = stem.is_some_and(|s| owner.contains_key(&s));
                    (std::cmp::Reverse(row.hit.score), !named)
                });
            }
            for row in &mut rows {
                if row.occurrences.len() > 1 {
                    row.occurrences.sort_by_key(|o| std::cmp::Reverse(o.1));
                    let (newest, start) = row.occurrences[0].clone();
                    row.hit = newest;
                    row.start = Some(start);
                }
            }
            if file_type == FileType::Event {
                rows.sort_by_key(|row| std::cmp::Reverse(row.start));
            }
            found.total += rows.len();
            let count = rows.len();
            let results = rows
                .into_iter()
                .take(per_kind)
                .map(|row| self.told_apart(row, &owner))
                .collect();
            found.groups.push(FoundGroup {
                file_type,
                count,
                results,
            });
        }
        found
    }

    /// The owner's relations that link to a file, by the file's stem: its word. None
    /// when this client may not read the owner's file.
    fn owner_relations(&self) -> HashMap<String, String> {
        let vault = &self.gateway.vault;
        let owner = vault.owner_file();
        let owner = owner.strip_prefix(vault.root()).unwrap_or(owner);
        let index = &self.gateway.index;
        let path = owner.to_string_lossy().replace('\\', "/");
        if index.at(&path, |c| self.reads(c.as_str())).is_none() {
            return HashMap::new();
        }
        let mut words = HashMap::new();
        for (word, target) in index.relations_at(owner) {
            words
                .entry(target.to_owned())
                .or_insert_with(|| word.to_owned());
        }
        words
    }

    /// A row with what tells it apart: an event's time, place and people, an entity's
    /// relation to the owner and organization.
    fn told_apart(&self, row: Row, owner: &HashMap<String, String>) -> FoundOut {
        let index = &self.gateway.index;
        let reads = |c: &crate::vault::Category| self.reads(c.as_str());
        let context = index.context(&row.hit.id);
        let stem = row
            .hit
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let shown = |text: &str| {
            let value = LinkOrText::parse(text);
            self.visible(&value).map(|_| self.link(&value))
        };
        let (mut start, mut end, mut place, mut people, mut organization) =
            (None, None, None, Vec::new(), None);
        if let Some(c) = &context {
            start = c.start.map(|t| self.time_text(t));
            end = c.end.map(|t| self.time_text(t));
            place = c.place.and_then(shown);
            people = c.people.iter().filter_map(|p| shown(p)).collect();
            if row.hit.file_type == FileType::Person {
                organization = c
                    .relations
                    .iter()
                    .find(|(_, target)| {
                        index
                            .linked(target, reads)
                            .is_some_and(|l| l.path.starts_with(FileType::Organization.folder()))
                    })
                    .map(|(_, target)| {
                        self.link(&LinkOrText::Link(WikiLink {
                            target: (*target).to_owned(),
                            display: None,
                        }))
                    });
            }
        }
        let series = row.series.map(|series| SeriesFound {
            series,
            occurrences: row
                .occurrences
                .iter()
                .filter_map(|(hit, _)| {
                    let start = index.context(&hit.id)?.start?;
                    Some((hit.id.to_string(), self.time_text(start)))
                })
                .collect(),
        });
        self.returned(row.hit.category.as_str());
        FoundOut {
            sensitive: self.sensitive(row.hit.category.as_str()),
            relation: owner.get(&stem).cloned(),
            hit: row.hit,
            start,
            end,
            place,
            people,
            organization,
            series,
        }
    }
}
