//! A streaming reader of iCalendar files (RFC 5545).
//!
//! Events and time zone definitions are returned one at a time, as they are read, so a
//! file of any size uses little memory. Values stay as written: dates and rules are
//! read by [`super::time`] and [`super::rrule`]. Alarms, to-dos and unknown properties
//! are skipped.

use std::io::BufRead;

use crate::connectors::content_line::{Lines, Property, non_empty, text};

/// A date or date-time value as written, with its time zone parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stamp {
    /// `20261114`, `20261114T200000` or `20261114T190000Z`.
    pub(crate) value: String,
    /// The `TZID` parameter.
    pub(crate) tzid: Option<String>,
}

/// A person named in an event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Person {
    pub(crate) name: Option<String>,
    pub(crate) email: Option<String>,
    /// `accepted`, `declined`, `tentative`, `needs-action`… for attendees.
    pub(crate) response: Option<String>,
}

/// What Ipse keeps from one `VEVENT`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Event {
    pub(crate) uid: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) location: Option<String>,
    /// Lower case: `confirmed`, `tentative`, `cancelled`.
    pub(crate) status: Option<String>,
    pub(crate) start: Option<Stamp>,
    pub(crate) end: Option<Stamp>,
    /// `PT1H30M`, as written.
    pub(crate) duration: Option<String>,
    pub(crate) organizer: Option<Person>,
    pub(crate) attendees: Vec<Person>,
    /// The `RRULE` value, as written.
    pub(crate) rule: Option<String>,
    pub(crate) rdates: Vec<Stamp>,
    pub(crate) exdates: Vec<Stamp>,
    pub(crate) recurrence_id: Option<Stamp>,
}

/// One `STANDARD` or `DAYLIGHT` part of a `VTIMEZONE`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Observance {
    pub(crate) daylight: bool,
    /// Local date-time of the first change, as written.
    pub(crate) start: String,
    /// `+0100`, as written.
    pub(crate) offset_from: String,
    pub(crate) offset_to: String,
    pub(crate) rule: Option<String>,
}

/// A `VTIMEZONE`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Definition {
    pub(crate) tzid: String,
    pub(crate) observances: Vec<Observance>,
}

/// One thing read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // nearly every item is an event: boxing would add an allocation each
pub(crate) enum Item {
    Event(Event),
    TimeZone(Definition),
    /// An event that cannot be read (it never ends); it is skipped.
    Unreadable,
}

/// The file itself cannot be read.
#[derive(Debug)]
pub(crate) struct ReadError;

/// The component being read.
#[allow(clippy::large_enum_variant)] // nearly always an event: boxing would add an allocation each
enum Open {
    Event(Event),
    TimeZone(Definition),
    Observance(Definition, Observance),
}

/// The events and time zones of an iCalendar file, in order.
pub(crate) struct Items<R> {
    lines: Lines<R>,
    open: Option<Open>,
    /// Depth of components nested in the open one and ignored (alarms).
    ignored: usize,
    done: bool,
}

impl<R: BufRead> Items<R> {
    pub(crate) fn new(input: R) -> Self {
        Self {
            lines: Lines::new(input),
            open: None,
            ignored: 0,
            done: false,
        }
    }
}

impl<R: BufRead> Iterator for Items<R> {
    type Item = Result<Item, ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            let line = match self.lines.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => {
                    self.done = true;
                    let unfinished = matches!(self.open.take(), Some(Open::Event(_)));
                    return unfinished.then_some(Ok(Item::Unreadable));
                }
                Err(_) => {
                    self.done = true;
                    return Some(Err(ReadError));
                }
            };
            let Some(property) = Property::parse(&line) else {
                continue;
            };
            let component = || property.value.trim().to_ascii_uppercase();
            match property.name.as_ref() {
                "BEGIN" if self.ignored > 0 => self.ignored += 1,
                "END" if self.ignored > 0 => self.ignored -= 1,
                _ if self.ignored > 0 => {}
                "BEGIN" => match (component().as_str(), self.open.take()) {
                    ("VEVENT", Some(Open::Event(_))) => {
                        // An event that never ended: skip it; the next call starts the new one.
                        self.lines.requeue(line.clone());
                        return Some(Ok(Item::Unreadable));
                    }
                    ("VEVENT", _) => self.open = Some(Open::Event(Event::default())),
                    ("VTIMEZONE", _) => self.open = Some(Open::TimeZone(Definition::default())),
                    (kind @ ("STANDARD" | "DAYLIGHT"), Some(Open::TimeZone(definition))) => {
                        let observance = Observance {
                            daylight: kind == "DAYLIGHT",
                            ..Observance::default()
                        };
                        self.open = Some(Open::Observance(definition, observance));
                    }
                    (_, open @ Some(_)) => {
                        self.open = open;
                        self.ignored = 1;
                    }
                    (_, None) => {}
                },
                "END" => match (component().as_str(), self.open.take()) {
                    ("VEVENT", Some(Open::Event(event))) => return Some(Ok(Item::Event(event))),
                    ("VTIMEZONE", Some(Open::TimeZone(definition))) => {
                        return Some(Ok(Item::TimeZone(definition)));
                    }
                    ("STANDARD" | "DAYLIGHT", Some(Open::Observance(mut definition, o))) => {
                        definition.observances.push(o);
                        self.open = Some(Open::TimeZone(definition));
                    }
                    (_, open) => self.open = open,
                },
                _ => match &mut self.open {
                    Some(Open::Event(event)) => add_to_event(event, &property),
                    Some(Open::TimeZone(definition)) if property.name == "TZID" => {
                        property.value.trim().clone_into(&mut definition.tzid);
                    }
                    Some(Open::Observance(_, observance)) => {
                        add_to_observance(observance, &property);
                    }
                    _ => {}
                },
            }
        }
    }
}

fn add_to_event(event: &mut Event, p: &Property) {
    match p.name.as_ref() {
        "UID" => event.uid = non_empty(p.value),
        "SUMMARY" => event.summary = text(p.value),
        "DESCRIPTION" => event.description = text(p.value),
        "LOCATION" => event.location = text(p.value),
        "STATUS" => event.status = non_empty(&p.value.to_ascii_lowercase()),
        "DTSTART" => event.start = stamps(p).into_iter().next(),
        "DTEND" => event.end = stamps(p).into_iter().next(),
        "DURATION" => event.duration = non_empty(p.value),
        "ORGANIZER" => event.organizer = person(p),
        "ATTENDEE" => event.attendees.extend(person(p)),
        "RRULE" => event.rule = non_empty(p.value),
        "RDATE" => event.rdates.extend(stamps(p)),
        "EXDATE" => event.exdates.extend(stamps(p)),
        "RECURRENCE-ID" => event.recurrence_id = stamps(p).into_iter().next(),
        _ => {}
    }
}

fn add_to_observance(observance: &mut Observance, p: &Property) {
    let value = p.value.trim().to_owned();
    match p.name.as_ref() {
        "DTSTART" => observance.start = value,
        "TZOFFSETFROM" => observance.offset_from = value,
        "TZOFFSETTO" => observance.offset_to = value,
        "RRULE" => observance.rule = Some(value),
        _ => {}
    }
}

/// The values of a date property: one, or several separated by commas (`RDATE`,
/// `EXDATE`). A period (`start/end`) keeps its start.
fn stamps(p: &Property) -> Vec<Stamp> {
    let tzid = p.param("TZID").and_then(non_empty);
    p.value
        .split(',')
        .filter_map(|v| non_empty(v.split('/').next().unwrap_or_default()))
        .map(|value| Stamp {
            value,
            tzid: tzid.clone(),
        })
        .collect()
}

/// An organizer or attendee: its `CN` and its `mailto:` address.
fn person(p: &Property) -> Option<Person> {
    let value = p.value.trim();
    let email = value
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
        .and_then(|_| non_empty(&value[7..]));
    let name = p
        .param("CN")
        .and_then(non_empty)
        .filter(|n| email.as_deref() != Some(n.as_str()));
    let response = p
        .param("PARTSTAT")
        .and_then(|s| non_empty(&s.to_ascii_lowercase()));
    (name.is_some() || email.is_some()).then_some(Person {
        name,
        email,
        response,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(text: &str) -> Vec<Item> {
        Items::new(text.as_bytes()).map(Result::unwrap).collect()
    }

    fn events(text: &str) -> Vec<Event> {
        items(text)
            .into_iter()
            .filter_map(|i| match i {
                Item::Event(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_event_with_everything() {
        let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:abc@example.org\r\n\
                    SUMMARY:Dîner chez Julie\\, enfin\r\nDTSTART;TZID=Europe/Paris:20261114T200000\r\n\
                    DTEND;TZID=Europe/Paris:20261114T230000\r\nLOCATION:12 rue des Lilas\r\n\
                    DESCRIPTION:Apporter\\nle dessert\r\nSTATUS:CONFIRMED\r\n\
                    ORGANIZER;CN=Julie Dupont:mailto:julie@example.org\r\n\
                    ATTENDEE;CN=\"Leroy, Marc\";PARTSTAT=ACCEPTED:MAILTO:marc@example.org\r\n\
                    ATTENDEE;CN=paul@example.org:mailto:paul@example.org\r\n\
                    ATTENDEE:urn:uuid:no-address\r\n\
                    BEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:Rappel\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\n\
                    END:VEVENT\r\nEND:VCALENDAR\r\n";
        let [event] = events(text).try_into().unwrap();
        assert_eq!(event.uid.as_deref(), Some("abc@example.org"));
        assert_eq!(event.summary.as_deref(), Some("Dîner chez Julie, enfin"));
        assert_eq!(event.description.as_deref(), Some("Apporter\nle dessert"));
        assert_eq!(event.status.as_deref(), Some("confirmed"));
        assert_eq!(
            event.start,
            Some(Stamp {
                value: "20261114T200000".into(),
                tzid: Some("Europe/Paris".into())
            })
        );
        assert_eq!(
            event.organizer.unwrap().name.as_deref(),
            Some("Julie Dupont")
        );
        assert_eq!(
            event.attendees,
            [
                Person {
                    name: Some("Leroy, Marc".into()),
                    email: Some("marc@example.org".into()),
                    response: Some("accepted".into()),
                },
                Person {
                    name: None,
                    email: Some("paul@example.org".into()),
                    response: None,
                },
            ]
        );
    }

    #[test]
    fn time_zones_and_repeats() {
        let text = "BEGIN:VCALENDAR\nBEGIN:VTIMEZONE\nTZID:Romance Standard Time\n\
                    BEGIN:STANDARD\nDTSTART:16010101T030000\nTZOFFSETFROM:+0200\nTZOFFSETTO:+0100\n\
                    RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10\nEND:STANDARD\n\
                    BEGIN:DAYLIGHT\nDTSTART:16010101T020000\nTZOFFSETFROM:+0100\nTZOFFSETTO:+0200\n\
                    RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=3\nEND:DAYLIGHT\nEND:VTIMEZONE\n\
                    BEGIN:VEVENT\nUID:w\nDTSTART;VALUE=DATE:20261102\nRRULE:FREQ=WEEKLY\n\
                    EXDATE;VALUE=DATE:20261109,20261116\nRDATE;VALUE=PERIOD:20261201T100000Z/PT1H\n\
                    END:VEVENT\nEND:VCALENDAR\n";
        let all = items(text);
        let Item::TimeZone(tz) = &all[0] else {
            panic!("{all:?}")
        };
        assert_eq!(tz.tzid, "Romance Standard Time");
        assert_eq!(tz.observances.len(), 2);
        assert!(tz.observances[1].daylight);
        assert_eq!(tz.observances[1].offset_to, "+0200");
        let Item::Event(event) = &all[1] else {
            panic!("{all:?}")
        };
        assert_eq!(event.rule.as_deref(), Some("FREQ=WEEKLY"));
        let exdates: Vec<&str> = event.exdates.iter().map(|s| s.value.as_str()).collect();
        assert_eq!(exdates, ["20261109", "20261116"]);
        assert_eq!(event.rdates[0].value, "20261201T100000Z");
    }

    #[test]
    fn an_unfinished_event_is_skipped_and_the_rest_read() {
        let text = "BEGIN:VEVENT\nUID:a\nBEGIN:VEVENT\nUID:b\nEND:VEVENT\nBEGIN:VEVENT\nUID:c\n";
        let all = items(text);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], Item::Unreadable);
        assert!(matches!(&all[1], Item::Event(e) if e.uid.as_deref() == Some("b")));
        assert_eq!(all[2], Item::Unreadable);
    }

    #[test]
    fn todos_and_empty_files_give_nothing() {
        assert!(
            items("BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:t\nEND:VTODO\nEND:VCALENDAR\n").is_empty()
        );
        assert!(items("").is_empty());
    }
}
