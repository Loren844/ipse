//! Connecting a CalDAV and CardDAV account and keeping it in sync, as `ipse dav add`
//! and `ipse sync` do, against a small WebDAV server on this computer. Every name,
//! event and card here is invented.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use jiff::ToSpan as _;

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");
const USER: &str = "ines";
const PASSWORD: &str = "app-pass-5c1e9a";

/// What the server holds: calendar events and cards by `href`, with a change log for
/// `sync-collection`.
#[derive(Default)]
struct Store {
    items: BTreeMap<String, (u32, String)>,
    /// Each change: its token number, the item's `href`, whether it was removed.
    log: Vec<(u32, String, bool)>,
    token: u32,
    /// Each request: method, path and, for a report, its kind.
    seen: Vec<String>,
}

impl Store {
    fn put(&mut self, href: &str, data: String) {
        self.token += 1;
        self.items.insert(href.to_owned(), (self.token, data));
        self.log.push((self.token, href.to_owned(), false));
    }

    fn remove(&mut self, href: &str) {
        self.token += 1;
        self.items.remove(href);
        self.log.push((self.token, href.to_owned(), true));
    }

    fn in_collection<'a>(
        &'a self,
        collection: &'a str,
    ) -> impl Iterator<Item = (&'a String, &'a (u32, String))> {
        self.items
            .iter()
            .filter(move |(href, _)| href.starts_with(collection))
    }

    /// The address book's ctag: the last change in it.
    fn ctag(&self, collection: &str) -> u32 {
        self.log
            .iter()
            .filter(|(_, href, _)| href.starts_with(collection))
            .map(|(n, _, _)| *n)
            .max()
            .unwrap_or(0)
    }
}

struct Server {
    url: String,
    store: Arc<Mutex<Store>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let store = Arc::new(Mutex::new(Store::default()));
        let shared = Arc::clone(&store);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut length = 0;
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("authorization:") {
                        authorized =
                            line["authorization:".len()..].trim() == format!("Basic {}", basic());
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap_or_default().to_owned();
                let path = parts.next().unwrap_or_default().to_owned();
                let (status, xml) = if authorized {
                    answer(&mut shared.lock().unwrap(), &method, &path, &body)
                } else {
                    (401, String::new())
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{xml}",
                    xml.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { url, store }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Store) -> T) -> T {
        f(&mut self.store.lock().unwrap())
    }

    /// The requests seen since the last call.
    fn seen(&self) -> Vec<String> {
        self.with(|s| std::mem::take(&mut s.seen))
    }
}

fn basic() -> String {
    // "ines:app-pass-5c1e9a", in base64.
    "aW5lczphcHAtcGFzcy01YzFlOWE=".to_owned()
}

const CALENDARS: &str = "/dav/calendars/ines/";
const PERSO: &str = "/dav/calendars/ines/perso/";
const TASKS: &str = "/dav/calendars/ines/taches/";
const BOOKS: &str = "/dav/addressbooks/ines/";
const CONTACTS: &str = "/dav/addressbooks/ines/contacts/";

fn multistatus(responses: &str, token: Option<u32>) -> String {
    let token = token.map_or(String::new(), |t| {
        format!("<d:sync-token>http://example.org/sync/{t}</d:sync-token>")
    });
    format!(
        r#"<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:card="urn:ietf:params:xml:ns:carddav" xmlns:cs="http://calendarserver.org/ns/">{responses}{token}</d:multistatus>"#
    )
}

fn response(href: &str, props: &str) -> String {
    format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
    )
}

/// What the server answers: a principal at `.well-known`, two homes, a calendar of
/// events with a `sync-token`, a calendar of tasks only, an address book with only a
/// ctag.
#[allow(clippy::too_many_lines)] // the server's whole routing, read in one place
fn answer(store: &mut Store, method: &str, path: &str, body: &str) -> (u16, String) {
    let report = [
        "sync-collection",
        "calendar-multiget",
        "addressbook-multiget",
    ]
    .into_iter()
    .find(|r| body.contains(r))
    .unwrap_or("");
    store
        .seen
        .push(format!("{method} {path} {report}").trim_end().to_owned());
    match (method, path) {
        ("PROPFIND", "/.well-known/caldav" | "/.well-known/carddav") => (
            207,
            multistatus(
                &response(
                    path,
                    "<d:current-user-principal><d:href>/dav/principals/ines/</d:href></d:current-user-principal>",
                ),
                None,
            ),
        ),
        ("PROPFIND", "/dav/principals/ines/") => (
            207,
            multistatus(
                &response(
                    path,
                    &format!(
                        "<cal:calendar-home-set><d:href>{CALENDARS}</d:href></cal:calendar-home-set><card:addressbook-home-set><d:href>{BOOKS}</d:href></card:addressbook-home-set>"
                    ),
                ),
                None,
            ),
        ),
        ("PROPFIND", CALENDARS) => {
            let token = store.token;
            let all = [
                response(
                    CALENDARS,
                    "<d:resourcetype><d:collection/></d:resourcetype>",
                ),
                response(
                    PERSO,
                    &format!(
                        "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>Perso</d:displayname><d:sync-token>http://example.org/sync/{token}</d:sync-token><cal:supported-calendar-component-set><cal:comp name=\"VEVENT\"/></cal:supported-calendar-component-set>"
                    ),
                ),
                response(
                    TASKS,
                    "<d:resourcetype><d:collection/><cal:calendar/></d:resourcetype><d:displayname>Tâches</d:displayname><cal:supported-calendar-component-set><cal:comp name=\"VTODO\"/></cal:supported-calendar-component-set>",
                ),
            ];
            (207, multistatus(&all.concat(), None))
        }
        ("PROPFIND", BOOKS) => {
            let ctag = store.ctag(CONTACTS);
            let all = [
                response(BOOKS, "<d:resourcetype><d:collection/></d:resourcetype>"),
                response(
                    CONTACTS,
                    &format!(
                        "<d:resourcetype><d:collection/><card:addressbook/></d:resourcetype><d:displayname>Contacts</d:displayname><cs:getctag>{ctag}</cs:getctag>"
                    ),
                ),
            ];
            (207, multistatus(&all.concat(), None))
        }
        ("PROPFIND", PERSO | CONTACTS) => {
            let mut all = response(path, "<d:resourcetype><d:collection/></d:resourcetype>");
            for (href, (etag, _)) in store.in_collection(path) {
                all.push_str(&response(
                    href,
                    &format!("<d:getetag>\"{etag}\"</d:getetag><d:resourcetype/>"),
                ));
            }
            (207, multistatus(&all, None))
        }
        ("REPORT", PERSO) if report == "sync-collection" => {
            let since: u32 = body
                .split("/sync/")
                .nth(1)
                .and_then(|t| t.split('<').next())
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            let mut all = String::new();
            for (n, href, removed) in &store.log {
                if *n <= since || !href.starts_with(PERSO) {
                    continue;
                }
                if *removed {
                    let _ = write!(
                        all,
                        "<d:response><d:href>{href}</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response>"
                    );
                } else if let Some((etag, _)) = store.items.get(href) {
                    all.push_str(&response(
                        href,
                        &format!("<d:getetag>\"{etag}\"</d:getetag>"),
                    ));
                }
            }
            (207, multistatus(&all, Some(store.token)))
        }
        ("REPORT", PERSO | CONTACTS) => {
            let data = if report == "calendar-multiget" {
                "cal:calendar-data"
            } else {
                "card:address-data"
            };
            let mut all = String::new();
            for href in body.split("<D:href>").skip(1) {
                let href = href.split('<').next().unwrap();
                if let Some((etag, text)) = store.items.get(href) {
                    let text = text.replace('&', "&amp;").replace('<', "&lt;");
                    all.push_str(&response(
                        href,
                        &format!("<d:getetag>\"{etag}\"</d:getetag><{data}>{text}</{data}>"),
                    ));
                }
            }
            (207, multistatus(&all, None))
        }
        _ => (404, String::new()),
    }
}

/// An all-day event `days` from today.
fn event(uid: &str, title: &str, days: i64) -> String {
    let day = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date()
        .checked_add(days.days())
        .unwrap();
    format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nSUMMARY:{title}\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        day.strftime("%Y%m%d"),
        day.tomorrow().unwrap().strftime("%Y%m%d")
    )
}

fn card(uid: &str, name: &str, phone: &str) -> String {
    format!("BEGIN:VCARD\r\nVERSION:3.0\r\nUID:{uid}\r\nFN:{name}\r\nTEL:{phone}\r\nEND:VCARD\r\n")
}

struct Memory {
    _dir: tempfile::TempDir,
    vault: PathBuf,
    secrets: PathBuf,
}

impl Memory {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("memoire");
        let secrets = dir.path().join("secrets.json");
        let memory = Self {
            vault,
            secrets,
            _dir: dir,
        };
        let path = memory.vault.to_str().unwrap().to_owned();
        let out = memory.ipse(
            &["init", &path, "--name", "Inès Roux", "--language", "fr"],
            "",
        );
        assert!(out.0, "{}", out.1);
        memory
    }

    /// Runs ipse with `input`; whether it succeeded, and what it said.
    fn ipse(&self, args: &[&str], input: &str) -> (bool, String) {
        let mut child = Command::new(IPSE)
            .args(args)
            .env("IPSE_TEST_SECRETS", &self.secrets)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), said)
    }

    fn vault_arg(&self) -> String {
        self.vault.to_str().unwrap().to_owned()
    }

    fn add(&self, server: &Server, password: &str) -> (bool, String) {
        let vault = self.vault_arg();
        self.ipse(
            &[
                "dav",
                "add",
                &server.url,
                "--user",
                USER,
                "--name",
                "Perso",
                "--vault",
                &vault,
            ],
            &format!("{password}\n"),
        )
    }

    fn sync(&self) -> String {
        let vault = self.vault_arg();
        let (ok, said) = self.ipse(&["sync", "--vault", &vault], "");
        assert!(ok, "{said}");
        said
    }

    /// The text of the file in `folder` that contains `text`.
    fn file_with(&self, folder: &str, text: &str) -> Option<String> {
        files(&self.vault.join(folder))
            .into_iter()
            .map(|p| std::fs::read_to_string(p).unwrap())
            .find(|t| t.contains(text))
    }

    fn count_with(&self, folder: &str, text: &str) -> usize {
        files(&self.vault.join(folder))
            .into_iter()
            .filter(|p| std::fs::read_to_string(p).unwrap().contains(text))
            .count()
    }

    /// The password and the user's password are in no file of the memory.
    fn assert_no_password(&self) {
        for path in files(&self.vault) {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            assert!(
                !text.contains(PASSWORD),
                "the password is in {}",
                path.display()
            );
            assert!(
                !text.contains(&basic()),
                "the credentials are in {}",
                path.display()
            );
        }
    }
}

fn files(dir: &Path) -> Vec<PathBuf> {
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

#[test]
fn an_account_is_followed_and_kept_in_sync() {
    let server = Server::start();
    server.with(|s| {
        s.put(
            &format!("{PERSO}soon.ics"),
            event("soon@test", "Pique-nique au parc", 1),
        );
        s.put(
            &format!("{PERSO}later.ics"),
            event("later@test", "Concert au kiosque", 10),
        );
        s.put(
            &format!("{PERSO}past.ics"),
            event("past@test", "Atelier poterie", -10),
        );
        s.put(
            &format!("{CONTACTS}a.vcf"),
            card("a@test", "Bastien Morel", "+33 6 12 34 56 78"),
        );
    });
    let memory = Memory::new();

    let (ok, said) = memory.add(&server, PASSWORD);
    assert!(ok, "{said}");
    assert!(said.contains("Found the calendar \"Perso\""), "{said}");
    assert!(
        said.contains("Found the address book \"Contacts\""),
        "{said}"
    );
    assert!(
        !said.contains("Tâches"),
        "a calendar of tasks only is left out: {said}"
    );
    assert!(
        memory
            .file_with("events", "title: Pique-nique au parc")
            .is_some()
    );
    assert!(memory.file_with("people", "Bastien Morel").is_some());
    let config = std::fs::read_to_string(memory.vault.join(".ipse/connectors.toml")).unwrap();
    assert!(config.contains("connector = \"dav\""), "{config}");
    assert!(
        config.contains(PERSO) && config.contains(CONTACTS),
        "{config}"
    );
    assert!(!config.contains(TASKS), "{config}");
    server.seen();

    // Nothing changed: the homes are listed, nothing is read.
    memory.sync();
    let seen = server.seen();
    assert!(
        seen.iter().all(|r| r.starts_with("PROPFIND /dav/")),
        "{seen:?}"
    );

    // An event moved and a card edited: only those are read.
    server.with(|s| {
        s.put(
            &format!("{PERSO}soon.ics"),
            event("soon@test", "Pique-nique au lac", 2),
        );
        s.put(
            &format!("{CONTACTS}a.vcf"),
            card("a@test", "Bastien Morel", "+33 6 98 76 54 32"),
        );
    });
    memory.sync();
    let seen = server.seen();
    assert!(
        seen.iter().any(|r| r.ends_with("sync-collection")),
        "{seen:?}"
    );
    assert!(
        seen.contains(&format!("PROPFIND {CONTACTS}")),
        "the etags, without a token: {seen:?}"
    );
    assert!(
        memory
            .file_with("events", "title: Pique-nique au lac")
            .is_some()
    );
    assert!(
        memory
            .file_with("events", "title: Pique-nique au parc")
            .is_none()
    );
    assert!(memory.file_with("people", "+33698765432").is_some());

    // Two events removed: everything is read whole and the future one is cancelled,
    // the past one stays as it happened.
    server.with(|s| {
        s.remove(&format!("{PERSO}later.ics"));
        s.remove(&format!("{PERSO}past.ics"));
    });
    memory.sync();
    assert!(
        memory
            .file_with("events", "title: Concert au kiosque")
            .unwrap()
            .contains("status: cancelled")
    );
    let past = memory
        .file_with("events", "title: Atelier poterie")
        .unwrap();
    assert!(!past.contains("cancelled"), "{past}");

    memory.assert_no_password();
}

#[test]
fn a_wrong_password_connects_nothing() {
    let server = Server::start();
    let memory = Memory::new();
    let (ok, said) = memory.add(&server, "not-the-password");
    assert!(!ok);
    assert!(said.contains("app password"), "{said}");
    let config =
        std::fs::read_to_string(memory.vault.join(".ipse/connectors.toml")).unwrap_or_default();
    assert!(!config.contains("dav"), "{config}");
    let secrets = std::fs::read_to_string(&memory.secrets).unwrap_or_default();
    assert!(
        !secrets.contains("not-the-password"),
        "the password is forgotten"
    );
    memory.assert_no_password();
}

#[test]
fn a_card_imported_from_a_file_is_found_again_not_doubled() {
    let server = Server::start();
    let shared = card("shared@test", "Chloé Vasseur", "+33 6 11 22 33 44");
    server.with(|s| s.put(&format!("{CONTACTS}shared.vcf"), shared.clone()));
    let memory = Memory::new();
    let file = memory.vault.parent().unwrap().join("export.vcf");
    std::fs::write(&file, &shared).unwrap();
    let vault = memory.vault_arg();
    let (ok, said) = memory.ipse(&["import", file.to_str().unwrap(), "--vault", &vault], "");
    assert!(ok, "{said}");

    let (ok, said) = memory.add(&server, PASSWORD);
    assert!(ok, "{said}");
    assert_eq!(memory.count_with("people", "Chloé Vasseur"), 1);
    let person = memory.file_with("people", "Chloé Vasseur").unwrap();
    assert!(
        person.contains("contacts") && person.contains("dav-2"),
        "{person}"
    );
}
