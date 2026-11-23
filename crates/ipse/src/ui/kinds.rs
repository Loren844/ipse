//! A page per kind of event (issue #134): the next and the last ones, who and where
//! most and, for celebrations, the birthdays to come from the person files.

use ipse_core::Result;
use ipse_core::rights::Gateway;
use ipse_core::vault::EventKind;
use serde::Serialize;

use super::api::{EventView, LinkView, event_view, today};

/// Events of each list on the page, and people and places shown.
const SHOWN: usize = 12;

/// How far ahead birthdays are listed, in days.
const BIRTHDAYS_AHEAD: i32 = 365;

/// Someone or somewhere, and in how many events of the kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct OftenView {
    link: LinkView,
    events: u32,
}

/// A birthday to come.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct BirthdayView {
    person: LinkView,
    /// `YYYY-MM-DD`.
    day: String,
    /// The age they turn, when their file gives the year.
    age: Option<i16>,
}

/// What `/api/kinds/<kind>` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct EventKindView {
    /// `meal`, `trip`…
    kind: String,
    /// How many events of the kind the memory holds.
    events: u32,
    /// The next ones, by start.
    next: Vec<EventView>,
    /// The last ones, the most recent first.
    last: Vec<EventView>,
    /// The people in most of them, most first.
    people: Vec<OftenView>,
    /// The places most of them happen at, most first.
    places: Vec<OftenView>,
    /// For celebrations: the birthdays of the next twelve months, the soonest first.
    birthdays: Vec<BirthdayView>,
}

/// The page of `kind`; `None` for a word that is not a kind of event.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn view(gateway: &mut Gateway, kind: &str) -> Result<Option<EventKindView>> {
    let Some(kind) = EventKind::parse(kind) else {
        return Ok(None);
    };
    let today = today(gateway);
    let owner = gateway.fresh_owner()?;
    let series = owner.series();
    let found = owner.kind(kind, today, SHOWN);
    let often = |list: Vec<(ipse_core::rights::LinkOut, u32)>| {
        list.into_iter()
            .map(|(link, events)| OftenView {
                link: link.into(),
                events,
            })
            .collect()
    };
    let birthdays = if kind == EventKind::Celebration {
        owner
            .birthdays(today, BIRTHDAYS_AHEAD)
            .into_iter()
            .map(|b| BirthdayView {
                person: b.person.into(),
                day: b.day.to_string(),
                age: b.age,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(Some(EventKindView {
        kind: kind.as_str().to_owned(),
        events: found.events,
        next: found
            .next
            .into_iter()
            .map(|d| event_view(d, &series))
            .collect(),
        last: found
            .last
            .into_iter()
            .map(|d| event_view(d, &series))
            .collect(),
        people: often(found.people),
        places: often(found.places),
        birthdays,
    }))
}
