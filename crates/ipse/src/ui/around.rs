//! The view around someone's API (issue #244): one person or place, and everything the
//! memory holds about them, by kind and by time, built from the index within the
//! owner's rights. Each section is capped, with how many there are in all; "see all"
//! asks for one section page by page.

use ipse_core::Result;
use ipse_core::rights::{Gateway, HappenedOut, SeenOut, SharedOut};
use ipse_core::vault::{EventTime, MemoryFile};
use serde::Serialize;

use super::graph::NodeView;

/// Items a section shows before "see all".
const SHOWN: usize = 5;
/// Items one page of "see all" adds.
const PAGE: usize = 50;
/// The category of the visits that location history writes.
const VISITS: &str = "location";
/// The types of the files an AI writes when told "remember this" (decision 0018).
const REMEMBERED: [&str; 5] = ["thing", "topic", "project", "organization", "work"];

/// The sections that "see all" pages through.
const SECTIONS: [&str; 7] = [
    "events",
    "visits",
    "places",
    "people",
    "remembered",
    "reminders",
    "linked",
];

/// Some of a section's items, and how many it holds in all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SectionView<T> {
    items: Vec<T>,
    total: u32,
}

/// Which section "see all" pages through, and from where.
struct Pager<'a> {
    section: Option<&'a str>,
    offset: usize,
}

impl Pager<'_> {
    /// The items of the section `name`: from the offset when it is the section paged
    /// through, else the first ones.
    fn of<T>(&self, name: &str, all: Vec<T>) -> SectionView<T> {
        let (section, offset) = (self.section, self.offset);
        let paged = section == Some(name);
        let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
        let (skip, take) = if paged { (offset, PAGE) } else { (0, SHOWN) };
        SectionView {
            items: all.into_iter().skip(skip).take(take).collect(),
            total,
        }
    }
}

/// An event around the center.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AroundEventView {
    file: NodeView,
    /// Its place, when it is a file and not the center.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    place: Option<NodeView>,
    /// `meal`, `trip`…, when someone said.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    kind: Option<String>,
}

/// A place or someone met in the center's events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SharedView {
    file: NodeView,
    /// In how many of the center's events.
    events: u32,
    /// When the last of them started.
    last: String,
}

/// A file that links to the center, with the relation's word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct LinkedView {
    file: NodeView,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    word: Option<String>,
}

/// A reminder about the center.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AroundReminderView {
    file: NodeView,
    /// When it is due, as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    at: Option<String>,
    /// The day it was last done.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    done: Option<String>,
}

/// What a state log says about the center: photos that show someone, stays at a place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct LoggedView {
    count: u32,
    first: String,
    last: String,
    sensitive: bool,
}

/// How many events there were in one month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MonthCount {
    /// `YYYY-MM`.
    month: String,
    events: u32,
}

/// What `GET /api/around?id=…` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AroundView {
    center: NodeView,
    /// True when the center is the owner's own file.
    mine: bool,
    /// How the owner's file calls them: "sœur".
    relation: Vec<String>,
    /// Their events, in all and this year.
    total: u32,
    this_year: u32,
    /// When the first and the last event until today started.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    first: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    last: Option<String>,
    /// When the next one starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    next: Option<String>,
    /// The months that have events, in order.
    months: Vec<MonthCount>,
    /// The events, the latest first (for a place, those of location history are
    /// `visits`).
    events: SectionView<AroundEventView>,
    /// For a place: the visits location history found, the latest first.
    visits: SectionView<AroundEventView>,
    /// For a place: the stays the state logs count (location history, Home Assistant).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    stays: Option<LoggedView>,
    /// For a person: the places of their events, the most frequent first.
    places: SectionView<SharedView>,
    /// The people of the events, the most frequent first: close to a person, who came
    /// to a place.
    people: SectionView<SharedView>,
    /// For a person: the photos that show them (Immich).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    photos: Option<LoggedView>,
    /// The facts of their file.
    facts: Vec<String>,
    /// The files an AI keeps that link to them: things, topics, projects…
    remembered: SectionView<LinkedView>,
    reminders: SectionView<AroundReminderView>,
    /// Their notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    notes: Option<String>,
    /// The other files that link to them.
    linked: SectionView<LinkedView>,
}

impl From<HappenedOut> for AroundEventView {
    fn from(h: HappenedOut) -> Self {
        Self {
            file: h.node.into(),
            place: h.place.map(Into::into),
            kind: h.kind,
        }
    }
}

impl From<SharedOut> for SharedView {
    fn from(s: SharedOut) -> Self {
        Self {
            file: s.node.into(),
            events: s.events,
            last: s.last,
        }
    }
}

impl From<SeenOut> for LoggedView {
    fn from(s: SeenOut) -> Self {
        Self {
            count: u32::try_from(s.count).unwrap_or(u32::MAX),
            first: s.first,
            last: s.last,
            sensitive: s.sensitive,
        }
    }
}

/// What `GET /api/around?id=…` answers: the entity `id` (not an event) and everything
/// around it, each section's first items; with `section` (one of [`SECTIONS`]), that
/// section's items from `offset`. `None` when there is no such entity.
///
/// # Errors
///
/// `Refused` for an unknown section; errors reading the vault.
pub(crate) fn view(
    gateway: &mut Gateway,
    id: Option<&str>,
    section: Option<&str>,
    offset: usize,
) -> Result<Option<AroundView>> {
    if let Some(section) = section
        && !SECTIONS.contains(&section)
    {
        return Err(ipse_core::Error::Refused(format!(
            "There is no section \"{section}\"."
        )));
    }
    let today = super::api::today(gateway);
    let root = gateway.vault().root().to_owned();
    let owner = gateway.fresh_owner()?;
    let Some(id) = id.filter(|i| !i.is_empty()) else {
        return Ok(None);
    };
    let Some(found) = owner.surroundings(id) else {
        return Ok(None);
    };
    let me = owner.about_me().map(|me| me.id);
    let file = owner.file_of(id);
    let stem = file
        .as_deref()
        .and_then(|f| std::path::Path::new(f).file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let place = found.center.file_type == "place";
    let person = found.center.file_type == "person";

    let when = When::of(&found.events, today);
    let page = Pager { section, offset };

    let (visits, events): (Vec<HappenedOut>, Vec<HappenedOut>) = found
        .events
        .into_iter()
        .partition(|e| place && e.node.category == VISITS);
    let (relation, remembered, linked) = split_linked(found.linked, me.as_deref());
    let mut reminders = page.of(
        "reminders",
        found
            .reminders
            .into_iter()
            .map(|node| AroundReminderView {
                file: node.into(),
                at: None,
                done: None,
            })
            .collect(),
    );
    // When the reminders shown are due, from their files: a few, never all of them.
    for reminder in &mut reminders.items {
        if let Some(path) = owner.file_of(reminder.file.id()) {
            reminder.due(&root.join(path));
        }
    }

    Ok(Some(AroundView {
        mine: me.as_deref() == Some(found.center.id.as_str()),
        center: found.center.into(),
        relation,
        total: when.total,
        this_year: when.this_year,
        first: when.first,
        last: when.last,
        next: when.next,
        months: when.months,
        events: page.of("events", events.into_iter().map(Into::into).collect()),
        visits: page.of("visits", visits.into_iter().map(Into::into).collect()),
        stays: if place {
            owner
                .seen_in("location", &stem)
                .or_else(|| owner.seen_in("home", &stem))
                .map(Into::into)
        } else {
            None
        },
        places: page.of(
            "places",
            if person {
                found.places.into_iter().map(Into::into).collect()
            } else {
                Vec::new()
            },
        ),
        people: page.of("people", found.people.into_iter().map(Into::into).collect()),
        photos: if person {
            owner.seen_in("photos", &stem).map(Into::into)
        } else {
            None
        },
        facts: found.facts,
        remembered: page.of("remembered", remembered),
        reminders,
        notes: found.notes,
        linked: page.of("linked", linked),
    }))
}

impl AroundReminderView {
    /// Reads when it is due and was done from its file, at `path`; left empty when
    /// the file does not read.
    fn due(&mut self, path: &std::path::Path) {
        let Ok(file) = MemoryFile::read(path) else {
            return;
        };
        if let Some(fields) = file.entity() {
            self.at = fields.at.as_ref().map(ToString::to_string);
            self.done = fields.done.map(|d| d.to_string());
        }
    }
}

/// When the center's events happened: for the header and the strip.
struct When {
    total: u32,
    this_year: u32,
    first: Option<String>,
    last: Option<String>,
    next: Option<String>,
    months: Vec<MonthCount>,
}

impl When {
    /// From `events`, the latest first, as of `today`.
    fn of(events: &[HappenedOut], today: jiff::civil::Date) -> Self {
        let mut when = Self {
            total: 0,
            this_year: 0,
            first: None,
            last: None,
            next: None,
            months: Vec::new(),
        };
        let mut months: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
        for event in events {
            let Some(day) = event
                .node
                .start
                .as_deref()
                .and_then(EventTime::parse)
                .map(|s| s.date())
            else {
                continue;
            };
            when.total += 1;
            if day.year() == today.year() {
                when.this_year += 1;
            }
            *months
                .entry(format!("{:04}-{:02}", day.year(), day.month()))
                .or_default() += 1;
            if day > today {
                when.next.clone_from(&event.node.start);
            } else {
                if when.last.is_none() {
                    when.last.clone_from(&event.node.start);
                }
                when.first.clone_from(&event.node.start);
            }
        }
        when.months = months
            .into_iter()
            .map(|(month, events)| MonthCount { month, events })
            .collect();
        when
    }
}

/// The files that link to the center, split: how the owner's file `me` calls it, the
/// files an AI keeps ([`REMEMBERED`]), and the others.
fn split_linked(
    all: Vec<(ipse_core::rights::NodeOut, Option<String>)>,
    me: Option<&str>,
) -> (Vec<String>, Vec<LinkedView>, Vec<LinkedView>) {
    let mut relation = Vec::new();
    let (remembered, linked) = all
        .into_iter()
        .filter(|(node, word)| {
            if me == Some(node.id.as_str()) {
                relation.extend(word.clone());
                return false;
            }
            true
        })
        .map(|(node, word)| LinkedView {
            file: node.into(),
            word,
        })
        .partition(|l| REMEMBERED.contains(&l.file.kind()));
    (relation, remembered, linked)
}
