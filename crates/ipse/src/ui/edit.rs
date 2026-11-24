//! Editing, creating and deleting files from the interface (issue #129): the person's
//! own changes, written by the core's proposal code at once, as one change of the
//! history by the person.

use ipse_core::proposals::{Change, Draft, NewRelation, Proposals, version};
use ipse_core::rights::{Access, Gateway};
use ipse_core::vault::{LinkOrText, MemoryFile};
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::api::{self, FileView, LinkView};

/// A link as the form edits it: the value written in the file, and what it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct LinkField {
    /// As written in the file: `[[marie-dupont]]`, or a name as written.
    value: String,
    link: LinkView,
}

/// A relation as the form edits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct RelationField {
    word: String,
    /// The linked file's name in the memory: `marie-dupont`.
    target: String,
    /// Text shown for the link, if not the linked file's name.
    display: Option<String>,
    link: LinkView,
}

/// What `GET /api/file/<id>/edit` answers: a file as its form edits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct EditView {
    id: String,
    /// `person`, `place`… or `event`.
    kind: String,
    /// The file as it was read: saving is refused if it changed since.
    version: String,
    /// True for the owner's own file, which cannot be deleted.
    owner: bool,
    name: Option<String>,
    aliases: Vec<String>,
    /// `YYYY-MM-DD`, or `--MM-DD` without the year.
    birthday: Option<String>,
    emails: Vec<String>,
    phones: Vec<String>,
    address: Option<String>,
    title: Option<String>,
    /// `YYYY-MM-DD` for a whole day, otherwise RFC 3339.
    start: Option<String>,
    end: Option<String>,
    place: Option<LinkField>,
    participants: Vec<LinkField>,
    /// `planned`, `done` or `cancelled`.
    status: Option<String>,
    /// What kind of event it is: `meal`, `trip`…
    event_kind: Option<String>,
    /// In order; a saved form names each by its position here.
    facts: Vec<String>,
    relations: Vec<RelationField>,
    notes: Option<String>,
    /// The other files that link to it, which deleting it leaves with a name only.
    links: Vec<LinkView>,
}

/// `POST /api/file/<id>/edit`.
#[derive(Debug, Deserialize)]
struct EditBody {
    version: String,
    #[serde(flatten)]
    draft: Draft,
}

/// What `GET /api/file/<id>/text` answers: the file as written, to edit as Markdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct TextView {
    /// The file as it was read: saving is refused if it changed since.
    version: String,
    /// Its whole text, front matter included.
    text: String,
}

/// `POST /api/file/<id>/text`.
#[derive(Debug, Deserialize)]
struct TextBody {
    version: String,
    text: String,
}

/// `POST /api/file/<id>/delete`.
#[derive(Debug, Deserialize)]
struct DeleteBody {
    version: String,
}

/// `POST /api/files`.
#[derive(Debug, Deserialize)]
struct CreateBody {
    /// `person`, `place`… or `event`.
    kind: String,
    #[serde(default)]
    keys: std::collections::BTreeMap<String, Value>,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default)]
    relations: Vec<NewRelation>,
    #[serde(default)]
    notes: Option<String>,
    /// The name a link already gives it, to create the file that link points to.
    #[serde(default)]
    stem: Option<String>,
}

/// What `POST /api/files` and `POST /api/file/<id>/delete` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SavedView {
    /// The file's id.
    id: String,
    /// `person`, `place`… or `event`, to open its screen or its list.
    kind: String,
}

/// `keys` with an event's `start` and `end` given as a day and a time of the vault's
/// time zone (`2026-11-14T20:00`, as the form's fields give them) written with their
/// offset, as files keep them.
fn with_offsets(keys: &mut std::collections::BTreeMap<String, Value>, zone: &jiff::tz::TimeZone) {
    for key in ["start", "end"] {
        let Some(Value::String(text)) = keys.get_mut(key) else {
            continue;
        };
        let local = text.len() <= 19 && text.contains('T') && !text.contains(['+', 'Z', 'z']);
        if let Some(zoned) = local
            .then(|| text.parse::<jiff::civil::DateTime>().ok())
            .flatten()
            .and_then(|t| t.to_zoned(zone.clone()).ok())
        {
            *text = zoned.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string();
        }
    }
}

fn read_body<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T> {
    serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))
}

/// The file of `id` and the file itself, read with the owner's rights.
fn find(owner: &Access<'_>, root: &std::path::Path, id: &str) -> Option<(String, MemoryFile)> {
    let file = owner.file_of(id)?;
    let read = MemoryFile::read(root.join(&file)).ok()?;
    Some((file, read))
}

fn link_field(owner: &Access<'_>, value: &LinkOrText) -> LinkField {
    LinkField {
        value: value.to_string(),
        link: owner.link(value).into(),
    }
}

/// What `GET /api/file/<id>/edit` answers; `None` when there is no such file, or it is
/// not one the person edits here (a summary, a state log).
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn view(gateway: &mut Gateway, id: &str) -> Result<Option<EditView>> {
    let root = gateway.vault().root().to_owned();
    let owner_file = gateway.vault().owner_file().to_owned();
    let owner = gateway.fresh_owner()?;
    let Some((file, read)) = find(&owner, &root, id) else {
        return Ok(None);
    };
    let common = read.common();
    let kind = common.file_type.as_str().to_owned();
    let sections = read.sections();
    let links = owner
        .backlinks(id)
        .into_iter()
        .map(|b| b.from.into())
        .collect();
    let mut view = EditView {
        id: common.id.to_string(),
        kind,
        version: version(&root.join(&file))?,
        owner: false,
        name: None,
        aliases: Vec::new(),
        birthday: None,
        emails: Vec::new(),
        phones: Vec::new(),
        address: None,
        title: None,
        start: None,
        end: None,
        place: None,
        participants: Vec::new(),
        status: None,
        event_kind: None,
        facts: sections.facts.into_iter().map(|f| f.text).collect(),
        relations: sections
            .relations
            .into_iter()
            .map(|r| RelationField {
                link: owner.link(&LinkOrText::Link(r.link.clone())).into(),
                word: r.word,
                target: r.link.target,
                display: r.link.display,
            })
            .collect(),
        notes: sections.notes,
        links,
    };
    if let Some(entity) = read.entity() {
        view.owner = entity.owner || root.join(&file) == owner_file;
        view.name.clone_from(&entity.name);
        view.aliases.clone_from(&entity.aliases);
        view.birthday = entity.birthday.map(|b| b.to_string());
        view.emails.clone_from(&entity.emails);
        view.phones.clone_from(&entity.phones);
        view.address.clone_from(&entity.address);
    } else if let Some(event) = read.event() {
        view.title = Some(event.title.clone());
        view.start = Some(event.start.to_string());
        view.end = event.end.map(|t| t.to_string());
        view.place = event.place.as_ref().map(|p| link_field(&owner, p));
        view.participants = event
            .participants
            .iter()
            .map(|p| link_field(&owner, p))
            .collect();
        view.status = event.status.map(|s| s.as_str().to_owned());
        view.event_kind = event.kind.map(|k| k.as_str().to_owned());
    } else {
        return Ok(None);
    }
    Ok(Some(view))
}

/// What `GET /api/file/<id>/text` answers (issue #142); `None` when there is no such
/// file, or it is not one the person edits here.
///
/// # Errors
///
/// Errors reading the vault or the file.
pub(crate) fn text(gateway: &mut Gateway, id: &str) -> Result<Option<TextView>> {
    let root = gateway.vault().root().to_owned();
    let owner = gateway.fresh_owner()?;
    let Some((file, read)) = find(&owner, &root, id) else {
        return Ok(None);
    };
    let kind = read.common().file_type;
    if !kind.is_entity() && kind != ipse_core::vault::FileType::Event {
        return Ok(None);
    }
    let path = root.join(&file);
    let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
        action: "read",
        path: path.clone(),
        source,
    })?;
    Ok(Some(TextView {
        version: version(&path)?,
        text,
    }))
}

/// `POST /api/file/<id>/edit`, `POST /api/file/<id>/text`, `POST /api/file/<id>/delete`
/// and `POST /api/files`.
///
/// # Errors
///
/// [`Error::Refused`] when the body cannot be read, a value is not valid, or the file
/// changed since the form read it; errors writing the files or the history.
pub(crate) fn change(gateway: &mut Gateway, path: &str, body: &[u8]) -> Result<Option<Vec<u8>>> {
    let root = gateway.vault().root().to_owned();
    if path == "files" {
        let mut body: CreateBody = read_body(body)?;
        with_offsets(&mut body.keys, gateway.settings().time_zone());
        let change = Change::Create {
            file: None,
            file_type: body.kind.clone(),
            category: None,
            keys: body.keys,
            facts: body.facts,
            relations: body.relations,
            notes: body.notes,
        };
        let created = Proposals::new(gateway.vault()).create(change, body.stem.as_deref())?;
        gateway.refresh()?;
        return Ok(Some(super::to_json(&SavedView {
            id: created.id,
            kind: body.kind,
        })));
    }
    let Some((id, action)) = path
        .strip_prefix("file/")
        .and_then(|rest| rest.split_once('/'))
    else {
        return Ok(None);
    };
    let (file, kind) = {
        let owner = gateway.fresh_owner()?;
        let Some((file, read)) = find(&owner, &root, id) else {
            return Ok(None);
        };
        (file, read.common().file_type.as_str().to_owned())
    };
    let proposals = Proposals::new(gateway.vault());
    match action {
        "edit" => {
            let mut body: EditBody = read_body(body)?;
            with_offsets(&mut body.draft.keys, gateway.settings().time_zone());
            proposals.edit(&file, &body.version, &body.draft)?;
            gateway.refresh()?;
            let view: Option<FileView> = api::file(gateway, id)?;
            Ok(view.map(|v| super::to_json(&v)))
        }
        "text" => {
            let body: TextBody = read_body(body)?;
            proposals.edit_text(&file, &body.version, &body.text)?;
            gateway.refresh()?;
            let view: Option<FileView> = api::file(gateway, id)?;
            Ok(view.map(|v| super::to_json(&v)))
        }
        "delete" => {
            let body: DeleteBody = read_body(body)?;
            proposals.delete(&file, &body.version)?;
            gateway.refresh()?;
            Ok(Some(super::to_json(&SavedView {
                id: id.to_owned(),
                kind,
            })))
        }
        _ => Ok(None),
    }
}
