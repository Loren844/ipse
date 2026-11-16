//! The interface's JSON API: what each screen shows, read through the query layer with
//! the owner's rights ([`Gateway::fresh_owner`]). Its types are written to
//! `ui/src/lib/types/` as TypeScript by `cargo test`, so the interface and the core
//! never drift.

use std::collections::HashSet;

use ipse_core::proposals::{Proposals, Status};
use ipse_core::rights::{Details, Gateway, LinkOut};
use ipse_core::vault::{EventTime, LinkOrText};
use ipse_core::{Error, Result};
use jiff::ToSpan;
use serde::Serialize;

/// Days of coming events on the home screen.
const HOME_DAYS_AHEAD: i32 = 30;
/// Coming events on the home screen, at most.
const HOME_EVENTS: usize = 6;
/// Days back to find the people seen recently.
const RECENT_DAYS: i32 = 90;
/// People seen recently, at most.
const RECENT_PEOPLE: usize = 8;
/// Events listed on a person's page, at most.
const PERSON_EVENTS: usize = 50;

/// What the interface shows in its frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct AppInfo {
    /// The owner's name: whose memory this is.
    pub(crate) name: String,
    /// The vault's language, `fr` or `en`: the interface speaks it until the person
    /// picks another.
    pub(crate) language: String,
    /// The vault's time zone, an IANA name: dates are shown in it.
    pub(crate) time_zone: String,
    /// Proposals waiting for the person, stale ones included, and questions.
    pub(crate) waiting: usize,
}

/// A link to another file, or a name as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct LinkView {
    /// What to show.
    name: String,
    /// The linked file's id, to open it; `null` for a name as written.
    id: Option<String>,
    /// The linked file's type: `person`, `place`…
    kind: Option<String>,
}

impl LinkView {
    /// A name as written, linking nowhere.
    pub(crate) fn text(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            id: None,
            kind: None,
        }
    }
}

impl From<LinkOut> for LinkView {
    fn from(link: LinkOut) -> Self {
        Self {
            name: link.name,
            id: link.id,
            kind: link.file_type,
        }
    }
}

/// One event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct EventView {
    id: String,
    title: String,
    /// `YYYY-MM-DD` for a whole day, otherwise RFC 3339 with the vault's offset.
    start: String,
    /// Exclusive; `null` for a point in time.
    end: Option<String>,
    place: Option<LinkView>,
    /// Who takes part, the owner implied.
    people: Vec<LinkView>,
    /// `planned`, `done` or `cancelled`, when known.
    status: Option<String>,
    notes: Option<String>,
    category: String,
    /// Closed to AIs unless the person opens its category.
    sensitive: bool,
    /// Where it comes from: `calendar`, `proposal`, `person`…
    sources: Vec<String>,
}

/// A relation of an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct RelationView {
    /// The relation word, as written: `sœur`.
    word: String,
    to: LinkView,
}

/// A person, a place or another entity, in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct EntityView {
    id: String,
    /// `person`, `place`, `organization`…
    kind: String,
    name: String,
    aliases: Vec<String>,
    /// `YYYY-MM-DD`, or `--MM-DD` without the year.
    birthday: Option<String>,
    emails: Vec<String>,
    phones: Vec<String>,
    address: Option<String>,
    facts: Vec<String>,
    relations: Vec<RelationView>,
    notes: Option<String>,
    category: String,
    /// Closed to AIs unless the person opens its category.
    sensitive: bool,
    /// Where it comes from: `contacts`, `proposal`, `person`…
    sources: Vec<String>,
    /// The events it takes part in, by start.
    events: Vec<EventView>,
    /// True for the owner's own file.
    owner: bool,
}

/// One file of the memory, as a screen opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) enum FileView {
    Entity(EntityView),
    Event(EventView),
}

/// One person in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct PersonLine {
    id: String,
    name: String,
    aliases: Vec<String>,
    sensitive: bool,
}

/// One page of the people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct PeopleView {
    items: Vec<PersonLine>,
    /// How many in all (for a search, how many found).
    total: usize,
    /// True when there are more after this page.
    more: bool,
}

/// The events of a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct EventsView {
    /// By start.
    events: Vec<EventView>,
    /// True when the period held more than were returned.
    more: bool,
    /// The period's first day, `YYYY-MM-DD`.
    from: String,
    /// The period's last day, included.
    to: String,
    /// The summaries of its days that have one.
    summaries: Vec<super::summaries::DaySummary>,
}

/// The home screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct HomeView {
    /// Today, in the vault's time zone, `YYYY-MM-DD`.
    today: String,
    /// The owner's own file.
    me: Option<LinkView>,
    /// The next events.
    upcoming: Vec<EventView>,
    /// People of recent events, the most recent first.
    recent: Vec<LinkView>,
    /// Proposals waiting for the person, stale ones included.
    waiting: usize,
    /// Questions waiting for the person's answer.
    questions: usize,
    /// The most recent day summary.
    latest: Option<super::summaries::SummaryView>,
}

/// What `/api/app` answers.
///
/// # Errors
///
/// Errors reading the vault or its proposals.
pub(crate) fn app(gateway: &mut Gateway) -> Result<AppInfo> {
    let settings = gateway.settings();
    let language = settings.language().code().to_owned();
    let time_zone = settings.time_zone_name().to_owned();
    super::questions::plan(gateway)?;
    let waiting = waiting(gateway)?;
    let owner = gateway.fresh_owner()?;
    let name = owner.about_me().and_then(|me| me.name).unwrap_or_default();
    Ok(AppInfo {
        name,
        language,
        time_zone,
        waiting,
    })
}

/// Proposals and questions waiting for the person.
fn waiting(gateway: &Gateway) -> Result<usize> {
    Ok(proposals_waiting(gateway)? + super::questions::waiting(gateway)?)
}

/// Proposals waiting for the person, stale ones included.
fn proposals_waiting(gateway: &Gateway) -> Result<usize> {
    Ok(Proposals::new(gateway.vault())
        .list()?
        .iter()
        .filter(|p| matches!(p.status, Status::Waiting | Status::Stale { .. }))
        .count())
}

/// Today in the vault's time zone.
fn today(gateway: &Gateway) -> jiff::civil::Date {
    jiff::Timestamp::now()
        .to_zoned(gateway.settings().time_zone().clone())
        .date()
}

/// What `/api/home` answers.
///
/// # Errors
///
/// Errors reading the vault or its proposals.
pub(crate) fn home(gateway: &mut Gateway) -> Result<HomeView> {
    let today = today(gateway);
    let waiting = proposals_waiting(gateway)?;
    let questions = super::questions::waiting(gateway)?;
    let latest = super::summaries::latest(gateway)?;
    let owner = gateway.fresh_owner()?;
    let ahead = today.saturating_add(HOME_DAYS_AHEAD.days());
    let upcoming = owner
        .events(
            &EventTime::Day(today),
            &EventTime::Day(ahead),
            None,
            HOME_EVENTS,
        )
        .events
        .into_iter()
        .filter_map(|e| owner.details(&e.id))
        .map(event_view)
        .collect();
    let back = today.saturating_sub(RECENT_DAYS.days());
    let past = owner.events(&EventTime::Day(back), &EventTime::Day(today), None, 200);
    let mut seen = HashSet::new();
    let mut recent = Vec::new();
    for event in past.events.iter().rev() {
        for person in &event.participants {
            let link = owner.link(&LinkOrText::parse(person));
            let Some(id) = link.id.clone() else {
                continue;
            };
            if seen.insert(id) && recent.len() < RECENT_PEOPLE {
                recent.push(LinkView::from(link));
            }
        }
    }
    let me = owner.about_me().map(|me| LinkView {
        name: me.name.unwrap_or_default(),
        id: Some(me.id),
        kind: Some(me.file_type),
    });
    Ok(HomeView {
        today: today.to_string(),
        me,
        upcoming,
        recent,
        waiting,
        questions,
        latest,
    })
}

/// What `/api/people` answers.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn people(
    gateway: &mut Gateway,
    query: Option<&str>,
    offset: usize,
) -> Result<PeopleView> {
    let owner = gateway.fresh_owner()?;
    let lines = owner.people(query, 100, offset);
    Ok(PeopleView {
        items: lines
            .items
            .into_iter()
            .map(|l| PersonLine {
                id: l.id,
                name: l.name,
                aliases: l.aliases,
                sensitive: l.sensitive,
            })
            .collect(),
        total: lines.total,
        more: lines.more,
    })
}

/// What `/api/events` answers: the period `from`–`to` (days, included), by default the
/// last 30 days and the next 60.
///
/// # Errors
///
/// [`Error::Refused`] for days that cannot be read; errors reading the vault.
pub(crate) fn events(
    gateway: &mut Gateway,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<EventsView> {
    let today = today(gateway);
    let day = |text: Option<&str>, fallback: jiff::civil::Date| -> Result<jiff::civil::Date> {
        match text {
            None => Ok(fallback),
            Some(text) => text
                .parse()
                .map_err(|_| Error::Refused("Give days as YYYY-MM-DD.".to_owned())),
        }
    };
    let from = day(from, today.saturating_sub(30.days()))?;
    let to = day(to, today.saturating_add(60.days()))?;
    if from > to {
        return Err(Error::Refused(
            "The period ends before it starts.".to_owned(),
        ));
    }
    let summaries = super::summaries::days(gateway, from, to)?;
    let owner = gateway.fresh_owner()?;
    let found = owner.events(&EventTime::Day(from), &EventTime::Day(to), None, 200);
    Ok(EventsView {
        events: found
            .events
            .into_iter()
            .filter_map(|e| owner.details(&e.id))
            .map(event_view)
            .collect(),
        more: found.more,
        from: from.to_string(),
        to: to.to_string(),
        summaries,
    })
}

/// What `/api/file/<id>` answers; `None` when there is no such file.
///
/// # Errors
///
/// Errors reading the vault.
pub(crate) fn file(gateway: &mut Gateway, id: &str) -> Result<Option<FileView>> {
    let today = today(gateway);
    let owner = gateway.fresh_owner()?;
    let Some(details) = owner.details(id) else {
        return Ok(None);
    };
    if details.entry.file_type == "event" {
        return Ok(Some(FileView::Event(event_view(details))));
    }
    let is_owner = owner.about_me().is_some_and(|me| me.id == details.entry.id);
    let events = if is_owner {
        Vec::new()
    } else {
        // Everything with them, from the earliest to a year ahead.
        let from = EventTime::Day(jiff::civil::date(1900, 1, 1));
        let to = EventTime::Day(today.saturating_add(366.days()));
        owner
            .events(&from, &to, Some(id), PERSON_EVENTS)
            .events
            .into_iter()
            .filter_map(|e| owner.details(&e.id))
            .map(event_view)
            .collect()
    };
    let entry = details.entry;
    Ok(Some(FileView::Entity(EntityView {
        id: entry.id,
        kind: entry.file_type,
        name: entry.name.unwrap_or_default(),
        aliases: entry.aliases,
        birthday: entry.birthday,
        emails: entry.emails,
        phones: entry.phones,
        address: entry.address,
        facts: entry.facts,
        relations: details
            .relations
            .into_iter()
            .map(|r| RelationView {
                word: r.word,
                to: r.to.into(),
            })
            .collect(),
        notes: entry.notes,
        category: details.category,
        sensitive: details.sensitive,
        sources: details.sources,
        events,
        owner: is_owner,
    })))
}

fn event_view(details: Details) -> EventView {
    let entry = details.entry;
    EventView {
        id: entry.id,
        title: entry.name.unwrap_or_default(),
        start: entry.start.unwrap_or_default(),
        end: entry.end,
        place: details.place.map(Into::into),
        people: details.participants.into_iter().map(Into::into).collect(),
        status: entry.status,
        notes: entry.notes,
        category: details.category,
        sensitive: details.sensitive,
        sources: details.sources,
    }
}
