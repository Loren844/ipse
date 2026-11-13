//! Syncing a calendar from its link, as `ipse sync` does: the core runs the calendar
//! connector in mode `sync` against a small HTTP server on this computer.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ipse_core::connector::{Instance, MemorySecrets, Outcome, SecretStore, SyncState, secret_key};
use ipse_core::sync::{SyncOptions, Synced, sync_instance};
use ipse_core::time::{SignedDuration, Timestamp};
use ipse_core::vault::{Language, Settings, Vault};
use ipse_protocol::{ErrorCode, Secret};
use jiff::ToSpan as _;

/// The part of the link that makes it secret. It must never reach a file.
const TOKEN: &str = "private-7d41c0ffee";

/// What the server answers.
#[derive(Debug, Clone)]
enum Answer {
    Calendar { body: String, etag: &'static str },
    Status(u16),
}

#[derive(Debug, Default)]
struct Seen {
    /// The `If-None-Match` header of each request, if any.
    if_none_match: Vec<Option<String>>,
}

/// A one-thread HTTP server that answers every request with the current [`Answer`].
struct Server {
    url: String,
    answer: Arc<Mutex<Answer>>,
    seen: Arc<Mutex<Seen>>,
}

impl Server {
    fn start(answer: Answer) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/calendar/ical/{TOKEN}/basic.ics",
            listener.local_addr().unwrap()
        );
        let answer = Arc::new(Mutex::new(answer));
        let seen = Arc::new(Mutex::new(Seen::default()));
        let (a, s) = (Arc::clone(&answer), Arc::clone(&seen));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut if_none_match = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("if-none-match")
                    {
                        if_none_match = Some(value.trim().to_owned());
                    }
                }
                let answer = a.lock().unwrap().clone();
                let response = match answer {
                    Answer::Calendar { body: _, etag }
                        if if_none_match.as_deref() == Some(etag) =>
                    {
                        format!(
                            "HTTP/1.1 304 Not Modified\r\nETag: {etag}\r\nConnection: close\r\n\r\n"
                        )
                    }
                    Answer::Calendar { body, etag } => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\nETag: {etag}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    ),
                    Answer::Status(code) => format!(
                        "HTTP/1.1 {code} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                };
                s.lock().unwrap().if_none_match.push(if_none_match);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { url, answer, seen }
    }

    fn answer(&self, answer: Answer) {
        *self.answer.lock().unwrap() = answer;
    }

    fn last_if_none_match(&self) -> Option<String> {
        self.seen
            .lock()
            .unwrap()
            .if_none_match
            .last()
            .cloned()
            .flatten()
    }
}

/// An all-day event `days` from today, in iCalendar.
fn event(uid: &str, title: &str, days: i64) -> String {
    let day = Timestamp::now()
        .to_zoned(ipse_core::time::TimeZone::UTC)
        .date()
        .checked_add(days.days())
        .unwrap();
    let next = day.tomorrow().unwrap();
    format!(
        "BEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{title}\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nEND:VEVENT\r\n",
        day.strftime("%Y%m%d"),
        next.strftime("%Y%m%d")
    )
}

fn calendar(events: &[String]) -> String {
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n{}END:VCALENDAR\r\n",
        events.concat()
    )
}

struct Setup {
    dir: tempfile::TempDir,
    vault: Vault,
    instance: Instance,
    secrets: MemorySecrets,
}

fn setup(server: &Server) -> Setup {
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
    };
    let secrets = MemorySecrets::default();
    secrets
        .set(
            &secret_key(vault.root(), &instance.id, "url"),
            &Secret::new(server.url.clone()),
        )
        .unwrap();
    Setup {
        dir,
        vault,
        instance,
        secrets,
    }
}

impl Setup {
    fn sync(&self, full: bool) -> Synced {
        let options = SyncOptions {
            command: vec![env!("CARGO_BIN_EXE_ipse").into(), "connector".into()],
            language: "en".into(),
            full,
            now: Timestamp::now(),
        };
        sync_instance(&self.vault, &self.instance, &self.secrets, &options).unwrap()
    }

    /// The text of the event file whose title is `title`.
    fn event_file(&self, title: &str) -> Option<String> {
        files(&self.dir.path().join("events"))
            .into_iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .find(|text| text.contains(&format!("title: {title}")))
    }
}

fn files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

fn failed_with(synced: &Synced) -> Option<ErrorCode> {
    match &synced.outcome {
        Outcome::Failed { code, .. } => Some(*code),
        _ => None,
    }
}

#[test]
fn a_subscription_follows_the_calendar() {
    let server = Server::start(Answer::Calendar {
        body: calendar(&[
            event("past@test", "Atelier poterie", -10),
            event("soon@test", "Pique-nique au parc", 1),
            event("later@test", "Concert au kiosque", 10),
        ]),
        etag: "\"v1\"",
    });
    let s = setup(&server);

    let first = s.sync(false);
    assert!(first.succeeded(), "{first:?}");
    assert_eq!(first.events.unwrap().added, 3, "{first:?}");
    assert_eq!(
        server.last_if_none_match(),
        None,
        "nothing to compare with yet"
    );
    assert!(s.event_file("Pique-nique au parc").is_some());

    // Same day, same calendar: the server says nothing changed, and nothing is removed.
    let unchanged = s.sync(false);
    assert_eq!(server.last_if_none_match().as_deref(), Some("\"v1\""));
    assert!(unchanged.succeeded(), "{unchanged:?}");
    assert_eq!(unchanged.removed, 0);
    let events = unchanged.events.unwrap();
    assert_eq!((events.added, events.updated, events.cancelled), (0, 0, 0));

    // A full sync asks for the whole calendar.
    let full = s.sync(true);
    assert_eq!(server.last_if_none_match(), None);
    assert!(full.succeeded());
    assert_eq!(full.removed, 0);

    // Two events disappear from the calendar: the future one is marked cancelled, the
    // past one stays as it happened.
    server.answer(Answer::Calendar {
        body: calendar(&[event("soon@test", "Pique-nique au parc", 1)]),
        etag: "\"v2\"",
    });
    let removed = s.sync(false);
    assert!(removed.succeeded(), "{removed:?}");
    assert_eq!(removed.removed, 2);
    assert_eq!(removed.events.unwrap().cancelled, 1, "{removed:?}");
    assert!(
        s.event_file("Concert au kiosque")
            .unwrap()
            .contains("status: cancelled")
    );
    let past = s.event_file("Atelier poterie").unwrap();
    assert!(!past.contains("cancelled"), "{past}");

    let state = SyncState::load(s.vault.root()).unwrap();
    assert_eq!(state.last("calendar-2").unwrap().failures, 0);
    assert_no_link(&s);
}

#[test]
fn what_is_not_a_whole_calendar_removes_nothing() {
    let events = [
        event("a@test", "Pique-nique au parc", 1),
        event("b@test", "Concert au kiosque", 10),
    ];
    let server = Server::start(Answer::Calendar {
        body: calendar(&events),
        etag: "\"v1\"",
    });
    let s = setup(&server);
    assert!(s.sync(false).succeeded());

    // A sign-in page instead of the calendar.
    server.answer(Answer::Calendar {
        body: "<!DOCTYPE html><html><body>Sign in</body></html>".into(),
        etag: "\"html\"",
    });
    let page = s.sync(false);
    assert_eq!(failed_with(&page), Some(ErrorCode::BadInput), "{page:?}");
    assert_eq!(page.removed, 0);

    // A calendar cut short: only the first event, and no end.
    let cut = calendar(&events);
    let cut = &cut[..cut.find("BEGIN:VEVENT\r\nUID:b@test").unwrap()];
    server.answer(Answer::Calendar {
        body: cut.to_owned(),
        etag: "\"cut\"",
    });
    let short = s.sync(false);
    assert_eq!(
        failed_with(&short),
        Some(ErrorCode::Unavailable),
        "{short:?}"
    );
    assert_eq!(short.removed, 0);

    // The link was reset.
    server.answer(Answer::Status(404));
    let gone = s.sync(false);
    assert_eq!(failed_with(&gone), Some(ErrorCode::Auth), "{gone:?}");

    // A server in trouble.
    server.answer(Answer::Status(503));
    let down = s.sync(false);
    assert_eq!(failed_with(&down), Some(ErrorCode::Unavailable), "{down:?}");

    assert!(
        !s.event_file("Concert au kiosque")
            .unwrap()
            .contains("cancelled"),
        "nothing was removed"
    );
    let state = SyncState::load(s.vault.root()).unwrap();
    assert_eq!(state.last("calendar-2").unwrap().failures, 4);
    for synced in [page, short, gone, down] {
        let Outcome::Failed { message, .. } = synced.outcome else {
            unreachable!()
        };
        assert!(!message.contains(TOKEN), "{message}");
    }
    assert_no_link(&s);
}

#[test]
fn a_missing_link_is_reported() {
    let server = Server::start(Answer::Status(200));
    let s = setup(&server);
    s.secrets
        .delete(&secret_key(s.vault.root(), "calendar-2", "url"))
        .unwrap();
    let synced = s.sync(false);
    assert_eq!(
        failed_with(&synced),
        Some(ErrorCode::BadInput),
        "{synced:?}"
    );
}

/// The secret part of the link is nowhere in the vault: not in its files, its journal,
/// its history or Ipse's own files.
fn assert_no_link(s: &Setup) {
    for path in files(s.dir.path()) {
        let bytes = std::fs::read(&path).unwrap();
        let found = bytes.windows(TOKEN.len()).any(|w| w == TOKEN.as_bytes());
        assert!(!found, "{} holds the link", path.display());
    }
}
