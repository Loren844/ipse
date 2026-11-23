//! The graph view's API (issue #174): the whole memory as one graph, computed by the
//! core within the owner's rights. The view filters it by kind and category itself, so
//! that each file keeps its place when a filter changes.

use ipse_core::Result;
use ipse_core::rights::{Categories, Gateway};
use serde::Serialize;

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
    /// For an event, when it starts: `YYYY-MM-DD`, or RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    start: Option<String>,
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
    /// The owner's own file first when `centered`, then entities by name, then events,
    /// the most recent first.
    nodes: Vec<NodeView>,
    /// The links, two numbers each: the places in `nodes` of the two files, the smaller
    /// first. Flat, to keep tens of thousands of them small.
    edges: Vec<u32>,
    /// True when the first node is the owner's own file.
    centered: bool,
    /// Every category of the memory, for the filters.
    categories: Vec<CategoryChoice>,
}

/// What `GET /api/graph` answers: every entity and event of the memory, and every link
/// between two of them.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn view(gateway: &mut Gateway) -> Result<GraphView> {
    let settings = gateway.settings();
    let all = Categories::of(settings);
    let sensitive = all.sensitive();
    let categories = all
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
    let graph = owner.whole_graph(&[], None);
    Ok(GraphView {
        nodes: graph
            .nodes
            .into_iter()
            .map(|n| NodeView {
                id: n.id,
                name: n.name,
                kind: n.file_type,
                category: n.category,
                sensitive: n.sensitive,
                start: n.start,
            })
            .collect(),
        edges: graph.edges.into_iter().flat_map(|(a, b)| [a, b]).collect(),
        centered: graph.centered,
        categories,
    })
}
