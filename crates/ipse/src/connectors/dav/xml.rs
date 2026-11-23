//! WebDAV answers (`207 Multi-Status`, RFC 4918), read for the few properties the
//! connector asks for. Elements are matched by their local name: the names asked for
//! are distinct across the `DAV:`, CalDAV, CardDAV and Calendar Server namespaces.

use quick_xml::Reader;
use quick_xml::events::Event;

/// One `response` of a multistatus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Response {
    /// Its `href`, as the server wrote it.
    pub(crate) href: String,
    /// The status of the response itself, when given: `404` for an item removed, in a
    /// `sync-collection` report.
    pub(crate) status: Option<u16>,
    /// The properties found (`200` propstats only).
    pub(crate) props: Props,
}

/// The properties the connector reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Props {
    pub(crate) etag: Option<String>,
    pub(crate) ctag: Option<String>,
    pub(crate) sync_token: Option<String>,
    pub(crate) name: Option<String>,
    /// The local names in `resourcetype`: `collection`, `calendar`, `addressbook`…
    pub(crate) types: Vec<String>,
    pub(crate) principal: Option<String>,
    pub(crate) calendar_homes: Vec<String>,
    pub(crate) contacts_homes: Vec<String>,
    /// `calendar-data` or `address-data`: the item itself.
    pub(crate) data: Option<String>,
    /// The `name` of each `comp` in `supported-calendar-component-set`.
    pub(crate) components: Vec<String>,
}

/// A whole multistatus.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Multistatus {
    pub(crate) responses: Vec<Response>,
    /// The new `sync-token` of a `sync-collection` report.
    pub(crate) sync_token: Option<String>,
}

/// Reads a multistatus. `None` when the text is not one.
pub(crate) fn multistatus(text: &str) -> Option<Multistatus> {
    let mut reader = Reader::from_str(text);
    let mut parser = Parser::default();
    loop {
        match reader.read_event().ok()? {
            Event::Start(start) => {
                let name = local(start.local_name().as_ref());
                let comp = attribute(&start, "name");
                parser.open(name, comp)?;
            }
            Event::Empty(empty) => {
                let name = local(empty.local_name().as_ref());
                let comp = attribute(&empty, "name");
                parser.open(name, comp)?;
                parser.close()?;
            }
            Event::Text(t) => parser.text.push_str(&t.xml10_content()),
            Event::CData(c) => parser.text.push_str(&c.into_inner()),
            Event::GeneralRef(r) => {
                let c = match r.resolve_char_ref() {
                    Ok(Some(c)) => c,
                    _ => match r.xml10_content().as_ref() {
                        "lt" => '<',
                        "gt" => '>',
                        "amp" => '&',
                        "quot" => '"',
                        "apos" => '\'',
                        _ => continue,
                    },
                };
                parser.text.push(c);
            }
            Event::End(_) => parser.close()?,
            Event::Eof => break,
            _ => {}
        }
    }
    parser.seen_root.then_some(parser.out)
}

/// What is being read: where in the tree, and the response and propstat open.
#[derive(Default)]
struct Parser {
    out: Multistatus,
    /// The local names of the open elements.
    path: Vec<String>,
    response: Response,
    propstat: Props,
    propstat_status: Option<u16>,
    text: String,
    seen_root: bool,
}

impl Parser {
    /// An element opens; `comp` is its `name` attribute. `None` when the root is not a
    /// multistatus.
    fn open(&mut self, name: String, comp: Option<String>) -> Option<()> {
        match self.path.last().map(String::as_str) {
            None if name != "multistatus" => return None,
            None => self.seen_root = true,
            Some("resourcetype") => self.propstat.types.push(name.clone()),
            Some("supported-calendar-component-set") if name == "comp" => {
                self.propstat.components.extend(comp);
            }
            Some(_) => {}
        }
        match name.as_str() {
            "response" => self.response = Response::default(),
            "propstat" => {
                self.propstat = Props::default();
                self.propstat_status = None;
            }
            _ => {}
        }
        self.path.push(name);
        self.text.clear();
        Some(())
    }

    /// The element last opened closes, with the text it held.
    fn close(&mut self) -> Option<()> {
        let name = self.path.pop()?;
        let parent = self.path.last().map(String::as_str);
        let value = std::mem::take(&mut self.text);
        let trimmed = value.trim().to_owned();
        let props = &mut self.propstat;
        match (name.as_str(), parent) {
            ("href", Some("response")) => self.response.href = trimmed,
            ("href", Some("current-user-principal")) => props.principal = Some(trimmed),
            ("href", Some("calendar-home-set")) => props.calendar_homes.push(trimmed),
            ("href", Some("addressbook-home-set")) => props.contacts_homes.push(trimmed),
            ("status", Some("response")) => self.response.status = status(&trimmed),
            ("status", Some("propstat")) => self.propstat_status = status(&trimmed),
            ("sync-token", Some("multistatus")) => self.out.sync_token = Some(trimmed),
            ("getetag", _) => props.etag = Some(trimmed),
            ("getctag", _) => props.ctag = Some(trimmed),
            ("sync-token", _) => props.sync_token = Some(trimmed),
            ("displayname", _) => props.name = Some(trimmed),
            ("calendar-data" | "address-data", _) => props.data = Some(value),
            ("propstat", _) => {
                if self.propstat_status.is_none_or(|s| (200..300).contains(&s)) {
                    merge(&mut self.response.props, std::mem::take(&mut self.propstat));
                }
            }
            ("response", _) => self.out.responses.push(std::mem::take(&mut self.response)),
            _ => {}
        }
        Some(())
    }
}

/// The local part of a qualified name: `d:href` is `href`.
fn local(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn attribute(start: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .map(|a| a.value.into_owned())
}

/// The code of `HTTP/1.1 404 Not Found`.
fn status(line: &str) -> Option<u16> {
    line.split_whitespace().nth(1)?.parse().ok()
}

fn merge(into: &mut Props, from: Props) {
    let Props {
        etag,
        ctag,
        sync_token,
        name,
        types,
        principal,
        calendar_homes,
        contacts_homes,
        data,
        components,
    } = from;
    into.etag = into.etag.take().or(etag);
    into.ctag = into.ctag.take().or(ctag);
    into.sync_token = into.sync_token.take().or(sync_token);
    into.name = into.name.take().or(name);
    into.types.extend(types);
    into.principal = into.principal.take().or(principal);
    into.calendar_homes.extend(calendar_homes);
    into.contacts_homes.extend(contacts_homes);
    into.data = into.data.take().or(data);
    into.components.extend(components);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_with_its_collections() {
        let text = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/">
  <d:response>
    <d:href>/dav/calendars/ines/</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/ines/perso/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <d:displayname>Perso &amp; famille</d:displayname>
        <cs:getctag>"ctag-1"</cs:getctag>
        <d:sync-token>http://example.org/ns/sync/7</d:sync-token>
        <cal:supported-calendar-component-set><cal:comp name="VEVENT"/><cal:comp name="VTODO"/></cal:supported-calendar-component-set>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
    <d:propstat><d:prop><d:getetag/></d:prop><d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
  </d:response>
</d:multistatus>"#;
        let all = multistatus(text).unwrap();
        assert_eq!(all.responses.len(), 2);
        let perso = &all.responses[1];
        assert_eq!(perso.href, "/dav/calendars/ines/perso/");
        assert_eq!(perso.props.types, ["collection", "calendar"]);
        assert_eq!(perso.props.name.as_deref(), Some("Perso & famille"));
        assert_eq!(perso.props.ctag.as_deref(), Some("\"ctag-1\""));
        assert_eq!(
            perso.props.sync_token.as_deref(),
            Some("http://example.org/ns/sync/7")
        );
        assert_eq!(perso.props.components, ["VEVENT", "VTODO"]);
        assert_eq!(perso.props.etag, None, "a 404 propstat is left out");
    }

    #[test]
    fn a_sync_report_with_an_item_removed_and_data() {
        let text = "<multistatus xmlns=\"DAV:\"><response><href>/c/a.ics</href><propstat><prop><getetag>\"2\"</getetag><calendar-data xmlns=\"urn:ietf:params:xml:ns:caldav\">BEGIN:VCALENDAR\r\nSUMMARY:D&#xEE;ner\r\nEND:VCALENDAR\r\n</calendar-data></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/c/b.ics</href><status>HTTP/1.1 404 Not Found</status></response><sync-token>tok-9</sync-token></multistatus>";
        let all = multistatus(text).unwrap();
        assert_eq!(all.sync_token.as_deref(), Some("tok-9"));
        assert_eq!(all.responses[0].props.etag.as_deref(), Some("\"2\""));
        let data = all.responses[0].props.data.as_deref().unwrap();
        assert!(data.contains("SUMMARY:Dîner"), "{data}");
        assert_eq!(all.responses[1].status, Some(404));
    }

    #[test]
    fn principals_and_homes() {
        let text = r#"<D:multistatus xmlns:D="DAV:"><D:response><D:href>/</D:href><D:propstat><D:prop><D:current-user-principal><D:href>/principals/ines/</D:href></D:current-user-principal><C:calendar-home-set xmlns:C="urn:ietf:params:xml:ns:caldav"><D:href>https://p42.example.org/123/calendars/</D:href></C:calendar-home-set></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
        let all = multistatus(text).unwrap();
        let props = &all.responses[0].props;
        assert_eq!(props.principal.as_deref(), Some("/principals/ines/"));
        assert_eq!(
            props.calendar_homes,
            ["https://p42.example.org/123/calendars/"]
        );
    }

    #[test]
    fn what_is_not_a_multistatus() {
        assert_eq!(multistatus("<html><body>Login</body></html>"), None);
        assert_eq!(multistatus("not xml at all"), None);
        assert_eq!(multistatus(""), None);
    }
}
