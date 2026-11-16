//! The review inbox's API: the proposals waiting for the person, one in full with what
//! it would change, and the person's decisions (decision 0005). Decisions go through
//! [`Proposals`], the same path as `ipse proposals`.

use ipse_core::proposals::{
    Accepted, Author, DiffItem, FileDiff, Preview, Proposal, Proposals, Status, What,
};
use ipse_core::rights::{Access, Clients, Gateway};
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Decided proposals listed for undoing or reading again, at most.
const RECENT: usize = 20;

/// Who made a proposal, for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AuthorView {
    /// `ai`, `source` or `ipse`.
    kind: String,
    /// The AI's name as the person gave it, or the source's instance (`contacts`).
    name: String,
}

/// One proposal in the inbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ProposalLine {
    id: String,
    by: AuthorView,
    /// When it was made, RFC 3339.
    created: String,
    /// Why, in its author's words.
    reason: String,
    /// `waiting`, `accepted`, `refused`, `undone` or `stale`; `stale` too for a waiting
    /// one that no longer applies to the files as they are.
    status: String,
    /// The names of the files it touches.
    touches: Vec<String>,
    /// How many changes it holds.
    changes: usize,
    /// When it was decided, for a decided one.
    decided: Option<String>,
}

/// The inbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct InboxView {
    /// Waiting and stale proposals, newest first.
    waiting: Vec<ProposalLine>,
    /// The last decided ones, most recently decided first.
    decided: Vec<ProposalLine>,
}

/// One change, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ChangeView {
    /// `key`, `fact`, `relation`, `notes` or `summary`.
    what: String,
    /// For a key: its name, `phones`.
    key: Option<String>,
    /// Before; `null` when added.
    before: Option<String>,
    /// After; `null` when removed.
    after: Option<String>,
}

/// The changes to one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct FileChangesView {
    /// Its name or title.
    name: String,
    /// `person`, `event`…
    kind: String,
    /// Its id, to open it; `null` for a file the proposal creates.
    id: Option<String>,
    /// True when the proposal creates it.
    created: bool,
    category: String,
    changes: Vec<ChangeView>,
}

/// One proposal in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct ProposalView {
    line: ProposalLine,
    /// What it would change in the files as they are now (or did, once accepted).
    files: Vec<FileChangesView>,
    /// Why it no longer applies; then it can only be refused.
    stale: Option<String>,
    /// What the person said when refusing it.
    comment: Option<String>,
}

/// What a decision did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct DecisionView {
    /// Each proposal decided, with its status now.
    decided: Vec<Decided>,
}

/// One proposal decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct Decided {
    id: String,
    /// `accepted`, `refused`, `undone` or `stale`.
    status: String,
    /// Why it could not be applied, for a stale one.
    reason: Option<String>,
}

/// The body of a `refuse`.
#[derive(Debug, Default, Deserialize)]
struct RefuseBody {
    #[serde(default)]
    comment: Option<String>,
}

/// The body of a grouped `accept`.
#[derive(Debug, Deserialize)]
struct GroupBody {
    ids: Vec<String>,
}

/// Proposals settled in one grouped action, at most.
const MAX_GROUP: usize = 200;
/// Longest comment kept with a refusal, in characters.
const MAX_COMMENT: usize = 2_000;

/// What `GET /api/proposals` answers.
///
/// # Errors
///
/// Errors reading the proposals.
pub(crate) fn list(gateway: &mut Gateway) -> Result<InboxView> {
    let names = ai_names(gateway);
    let proposals = Proposals::new(gateway.vault());
    let all = proposals.list()?;
    // Whether each waiting one still applies, so the inbox does not offer to accept
    // one that would only turn stale: one plan each, reading only its files.
    let mut no_longer: Vec<bool> = Vec::with_capacity(all.len());
    for proposal in &all {
        no_longer.push(
            proposal.status == Status::Waiting && proposals.preview(proposal)?.stale.is_some(),
        );
    }
    let owner = gateway.fresh_owner()?;
    let mut waiting = Vec::new();
    let mut decided = Vec::new();
    for (proposal, stale) in all.iter().zip(no_longer) {
        let mut line = line(proposal, &names, &owner);
        if stale {
            "stale".clone_into(&mut line.status);
        }
        if matches!(proposal.status, Status::Waiting | Status::Stale { .. }) {
            waiting.push(line);
        } else {
            decided.push(line);
        }
    }
    decided.sort_by(|a, b| b.decided.cmp(&a.decided));
    decided.truncate(RECENT);
    Ok(InboxView { waiting, decided })
}

/// What `GET /api/proposals/<id>` answers; `None` when there is no such proposal.
///
/// # Errors
///
/// Errors reading the proposal or its files.
pub(crate) fn one(gateway: &mut Gateway, id: &str) -> Result<Option<ProposalView>> {
    let names = ai_names(gateway);
    let proposals = Proposals::new(gateway.vault());
    let proposal = match proposals.get(id) {
        Ok(p) => p,
        Err(Error::Refused(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    // A waiting proposal shows what it would change now; a decided or stale one, what
    // it said.
    let preview = if proposal.status == Status::Waiting {
        let now = proposals.preview(&proposal)?;
        if now.stale.is_none() {
            now
        } else {
            Preview {
                stale: now.stale,
                ..proposals.described(&proposal)
            }
        }
    } else {
        proposals.described(&proposal)
    };
    let owner = gateway.fresh_owner()?;
    let (stale, comment) = match &proposal.status {
        Status::Stale { reason, .. } => (Some(reason.clone()), None),
        Status::Refused { comment, .. } => (None, comment.clone()),
        Status::Waiting => (preview.stale.clone(), None),
        _ => (None, None),
    };
    let mut line = line(&proposal, &names, &owner);
    if stale.is_some() {
        "stale".clone_into(&mut line.status);
    }
    Ok(Some(ProposalView {
        line,
        files: files(preview, &owner),
        stale,
        comment,
    }))
}

/// `POST /api/proposals/<id>/accept|refuse|undo`, and `POST /api/proposals/accept`
/// with `{"ids": […]}` to accept several in one action: the person's decisions,
/// through [`Proposals`]. `None` for an unknown address.
///
/// # Errors
///
/// [`Error::Refused`] when a proposal cannot be decided so (unknown, already decided,
/// changed since for an undo) or the body cannot be read, with the reason in plain
/// words; errors writing the files or the history.
pub(crate) fn decide(
    gateway: &mut Gateway,
    path: &str,
    body: &[u8],
) -> Result<Option<DecisionView>> {
    let proposals = Proposals::new(gateway.vault());
    let decided = if path == "proposals/accept" {
        let group: GroupBody = read_body(body)?;
        if group.ids.is_empty() || group.ids.len() > MAX_GROUP {
            return Err(Error::Refused(format!(
                "Give between 1 and {MAX_GROUP} proposals to accept."
            )));
        }
        // Each one in turn: one that no longer applies does not stop the others.
        let mut decided = Vec::with_capacity(group.ids.len());
        for id in &group.ids {
            decided.push(accept(&proposals, id)?);
        }
        decided
    } else {
        let Some((id, action)) = path
            .strip_prefix("proposals/")
            .and_then(|rest| rest.split_once('/'))
        else {
            return Ok(None);
        };
        vec![match action {
            "accept" => accept(&proposals, id)?,
            "refuse" => {
                let body: RefuseBody = if body.is_empty() {
                    RefuseBody::default()
                } else {
                    read_body(body)?
                };
                let comment: Option<String> =
                    body.comment.map(|c| c.chars().take(MAX_COMMENT).collect());
                settled(&proposals.refuse(id, comment.as_deref())?)
            }
            "undo" => settled(&proposals.undo(id)?),
            _ => return Ok(None),
        }]
    };
    Ok(Some(DecisionView { decided }))
}

fn read_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))
}

fn accept(proposals: &Proposals<'_>, id: &str) -> Result<Decided> {
    let id = proposals.get(id)?.id;
    Ok(match proposals.accept(&id)? {
        Accepted::Applied(_) => Decided {
            id,
            status: "accepted".to_owned(),
            reason: None,
        },
        Accepted::Stale(reason) => Decided {
            id,
            status: "stale".to_owned(),
            reason: Some(reason),
        },
    })
}

fn settled(proposal: &Proposal) -> Decided {
    Decided {
        id: proposal.id.clone(),
        status: proposal.status.as_str().to_owned(),
        reason: None,
    }
}

/// The connected AIs' names, by client id.
fn ai_names(gateway: &Gateway) -> Vec<(String, String)> {
    Clients::load(gateway.vault().root())
        .map(|clients| {
            clients
                .list()
                .iter()
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn line(proposal: &Proposal, names: &[(String, String)], owner: &Access<'_>) -> ProposalLine {
    let by = match &proposal.by {
        Author::Client(id) => AuthorView {
            kind: "ai".to_owned(),
            // An AI disconnected since is still named by its id.
            name: names
                .iter()
                .find(|(client, _)| client == id)
                .map_or_else(|| id.clone(), |(_, name)| name.clone()),
        },
        Author::Connector(instance) => AuthorView {
            kind: "source".to_owned(),
            name: instance.clone(),
        },
        Author::Ipse => AuthorView {
            kind: "ipse".to_owned(),
            name: "Ipse".to_owned(),
        },
    };
    let mut touches: Vec<String> = Vec::new();
    for change in &proposal.changes {
        let name = touched(change, owner);
        if !touches.contains(&name) {
            touches.push(name);
        }
    }
    let decided = match &proposal.status {
        Status::Accepted { at, .. } | Status::Refused { at, .. } | Status::Undone { at, .. } => {
            Some(at.clone())
        }
        Status::Waiting | Status::Stale { .. } => None,
    };
    ProposalLine {
        id: proposal.id.clone(),
        by,
        created: proposal.created.clone(),
        reason: proposal.reason.clone(),
        status: proposal.status.as_str().to_owned(),
        touches,
        changes: proposal.changes.len(),
        decided,
    }
}

/// The name of the file a change touches: the new file's name for a creation, the
/// file's name from the index, else its name on disk.
fn touched(change: &ipse_core::proposals::Change, owner: &Access<'_>) -> String {
    if let ipse_core::proposals::Change::Create { keys, .. } = change
        && let Some(name) = keys
            .get("name")
            .or_else(|| keys.get("title"))
            .and_then(serde_json::Value::as_str)
    {
        return name.to_owned();
    }
    let file = change.file().unwrap_or_default();
    owner.at(file).map_or_else(
        || {
            file.rsplit('/')
                .next()
                .unwrap_or(file)
                .trim_end_matches(".md")
                .to_owned()
        },
        |link| link.name,
    )
}

fn files(preview: Preview, owner: &Access<'_>) -> Vec<FileChangesView> {
    preview
        .files
        .into_iter()
        .map(|file| file_view(file, owner))
        .collect()
}

fn file_view(file: FileDiff, owner: &Access<'_>) -> FileChangesView {
    FileChangesView {
        id: if file.created {
            None
        } else {
            owner.at(&file.file).and_then(|link| link.id)
        },
        name: file.name,
        kind: file.file_type,
        created: file.created,
        category: file.category,
        changes: file.items.into_iter().map(change_view).collect(),
    }
}

fn change_view(item: DiffItem) -> ChangeView {
    let (what, key) = match item.what {
        What::Key(key) => ("key", Some(key)),
        What::Fact => ("fact", None),
        What::Relation => ("relation", None),
        What::Notes => ("notes", None),
        What::Summary => ("summary", None),
    };
    ChangeView {
        what: what.to_owned(),
        key,
        before: item.before,
        after: item.after,
    }
}
