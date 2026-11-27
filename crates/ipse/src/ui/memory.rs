//! The whole memory, for the interface (issue #128): every kind of entity, counted and
//! listed; a search across every kind; what is going on right now.

use ipse_core::Result;
use ipse_core::rights::{Access, ENTITY_KINDS, FoundOut, Gateway, MAX_PER_KIND};
use ipse_core::search::{Matched, matched_words};
use ipse_core::vault::{Language, LinkOrText};
use serde::Serialize;

use super::api::{LinkView, PersonLine, SeriesView};

/// One kind of entity, and how many the memory holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct KindView {
    /// `person`, `place`, `organization`, `project`, `work`, `thing`, `topic`.
    kind: String,
    /// Its folder, which the interface's addresses use: `people`, `places`…
    folder: String,
    count: u32,
}

/// Something going on right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct StateView {
    /// `music`, `watching`, `reading`, `home`, `location`, or a connector's own.
    topic: String,
    /// What: a file of the memory, or a name as written.
    subject: LinkView,
    /// When it was last seen, `YYYY-MM-DD` or RFC 3339.
    last_seen: String,
    /// How many times it was seen.
    count: u32,
}

/// What `GET /api/memory` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MemoryView {
    kinds: Vec<KindView>,
    now: Vec<StateView>,
    /// Files that hold another file's id, most often copies made by hand (issue #230).
    copies: Vec<CopyView>,
}

/// A file that holds another file's id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct CopyView {
    /// The copy, relative to the memory's folder: `people/marie-dupont 1.md`.
    file: String,
    /// The file kept as the one with that id.
    original: String,
}

/// What `GET /api/memory/<folder>` answers: one page of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ListView {
    kind: String,
    items: Vec<PersonLine>,
    /// How many there are in all (for a search, those found).
    total: u32,
    more: bool,
}

/// One result of a search across the memory, with what tells it from the others
/// (issue #258).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct FoundView {
    id: String,
    /// Its file's name in the memory, for a link to it: `marie-dupont`.
    stem: String,
    /// `person`, `place`, `event`, `summary`…
    kind: String,
    name: String,
    /// Words around what matched, those that matched marked; empty when only the
    /// name matched.
    excerpt: Vec<ExcerptPart>,
    /// Where the excerpt comes from: `name`, `alias`, `fact`, `relation` or `notes`.
    matched: String,
    sensitive: bool,
    /// For an event: `YYYY-MM-DD` for a whole day, otherwise RFC 3339.
    start: Option<String>,
    /// For an event: when it ends, exclusive.
    end: Option<String>,
    /// For an event: its place.
    place: Option<LinkView>,
    /// For an event: its people.
    people: Vec<LinkView>,
    /// What it is to the owner, in the owner's words: `sœur`, `travaille chez`.
    relation: Option<String>,
    /// For a person: the organization their file links to.
    organization: Option<LinkView>,
    /// For an event that repeats: shown once, its occurrences found folded under it.
    series: Option<FoundSeriesView>,
}

/// A piece of an excerpt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ExcerptPart {
    text: String,
    /// True for a word that matched what was searched.
    hit: bool,
}

/// A repeating event, found once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct FoundSeriesView {
    series: SeriesView,
    /// The occurrences that matched, newest first.
    occurrences: Vec<OccurrenceView>,
}

/// One occurrence of a repeating event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct OccurrenceView {
    id: String,
    /// As [`FoundView::start`].
    start: String,
}

/// The results of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct FoundGroupView {
    /// `person`, `event`…
    kind: String,
    /// How many matched, a series counting once: more than `items` when cut.
    count: u32,
    /// Events newest first, the others best first.
    items: Vec<FoundView>,
}

/// What `GET /api/search?q=` answers: the results by kind, the kind with the best
/// match first. With `few=1`, a few of each kind, for results while typing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SearchView {
    groups: Vec<FoundGroupView>,
    /// How many results in all, a series counting once.
    total: u32,
}

/// Results of each kind shown while typing.
const FEW: usize = 3;

/// Things going on right now, at most.
const NOW: usize = 12;

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The memory at a glance: how many of each kind, and what is going on.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn overview(gateway: &mut Gateway) -> Result<MemoryView> {
    let owner = gateway.fresh_owner()?;
    Ok(MemoryView {
        kinds: owner
            .kinds()
            .into_iter()
            .map(|(kind, n)| KindView {
                kind: kind.as_str().to_owned(),
                folder: kind.folder().to_owned(),
                count: count(n),
            })
            .collect(),
        now: now(&owner),
        copies: copies(gateway),
    })
}

/// The copies of the memory, as the interface shows them.
fn copies(gateway: &Gateway) -> Vec<CopyView> {
    let text = |p: &std::path::Path| p.to_string_lossy().replace('\\', "/");
    ipse_core::vault::copies_among(gateway.vault().root(), gateway.index().shared_ids())
        .into_iter()
        .map(|c| CopyView {
            file: text(&c.path),
            original: text(&c.original),
        })
        .collect()
}

/// What is going on right now, most recently seen first.
pub(crate) fn now(owner: &Access<'_>) -> Vec<StateView> {
    owner
        .now(&[])
        .states
        .into_iter()
        .take(NOW)
        .map(|s| StateView {
            subject: owner.link(&LinkOrText::parse(&s.subject)).into(),
            topic: s.topic,
            last_seen: s.last_seen,
            count: u32::try_from(s.count).unwrap_or(u32::MAX),
        })
        .collect()
}

/// One page of the entities in `folder` (`people`, `places`…), or those matching
/// `query`; `None` for a folder that holds no kind of entity.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn list(
    gateway: &mut Gateway,
    folder: &str,
    query: Option<&str>,
    offset: usize,
) -> Result<Option<ListView>> {
    let Some(kind) = ENTITY_KINDS.into_iter().find(|k| k.folder() == folder) else {
        return Ok(None);
    };
    let owner = gateway.fresh_owner()?;
    let lines = owner.entities(kind, query, 100, offset);
    Ok(Some(ListView {
        kind: kind.as_str().to_owned(),
        items: lines.items.into_iter().map(PersonLine::from).collect(),
        total: count(lines.total),
        more: lines.more,
    }))
}

/// The files matching `query`, by kind (see [`SearchView`]); with `few`, a few of each
/// kind only.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn search(gateway: &mut Gateway, query: Option<&str>, few: bool) -> Result<SearchView> {
    let query = query.map(str::trim).unwrap_or_default();
    if query.is_empty() {
        return Ok(SearchView {
            groups: Vec::new(),
            total: 0,
        });
    }
    let owner = gateway.fresh_owner()?;
    let language = owner.language();
    let found = owner.find(query, if few { FEW } else { MAX_PER_KIND });
    Ok(SearchView {
        total: count(found.total),
        groups: found
            .groups
            .into_iter()
            .map(|g| FoundGroupView {
                kind: g.file_type.as_str().to_owned(),
                count: count(g.count),
                items: g
                    .results
                    .into_iter()
                    .map(|f| found_view(&owner, f, query, language))
                    .collect(),
            })
            .collect(),
    })
}

/// One result as the interface shows it.
fn found_view(owner: &Access<'_>, found: FoundOut, query: &str, language: Language) -> FoundView {
    let hit = found.hit;
    let excerpt = readable(owner, &hit.excerpt);
    let excerpt = if excerpt == hit.name {
        Vec::new()
    } else {
        parts(&excerpt, query, language)
    };
    FoundView {
        id: hit.id.to_string(),
        stem: hit
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        kind: hit.file_type.as_str().to_owned(),
        name: hit.name,
        excerpt,
        matched: match hit.matched {
            Matched::Name => "name",
            Matched::Alias => "alias",
            Matched::Fact => "fact",
            Matched::Relation => "relation",
            Matched::Notes => "notes",
        }
        .to_owned(),
        sensitive: found.sensitive,
        start: found.start,
        end: found.end,
        place: found.place.map(Into::into),
        people: found.people.into_iter().map(Into::into).collect(),
        relation: found.relation,
        organization: found.organization.map(Into::into),
        series: found.series.map(|s| FoundSeriesView {
            series: s.series.into(),
            occurrences: s
                .occurrences
                .into_iter()
                .map(|(id, start)| OccurrenceView { id, start })
                .collect(),
        }),
    }
}

/// `text` in pieces, the words matching `query` apart.
fn parts(text: &str, query: &str, language: Language) -> Vec<ExcerptPart> {
    let mut out = Vec::new();
    let mut at = 0;
    for range in matched_words(text, query, language) {
        if range.start > at {
            out.push(ExcerptPart {
                text: text[at..range.start].to_owned(),
                hit: false,
            });
        }
        out.push(ExcerptPart {
            text: text[range.clone()].to_owned(),
            hit: true,
        });
        at = range.end;
    }
    if at < text.len() || out.is_empty() {
        out.push(ExcerptPart {
            text: text[at..].to_owned(),
            hit: false,
        });
    }
    out
}

/// `text` as the person reads it: each `[[link]]` replaced by the name of what it
/// links to (or its own text), so that no file syntax shows.
fn readable(owner: &Access<'_>, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find("]]") else {
            out.push_str(after);
            return out;
        };
        let link = &after[..end + 2];
        out.push_str(&owner.link(&LinkOrText::parse(link)).name);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}
