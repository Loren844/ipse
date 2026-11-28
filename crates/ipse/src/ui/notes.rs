//! The note of the day on the home screen (issue #277): `POST /api/notes` adds a line,
//! written at once as the person's; the home screen shows today's lines, each mention a
//! link. The tray menu and, later, the phone's quick capture use the same address.

use ipse_core::Result;
use ipse_core::notes::{self, DayNote};
use ipse_core::rights::{Access, Gateway};
use ipse_core::vault::{LinkOrText, WikiLink};
use serde::{Deserialize, Serialize};

use super::api::LinkView;

/// A piece of a note's line: text as written, or a mention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct NotePart {
    /// The text, or the mention's name.
    text: String,
    /// The file a mention links to; `null` for text, or a file that no longer exists.
    link: Option<LinkView>,
}

/// One line of the day's notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct NoteLine {
    parts: Vec<NotePart>,
}

/// The notes of one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct DayNoteView {
    /// The day, `YYYY-MM-DD`.
    day: String,
    /// Its lines, oldest first, each starting with the time it was written.
    lines: Vec<NoteLine>,
}

#[derive(Deserialize)]
struct NoteBody {
    text: String,
}

/// Today's notes, for the home screen; `None` when there are none yet.
pub(crate) fn today(owner: &Access<'_>, root: &std::path::Path, day: &str) -> Option<DayNoteView> {
    notes::read(root, day).map(|n| view(owner, &n))
}

/// `GET /api/notes/<day>`: the notes of that day (`2026-11-28`); `None` when it holds
/// none, or `day` is not a day.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn day(gateway: &mut Gateway, day: &str) -> Result<Option<DayNoteView>> {
    if ipse_core::vault::EventTime::parse(day).is_none_or(|t| t.to_string() != day) {
        return Ok(None);
    }
    let root = gateway.vault().root().to_owned();
    let owner = gateway.fresh_owner()?;
    Ok(today(&owner, &root, day))
}

/// `POST /api/notes`: adds a line to today's notes and answers them.
///
/// # Errors
///
/// [`ipse_core::Error::Refused`] when the body cannot be read or the note is empty or
/// too long; errors writing the file or the history.
pub(crate) fn add(gateway: &mut Gateway, body: &[u8]) -> Result<DayNoteView> {
    let body: NoteBody = serde_json::from_slice(body)
        .map_err(|_| ipse_core::Error::Refused("The request could not be read.".to_owned()))?;
    let note = notes::add(gateway.vault(), &body.text)?;
    let owner = gateway.fresh_owner()?;
    Ok(view(&owner, &note))
}

fn view(owner: &Access<'_>, note: &DayNote) -> DayNoteView {
    DayNoteView {
        day: note.day.clone(),
        lines: note
            .lines
            .iter()
            .map(|line| NoteLine {
                parts: parts(owner, line),
            })
            .collect(),
    }
}

/// `line` cut into text and mentions, each mention resolved as the owner sees it.
fn parts(owner: &Access<'_>, line: &str) -> Vec<NotePart> {
    let mut parts = Vec::new();
    let mut rest = line;
    let text = |t: &str| NotePart {
        text: t.to_owned(),
        link: None,
    };
    while let Some(start) = rest.find("[[") {
        let Some(len) = rest[start..].find("]]") else {
            break;
        };
        let Some(mention) = WikiLink::parse(&rest[start..start + len + 2]) else {
            parts.push(text(&rest[..start + 2]));
            rest = &rest[start + 2..];
            continue;
        };
        if start > 0 {
            parts.push(text(&rest[..start]));
        }
        let resolved = LinkView::from(owner.link(&LinkOrText::Link(mention)));
        parts.push(NotePart {
            text: resolved.name().to_owned(),
            link: resolved.id().is_some().then_some(resolved),
        });
        rest = &rest[start + len + 2..];
    }
    if !rest.is_empty() {
        parts.push(text(rest));
    }
    parts
}
