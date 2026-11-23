//! The CalDAV and CardDAV connector (`docs/connectors/dav.md`): keeps the agenda and
//! the contacts of an account on a server in sync, read only.
//!
//! Decision 0008, rung 1: CalDAV (RFC 4791) and CardDAV (RFC 6352) are the open
//! standards these servers offer other apps. Events and cards are read as the calendar
//! and contacts connectors read their files, and sent as the same records, so that
//! the vault code is shared.

mod http;
mod xml;

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use ipse_protocol::connector::{Hello, Session, SessionResult};
use ipse_protocol::{
    Access, DoneSummary, ErrorCode, FileAccess, Localized, Manifest, Mode, PROTOCOL_VERSION,
    Record, SyncSettings,
};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::calendar::Reader;
use super::contacts::{self, Country};
use http::{Client, Failure};

pub(crate) use http::address;

/// The connector's identifier.
pub(crate) const ID: &str = "dav";

/// The secret holding the app password.
pub(crate) const PASSWORD_SECRET: &str = "password";

/// Items read per multiget request.
const MULTIGET: usize = 100;

/// The built-in manifest (`docs/spec/connectors.md`, section 2).
#[allow(dead_code)] // read by the connector host, #25
pub(crate) fn manifest() -> Manifest {
    let localized = |pairs: [(&str, &str); 2]| {
        Localized::ByLanguage(
            pairs
                .into_iter()
                .map(|(l, t)| (l.to_owned(), t.to_owned()))
                .collect(),
        )
    };
    Manifest {
        protocol: PROTOCOL_VERSION,
        id: ID.to_owned(),
        name: localized([
            ("en", "Calendars and contacts account"),
            ("fr", "Compte d'agendas et de contacts"),
        ]),
        summary: localized([
            (
                "en",
                "Your agenda and your contacts, kept in sync with a CalDAV and CardDAV server (Nextcloud, iCloud, Fastmail…).",
            ),
            (
                "fr",
                "Votre agenda et vos contacts, synchronisés avec un serveur CalDAV et CardDAV (Nextcloud, iCloud, Fastmail…).",
            ),
        ]),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        command: vec!["ipse".into(), "connector".into(), ID.into()],
        modes: vec![Mode::Sync],
        rung: 1,
        kinds: vec![
            super::calendar::KIND.to_owned(),
            super::contacts::KIND.to_owned(),
        ],
        category: "agenda".to_owned(),
        access: Access {
            // Only the server the person gives.
            network: vec!["*".to_owned()],
            files: FileAccess::None,
            secrets: vec![PASSWORD_SECRET.to_owned()],
        },
        sync: Some(SyncSettings {
            default_every: "15m".to_owned(),
        }),
    }
}

/// What a collection holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    /// Events (CalDAV).
    Calendar,
    /// Cards (CardDAV).
    Contacts,
}

/// A calendar or an address book found on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Found {
    /// Its address: what the person chooses, in the instance's `collections`.
    pub(crate) url: String,
    pub(crate) kind: Kind,
    /// Its name on the server.
    pub(crate) name: String,
}

/// Where the server keeps the person's calendars or address books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Home {
    url: String,
    kind: Kind,
}

/// What the connector remembers of one chosen collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ctag: Option<String>,
    /// Without a `sync-token`: the `getetag` of each item, by `href`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    etags: BTreeMap<String, String>,
}

/// What the connector remembers between sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Cursor {
    /// The day, in the vault's time zone, of the last whole read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    day: Option<String>,
    #[serde(default)]
    homes: Vec<Home>,
    /// Every calendar and address book found, to choose from.
    #[serde(default)]
    pub(crate) found: Vec<Found>,
    /// The chosen collections, by address.
    #[serde(default)]
    collections: BTreeMap<String, State>,
}

/// A collection as a home lists it.
#[derive(Debug, Clone)]
struct Listed {
    found: Found,
    token: Option<String>,
    ctag: Option<String>,
}

/// The settings of an account, from the instance's `config`.
struct Account {
    url: String,
    local: bool,
    user: String,
    /// `None` until the person chose: the session only discovers.
    collections: Option<Vec<String>>,
}

fn account(config: &serde_json::Map<String, Value>) -> Result<Account, Failure> {
    let text = |key: &str| config.get(key).and_then(Value::as_str).unwrap_or_default();
    let (url, local) = address(text("url"))?;
    let collections = config
        .get("collections")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });
    Ok(Account {
        url,
        local,
        user: text("user").to_owned(),
        collections,
    })
}

/// Runs one session.
pub(crate) fn run(input: impl BufRead, output: impl Write) -> SessionResult<()> {
    let (mut session, hello) = Session::open(input, output, ID, env!("CARGO_PKG_VERSION"))?;
    let french = hello.language == "fr";
    if hello.mode != Mode::Sync {
        session.error(
            ErrorCode::Unsupported,
            say(
                french,
                "This connector keeps an account in sync; it imports no file.",
                "Ce connecteur synchronise un compte ; il n'importe aucun fichier.",
            ),
        )?;
        return session.finish();
    }
    let full = session.wait_start()?;
    let account = match account(&hello.config) {
        Ok(account) => account,
        Err(failure) => return fail(session, &failure, french),
    };
    let Some(password) = hello.secrets.get(PASSWORD_SECRET) else {
        session.error(
            ErrorCode::BadInput,
            say(
                french,
                "This account's password is missing from the keychain. Connect the account again.",
                "Le mot de passe de ce compte manque dans le trousseau. Connectez à nouveau le compte.",
            ),
        )?;
        return session.finish();
    };
    let client = Client::new(
        &account.url,
        account.local,
        &account.user,
        password.expose(),
    );
    let cursor = hello
        .cursor
        .clone()
        .filter(|_| !full)
        .and_then(|c| serde_json::from_value::<Cursor>(c).ok())
        .unwrap_or_default();
    match sync(&client, &account, &hello, cursor, full) {
        Ok(Synced {
            records,
            cursor,
            complete,
            upserts,
            report,
        }) => {
            // Batches of at most 1000 records, the cursor with the last.
            session.push(records, serde_json::to_value(&cursor).ok())?;
            let sent = session.flush()?.accepted;
            if let Some(message) = report {
                session.progress(sent, Some(sent), Some(message))?;
            }
            session.done(DoneSummary {
                upserts,
                complete,
                ..DoneSummary::default()
            })?;
            session.finish()
        }
        Err(failure) => fail(session, &failure, french),
    }
}

/// What a session found.
struct Synced {
    records: Vec<Record>,
    cursor: Cursor,
    /// Every chosen collection was read whole.
    complete: bool,
    upserts: u64,
    report: Option<String>,
}

/// Finds the collections, then reads what changed in the chosen ones, or all of them.
fn sync(
    client: &Client,
    account: &Account,
    hello: &Hello,
    mut cursor: Cursor,
    full: bool,
) -> Result<Synced, Failure> {
    let french = hello.language == "fr";
    let time_zone = TimeZone::get(&hello.time_zone).unwrap_or(TimeZone::UTC);
    let now = Timestamp::now();
    let today = now.to_zoned(time_zone.clone()).date().to_string();

    let listed = match (cursor.homes.is_empty(), list(client, &cursor.homes)) {
        (false, Ok(listed)) if !listed.is_empty() => listed,
        // No home known yet, or the server moved them: found again.
        _ => {
            cursor.homes = discover(client, &account.url)?;
            list(client, &cursor.homes)?
        }
    };
    if listed.is_empty() {
        return Err(http::NOT_DAV);
    }
    cursor.found = listed.iter().map(|l| l.found.clone()).collect();

    let Some(chosen) = &account.collections else {
        // Nothing chosen yet: the person picks from what was found.
        return Ok(Synced {
            records: Vec::new(),
            cursor,
            complete: false,
            upserts: 0,
            report: None,
        });
    };
    let chosen: Vec<&Listed> = listed
        .iter()
        .filter(|l| chosen.contains(&l.found.url))
        .collect();
    let country = Country::of_time_zone(&hello.time_zone);
    let mut read = Read::new(time_zone.clone(), now, country);

    let mut whole = full
        || cursor.day.as_deref() != Some(today.as_str())
        || chosen
            .iter()
            .any(|l| !cursor.collections.contains_key(&l.found.url));
    let mut states = BTreeMap::new();
    if !whole {
        for l in &chosen {
            let before = cursor
                .collections
                .get(&l.found.url)
                .cloned()
                .unwrap_or_default();
            match changes(client, l, &before)? {
                Changes::None => {
                    states.insert(l.found.url.clone(), before);
                }
                Changes::Some { hrefs, state } => {
                    for (_, data) in multiget(client, &l.found, &hrefs)? {
                        read.add(l.found.kind, &data);
                    }
                    states.insert(l.found.url.clone(), state);
                }
                Changes::Removed => {
                    whole = true;
                    break;
                }
            }
        }
    }
    if whole {
        read = Read::new(time_zone, now, country);
        states.clear();
        for l in &chosen {
            let etags = etags(client, &l.found.url)?;
            let hrefs: Vec<String> = etags.keys().cloned().collect();
            for (_, data) in multiget(client, &l.found, &hrefs)? {
                read.add(l.found.kind, &data);
            }
            let state = State {
                token: l.token.clone(),
                ctag: l.ctag.clone(),
                etags: if l.token.is_some() {
                    BTreeMap::new()
                } else {
                    etags
                },
            };
            states.insert(l.found.url.clone(), state);
        }
        cursor.day = Some(today);
    }
    cursor.collections = states;
    let (records, upserts, report) = read.finish(french);
    Ok(Synced {
        records,
        cursor,
        complete: whole,
        upserts,
        report,
    })
}

/// Events and cards read from the server's items.
struct Read {
    events: Reader,
    records: Vec<Record>,
    country: Option<Country>,
    now: Timestamp,
    unreadable: u64,
}

impl Read {
    fn new(time_zone: TimeZone, now: Timestamp, country: Option<Country>) -> Self {
        Self {
            events: Reader::new(time_zone, now),
            records: Vec::new(),
            country,
            now,
            unreadable: 0,
        }
    }

    fn add(&mut self, kind: Kind, data: &str) {
        match kind {
            Kind::Calendar => {
                if !self.events.read(data) {
                    self.unreadable += 1;
                }
                self.records.extend(self.events.take());
            }
            Kind::Contacts => match contacts::records(data, self.country, self.now) {
                Some((cards, skipped)) => {
                    self.records.extend(cards);
                    self.unreadable += skipped;
                }
                None => self.unreadable += 1,
            },
        }
    }

    fn finish(mut self, french: bool) -> (Vec<Record>, u64, Option<String>) {
        let (rest, _, _, events_report) = self.events.finish(french);
        self.records.extend(rest);
        let upserts = self
            .records
            .iter()
            .filter(|r| r.op == ipse_protocol::Op::Upsert)
            .count() as u64;
        let mut report = events_report;
        if self.unreadable > 0 {
            let n = self.unreadable;
            let line = if french {
                format!("{n} élément(s) illisible(s) ignoré(s).")
            } else {
                format!("{n} unreadable item(s) skipped.")
            };
            report = Some(report.map_or(line.clone(), |r| format!("{r} {line}")));
        }
        (self.records, upserts, report)
    }
}

/// What changed in a collection since `before`.
enum Changes {
    None,
    /// Items changed or added, to read; the collection's state once read.
    Some {
        hrefs: Vec<String>,
        state: State,
    },
    /// An item was removed, or the token refused: everything is read whole.
    Removed,
}

fn changes(client: &Client, listed: &Listed, before: &State) -> Result<Changes, Failure> {
    let unchanged = match (&listed.token, &listed.ctag) {
        (Some(token), _) => before.token.as_ref() == Some(token),
        (None, Some(ctag)) => before.ctag.as_ref() == Some(ctag) && !before.etags.is_empty(),
        (None, None) => false,
    };
    if unchanged {
        return Ok(Changes::None);
    }
    if let (Some(_), Some(old)) = (&listed.token, &before.token) {
        let Some(delta) = sync_collection(client, &listed.found.url, old)? else {
            return Ok(Changes::Removed);
        };
        if delta.removed {
            return Ok(Changes::Removed);
        }
        let hrefs = delta.changed;
        let state = State {
            token: delta.token.or_else(|| listed.token.clone()),
            ctag: listed.ctag.clone(),
            etags: BTreeMap::new(),
        };
        return Ok(Changes::Some { hrefs, state });
    }
    let now = etags(client, &listed.found.url)?;
    if before.etags.keys().any(|href| !now.contains_key(href)) {
        return Ok(Changes::Removed);
    }
    let hrefs = now
        .iter()
        .filter(|(href, etag)| before.etags.get(*href) != Some(*etag))
        .map(|(href, _)| href.clone())
        .collect();
    let state = State {
        token: listed.token.clone(),
        ctag: listed.ctag.clone(),
        etags: if listed.token.is_some() {
            BTreeMap::new()
        } else {
            now
        },
    };
    Ok(Changes::Some { hrefs, state })
}

const PROPFIND_PRINCIPAL: &str = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:current-user-principal/></D:prop></D:propfind>"#;

const PROPFIND_HOMES: &str = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="urn:ietf:params:xml:ns:carddav"><D:prop><C:calendar-home-set/><A:addressbook-home-set/></D:prop></D:propfind>"#;

const PROPFIND_COLLECTIONS: &str = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/"><D:prop><D:resourcetype/><D:displayname/><CS:getctag/><D:sync-token/><C:supported-calendar-component-set/></D:prop></D:propfind>"#;

const PROPFIND_ETAGS: &str = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:getetag/><D:resourcetype/></D:prop></D:propfind>"#;

/// Finds the homes of the account at `base`: the principal from `.well-known` (RFC
/// 6764) or the address itself, then its home sets.
fn discover(client: &Client, base: &str) -> Result<Vec<Home>, Failure> {
    let mut homes: Vec<Home> = Vec::new();
    for (service, kind) in [("caldav", Kind::Calendar), ("carddav", Kind::Contacts)] {
        let mut starts = known(base, kind);
        starts.push(format!("{base}/.well-known/{service}"));
        starts.push(base.to_owned());
        for start in starts {
            let Some(principal) = principal(client, &start)? else {
                continue;
            };
            let answer = client.send("PROPFIND", &principal, Some("0"), PROPFIND_HOMES)?;
            let Some(all) = (answer.status == 207)
                .then(|| xml::multistatus(&answer.body))
                .flatten()
            else {
                continue;
            };
            let hrefs: Vec<String> = all
                .responses
                .iter()
                .flat_map(|r| match kind {
                    Kind::Calendar => r.props.calendar_homes.clone(),
                    Kind::Contacts => r.props.contacts_homes.clone(),
                })
                .filter_map(|href| http::join(&answer.url, &href))
                .collect();
            if hrefs.is_empty() {
                continue;
            }
            for url in hrefs {
                if !homes.iter().any(|h| h.url == url && h.kind == kind) {
                    homes.push(Home { url, kind });
                }
            }
            break;
        }
    }
    if homes.is_empty() {
        return Err(http::NOT_DAV);
    }
    Ok(homes)
}

/// Where known services keep each kind, when it is not where the address says.
fn known(base: &str, kind: Kind) -> Vec<String> {
    let host = http::host_of(base).unwrap_or_default();
    let ends = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    let url = match (kind, ends("icloud.com"), ends("fastmail.com")) {
        (Kind::Calendar, true, _) => "https://caldav.icloud.com",
        (Kind::Contacts, true, _) => "https://contacts.icloud.com",
        (Kind::Calendar, _, true) => "https://caldav.fastmail.com",
        (Kind::Contacts, _, true) => "https://carddav.fastmail.com",
        _ => return Vec::new(),
    };
    vec![url.to_owned()]
}

/// The principal `start` names, if it answers as WebDAV.
fn principal(client: &Client, start: &str) -> Result<Option<String>, Failure> {
    let answer = client.send("PROPFIND", start, Some("0"), PROPFIND_PRINCIPAL)?;
    if answer.status != 207 {
        return Ok(None);
    }
    Ok(xml::multistatus(&answer.body)
        .and_then(|all| all.responses.into_iter().find_map(|r| r.props.principal))
        .and_then(|href| http::join(&answer.url, &href)))
}

/// The calendars and address books in `homes`.
fn list(client: &Client, homes: &[Home]) -> Result<Vec<Listed>, Failure> {
    let mut listed: Vec<Listed> = Vec::new();
    for home in homes {
        let answer = client.send("PROPFIND", &home.url, Some("1"), PROPFIND_COLLECTIONS)?;
        if answer.status != 207 {
            return Ok(Vec::new());
        }
        let all = xml::multistatus(&answer.body).ok_or(http::NOT_DAV)?;
        for response in all.responses {
            let props = &response.props;
            let types = &props.types;
            let kind = if home.kind == Kind::Calendar
                && types.iter().any(|t| t == "calendar")
                && (props.components.is_empty() || props.components.iter().any(|c| c == "VEVENT"))
            {
                Kind::Calendar
            } else if home.kind == Kind::Contacts && types.iter().any(|t| t == "addressbook") {
                Kind::Contacts
            } else {
                continue;
            };
            let Some(url) = http::join(&answer.url, &response.href) else {
                continue;
            };
            if listed.iter().any(|l| l.found.url == url) {
                continue;
            }
            let name = props
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| last_segment(&url));
            listed.push(Listed {
                found: Found { url, kind, name },
                token: props.sync_token.clone().filter(|t| !t.is_empty()),
                ctag: props.ctag.clone().filter(|t| !t.is_empty()),
            });
        }
    }
    Ok(listed)
}

fn last_segment(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Every item of the collection at `url`, by `href`, with its `getetag`.
fn etags(client: &Client, url: &str) -> Result<BTreeMap<String, String>, Failure> {
    let answer = client.send("PROPFIND", url, Some("1"), PROPFIND_ETAGS)?;
    if answer.status != 207 {
        return Err(http::NOT_DAV);
    }
    let all = xml::multistatus(&answer.body).ok_or(http::NOT_DAV)?;
    let own = url.trim_end_matches('/');
    Ok(all
        .responses
        .into_iter()
        .filter(|r| !r.props.types.iter().any(|t| t == "collection"))
        .filter(|r| http::join(&answer.url, &r.href).is_none_or(|u| u.trim_end_matches('/') != own))
        .filter_map(|r| Some((r.href, r.props.etag?)))
        .collect())
}

/// What a `sync-collection` report says changed.
struct Delta {
    /// The items changed or added, by `href`.
    changed: Vec<String>,
    /// Whether an item was removed.
    removed: bool,
    /// The collection's new token.
    token: Option<String>,
}

/// What changed since `token` (RFC 6578). `None` when the server no longer knows the
/// token.
fn sync_collection(client: &Client, url: &str, token: &str) -> Result<Option<Delta>, Failure> {
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><D:sync-collection xmlns:D="DAV:"><D:sync-token>{}</D:sync-token><D:sync-level>1</D:sync-level><D:prop><D:getetag/></D:prop></D:sync-collection>"#,
        escape(token)
    );
    let answer = client.send("REPORT", url, Some("0"), &body)?;
    if answer.status != 207 {
        return Ok(None);
    }
    let Some(all) = xml::multistatus(&answer.body) else {
        return Ok(None);
    };
    let own = url.trim_end_matches('/');
    let mut changed = Vec::new();
    let mut removed = false;
    for response in all.responses {
        let absolute = http::join(&answer.url, &response.href);
        if absolute.is_some_and(|u| u.trim_end_matches('/') == own) {
            continue;
        }
        if response.status == Some(404) {
            removed = true;
        } else {
            changed.push(response.href);
        }
    }
    Ok(Some(Delta {
        changed,
        removed,
        token: all.sync_token,
    }))
}

/// The items `hrefs` of `collection`, with their data, read 100 at a time.
fn multiget(
    client: &Client,
    collection: &Found,
    hrefs: &[String],
) -> Result<Vec<(String, String)>, Failure> {
    let (report, namespace, data) = match collection.kind {
        Kind::Calendar => (
            "calendar-multiget",
            "urn:ietf:params:xml:ns:caldav",
            "calendar-data",
        ),
        Kind::Contacts => (
            "addressbook-multiget",
            "urn:ietf:params:xml:ns:carddav",
            "address-data",
        ),
    };
    let mut items = Vec::with_capacity(hrefs.len());
    for chunk in hrefs.chunks(MULTIGET) {
        let mut body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><C:{report} xmlns:D="DAV:" xmlns:C="{namespace}"><D:prop><D:getetag/><C:{data}/></D:prop>"#
        );
        for href in chunk {
            body.push_str("<D:href>");
            body.push_str(&escape(href));
            body.push_str("</D:href>");
        }
        body.push_str("</C:");
        body.push_str(report);
        body.push('>');
        let answer = client.send("REPORT", &collection.url, Some("1"), &body)?;
        if answer.status != 207 {
            return Err(http::NOT_DAV);
        }
        let all = xml::multistatus(&answer.body).ok_or(http::NOT_DAV)?;
        items.extend(
            all.responses
                .into_iter()
                .filter_map(|r| Some((r.href, r.props.data?))),
        );
    }
    Ok(items)
}

/// `text` with XML's special characters escaped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn fail<R: BufRead, W: Write>(
    mut session: Session<R, W>,
    failure: &Failure,
    french: bool,
) -> SessionResult<()> {
    session.error_retry(
        failure.code,
        failure.retry_after.clone(),
        failure.message(french),
    )?;
    session.finish()
}

fn say(french: bool, en: &str, fr: &str) -> String {
    if french { fr } else { en }.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_follows_the_spec() {
        let m = manifest();
        assert_eq!((m.id.as_str(), m.rung), ("dav", 1));
        assert_eq!(m.modes, [Mode::Sync]);
        assert_eq!(m.access.secrets, ["password"]);
        assert_eq!(m.kinds, ["calendar.event", "contact.card"]);
    }

    #[test]
    fn known_services() {
        assert_eq!(
            known("https://caldav.icloud.com", Kind::Contacts),
            ["https://contacts.icloud.com"]
        );
        assert_eq!(
            known("https://fastmail.com", Kind::Calendar),
            ["https://caldav.fastmail.com"]
        );
        assert!(known("https://cloud.example.org", Kind::Calendar).is_empty());
        assert!(known("https://noticloud.com", Kind::Calendar).is_empty());
    }

    #[test]
    fn hrefs_are_escaped() {
        assert_eq!(
            escape("/c/a&b<\"x\">.ics"),
            "/c/a&amp;b&lt;&quot;x&quot;&gt;.ics"
        );
    }
}
