//! Stays found in a phone's positions, for connectors that read them from a server of
//! the person's (issue #208): a run of positions within [`RADIUS`] of one another for at
//! least [`MIN_STAY`] is a stay, sent as a `location.visit` record
//! (`docs/connectors/location.md`).
//!
//! Phones send their position when it changes, rarely while they stay put: a stay ends
//! at the first position away from it, when that one comes within [`LONGEST_GAP`]. A
//! lone position counts only in a zone the server knows (home, work).

use jiff::{SignedDuration, Timestamp};
use serde_json::json;

use ipse_core::materialize::meters;
use ipse_protocol::{Op, Record};

/// How far from the middle of a stay a position may be and still be in it, in meters.
pub(crate) const RADIUS: f64 = 100.0;

/// The shortest stay: a stop at a red light is not one.
pub(crate) const MIN_STAY: SignedDuration = SignedDuration::from_mins(10);

/// Positions less precise than this, in meters, are left out.
pub(crate) const WORST_ACCURACY: f64 = 250.0;

/// The longest a phone may stay silent and its next position still end the stay at
/// that time; after a longer silence the stay ends at its last position.
pub(crate) const LONGEST_GAP: SignedDuration = SignedDuration::from_hours(12);

/// The record kind of a stay.
pub(crate) const VISIT: &str = "location.visit";

/// One position of the phone.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Position {
    /// When it was taken.
    pub(crate) at: Timestamp,
    pub(crate) lat: f64,
    pub(crate) lon: f64,
    /// How precise it is, in meters, when known.
    pub(crate) accuracy: Option<f64>,
    /// The name of the zone the server placed it in (`home`, `Travail`), when it did.
    pub(crate) zone: Option<String>,
}

/// One stay.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stay {
    pub(crate) start: Timestamp,
    pub(crate) end: Timestamp,
    /// The middle of its positions.
    pub(crate) lat: f64,
    pub(crate) lon: f64,
    /// The zone most of its positions were in.
    pub(crate) zone: Option<String>,
}

/// The positions gathered while the phone stays near one place.
struct Run {
    first: Timestamp,
    last: Timestamp,
    lat: f64,
    lon: f64,
    count: f64,
    zones: Vec<String>,
}

impl Run {
    fn new(p: &Position) -> Self {
        Self {
            first: p.at,
            last: p.at,
            lat: p.lat,
            lon: p.lon,
            count: 1.0,
            zones: p.zone.iter().cloned().collect(),
        }
    }

    fn near(&self, p: &Position) -> bool {
        meters((self.lat, self.lon), (p.lat, p.lon)) <= RADIUS
    }

    fn add(&mut self, p: &Position) {
        self.count += 1.0;
        self.lat += (p.lat - self.lat) / self.count;
        self.lon += (p.lon - self.lon) / self.count;
        self.last = p.at;
        self.zones.extend(p.zone.iter().cloned());
    }

    /// The stay this run is, ended by a position at `next`, if it lasted long enough.
    /// A lone position is a stay only in a zone of the server: on the way, the next
    /// position can come long after it.
    fn stay(&self, next: Option<Timestamp>) -> Option<Stay> {
        let end = next
            .filter(|n| n.duration_since(self.last) <= LONGEST_GAP)
            .unwrap_or(self.last);
        let stopped = self.count >= 2.0 || !self.zones.is_empty();
        (stopped && end.duration_since(self.first) >= MIN_STAY).then(|| Stay {
            start: self.first,
            end,
            lat: self.lat,
            lon: self.lon,
            zone: most(&self.zones),
        })
    }
}

/// The name that comes most often, the first of them on a tie.
fn most(names: &[String]) -> Option<String> {
    let mut best: Option<(&String, usize)> = None;
    for name in names {
        let count = names.iter().filter(|n| *n == name).count();
        if best.is_none_or(|(_, c)| count > c) {
            best = Some((name, count));
        }
    }
    best.map(|(name, _)| name.clone())
}

/// The stays that ended among `positions`, in order, and the time the positions of the
/// one going on start from, to read again at the next session; `None` when there is
/// none.
pub(crate) fn stays(positions: &[Position]) -> (Vec<Stay>, Option<Timestamp>) {
    let mut sorted: Vec<&Position> = positions
        .iter()
        .filter(|p| p.accuracy.is_none_or(|a| a <= WORST_ACCURACY))
        .filter(|p| p.lat.is_finite() && p.lon.is_finite())
        .collect();
    sorted.sort_by_key(|p| p.at);
    let mut found = Vec::new();
    let mut run: Option<Run> = None;
    for p in sorted {
        match &mut run {
            Some(current) if current.near(p) => current.add(p),
            Some(current) => {
                found.extend(current.stay(Some(p.at)));
                run = Some(Run::new(p));
            }
            None => run = Some(Run::new(p)),
        }
    }
    (found, run.map(|r| r.first))
}

/// One `upsert` of a `location.visit` per stay; a stay in the zone `home` is said to be
/// at home.
pub(crate) fn records(stays: &[Stay]) -> Vec<Record> {
    let round = |x: f64| (x * 1e6).round() / 1e6;
    stays
        .iter()
        .map(|stay| {
            let (lat, lon) = (round(stay.lat), round(stay.lon));
            let mut data = json!({
                "start": stay.start.to_string(),
                "end": stay.end.to_string(),
                "lat": lat,
                "lon": lon,
            });
            match stay.zone.as_deref() {
                Some("home") => data["semantic"] = json!("home"),
                Some(zone) => data["name"] = json!(zone),
                None => {}
            }
            Record {
                kind: VISIT.to_owned(),
                op: Op::Upsert,
                external_id: Some(format!(
                    "visit:{}:{lat:.6},{lon:.6}",
                    stay.start.as_second()
                )),
                observed_at: stay.start,
                data: Some(data),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: (f64, f64) = (48.8566, 2.3522);
    const WORK: (f64, f64) = (48.8738, 2.2950);

    fn at(time: &str) -> Timestamp {
        format!("2026-11-21T{time}:00Z").parse().unwrap()
    }

    fn p(time: &str, (lat, lon): (f64, f64), zone: Option<&str>) -> Position {
        Position {
            at: at(time),
            lat,
            lon,
            accuracy: Some(20.0),
            zone: zone.map(str::to_owned),
        }
    }

    #[test]
    fn a_stay_ends_at_the_first_position_away() {
        let positions = [
            p("07:00", HOME, Some("home")),
            // 30 m away, still at home.
            p("07:20", (HOME.0 + 0.0003, HOME.1), Some("home")),
            // On the way, a position every few minutes: no stay.
            p("08:10", (48.8650, 2.3200), None),
            p("08:12", (48.8652, 2.3202), None),
            p("08:17", (48.8690, 2.3100), None),
            p("08:24", (48.8720, 2.3000), None),
            p("08:30", WORK, Some("Travail")),
            p("12:00", WORK, None),
            p("18:00", (48.8600, 2.3400), None),
            p("18:30", HOME, Some("home")),
        ];
        let (found, open) = stays(&positions);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!((found[0].start, found[0].end), (at("07:00"), at("08:10")));
        assert_eq!(found[0].zone.as_deref(), Some("home"));
        assert!(meters((found[0].lat, found[0].lon), HOME) < 20.0);
        assert_eq!((found[1].start, found[1].end), (at("08:30"), at("18:00")));
        assert_eq!(found[1].zone.as_deref(), Some("Travail"));
        // At home since 18:30: read again from there.
        assert_eq!(open, Some(at("18:30")));

        let sent = records(&found);
        assert_eq!(sent[0].data.as_ref().unwrap()["semantic"], "home");
        assert_eq!(sent[1].data.as_ref().unwrap()["name"], "Travail");
        assert_eq!(
            sent[1].external_id.as_deref(),
            Some("visit:1795249800:48.873800,2.295000")
        );
    }

    #[test]
    fn imprecise_positions_and_long_silences() {
        let mut far = p("09:00", (48.90, 2.40), None);
        far.accuracy = Some(1500.0);
        let positions = [
            p("00:00", HOME, None),
            far,
            // The phone was off from midnight: the stay ends at its last position, and
            // is too short.
            p("13:00", WORK, None),
            p("13:30", WORK, None),
            p("14:00", HOME, None),
        ];
        let (found, open) = stays(&positions);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].start, found[0].end), (at("13:00"), at("14:00")));
        assert_eq!(open, Some(at("14:00")));
        assert_eq!(stays(&[]), (Vec::new(), None));
    }
}
