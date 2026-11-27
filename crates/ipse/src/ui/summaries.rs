//! The summaries' screen: the person reads their days, weeks, months and years as
//! their AI summarized them, and corrects one in place (`docs/spec/vault.md`,
//! section 7). A correction is the person's own change: Ipse never writes over it.

use ipse_core::proposals::{Change, Proposals, Status};
use ipse_core::rights::Gateway;
use ipse_core::summaries::{self, Period, Span, Summary};
use ipse_core::vault::{LinkOrText, WikiLink};
use ipse_core::{Error, Result};
use jiff::ToSpan;
use serde::{Deserialize, Serialize};

use super::api::LinkView;

/// Summaries on one page.
const PAGE: usize = 20;

/// A shorter summary a longer one was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct PartView {
    /// `day` or `month`.
    period: String,
    /// Its first day, `YYYY-MM-DD`.
    start: String,
    /// Its name: `2026-11-14`, `2026-11`.
    label: String,
}

/// One summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SummaryView {
    /// `day`, `week`, `month` or `year`.
    period: String,
    /// Its first day, `YYYY-MM-DD`.
    start: String,
    /// Its name: `2026-11-14`, `2026-W46`, `2026-11`, `2026`.
    label: String,
    /// Its text, as the AI wrote it or the person corrected it.
    text: String,
    /// For a day: the events it links to.
    events: Vec<LinkView>,
    /// For a longer period: the summaries inside it.
    parts: Vec<PartView>,
}

/// One page of summaries of one period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SummariesView {
    period: String,
    items: Vec<SummaryView>,
    /// There are older ones.
    more: bool,
    /// Summaries the AI wrote, waiting in the review inbox.
    waiting: usize,
}

/// A day's summary, for the agenda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct DaySummary {
    /// `YYYY-MM-DD`.
    day: String,
    text: String,
}

#[derive(Deserialize)]
struct CorrectBody {
    text: String,
}

fn period_of(text: Option<&str>) -> Result<Period> {
    match text {
        None => Ok(Period::Day),
        Some(text) => Period::parse(text)
            .ok_or_else(|| Error::Refused("`period` must be day, week, month or year.".to_owned())),
    }
}

/// What `GET /api/summaries?period=&offset=&before=` answers.
///
/// # Errors
///
/// [`Error::Refused`] for an unknown period or a day not written `YYYY-MM-DD`; errors
/// reading the summaries or the proposals.
pub(crate) fn page(
    gateway: &mut Gateway,
    period: Option<&str>,
    offset: usize,
    before: Option<&str>,
) -> Result<SummariesView> {
    let period = period_of(period)?;
    let before = match before {
        None => None,
        Some(text) => Some(
            text.parse()
                .map_err(|_| Error::Refused("Give days as YYYY-MM-DD.".to_owned()))?,
        ),
    };
    let root = gateway.vault().root().to_owned();
    let (found, more) = summaries::list(&root, period, (None, before), offset, PAGE)?;
    let waiting = Proposals::new(gateway.vault())
        .list()?
        .iter()
        .filter(|p| p.status == Status::Waiting)
        .filter(|p| {
            p.changes
                .iter()
                .any(|c| matches!(c, Change::WriteSummary { .. }))
        })
        .count();
    let items = found
        .into_iter()
        .map(|summary| view(gateway, summary))
        .collect::<Result<_>>()?;
    Ok(SummariesView {
        period: period.as_str().to_owned(),
        items,
        more,
        waiting,
    })
}

/// A summary for the interface: its events resolved for a day, the summaries it was
/// built from for a longer period.
pub(crate) fn view(gateway: &mut Gateway, summary: Summary) -> Result<SummaryView> {
    let root = gateway.vault().root().to_owned();
    let owner = gateway.fresh_owner()?;
    let span = summary.span;
    // A longer period shows the summaries it was built from, not its many events.
    let day = span.period == Period::Day;
    let events = summary
        .events
        .iter()
        .filter(|_| day)
        .filter_map(|stem| WikiLink::parse(&format!("[[{stem}]]")))
        .map(|link| LinkView::from(owner.link(&LinkOrText::Link(link))))
        .collect();
    let parts = span
        .period
        .shorter()
        .map(|shorter| {
            span.parts(shorter)
                .into_iter()
                .filter(|part| root.join(part.file()).is_file())
                .map(|part| PartView {
                    period: part.period.as_str().to_owned(),
                    start: part.start.to_string(),
                    label: part.label(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(SummaryView {
        period: span.period.as_str().to_owned(),
        start: span.start.to_string(),
        label: span.label(),
        text: summary.text,
        events,
        parts,
    })
}

/// The day summaries from `from` to `to`, both included, for the agenda.
///
/// # Errors
///
/// Errors reading the summaries.
pub(crate) fn days(
    gateway: &Gateway,
    from: jiff::civil::Date,
    to: jiff::civil::Date,
) -> Result<Vec<DaySummary>> {
    let root = gateway.vault().root();
    let (found, _) = summaries::list(root, Period::Day, (Some(from), Some(to)), 0, usize::MAX)?;
    Ok(found
        .into_iter()
        .map(|s| DaySummary {
            day: s.span.start.to_string(),
            text: s.text,
        })
        .collect())
}

/// How many days a summary stays on the home screen after its period ends (issue #261).
const FRESH_DAYS: i64 = 7;

/// A one-line summary at most this long only repeats the agenda (`A quiet day: yoga.`):
/// it stays in the summaries' screen, not on the home screen.
const THIN_CHARS: usize = 80;

/// Summaries of one period looked at for the home screen, newest first.
const LOOKED_AT: usize = 7;

/// Whether `summary` says more than one short line: a single line of at most
/// [`THIN_CHARS`] characters does not.
fn worth_showing(summary: &Summary) -> bool {
    let text = summary.text.trim();
    text.lines().filter(|l| !l.trim().is_empty()).count() > 1 || text.chars().count() > THIN_CHARS
}

/// The summary for the home screen on `today`: the latest day's whose day ended less
/// than [`FRESH_DAYS`] days ago, else the latest week's, else the latest month's, each
/// skipped when it is only one short line. `None` when none is recent (issue #261).
///
/// # Errors
///
/// Errors reading the summaries.
pub(crate) fn latest(
    gateway: &mut Gateway,
    today: jiff::civil::Date,
) -> Result<Option<SummaryView>> {
    let root = gateway.vault().root().to_owned();
    let oldest = today.saturating_sub(FRESH_DAYS.days());
    for period in [Period::Day, Period::Week, Period::Month] {
        let (found, _) = summaries::list(&root, period, (None, Some(today)), 0, LOOKED_AT)?;
        let chosen = found
            .into_iter()
            .take_while(|s| s.span.end() > oldest)
            .find(worth_showing);
        if let Some(summary) = chosen {
            return view(gateway, summary).map(Some);
        }
    }
    Ok(None)
}

/// `POST /api/summaries/<period>/<start>` with `{"text": …}`: the person corrects a
/// summary. `None` for an address that names no period.
///
/// # Errors
///
/// [`Error::Refused`] when there is no such summary or the text is empty or too long;
/// errors writing the file or the history.
pub(crate) fn correct(
    gateway: &mut Gateway,
    path: &str,
    body: &[u8],
) -> Result<Option<SummaryView>> {
    let Some((period, start)) = path
        .strip_prefix("summaries/")
        .and_then(|rest| rest.split_once('/'))
    else {
        return Ok(None);
    };
    let span = Span::parse(period, start)?;
    let body: CorrectBody = serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))?;
    summaries::correct(gateway.vault(), span, &body.text)?;
    let root = gateway.vault().root().to_owned();
    let summary = summaries::read(&root, span)
        .ok_or_else(|| Error::Refused(format!("There is no summary {}.", span.label())))?;
    view(gateway, summary).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> Summary {
        Summary {
            span: Span::holding(Period::Day, jiff::civil::date(2026, 11, 26)),
            text: text.to_owned(),
            events: Vec::new(),
        }
    }

    #[test]
    fn one_short_line_is_not_worth_the_home_screen() {
        assert!(!worth_showing(&day("A quiet day: yoga.")));
        assert!(!worth_showing(&day(
            "  Team meeting, then lunch with Emma.  \n"
        )));
        assert!(worth_showing(&day(
            "Lunch with Aaron.\nA film in the evening."
        )));
        assert!(worth_showing(&day(&"A long walk by the lake. ".repeat(4))));
    }
}
