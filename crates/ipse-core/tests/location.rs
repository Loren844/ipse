//! Location history (issue #106): visits in the journal become places, visit events and
//! the `location` state log; agenda events get their place; an AI sees none of it
//! unless the person opens the `location` category. All data is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use ipse_core::journal::Journal;
use ipse_core::materialize::{LocationSummary, location_to_vault, meters};
use ipse_core::rights::{Categories, Clients, Gateway};
use ipse_core::vault::{
    DateTime, EventTime, History, Language, LinkOrText, MemoryFile, Settings, Source, Vault,
};
use ipse_protocol::{Op, Record};
use serde_json::{Value, json};

const SOURCE: &str = "google-timeline";

fn vault(root: &Path) -> Vault {
    let vault = Vault::init(
        root,
        "Inès Roux",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let write = |rel: &str, text: &str| {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    // A place the person knows, with its position; a yoga studio without one; a dinner
    // whose place is an address; a class at the studio.
    write(
        "places/marche-bastille.md",
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z801\ntype: place\ncategory: places\nname: Marché Bastille\ngeo: [48.8532, 2.3703]\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n",
    );
    write(
        "places/studio-lotus.md",
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z802\ntype: place\ncategory: places\nname: Studio Lotus\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n",
    );
    write(
        "events/2026/2026-03-06-diner-chez-julie.md",
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z803\ntype: event\ncategory: agenda\ntitle: Dîner chez Julie\nstart: 2026-03-06T20:00:00+01:00\nend: 2026-03-06T23:00:00+01:00\nplace: 12 rue Inventée\nsources:\n  - calendar:abc\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n",
    );
    write(
        "events/2026/2026-03-10-yoga.md",
        "---\nid: 01jcx8m2p4q6r8t0v2w4y6z804\ntype: event\ncategory: agenda\ntitle: Yoga\nstart: 2026-03-10T19:00:00+01:00\nend: 2026-03-10T20:00:00+01:00\nplace: \"[[studio-lotus]]\"\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n",
    );
    History::open(root).unwrap().record_outside_edits().unwrap();
    vault
}

fn visit(
    start: &str,
    end: &str,
    place_id: &str,
    lat: f64,
    lon: f64,
    semantic: Option<&str>,
) -> Record {
    let mut data =
        json!({"start": start, "end": end, "lat": lat, "lon": lon, "place_id": place_id});
    if let Some(semantic) = semantic {
        data["semantic"] = json!(semantic);
    }
    Record {
        kind: "location.visit".into(),
        op: Op::Upsert,
        external_id: Some(format!("visit:{start}:{place_id}")),
        observed_at: start.parse().unwrap(),
        data: Some(data),
    }
}

fn visits() -> Vec<Record> {
    vec![
        // At Julie's: the dinner of the agenda.
        visit(
            "2026-03-06T18:55:00Z",
            "2026-03-06T22:10:00Z",
            "ChIJjulie",
            48.8641,
            2.3790,
            None,
        ),
        // Home, then twice at the market, once at a park.
        visit(
            "2026-03-06T22:40:00Z",
            "2026-03-07T08:30:00Z",
            "ChIJhome",
            48.8600,
            2.3900,
            Some("home"),
        ),
        visit(
            "2026-03-07T10:00:00Z",
            "2026-03-07T11:30:00Z",
            "ChIJmarket",
            48.8530,
            2.3700,
            None,
        ),
        visit(
            "2026-03-14T10:05:00Z",
            "2026-03-14T11:10:00Z",
            "ChIJmarket",
            48.8530,
            2.3700,
            None,
        ),
        visit(
            "2026-03-07T14:00:00Z",
            "2026-03-07T15:30:00Z",
            "ChIJpark",
            48.8462,
            2.3372,
            None,
        ),
        // At the yoga class.
        visit(
            "2026-03-10T18:02:00Z",
            "2026-03-10T19:01:00Z",
            "ChIJlotus",
            48.8710,
            2.3450,
            None,
        ),
    ]
}

fn import(vault: &Vault, records: &[Record]) -> LocationSummary {
    let mut journal = Journal::new(vault.root());
    journal.append(SOURCE, records, None).unwrap();
    let history = vault.history().unwrap();
    let mut change = history.change(Source::Connector(SOURCE.into()));
    let summary = location_to_vault(vault, &journal, SOURCE, &mut change).unwrap();
    change.commit("Import location history").unwrap();
    summary
}

fn read(root: &Path, rel: &str) -> MemoryFile {
    MemoryFile::read(root.join(rel)).unwrap()
}

fn place_of(file: &MemoryFile) -> Option<String> {
    file.event()?
        .place
        .as_ref()
        .and_then(LinkOrText::link)
        .map(|l| l.target.clone())
}

#[test]
fn visits_become_places_events_and_a_state_log() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    let summary = import(&vault, &visits());
    assert_eq!(
        summary,
        LocationSummary {
            visits: 6,
            places_added: 3,
            places_found: 2,
            events: 3,
            linked: 1,
            home_or_work: 1,
        },
        "Julie's and the park and home are new; the market by position, the studio by the class"
    );

    // The dinner gets a place named from its address, which is kept.
    let dinner = read(root, "events/2026/2026-03-06-diner-chez-julie.md");
    assert_eq!(place_of(&dinner).as_deref(), Some("12-rue-inventee"));
    let julie = read(root, "places/12-rue-inventee.md");
    assert_eq!(
        julie.common().category.as_str(),
        "location",
        "closed to AIs"
    );
    assert_eq!(
        julie.entity().unwrap().address.as_deref(),
        Some("12 rue Inventée")
    );
    let geo = julie.entity().unwrap().geo.unwrap();
    assert!(meters((geo.lat(), geo.lon()), (48.8641, 2.3790)) < 1.0);

    // The market is the person's own place, found by position; it keeps its category.
    let market = read(root, "places/marche-bastille.md");
    assert_eq!(market.common().category.as_str(), "places");
    assert!(
        market
            .common()
            .sources
            .contains(&"google-timeline:place:ChIJmarket".to_owned())
    );
    // The studio had no position: the class it links to gives it one.
    let studio = read(root, "places/studio-lotus.md");
    assert!(studio.entity().unwrap().geo.is_some());
    assert!(
        studio
            .common()
            .sources
            .contains(&"google-timeline:place:ChIJlotus".to_owned())
    );
    assert!(!root.join("places/lieu-pres-de-48-8710-2-3450.md").exists());

    // Two visits to the market and one to the park are events; home is not.
    let market_visit = read(root, "events/2026/2026-03-07-marche-bastille.md");
    assert_eq!(market_visit.common().category.as_str(), "location");
    assert_eq!(place_of(&market_visit).as_deref(), Some("marche-bastille"));
    let event = market_visit.event().unwrap();
    assert_eq!(
        event.start,
        EventTime::parse("2026-03-07T11:00:00+01:00").unwrap()
    );
    assert!(
        root.join("events/2026/2026-03-14-marche-bastille.md")
            .is_file()
    );
    assert!(
        root.join("events/2026/2026-03-07-lieu-pres-de-48-8462-2-3372.md")
            .is_file()
    );
    assert!(root.join("places/domicile.md").is_file());
    assert!(!root.join("events/2026/2026-03-06-domicile.md").exists());

    // The state log: one row per place, with its stays.
    let log = std::fs::read_to_string(root.join("states/location/2026.md")).unwrap();
    assert!(log.contains("category: location"), "{log}");
    assert!(
        log.contains("| [[marche-bastille]] | 2026-03-07 | 2026-03-14 | 2 | google-timeline |"),
        "{log}"
    );
    assert!(
        log.contains("| [[domicile]] | 2026-03-06 | 2026-03-06 | 1 |"),
        "{log}"
    );

    // The same export again changes nothing.
    let again = import(&vault, &visits());
    assert_eq!(again, LocationSummary::default());
    let history = vault.history().unwrap();
    assert_eq!(
        history.entries(1).unwrap()[0].summary,
        "Import location history"
    );
}

#[test]
fn a_deleted_place_is_not_made_again_and_a_moved_visit_follows() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    import(&vault, &visits());
    std::fs::remove_file(root.join("places/lieu-pres-de-48-8462-2-3372.md")).unwrap();
    // The export is corrected: the market visit ended later.
    let mut corrected = visits();
    corrected[2] = visit(
        "2026-03-07T10:00:00Z",
        "2026-03-07T12:00:00Z",
        "ChIJmarket",
        48.8530,
        2.3700,
        None,
    );
    let summary = import(&vault, &corrected);
    assert_eq!(summary.visits, 1);
    assert_eq!(summary.places_added, 0);
    assert!(!root.join("places/lieu-pres-de-48-8462-2-3372.md").exists());
    let moved = read(root, "events/2026/2026-03-07-marche-bastille.md");
    assert_eq!(
        moved.event().unwrap().end,
        EventTime::parse("2026-03-07T13:00:00+01:00")
    );
}

#[test]
fn an_ai_sees_where_the_person_went_only_when_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let vault = vault(root);
    import(&vault, &visits());
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(root).unwrap();
    let now = DateTime::now(vault.settings().time_zone());
    let (closed, closed_token) = clients.add("Claude", now, &categories, false).unwrap();
    let (open, open_token) = clients.add("Local", now, &categories, true).unwrap();
    let mut read = open.read.clone();
    read.push("location".into());
    clients
        .set_rights(&open.id, Some(read), None, &categories)
        .unwrap();
    assert!(!closed.read.contains(&"location".to_owned()));

    let gateway = Gateway::open(root).unwrap();
    let day = |d: &str| EventTime::parse(d).unwrap();
    let titles = |token: &str| -> Vec<Value> {
        let access = gateway.access(Some(token)).unwrap();
        access
            .events(&day("2026-03-07"), &day("2026-03-07"), None, 50)
            .events
            .into_iter()
            .map(|e| json!(e.name))
            .collect()
    };
    assert_eq!(titles(closed_token.expose()), Vec::<Value>::new());
    assert_eq!(
        titles(open_token.expose()),
        [
            json!("Marché Bastille"),
            json!("Lieu près de 48.8462, 2.3372")
        ]
    );

    // "When did I go to the market?": the events at that place.
    let access = gateway.access(Some(open_token.expose())).unwrap();
    let market = access.get_by_name("Marché Bastille")[0].id.clone();
    let at_market = access.events(&day("2026-01-01"), &day("2026-12-31"), Some(&market), 50);
    assert_eq!(at_market.events.len(), 2);
}
