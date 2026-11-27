//! The whole memory, for the interface (issue #128): every kind of entity, counted and
//! listed; a search across every kind; what is going on right now.

use ipse_core::Result;
use ipse_core::rights::{Access, ENTITY_KINDS, Gateway};
use ipse_core::vault::{FileType, LinkOrText};
use serde::Serialize;

use super::api::{LinkView, PersonLine};

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

/// One result of a search across the memory.
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
    /// Words around what matched.
    excerpt: String,
    sensitive: bool,
}

/// What `GET /api/search?q=` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SearchView {
    items: Vec<FoundView>,
    more: bool,
}

/// Results of a search across the memory, at most.
const FOUND: usize = 50;

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

/// The files matching `query`, of every kind, best first.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn search(gateway: &mut Gateway, query: Option<&str>) -> Result<SearchView> {
    let query = query.map(str::trim).unwrap_or_default();
    if query.is_empty() {
        return Ok(SearchView {
            items: Vec::new(),
            more: false,
        });
    }
    let owner = gateway.fresh_owner()?;
    let found = owner.search(query, FOUND);
    Ok(SearchView {
        items: found
            .hits
            .into_iter()
            .filter(|h| h.file_type != FileType::StateLog)
            .map(|h| {
                let excerpt = readable(&owner, &h.excerpt);
                FoundView {
                    sensitive: owner.sensitive(h.category.as_str()),
                    id: h.id.to_string(),
                    stem: h
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    kind: h.file_type.as_str().to_owned(),
                    excerpt: if excerpt == h.name {
                        String::new()
                    } else {
                        excerpt
                    },
                    name: h.name,
                }
            })
            .collect(),
        more: found.more,
    })
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
