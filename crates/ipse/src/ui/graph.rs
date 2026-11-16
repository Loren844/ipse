//! The graph view's API (issue #130): the files around one file, computed by the core
//! within the owner's rights, filtered by kind and category.

use ipse_core::rights::{Categories, Gateway};
use ipse_core::vault::FileType;
use ipse_core::{Error, Result};
use serde::Serialize;

/// Most files the graph view asks for.
const LIMIT: usize = 150;

/// One file of the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct NodeView {
    id: String,
    name: String,
    /// `person`, `place`… or `event`.
    kind: String,
    category: String,
    /// Closed to AIs unless the person opens its category.
    sensitive: bool,
    /// Steps from the file at the center: 0 for that file.
    depth: u8,
    /// For an event, when it starts: `YYYY-MM-DD`, or RFC 3339.
    start: Option<String>,
}

/// A link between two files of the graph, by their places in `nodes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct EdgeView {
    from: u32,
    to: u32,
    /// The relation's word; `null` for an event's place or people.
    word: Option<String>,
}

/// A category the graph can be filtered by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct CategoryChoice {
    name: String,
    /// The person's own name for a category of their own.
    label: Option<String>,
    sensitive: bool,
}

/// What `GET /api/graph` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct GraphView {
    /// The file at the center first, then one step away, then two.
    nodes: Vec<NodeView>,
    edges: Vec<EdgeView>,
    /// Files left out to keep the graph readable.
    more: u32,
    /// Every category of the memory, for the filters.
    categories: Vec<CategoryChoice>,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What `GET /api/graph` answers: around `id` (the owner's file when `None`), `depth`
/// steps away (1 or 2), the kinds and categories given (comma-separated; all when
/// left out). `None` when there is no such file.
///
/// # Errors
///
/// [`Error::Refused`] for an unknown kind; errors reading the vault.
pub(crate) fn view(
    gateway: &mut Gateway,
    id: Option<&str>,
    depth: Option<&str>,
    kinds: Option<&str>,
    categories: Option<&str>,
) -> Result<Option<GraphView>> {
    let list = |text: Option<&str>| -> Vec<String> {
        text.unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let kinds: Vec<FileType> = list(kinds)
        .iter()
        .map(|k| {
            FileType::parse(k)
                .filter(|t| t.is_entity() || *t == FileType::Event)
                .ok_or_else(|| Error::Refused(format!("`{k}` is not a kind of file.")))
        })
        .collect::<Result<_>>()?;
    let keep = categories.map(|c| list(Some(c)));
    let depth = if depth == Some("2") { 2 } else { 1 };
    let settings = gateway.settings();
    let all = Categories::of(settings);
    let sensitive = all.sensitive();
    let choices = all
        .all()
        .into_iter()
        .map(|name| CategoryChoice {
            name: name.to_owned(),
            label: settings
                .categories()
                .get(name)
                .and_then(|c| c.label.clone()),
            sensitive: sensitive.contains(&name),
        })
        .collect();
    let owner = gateway.fresh_owner()?;
    let center = match id {
        Some(id) => id.to_owned(),
        None => match owner.about_me() {
            Some(me) => me.id,
            None => return Ok(None),
        },
    };
    let Some(graph) = owner.graph(&center, depth, &kinds, keep.as_deref(), LIMIT) else {
        return Ok(None);
    };
    Ok(Some(GraphView {
        nodes: graph
            .nodes
            .into_iter()
            .map(|n| NodeView {
                id: n.id,
                name: n.name,
                kind: n.file_type,
                category: n.category,
                sensitive: n.sensitive,
                depth: n.depth,
                start: n.start,
            })
            .collect(),
        edges: graph
            .edges
            .into_iter()
            .map(|e| EdgeView {
                from: count(e.from),
                to: count(e.to),
                word: e.word,
            })
            .collect(),
        more: count(graph.more),
        categories: choices,
    }))
}
