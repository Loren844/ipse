//! The map's API (issue #135): the places with a position, how often one went there,
//! when last, and who was there.

use std::collections::HashMap;

use ipse_core::Result;
use ipse_core::rights::{Categories, Gateway};
use ipse_core::vault::{LinkOrText, MemoryFile, WikiLink};
use serde::Serialize;

use super::api::LinkView;

/// Someone seen at a place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MetView {
    person: LinkView,
    /// Events there they are in.
    events: u32,
}

/// A place on the map.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MapPlaceView {
    id: String,
    name: String,
    /// WGS 84, decimal degrees.
    lat: f64,
    lon: f64,
    /// Events there.
    visits: u32,
    /// When the last one started.
    last: Option<String>,
    people: Vec<MetView>,
    /// Its category: `location` for the places location history brought.
    category: String,
    /// True when its category is closed to AIs unless the person opens it.
    sensitive: bool,
}

/// What `GET /api/map` answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct MapView {
    /// The places with a position, the most visited first.
    places: Vec<MapPlaceView>,
    /// Places without a position, left off the map.
    without_position: u32,
}

/// The places of the memory on a map, as the person sees them.
///
/// # Errors
///
/// Errors reading the index or the places' files. A place file that cannot be read
/// is left off the map.
pub(crate) fn view(gateway: &mut Gateway) -> Result<MapView> {
    let root = gateway.vault().root().to_owned();
    let all = Categories::of(gateway.settings());
    let sensitive: Vec<String> = all.sensitive().into_iter().map(str::to_owned).collect();
    let owner = gateway.fresh_owner()?;
    let visited: HashMap<String, _> = owner
        .places_visited()
        .into_iter()
        .map(|v| (v.stem.clone(), v))
        .collect();
    let mut places = Vec::new();
    let mut without_position = 0_u32;
    for place in owner.places() {
        let geo = MemoryFile::read(root.join(place.path))
            .ok()
            .and_then(|file| file.entity().and_then(|e| e.geo));
        let Some(geo) = geo else {
            without_position += 1;
            continue;
        };
        let stem = place
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let seen = visited.get(&stem);
        places.push(MapPlaceView {
            id: place.id.to_string(),
            name: place.name.to_owned(),
            lat: geo.lat(),
            lon: geo.lon(),
            visits: seen.map_or(0, |v| v.events),
            last: seen.map(|v| v.last.to_string()),
            people: seen
                .map(|v| {
                    v.people
                        .iter()
                        .map(|m| MetView {
                            person: LinkView::from(owner.link(&LinkOrText::Link(WikiLink {
                                target: m.stem.clone(),
                                display: None,
                            }))),
                            events: m.events,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            category: place.category.as_str().to_owned(),
            sensitive: sensitive.iter().any(|s| s == place.category.as_str()),
        });
    }
    places.sort_by(|a, b| b.visits.cmp(&a.visits).then_with(|| a.name.cmp(&b.name)));
    Ok(MapView {
        places,
        without_position,
    })
}
