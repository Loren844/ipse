//! The graph view's API (issue #213): one file and the files directly linked to it,
//! computed by the core within the owner's rights. Events are not drawn: two files in
//! the same events are linked by how many there are. The view filters by kind itself.

use ipse_core::Result;
use ipse_core::rights::{Gateway, NodeOut};
use jiff::{SignedDuration, Timestamp};
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

impl From<NodeOut> for NodeView {
    fn from(n: NodeOut) -> Self {
        Self {
            id: n.id,
            name: n.name,
            kind: n.file_type,
            category: n.category,
            sensitive: n.sensitive,
            start: n.start,
        }
    }
}

/// A file linked to the center.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct NeighbourView {
    file: NodeView,
    /// The relation's words as the center's file writes them: "mère".
    words: Vec<String>,
    /// The relation's words as its own file writes them about the center.
    back: Vec<String>,
    /// How many events of the period both are in.
    together: u32,
    /// When the last of them started.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    last: Option<String>,
}

/// What `GET /api/graph` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct GraphView {
    center: NodeView,
    /// True when the center is the owner's own file.
    mine: bool,
    /// The id of the owner's own file, when there is one: shown as "you".
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    owner: Option<String>,
    /// The most strongly linked first.
    neighbours: Vec<NeighbourView>,
}

/// What `GET /api/graph?id=…&period=year` answers: the file `id` (the owner's own file
/// without one) and its neighbours; `period=year` counts only the events of the last
/// twelve months. `None` when there is no such file.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn view(
    gateway: &mut Gateway,
    id: Option<&str>,
    period: Option<&str>,
) -> Result<Option<GraphView>> {
    let now = Timestamp::now();
    let from = if period == Some("year") {
        now.checked_sub(SignedDuration::from_hours(24 * 365))
            .unwrap_or(Timestamp::MIN)
    } else {
        Timestamp::MIN
    };
    let owner = gateway.fresh_owner()?;
    Ok(owner
        .around(id.filter(|i| !i.is_empty()), (from, now))
        .map(|found| GraphView {
            center: found.center.into(),
            mine: found.mine,
            owner: found.owner,
            neighbours: found
                .neighbours
                .into_iter()
                .map(|n| NeighbourView {
                    file: n.node.into(),
                    words: n.words,
                    back: n.back,
                    together: n.together,
                    last: n.last,
                })
                .collect(),
        }))
}
