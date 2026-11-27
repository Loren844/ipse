//! The timeline's API (issue #133): a life by years and months, what marked each
//! period, the people seen most, and the month and year summaries.

use std::collections::HashMap;

use ipse_core::Result;
use ipse_core::rights::Gateway;
use ipse_core::search::Seen;
use ipse_core::summaries::{self, Period};
use ipse_core::vault::{LinkOrText, WikiLink};
use serde::Serialize;

use super::api::LinkView;

/// Someone seen in a period, and how often.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SeenView {
    person: LinkView,
    /// Events of the period they are in.
    events: u32,
}

/// A trip or a celebration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MarkView {
    id: String,
    title: String,
    /// `YYYY-MM-DD`, or a date and time.
    start: String,
    /// `trip` or `celebration`.
    kind: String,
}

/// One month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MonthView {
    /// `YYYY-MM`.
    month: String,
    events: u32,
    people: Vec<SeenView>,
    marks: Vec<MarkView>,
    /// Its summary, when the person's AI wrote one.
    summary: Option<String>,
}

/// One year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct YearView {
    year: i16,
    events: u32,
    people: Vec<SeenView>,
    /// Its months that have events, January first.
    months: Vec<MonthView>,
    summary: Option<String>,
}

/// What `GET /api/timeline` answers: the years that have events, the latest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct TimelineView {
    years: Vec<YearView>,
    /// The file whose events these are, when the timeline is of one (issue #244).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    with: Option<LinkView>,
}

/// The timeline of the whole memory, as the person sees it; with `person`, of the
/// events that link to that file only (issue #244), without the summaries, which are
/// of the whole period.
///
/// # Errors
///
/// Errors reading the index or the summaries.
pub(crate) fn view(gateway: &mut Gateway, person: Option<&str>) -> Result<TimelineView> {
    let person = person.filter(|p| !p.is_empty());
    let root = gateway.vault().root().to_owned();
    let texts = |period: Period| -> Result<HashMap<String, String>> {
        if person.is_some() {
            return Ok(HashMap::new());
        }
        let (found, _) = summaries::list(&root, period, (None, None), 0, usize::MAX)?;
        Ok(found
            .into_iter()
            .map(|s| (s.span.start.to_string(), s.text))
            .collect())
    };
    let months = texts(Period::Month)?;
    let years = texts(Period::Year)?;
    let owner = gateway.fresh_owner()?;
    let timeline = owner.timeline_with(person);
    let seen = |people: Vec<Seen>| -> Vec<SeenView> {
        people
            .into_iter()
            .map(|s| SeenView {
                person: LinkView::from(owner.link(&LinkOrText::Link(WikiLink {
                    target: s.stem,
                    display: None,
                }))),
                events: s.events,
            })
            // On the timeline of one person, they are with them in every event.
            .filter(|s| person.is_none() || s.person.id() != person)
            .collect()
    };
    let with = person.and_then(|id| owner.details(id)).map(|d| {
        LinkView::from(ipse_core::rights::LinkOut {
            name: d.entry.name.unwrap_or_default(),
            id: Some(d.entry.id),
            file_type: Some(d.entry.file_type),
        })
    });
    Ok(TimelineView {
        with,
        years: timeline
            .years
            .into_iter()
            .map(|year| YearView {
                year: year.year,
                events: year.events,
                people: seen(year.people),
                summary: years.get(&format!("{:04}-01-01", year.year)).cloned(),
                months: year
                    .months
                    .into_iter()
                    .map(|month| {
                        let key = format!("{:04}-{:02}", year.year, month.month);
                        MonthView {
                            summary: months.get(&format!("{key}-01")).cloned(),
                            month: key,
                            events: month.events,
                            people: seen(month.people),
                            marks: month
                                .marks
                                .into_iter()
                                .map(|m| MarkView {
                                    id: m.id.to_string(),
                                    title: m.title.to_owned(),
                                    start: m.start.to_string(),
                                    kind: m.kind.as_str().to_owned(),
                                })
                                .collect(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    })
}
