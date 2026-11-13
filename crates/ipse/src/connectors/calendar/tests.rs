//! The calendar connector's records, with a fixed "now" so that the expansion window
//! does not move: 2026-11-13 12:00 in Paris. Occurrences are expanded from
//! 2025-11-13 to 2027-02-11.

use super::*;

const NOW: &str = "2026-11-13T11:00:00Z";

fn import(text: &str) -> Import {
    let mut import = Import::new(TimeZone::get("Europe/Paris").unwrap(), NOW.parse().unwrap());
    for item in Items::new(text.as_bytes()) {
        import.add(item.unwrap());
    }
    import.finish();
    import
}

fn calendar(events: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{events}END:VCALENDAR\r\n")
        .replace("\n        ", "\n")
}

/// The records' ids and data, by id.
fn by_id(import: &Import) -> BTreeMap<String, Option<Value>> {
    import
        .records
        .iter()
        .map(|r| (r.external_id.clone().unwrap(), r.data.clone()))
        .collect()
}

fn starts(import: &Import, uid: &str) -> Vec<String> {
    import
        .records
        .iter()
        .filter(|r| r.op == Op::Upsert)
        .filter_map(|r| r.data.as_ref())
        .filter(|d| d["uid"] == uid && d.get("recurrence_id").is_some())
        .map(|d| d["start"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_single_event() {
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:diner@example.org\r\nSUMMARY:Dîner chez Julie\r\n\
         DTSTART;TZID=Europe/Paris:20261114T200000\r\nDTEND;TZID=Europe/Paris:20261114T230000\r\n\
         LOCATION:12 rue des Lilas\r\nDESCRIPTION:Apporter le dessert\r\nSTATUS:CONFIRMED\r\n\
         ORGANIZER;CN=Julie Dupont:mailto:julie@example.org\r\n\
         ATTENDEE;CN=Marc Leroy;PARTSTAT=ACCEPTED:mailto:marc@example.org\r\nEND:VEVENT\r\n",
    ));
    assert_eq!(i.records.len(), 1);
    let record = &i.records[0];
    assert_eq!(record.kind, "calendar.event");
    assert_eq!(record.external_id.as_deref(), Some("diner@example.org"));
    assert_eq!(
        record.data,
        Some(json!({
            "uid": "diner@example.org",
            "title": "Dîner chez Julie",
            "start": "2026-11-14T20:00:00+01:00",
            "end": "2026-11-14T23:00:00+01:00",
            "time_zone": "Europe/Paris",
            "location": "12 rue des Lilas",
            "description": "Apporter le dessert",
            "status": "confirmed",
            "organizer": {"name": "Julie Dupont", "email": "julie@example.org"},
            "attendees": [{"name": "Marc Leroy", "email": "marc@example.org", "response": "accepted"}],
        }))
    );
    assert_eq!((i.upserts, i.deletes, i.unreadable), (1, 0, 0));
}

#[test]
fn a_weekly_event_with_a_moved_and_a_cancelled_occurrence() {
    // Tuesdays at 18:30 from 3 November 2026, ten times. The 17th moves to Wednesday,
    // the 24th is removed, and the 1st of December is cancelled by the organizer.
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:yoga\r\nSUMMARY:Yoga\r\nDTSTART;TZID=Europe/Paris:20261103T183000\r\n\
         DTEND;TZID=Europe/Paris:20261103T193000\r\nRRULE:FREQ=WEEKLY;COUNT=10\r\n\
         EXDATE;TZID=Europe/Paris:20261124T183000\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:yoga\r\nRECURRENCE-ID;TZID=Europe/Paris:20261117T183000\r\n\
         SUMMARY:Yoga (décalé)\r\nDTSTART;TZID=Europe/Paris:20261118T183000\r\n\
         DTEND;TZID=Europe/Paris:20261118T193000\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:yoga\r\nRECURRENCE-ID:20261201T173000Z\r\nSTATUS:CANCELLED\r\n\
         SUMMARY:Yoga\r\nDTSTART;TZID=Europe/Paris:20261201T183000\r\n\
         DTEND;TZID=Europe/Paris:20261201T193000\r\nEND:VEVENT\r\n",
    ));
    let records = by_id(&i);
    let master = records["yoga"].as_ref().unwrap();
    assert_eq!(
        master["rule"], "FREQ=WEEKLY;COUNT=10",
        "the rule is sent once"
    );
    assert!(master.get("recurrence_id").is_none());

    let moved = records["yoga#2026-11-17T17:30:00Z"].as_ref().unwrap();
    assert_eq!(moved["start"], "2026-11-18T18:30:00+01:00");
    assert_eq!(moved["title"], "Yoga (décalé)");
    assert_eq!(moved["recurrence_id"], "2026-11-17T17:30:00Z");

    assert_eq!(
        records["yoga#2026-11-24T17:30:00Z"], None,
        "removed: a deletion"
    );
    let cancelled = records["yoga#2026-12-01T17:30:00Z"].as_ref().unwrap();
    assert_eq!(cancelled["status"], "cancelled");

    let mut generated: Vec<String> = starts(&i, "yoga")
        .into_iter()
        .filter(|s| s.ends_with("18:30:00+01:00"))
        .collect();
    generated.sort();
    assert_eq!(
        generated,
        [
            "2026-11-03T18:30:00+01:00",
            "2026-11-10T18:30:00+01:00",
            "2026-11-18T18:30:00+01:00",
            "2026-12-01T18:30:00+01:00",
            "2026-12-08T18:30:00+01:00",
            "2026-12-15T18:30:00+01:00",
            "2026-12-22T18:30:00+01:00",
            "2026-12-29T18:30:00+01:00",
            "2027-01-05T18:30:00+01:00",
        ],
        "ten Tuesdays: one moved to Wednesday, one removed"
    );
    let ten = records
        .get("yoga#2027-01-05T17:30:00Z")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(ten["end"], "2027-01-05T19:30:00+01:00");
    assert!(ten.get("rule").is_none());
    assert_eq!((i.deletes, i.unreadable, i.not_expanded), (1, 0, 0));
}

#[test]
fn an_all_day_event_over_several_days() {
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:noel\r\nSUMMARY:Noël en famille\r\nDTSTART;VALUE=DATE:20261224\r\n\
         DTEND;VALUE=DATE:20261227\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:anniv\r\nSUMMARY:Anniversaire de Marie\r\nDTSTART;VALUE=DATE:20200312\r\n\
         RRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n",
    ));
    let records = by_id(&i);
    let noel = records["noel"].as_ref().unwrap();
    assert_eq!(
        (noel["start"].as_str(), noel["end"].as_str()),
        (Some("2026-12-24"), Some("2026-12-27"))
    );
    assert!(noel.get("time_zone").is_none());

    let anniv = records["anniv"].as_ref().unwrap();
    assert_eq!(
        (anniv["start"].as_str(), anniv["end"].as_str()),
        (Some("2020-03-12"), Some("2020-03-13")),
        "a day without an end lasts one day"
    );
    assert_eq!(
        starts(&i, "anniv"),
        ["2026-03-12"],
        "only inside the window"
    );
    let occurrence = records["anniv#2026-03-12"].as_ref().unwrap();
    assert_eq!(occurrence["end"], "2026-03-13");
}

#[test]
fn daylight_saving_time_keeps_the_local_time() {
    // Saturdays at 10:00 in Paris across the change of 25 October 2026.
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:marche\r\nSUMMARY:Marché\r\nDTSTART;TZID=Europe/Paris:20261017T100000\r\n\
         DURATION:PT1H\r\nRRULE:FREQ=WEEKLY;UNTIL=20261031T090000Z\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:appel\r\nSUMMARY:Appel\r\nDTSTART:20261017T080000Z\r\n\
         RRULE:FREQ=WEEKLY;COUNT=3\r\nEND:VEVENT\r\n",
    ));
    assert_eq!(
        starts(&i, "marche"),
        [
            "2026-10-17T10:00:00+02:00",
            "2026-10-24T10:00:00+02:00",
            "2026-10-31T10:00:00+01:00",
        ],
        "UNTIL is included"
    );
    let records = by_id(&i);
    let last = records["marche#2026-10-31T09:00:00Z"].as_ref().unwrap();
    assert_eq!(last["end"], "2026-10-31T11:00:00+01:00");
    assert_eq!(
        starts(&i, "appel"),
        [
            "2026-10-17T10:00:00+02:00",
            "2026-10-24T10:00:00+02:00",
            "2026-10-31T09:00:00+01:00",
        ],
        "a UTC start keeps its UTC time"
    );
}

#[test]
fn occurrences_are_expanded_from_a_year_back_to_90_days_ahead() {
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:rent\r\nSUMMARY:Loyer\r\nDTSTART;VALUE=DATE:20200105\r\n\
         RRULE:FREQ=MONTHLY\r\nEXDATE;VALUE=DATE:20100105\r\nEND:VEVENT\r\n",
    ));
    let starts = starts(&i, "rent");
    assert_eq!(starts.first().map(String::as_str), Some("2025-12-05"));
    assert_eq!(starts.last().map(String::as_str), Some("2027-02-05"));
    assert_eq!(starts.len(), 15);
    assert_eq!(
        i.deletes, 0,
        "removed dates outside the window are not sent"
    );
}

#[test]
fn a_time_zone_defined_after_the_event() {
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:o\r\nSUMMARY:Réunion\r\nDTSTART;TZID=Heure de Paris:20260715T090000\r\n\
         END:VEVENT\r\nBEGIN:VTIMEZONE\r\nTZID:Heure de Paris\r\n\
         BEGIN:STANDARD\r\nDTSTART:16010101T030000\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\n\
         RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\r\nEND:STANDARD\r\n\
         BEGIN:DAYLIGHT\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\n\
         RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n\
         BEGIN:VEVENT\r\nUID:u\r\nSUMMARY:Inconnu\r\nDTSTART;TZID=Nowhere:20260715T090000\r\nEND:VEVENT\r\n",
    ));
    let records = by_id(&i);
    assert_eq!(
        records["o"].as_ref().unwrap()["start"],
        "2026-07-15T09:00:00+02:00"
    );
    assert_eq!(
        records["u"].as_ref().unwrap()["start"],
        "2026-07-15T09:00:00+02:00",
        "an unknown time zone is read as the vault's"
    );
    assert!(records["o"].as_ref().unwrap().get("time_zone").is_none());
}

#[test]
fn rules_ipse_cannot_read_keep_the_first_date() {
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:h\r\nSUMMARY:Médicament\r\nDTSTART;TZID=Europe/Paris:20261101T080000\r\n\
         RRULE:FREQ=HOURLY;INTERVAL=8\r\nEND:VEVENT\r\n",
    ));
    assert_eq!(starts(&i, "h"), ["2026-11-01T08:00:00+01:00"]);
    assert_eq!(i.not_expanded, 1);
    assert_eq!(
        i.report(false).as_deref(),
        Some(
            "1 repeating event(s) use a rule Ipse cannot read yet: only the first date is imported."
        )
    );
}

#[test]
fn unreadable_events_are_counted_and_ids_without_uid_are_stable() {
    let text = calendar(
        "BEGIN:VEVENT\r\nUID:nostart\r\nSUMMARY:Sans date\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nSUMMARY:Bad\r\nDTSTART:2026-11-14\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nSUMMARY:Sans UID\r\nDTSTART;VALUE=DATE:20261114\r\nEND:VEVENT\r\n",
    );
    let a = import(&text);
    let b = import(&text);
    assert_eq!(a.unreadable, 2);
    assert_eq!(a.records.len(), 1);
    let id = a.records[0].external_id.clone().unwrap();
    assert!(id.starts_with("b3:"), "{id}");
    assert_eq!(b.records[0].external_id.as_deref(), Some(id.as_str()));
    assert!(a.records[0].data.as_ref().unwrap().get("uid").is_none());
    assert_eq!(
        a.report(true).as_deref(),
        Some("2 événement(s) illisible(s) ignoré(s).")
    );
}

#[test]
fn rdates_add_occurrences() {
    let i = import(&calendar(
        "BEGIN:VEVENT\r\nUID:r\r\nSUMMARY:Cours\r\nDTSTART;VALUE=DATE:20261105\r\n\
         RRULE:FREQ=WEEKLY;COUNT=2\r\nRDATE;VALUE=DATE:20261120,20261105\r\nEND:VEVENT\r\n",
    ));
    let mut starts = starts(&i, "r");
    starts.sort();
    assert_eq!(starts, ["2026-11-05", "2026-11-12", "2026-11-20"]);
}

#[test]
fn manifest_follows_the_spec() {
    let m = manifest();
    assert_eq!(
        (m.id.as_str(), m.rung, m.category.as_str()),
        ("calendar", 4, "agenda")
    );
    assert_eq!(m.access.files, FileAccess::Picked);
    assert!(m.access.network.is_empty() && m.access.secrets.is_empty());
    assert_eq!(m.modes, [Mode::Import]);
    assert_eq!(m.kinds, [KIND]);
}
