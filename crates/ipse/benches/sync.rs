//! Benchmark of what the desktop app does in the background between two changes of a
//! followed calendar (issue #153): a sync that finds nothing new, against a calendar
//! of 2 000 events served on this computer. Once with the server answering "not
//! modified" (`ETag`), once sending the same calendar again. Then the same for a
//! calendars and contacts account (issue #154): a CalDAV calendar of 5 000 events
//! whose `sync-token` did not change.
#![allow(missing_docs, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;

use criterion::{Criterion, criterion_group, criterion_main};
use ipse_core::connector::{Instance, MemorySecrets, SecretStore as _, secret_key};
use ipse_core::sync::{SyncOptions, sync_instance};
use ipse_core::time::{SignedDuration, Timestamp};
use ipse_core::vault::{Language, Settings, Vault};
use ipse_protocol::Secret;

const EVENTS: usize = 2_000;
const ETAG: &str = "\"v1\"";

fn calendar() -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Ipse//Bench//EN\r\n");
    for i in 0..EVENTS {
        let month = i % 12 + 1;
        let day = i % 28 + 1;
        let hour = 8 + i % 10;
        let _ = write!(
            out,
            "BEGIN:VEVENT\r\nUID:event-{i}@example.org\r\nDTSTAMP:20261113T101500Z\r\n\
             DTSTART:2026{month:02}{day:02}T{hour:02}0000Z\r\nDTEND:2026{month:02}{day:02}T{:02}0000Z\r\n\
             SUMMARY:Réunion numéro {i}\r\nLOCATION:Salle {}\r\nEND:VEVENT\r\n",
            hour + 1,
            i % 40,
        );
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

/// Serves `body` on this computer; with `etag`, answers "not modified" to a request
/// that already has it.
fn serve(body: String, etag: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/calendar/ical/private-bench/basic.ics",
        listener.local_addr().unwrap()
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut known = false;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                known |=
                    line.to_ascii_lowercase().starts_with("if-none-match") && line.contains(ETAG);
            }
            let response = if etag && known {
                format!("HTTP/1.1 304 Not Modified\r\nETag: {ETAG}\r\nConnection: close\r\n\r\n")
            } else {
                let tag = if etag {
                    format!("ETag: {ETAG}\r\n")
                } else {
                    String::new()
                };
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\n{tag}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

fn bench_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync");
    group.sample_size(10);
    for (name, etag) in [
        ("nothing_new_2000_events_not_modified", true),
        ("nothing_new_2000_events_same_calendar", false),
    ] {
        let url = serve(calendar(), etag);
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(
            dir.path(),
            "Camille Durand",
            Settings::new(Language::Fr, "Europe/Paris").unwrap(),
        )
        .unwrap();
        let instance = Instance {
            id: "calendar-2".into(),
            connector: "calendar".into(),
            name: "Perso".into(),
            every: SignedDuration::from_mins(15),
            config: serde_json::Map::new(),
        };
        let secrets = MemorySecrets::default();
        secrets
            .set(
                &secret_key(vault.root(), &instance.id, "url"),
                &Secret::new(url),
            )
            .unwrap();
        let sync = || {
            let options = SyncOptions {
                command: vec![env!("CARGO_BIN_EXE_ipse").into(), "connector".into()],
                language: "en".into(),
                full: false,
                now: Timestamp::now(),
            };
            let synced = sync_instance(&vault, &instance, &secrets, &options).unwrap();
            assert!(synced.succeeded());
        };
        // The first sync brings everything; the ones measured find nothing new.
        sync();
        group.bench_function(name, |b| b.iter(sync));
    }
    group.finish();
}

const ACCOUNT_EVENTS: usize = 5_000;

/// One CalDAV event, as a server holds it.
fn caldav_event(i: usize) -> String {
    let month = i % 12 + 1;
    let day = i % 28 + 1;
    let hour = 8 + i % 10;
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:event-{i}@example.org\r\n\
         DTSTART:2026{month:02}{day:02}T{hour:02}0000Z\r\nDTEND:2026{month:02}{day:02}T{:02}0000Z\r\n\
         SUMMARY:Réunion numéro {i}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        hour + 1
    )
}

/// A CalDAV server on this computer with one calendar of `ACCOUNT_EVENTS` events,
/// whose `sync-token` never changes.
fn serve_account() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let items: BTreeMap<String, String> = (0..ACCOUNT_EVENTS)
        .map(|i| (format!("/cal/perso/{i}.ics"), caldav_event(i)))
        .collect();
    let ok = |href: &str, props: &str| {
        format!(
            "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
        )
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let body = String::from_utf8_lossy(&body);
            let path = first.split_whitespace().nth(1).unwrap_or_default();
            let responses = match path {
                p if p.starts_with("/.well-known/") => ok(
                    p,
                    "<d:current-user-principal><d:href>/me/</d:href></d:current-user-principal>",
                ),
                "/me/" => ok(
                    "/me/",
                    "<c:calendar-home-set><d:href>/cal/</d:href></c:calendar-home-set>",
                ),
                "/cal/" => ok(
                    "/cal/perso/",
                    "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Perso</d:displayname><d:sync-token>t1</d:sync-token>",
                ),
                "/cal/perso/" if body.contains("calendar-multiget") => body
                    .split("<D:href>")
                    .skip(1)
                    .filter_map(|h| h.split('<').next())
                    .filter_map(|href| {
                        let data = items.get(href)?;
                        Some(ok(
                            href,
                            &format!("<d:getetag>\"1\"</d:getetag><c:calendar-data>{data}</c:calendar-data>"),
                        ))
                    })
                    .collect(),
                "/cal/perso/" => items
                    .keys()
                    .map(|href| ok(href, "<d:getetag>\"1\"</d:getetag><d:resourcetype/>"))
                    .collect(),
                _ => String::new(),
            };
            let xml = format!(
                r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">{responses}</d:multistatus>"#
            );
            let response = format!(
                "HTTP/1.1 207 Multi-Status\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
                xml.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

fn bench_account(c: &mut Criterion) {
    let mut group = c.benchmark_group("sync");
    group.sample_size(10);
    let url = serve_account();
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::init(
        dir.path(),
        "Camille Durand",
        Settings::new(Language::Fr, "Europe/Paris").unwrap(),
    )
    .unwrap();
    let mut config = serde_json::Map::new();
    config.insert("url".into(), url.clone().into());
    config.insert("user".into(), "camille".into());
    config.insert(
        "collections".into(),
        serde_json::json!([format!("{url}/cal/perso/")]),
    );
    let instance = Instance {
        id: "dav-2".into(),
        connector: "dav".into(),
        name: "Compte".into(),
        every: SignedDuration::from_mins(15),
        config,
    };
    let secrets = MemorySecrets::default();
    secrets
        .set(
            &secret_key(vault.root(), &instance.id, "password"),
            &Secret::new("bench"),
        )
        .unwrap();
    let sync = || {
        let options = SyncOptions {
            command: vec![env!("CARGO_BIN_EXE_ipse").into(), "connector".into()],
            language: "en".into(),
            full: false,
            now: Timestamp::now(),
        };
        let synced = sync_instance(&vault, &instance, &secrets, &options).unwrap();
        assert!(synced.succeeded(), "{:?}", synced.outcome);
        synced
    };
    // The first sync reads everything; the ones measured find nothing new.
    let first = sync();
    assert_eq!(
        usize::try_from(first.events.unwrap().added).unwrap(),
        ACCOUNT_EVENTS
    );
    group.bench_function("nothing_new_5000_events_account", |b| b.iter(sync));
    group.finish();
}

criterion_group!(benches, bench_sync, bench_account);
criterion_main!(benches);
