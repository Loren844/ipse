//! The demo's dates (issue #269): what the demo made in one go is spread over the last
//! days, so that it does not read "now" everywhere. Proposals waiting for the person
//! were made over the last two days, decided ones over the last two weeks, each at an
//! hour someone is awake; the sources were imported three weeks ago, and the agenda
//! and the location history imported again this morning.

use std::path::Path;

use ipse_core::connector::CONNECTORS_DIR;
use ipse_core::proposals::{Change, Proposal, Proposals, Status};
use ipse_core::vault::{Vault, write_atomic};
use ipse_core::{Error, Result};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{ToSpan, Zoned};
use serde_json::Value;

/// Days back the decided proposals were made: from two weeks ago to two days ago.
const DECIDED_DAYS: (i64, i64) = (2, 14);

/// Days back the waiting proposals were made: yesterday or today.
const WAITING_DAYS: (i64, i64) = (0, 1);

/// Days back the sources were first imported.
const IMPORTED_DAYS: i64 = 21;

/// Sources imported again this morning.
const AGAIN: [&str; 2] = ["calendar", "google-timeline"];

/// A small deterministic generator (xorshift), apart from the demo's: dates do not
/// change who is in it.
struct Seed(u64);

impl Seed {
    fn below(&mut self, n: i64) -> i64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        i64::try_from(self.0 % n.unsigned_abs().max(1)).unwrap_or(0)
    }
}

/// `day` at `hour`:`minute` in `zone`, or `None` for a time that does not exist.
fn at(zone: &TimeZone, day: Date, hour: i64, minute: i64) -> Option<Zoned> {
    let hour = i8::try_from(hour).ok()?;
    let minute = i8::try_from(minute).ok()?;
    let time = jiff::civil::Time::new(hour, minute, 0, 0).ok()?;
    day.to_datetime(time).to_zoned(zone.clone()).ok()
}

/// `n` times between `days.1` and `days.0` days before `now`, each between 8:00 and
/// 22:00 and before `now`, oldest first, no two the same minute.
fn times(n: usize, days: (i64, i64), now: &Zoned, seed: &mut Seed) -> Vec<Zoned> {
    let zone = now.time_zone();
    let today = now.date();
    let latest = now.saturating_sub(15.minutes());
    let mut out: Vec<Zoned> = Vec::with_capacity(n);
    for _ in 0..n {
        let mut back = days.0 + seed.below(days.1 - days.0 + 1);
        let (hour, minute) = (8 + seed.below(14), seed.below(60));
        let time = loop {
            let day = today.saturating_sub(back.days());
            match at(zone, day, hour, minute) {
                Some(time) if time <= latest => break time,
                // Later than now today: the same hour, a day earlier.
                _ => back += 1,
            }
        };
        out.push(time);
    }
    out.sort();
    for i in 1..out.len() {
        if out[i] <= out[i - 1] {
            out[i] = out[i - 1].saturating_add(1.minute());
        }
    }
    out
}

/// RFC 3339 with the offset, as Ipse writes times: `2026-11-14T21:05:00+01:00`.
fn text(time: &Zoned) -> String {
    time.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Dates the proposals and the sources' runs of the demo in `vault`, made at `now`.
///
/// # Errors
///
/// Errors reading or writing the proposals or the log of runs.
pub(crate) fn spread(vault: &Vault, now: &Zoned) -> Result<()> {
    let mut seed = Seed(0x2690_d3e0_5eed_0001);
    proposals(vault, now, &mut seed)?;
    runs(vault.root(), now, &mut seed)
}

fn proposals(vault: &Vault, now: &Zoned, seed: &mut Seed) -> Result<()> {
    // Oldest first, as their ids were made.
    let mut all = Proposals::new(vault).list()?;
    all.reverse();
    let (waiting, decided): (Vec<_>, Vec<_>) =
        all.into_iter().partition(|p| p.status == Status::Waiting);
    let latest = now.saturating_sub(10.minutes());
    let mut dated: Vec<(&Proposal, Zoned)> = Vec::with_capacity(decided.len() + waiting.len());
    let decided_at = times(decided.len(), DECIDED_DAYS, now, seed);
    let waiting_at = times(waiting.len(), WAITING_DAYS, now, seed);
    for (proposal, created) in decided
        .iter()
        .zip(decided_at)
        .chain(waiting.iter().zip(waiting_at))
    {
        // A day's summary is written once the day is over: the next morning at the
        // earliest.
        let created = match summary_day(proposal).and_then(|day| {
            at(
                now.time_zone(),
                day.saturating_add(1.day()),
                7,
                30 + seed.below(60),
            )
        }) {
            Some(morning) if morning > created => morning.min(latest.clone()),
            _ => created,
        };
        dated.push((proposal, created));
    }
    // No two the same minute.
    dated.sort_by(|a, b| a.1.cmp(&b.1));
    for i in 1..dated.len() {
        if dated[i].1 <= dated[i - 1].1 {
            dated[i].1 = dated[i - 1].1.saturating_add(1.minute());
        }
    }
    let dir = vault.root().join(".ipse/proposals");
    for (proposal, created) in &dated {
        let decision = (proposal.status != Status::Waiting).then(|| {
            // Decided within a few hours, the same evening at the latest, before now.
            let evening = at(now.time_zone(), created.date(), 22, 45)
                .filter(|e| e > created)
                .unwrap_or_else(|| created.saturating_add(10.minutes()));
            created
                .saturating_add((10 + seed.below(170)).minutes())
                .min(evening)
                .min(latest.clone())
                .max(created.clone())
        });
        redate(
            &dir.join(format!("{}.json", proposal.id)),
            created,
            decision.as_ref(),
        )?;
    }
    Ok(())
}

/// The day a proposal summarizes, for a day's summary.
fn summary_day(proposal: &Proposal) -> Option<Date> {
    proposal.changes.iter().find_map(|change| match change {
        Change::WriteSummary { period, start, .. } if period == "day" => start.parse().ok(),
        _ => None,
    })
}

/// Sets when the proposal in `path` was made, and when it was decided.
fn redate(path: &Path, created: &Zoned, decided: Option<&Zoned>) -> Result<()> {
    let mut proposal = read_json(path)?;
    proposal["created"] = Value::String(text(created));
    if let (Some(decided), Some(status)) = (decided, proposal.get_mut("status"))
        && status.get("at").is_some()
    {
        status["at"] = Value::String(text(decided));
    }
    let mut out = serde_json::to_string_pretty(&proposal).map_err(|_| invalid(path))?;
    out.push('\n');
    write_atomic(path, out.as_bytes())
}

fn runs(root: &Path, now: &Zoned, seed: &mut Seed) -> Result<()> {
    let path = root.join(CONNECTORS_DIR).join("runs.jsonl");
    let text_of = std::fs::read_to_string(&path).map_err(|source| Error::Io {
        action: "read",
        path: path.clone(),
        source,
    })?;
    let mut lines: Vec<Value> = text_of
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|_| invalid(&path)))
        .collect::<Result<_>>()?;
    // One evening three weeks ago, a minute or two apart.
    let zone = now.time_zone();
    let day = now.date().saturating_sub(IMPORTED_DAYS.days());
    let first = at(zone, day, 20, 10 + seed.below(30))
        .unwrap_or_else(|| now.saturating_sub(IMPORTED_DAYS.days()));
    let mut again = Vec::new();
    for (i, run) in lines.iter_mut().enumerate() {
        let minutes = i64::try_from(i).unwrap_or(0) * 2;
        run["at"] = Value::String(text(&first.saturating_add(minutes.minutes())));
        let instance = run["instance"].as_str().unwrap_or_default();
        if AGAIN.contains(&instance) {
            again.push(run.clone());
        }
    }
    // This morning, nothing new since: everything already up to date.
    let morning = at(zone, now.date(), 8, 5 + seed.below(40))
        .filter(|t| *t <= now.saturating_sub(15.minutes()))
        .unwrap_or_else(|| now.saturating_sub(20.minutes()));
    for (i, mut run) in again.into_iter().enumerate() {
        let minutes = i64::try_from(i).unwrap_or(0);
        run["at"] = Value::String(text(&morning.saturating_add(minutes.minutes())));
        let known = run["added"].as_u64().unwrap_or(0) + run["updated"].as_u64().unwrap_or(0);
        for key in ["added", "updated", "tasks", "written"] {
            run[key] = Value::from(0);
        }
        run["unchanged"] = Value::from(known);
        lines.push(run);
    }
    let mut out = String::new();
    for run in &lines {
        out.push_str(&serde_json::to_string(run).map_err(|_| invalid(&path))?);
        out.push('\n');
    }
    write_atomic(&path, out.as_bytes())
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        action: "read",
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|_| invalid(path))
}

fn invalid(path: &Path) -> Error {
    Error::Invalid {
        path: path.to_owned(),
        line: None,
        problem: "the demo could not date it".to_owned(),
    }
}
