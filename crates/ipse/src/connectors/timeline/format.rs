//! Reading Google Maps Timeline exports as a stream: visits, points and frequent
//! places are handed out one by one, as the file is read, never the whole file at once
//! (a year holds hundreds of thousands of points).
//!
//! Three shapes are read:
//! - the export from an Android phone (`Timeline.json`): an object with
//!   `semanticSegments` (visits, paths), `rawSignals` (positions) and
//!   `userLocationProfile` (frequent places, such as home and work);
//! - the export from an iPhone (`location-history.json`): an array of segments, with
//!   `geo:` positions and numbers written as text;
//! - the older Google Takeout: `timelineObjects` (monthly files of Semantic Location
//!   History, with places' names and addresses) and `locations` (`Records.json`).

use std::fmt;
use std::io::Read;

use jiff::Timestamp;
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

/// A stay somewhere.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Visit {
    pub(crate) start: Timestamp,
    pub(crate) end: Timestamp,
    pub(crate) lat: f64,
    pub(crate) lon: f64,
    /// Google's identifier of the place.
    pub(crate) place_id: Option<String>,
    /// Its name and address, in the older exports only.
    pub(crate) name: Option<String>,
    pub(crate) address: Option<String>,
    /// `home` or `work`, when Google says so.
    pub(crate) semantic: Option<&'static str>,
    pub(crate) probability: Option<f64>,
}

/// One position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    /// Unix time, seconds.
    pub(crate) t: i64,
    pub(crate) lat: f64,
    pub(crate) lon: f64,
    /// Accuracy in meters, when known.
    pub(crate) acc: Option<f64>,
}

/// A place Google marked as frequent: home or work.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frequent {
    pub(crate) place_id: String,
    pub(crate) label: &'static str,
    pub(crate) lat: Option<f64>,
    pub(crate) lon: Option<f64>,
}

/// What reading a file hands out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Found {
    Visit(Visit),
    Point(Point),
    Frequent(Frequent),
}

/// Why a file cannot be read.
#[derive(Debug)]
pub(crate) enum ReadError {
    /// Not a Timeline export: not JSON, or none of the known keys.
    NotTimeline,
    /// The sink refused an item (the connector could not send it).
    Stopped(String),
}

/// Reads one export, handing each item to `sink` as it is read. Returns how many items
/// were handed out.
pub(crate) fn read(
    reader: impl Read,
    sink: &mut dyn FnMut(Found) -> Result<(), String>,
) -> Result<u64, ReadError> {
    let mut count = 0_u64;
    let mut counted = |found: Found| {
        count += 1;
        sink(found)
    };
    let mut de = serde_json::Deserializer::from_reader(std::io::BufReader::new(reader));
    let known = Top { sink: &mut counted }
        .deserialize(&mut de)
        .map_err(|e| {
            let text = e.to_string();
            match text.strip_prefix(STOPPED) {
                Some(reason) => ReadError::Stopped(reason.to_owned()),
                None => ReadError::NotTimeline,
            }
        })?;
    de.end().map_err(|_| ReadError::NotTimeline)?;
    if known {
        Ok(count)
    } else {
        Err(ReadError::NotTimeline)
    }
}

/// Marks an error of the sink, to tell it from a malformed file.
const STOPPED: &str = "stopped: ";

type Sink<'s> = &'s mut dyn FnMut(Found) -> Result<(), String>;

/// The whole file: an object (Android, Takeout) or an array (iPhone). Says whether a
/// known key was found.
struct Top<'s> {
    sink: Sink<'s>,
}

impl<'de> DeserializeSeed<'de> for Top<'_> {
    type Value = bool;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<bool, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Top<'_> {
    type Value = bool;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a Google Maps Timeline export")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<bool, A::Error> {
        let mut known = false;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "semanticSegments" => {
                    map.next_value_seed(Each::<Segment>::new(self.sink))?;
                    known = true;
                }
                "rawSignals" => {
                    map.next_value_seed(Each::<RawSignal>::new(self.sink))?;
                    known = true;
                }
                "userLocationProfile" => {
                    let profile: Profile = map.next_value()?;
                    for found in profile.found() {
                        (self.sink)(found).map_err(stopped)?;
                    }
                    known = true;
                }
                "timelineObjects" => {
                    map.next_value_seed(Each::<TimelineObject>::new(self.sink))?;
                    known = true;
                }
                "locations" => {
                    map.next_value_seed(Each::<RecordsLocation>::new(self.sink))?;
                    known = true;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(known)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<bool, A::Error> {
        Each::<Segment>::new(self.sink).visit_seq(seq)?;
        Ok(true)
    }
}

#[allow(clippy::needless_pass_by_value)] // called by `map_err`, which hands over the error
fn stopped<E: de::Error>(reason: String) -> E {
    E::custom(format!("{STOPPED}{reason}"))
}

/// Something read from an array, one element at a time, that hands out what it holds.
trait Items {
    fn found(self) -> Vec<Found>;
}

/// Reads an array of `T`, handing out each element's items before reading the next.
struct Each<'s, T> {
    sink: Sink<'s>,
    kind: std::marker::PhantomData<T>,
}

impl<'s, T> Each<'s, T> {
    fn new(sink: Sink<'s>) -> Self {
        Self {
            sink,
            kind: std::marker::PhantomData,
        }
    }
}

impl<'de, T: Deserialize<'de> + Items> DeserializeSeed<'de> for Each<'_, T> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de, T: Deserialize<'de> + Items> Visitor<'de> for Each<'_, T> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        // An element that does not have the expected shape is skipped, not fatal.
        while let Some(item) = seq.next_element::<Lenient<T>>()? {
            if let Lenient(Some(item)) = item {
                for found in item.found() {
                    (self.sink)(found).map_err(stopped)?;
                }
            }
        }
        Ok(())
    }
}

/// `T`, or nothing when the element has another shape.
struct Lenient<T>(Option<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Lenient<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(Self(T::deserialize(value).ok()))
    }
}

/// A position as the exports write it: `48.8566°, 2.3522°`, `geo:48.8566,2.3522`, or an
/// object `{"latLng": "…"}`.
fn position(value: &Value) -> Option<(f64, f64)> {
    let text = match value {
        Value::String(s) => s.as_str(),
        Value::Object(map) => map
            .get("latLng")
            .or_else(|| map.get("LatLng"))
            .and_then(Value::as_str)?,
        _ => return None,
    };
    let text = text.trim();
    let text = text.strip_prefix("geo:").unwrap_or(text);
    let (lat, lon) = text.split_once(',')?;
    let number = |t: &str| t.trim().trim_end_matches('°').trim().parse::<f64>().ok();
    let (lat, lon) = (number(lat)?, number(lon)?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// A position in `E7` integers, as the older exports write it.
fn e7(lat: Option<i64>, lon: Option<i64>) -> Option<(f64, f64)> {
    #[allow(clippy::cast_precision_loss)] // degrees × 10⁷ fit in an f64's 52 bits
    let (lat, lon) = (lat? as f64 / 1e7, lon? as f64 / 1e7);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// A number written as a number or as text.
fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A time as RFC 3339 text, or milliseconds since 1970 as text (`timestampMs`).
fn time(text: Option<&str>) -> Option<Timestamp> {
    let text = text?.trim();
    if let Ok(t) = text.parse::<Timestamp>() {
        return Some(t);
    }
    let ms: i64 = text.parse().ok()?;
    Timestamp::from_millisecond(ms).ok()
}

/// `home` or `work` from Google's place types (`HOME`, `INFERRED_HOME`, `Home`…).
fn semantic(text: Option<&str>) -> Option<&'static str> {
    let text = text?.to_ascii_lowercase();
    if text.contains("home") {
        Some("home")
    } else if text.contains("work") {
        Some("work")
    } else {
        None
    }
}

/// A segment of the phone exports: a visit, or a path.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Segment {
    start_time: Option<String>,
    end_time: Option<String>,
    visit: Option<SegmentVisit>,
    timeline_path: Option<Vec<PathPoint>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SegmentVisit {
    probability: Option<Value>,
    top_candidate: Option<Candidate>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    #[serde(alias = "placeID")]
    place_id: Option<String>,
    semantic_type: Option<String>,
    place_location: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathPoint {
    point: Option<Value>,
    time: Option<String>,
    duration_minutes_offset_from_start_time: Option<Value>,
}

impl Items for Segment {
    fn found(self) -> Vec<Found> {
        let start = time(self.start_time.as_deref());
        let end = time(self.end_time.as_deref());
        let mut out = Vec::new();
        if let (Some(visit), Some(start), Some(end)) = (self.visit, start, end) {
            let candidate = visit.top_candidate;
            let at = candidate
                .as_ref()
                .and_then(|c| c.place_location.as_ref())
                .and_then(position);
            if let (Some(candidate), Some((lat, lon))) = (candidate, at) {
                out.push(Found::Visit(Visit {
                    start,
                    end,
                    lat,
                    lon,
                    place_id: candidate.place_id.filter(|p| !p.is_empty()),
                    name: None,
                    address: None,
                    semantic: semantic(candidate.semantic_type.as_deref()),
                    probability: number(visit.probability.as_ref()),
                }));
            }
        }
        for point in self.timeline_path.unwrap_or_default() {
            let Some((lat, lon)) = point.point.as_ref().and_then(position) else {
                continue;
            };
            let t = time(point.time.as_deref()).or_else(|| {
                let minutes = number(point.duration_minutes_offset_from_start_time.as_ref())?;
                #[allow(clippy::cast_possible_truncation)] // minutes within a segment
                let seconds = (minutes * 60.0) as i64;
                start?
                    .checked_add(jiff::SignedDuration::from_secs(seconds))
                    .ok()
            });
            if let Some(t) = t {
                out.push(Found::Point(Point {
                    t: t.as_second(),
                    lat,
                    lon,
                    acc: None,
                }));
            }
        }
        out
    }
}

/// One of the Android export's raw signals; only positions are kept.
#[derive(Deserialize)]
struct RawSignal {
    position: Option<RawPosition>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPosition {
    #[serde(rename = "LatLng", alias = "latLng")]
    lat_lng: Option<Value>,
    accuracy_meters: Option<Value>,
    timestamp: Option<String>,
}

impl Items for RawSignal {
    fn found(self) -> Vec<Found> {
        let Some(p) = self.position else {
            return Vec::new();
        };
        let (Some((lat, lon)), Some(t)) = (
            p.lat_lng.as_ref().and_then(position),
            time(p.timestamp.as_deref()),
        ) else {
            return Vec::new();
        };
        vec![Found::Point(Point {
            t: t.as_second(),
            lat,
            lon,
            acc: number(p.accuracy_meters.as_ref()),
        })]
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    #[serde(default)]
    frequent_places: Vec<FrequentPlace>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrequentPlace {
    place_id: Option<String>,
    place_location: Option<Value>,
    label: Option<String>,
}

impl Profile {
    fn found(self) -> Vec<Found> {
        self.frequent_places
            .into_iter()
            .filter_map(|p| {
                let label = semantic(p.label.as_deref())?;
                let at = p.place_location.as_ref().and_then(position);
                Some(Found::Frequent(Frequent {
                    place_id: p.place_id.filter(|id| !id.is_empty())?,
                    label,
                    lat: at.map(|(lat, _)| lat),
                    lon: at.map(|(_, lon)| lon),
                }))
            })
            .collect()
    }
}

/// An item of the older Semantic Location History.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineObject {
    place_visit: Option<PlaceVisit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceVisit {
    location: Option<TakeoutLocation>,
    duration: Option<Duration>,
    visit_confidence: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TakeoutLocation {
    latitude_e7: Option<i64>,
    longitude_e7: Option<i64>,
    place_id: Option<String>,
    name: Option<String>,
    address: Option<String>,
    semantic_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Duration {
    start_timestamp: Option<String>,
    end_timestamp: Option<String>,
    start_timestamp_ms: Option<String>,
    end_timestamp_ms: Option<String>,
}

impl Items for TimelineObject {
    fn found(self) -> Vec<Found> {
        let Some(visit) = self.place_visit else {
            return Vec::new();
        };
        let (Some(location), Some(duration)) = (visit.location, visit.duration) else {
            return Vec::new();
        };
        let start = time(
            duration
                .start_timestamp
                .as_deref()
                .or(duration.start_timestamp_ms.as_deref()),
        );
        let end = time(
            duration
                .end_timestamp
                .as_deref()
                .or(duration.end_timestamp_ms.as_deref()),
        );
        let (Some(start), Some(end), Some((lat, lon))) =
            (start, end, e7(location.latitude_e7, location.longitude_e7))
        else {
            return Vec::new();
        };
        let text = |t: Option<String>| t.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
        vec![Found::Visit(Visit {
            start,
            end,
            lat,
            lon,
            place_id: text(location.place_id),
            name: text(location.name),
            address: text(location.address),
            semantic: semantic(location.semantic_type.as_deref()),
            probability: number(visit.visit_confidence.as_ref()).map(|c| c / 100.0),
        })]
    }
}

/// A position of the older `Records.json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordsLocation {
    latitude_e7: Option<i64>,
    longitude_e7: Option<i64>,
    accuracy: Option<Value>,
    timestamp: Option<String>,
    timestamp_ms: Option<String>,
}

impl Items for RecordsLocation {
    fn found(self) -> Vec<Found> {
        let at = e7(self.latitude_e7, self.longitude_e7);
        let t = time(self.timestamp.as_deref().or(self.timestamp_ms.as_deref()));
        match (at, t) {
            (Some((lat, lon)), Some(t)) => vec![Found::Point(Point {
                t: t.as_second(),
                lat,
                lon,
                acc: number(self.accuracy.as_ref()),
            })],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(text: &str) -> Result<Vec<Found>, ReadError> {
        let mut out = Vec::new();
        read(text.as_bytes(), &mut |f| {
            out.push(f);
            Ok(())
        })?;
        Ok(out)
    }

    #[test]
    fn positions_in_every_form() {
        assert_eq!(
            position(&Value::from("48.8566°, 2.3522°")),
            Some((48.8566, 2.3522))
        );
        assert_eq!(
            position(&Value::from("geo:48.856600,2.352200")),
            Some((48.8566, 2.3522))
        );
        assert_eq!(
            position(&serde_json::json!({"latLng": "-33.8688°, 151.2093°"})),
            Some((-33.8688, 151.2093))
        );
        assert_eq!(position(&Value::from("91°, 2°")), None);
        assert_eq!(position(&Value::from("nowhere")), None);
        assert_eq!(
            e7(Some(488_566_000), Some(23_522_000)),
            Some((48.8566, 2.3522))
        );
    }

    #[test]
    fn the_android_export() {
        let text = r#"{
          "semanticSegments": [
            {"startTime": "2026-03-01T19:30:00.000+01:00", "endTime": "2026-03-01T22:45:00.000+01:00",
             "startTimeTimezoneUtcOffsetMinutes": 60,
             "visit": {"hierarchyLevel": 0, "probability": 0.92,
               "topCandidate": {"placeId": "ChIJfake1", "semanticType": "UNKNOWN", "probability": 0.8,
                 "placeLocation": {"latLng": "48.8530°, 2.3690°"}}}},
            {"startTime": "2026-03-01T23:00:00.000+01:00", "endTime": "2026-03-02T08:00:00.000+01:00",
             "visit": {"topCandidate": {"placeId": "ChIJhome", "semanticType": "INFERRED_HOME",
               "placeLocation": {"latLng": "48.8600°, 2.3800°"}}}},
            {"startTime": "2026-03-01T22:45:00.000+01:00", "endTime": "2026-03-01T23:00:00.000+01:00",
             "timelinePath": [{"point": "48.8540°, 2.3700°", "time": "2026-03-01T22:50:00.000+01:00"}]},
            {"activity": {"distanceMeters": 1200}},
            "not a segment"
          ],
          "rawSignals": [
            {"position": {"LatLng": "48.8550°, 2.3710°", "accuracyMeters": 12, "timestamp": "2026-03-01T22:52:00.000+01:00"}},
            {"wifiScan": {"deliveryTime": "2026-03-01T22:53:00.000+01:00"}}
          ],
          "userLocationProfile": {"frequentPlaces": [
            {"placeId": "ChIJhome", "placeLocation": "48.8600°, 2.3800°", "label": "HOME"},
            {"placeId": "ChIJgym", "placeLocation": "48.8700°, 2.3900°"}
          ]}
        }"#;
        let found = all(text).unwrap();
        let Found::Visit(dinner) = &found[0] else {
            panic!("{found:?}")
        };
        assert_eq!(dinner.place_id.as_deref(), Some("ChIJfake1"));
        assert_eq!((dinner.lat, dinner.lon), (48.853, 2.369));
        assert_eq!(dinner.start.to_string(), "2026-03-01T18:30:00Z");
        assert_eq!(dinner.semantic, None);
        assert_eq!(dinner.probability, Some(0.92));
        let Found::Visit(home) = &found[1] else {
            panic!()
        };
        assert_eq!(home.semantic, Some("home"));
        let points: Vec<&Point> = found
            .iter()
            .filter_map(|f| match f {
                Found::Point(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].acc, Some(12.0));
        assert_eq!(
            found.last(),
            Some(&Found::Frequent(Frequent {
                place_id: "ChIJhome".into(),
                label: "home",
                lat: Some(48.86),
                lon: Some(2.38)
            }))
        );
    }

    #[test]
    fn the_iphone_export() {
        let text = r#"[
          {"endTime": "2026-03-01T22:45:00.000+01:00", "startTime": "2026-03-01T19:30:00.000+01:00",
           "visit": {"hierarchyLevel": "0", "topCandidate": {"probability": "0.81", "semanticType": "Work",
             "placeID": "ChIJwork", "placeLocation": "geo:48.875000,2.300000"}, "probability": "0.90"}},
          {"endTime": "2026-03-01T23:30:00.000+01:00", "startTime": "2026-03-01T22:45:00.000+01:00",
           "timelinePath": [{"point": "geo:48.870000,2.310000", "durationMinutesOffsetFromStartTime": "15"}]}
        ]"#;
        let found = all(text).unwrap();
        let Found::Visit(work) = &found[0] else {
            panic!()
        };
        assert_eq!(work.semantic, Some("work"));
        assert_eq!(work.place_id.as_deref(), Some("ChIJwork"));
        assert_eq!(work.probability, Some(0.9));
        let Found::Point(p) = &found[1] else { panic!() };
        assert_eq!(
            Timestamp::from_second(p.t).unwrap().to_string(),
            "2026-03-01T22:00:00Z"
        );
    }

    #[test]
    fn the_older_takeout() {
        let text = r#"{"timelineObjects": [
          {"placeVisit": {"location": {"latitudeE7": 488530000, "longitudeE7": 23690000,
             "placeId": "ChIJfake1", "address": "1 rue Inventée, Paris", "name": "Le Petit Zinc"},
           "duration": {"startTimestamp": "2022-05-04T18:00:00Z", "endTimestamp": "2022-05-04T20:00:00Z"},
           "visitConfidence": 87}},
          {"activitySegment": {"distance": 300}}
        ]}"#;
        let found = all(text).unwrap();
        assert_eq!(found.len(), 1);
        let Found::Visit(v) = &found[0] else { panic!() };
        assert_eq!(v.name.as_deref(), Some("Le Petit Zinc"));
        assert_eq!(v.address.as_deref(), Some("1 rue Inventée, Paris"));
        assert_eq!(v.probability, Some(0.87));

        let records = r#"{"locations": [
          {"latitudeE7": 488530000, "longitudeE7": 23690000, "accuracy": 20, "timestampMs": "1651687200000"},
          {"latitudeE7": 488531000, "longitudeE7": 23691000, "timestamp": "2022-05-04T18:01:00Z"}
        ]}"#;
        let found = all(records).unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn other_files_are_not_timelines() {
        for text in [
            "{\"something\": 1}",
            "not json",
            "{\"semanticSegments\": [",
            "42",
        ] {
            assert!(matches!(all(text), Err(ReadError::NotTimeline)), "{text}");
        }
        let mut stopped = |_: Found| Err("the core stopped".to_owned());
        let result = read(
            r#"{"rawSignals": [{"position": {"LatLng": "1°, 2°", "timestamp": "2026-01-01T00:00:00Z"}}]}"#
                .as_bytes(),
            &mut stopped,
        );
        assert!(matches!(result, Err(ReadError::Stopped(r)) if r.contains("the core stopped")));
    }
}
