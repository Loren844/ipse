# Calendar connector

Follows your calendar's link and keeps your events in sync, or imports them from a calendar file (`.ics`).

| | |
| --- | --- |
| Id | `calendar` |
| Rung (decision 0008) | 1 for a link: calendar apps give each calendar an address in iCal format, their official way to let other apps read it, with no account to sign in to. 4 for a file: every calendar app exports iCalendar files. |
| Modes | `sync` (a link), `import` (files) |
| Access | the files the person picks; the network, only to the host of the link the person gives (`network = ["*"]`); one secret, `url`, the link, kept in the system keychain |
| Sync | every 15 minutes by default |
| Category | `agenda` |
| Kind | `calendar.event` |

## Getting the link

The link is enough to read the calendar: treat it like a password. Ipse keeps it in your system's keychain, never in your memory's files, its journal or its history, and never shows it in a message.

- **Google Calendar**: on calendar.google.com, Settings → under "Settings for my calendars", pick the calendar → Integrate calendar → **Secret address in iCal format**. If it leaks, "Reset" there gives a new one and the old one stops working.
- **Outlook / Microsoft 365**: Settings → Calendar → Shared calendars → **Publish a calendar**, pick the calendar and "Can view all details" → Publish → copy the ICS link.
- **Apple iCloud**: in the Calendar app, the calendar's menu → Share Calendar → **Public Calendar** → copy the `webcal://` link. Apple makes that calendar readable by anyone who has the link, and lists no secret address: share only a calendar you accept that for.

Then: `ipse calendar subscribe "<link>" --name Perso --vault ~/Ipse`. Ipse checks the link, syncs the calendar once, and keeps the subscription. `webcal://` links are read as `https://`; plain `http://` links are refused, except to this computer, because anyone on the way could read them.

## Syncing a link

- The connector fetches the link and sends **the whole calendar** each time: an iCal link cannot give only what changed. It ends with `done.complete`, and the core removes what the calendar no longer holds (`docs/spec/connectors.md`, section 5.8): an event removed before it happened is marked `cancelled`; a past event stays as it happened.
- Its cursor keeps the server's `ETag` and `Last-Modified`, and the day the calendar was last read whole. The same day, the connector asks the server whether the calendar changed; when it did not (HTTP 304), it sends nothing and leaves out `complete`. The first sync of each day reads the calendar whole, so that repeating events are expanded for the days that came since.
- Something that is not a whole calendar removes nothing: a web page instead of a calendar (`bad_input`), a calendar cut short (`unavailable`).
- Failures, in the person's language, never quoting the link: the server cannot be reached or answers 5xx (`unavailable`, tried again with the backoff); 429 (`rate_limited`, with `Retry-After` in seconds when given); 401, 403, 404 or 410 (`auth`: the link was reset or the calendar deleted); a redirect to `http://` or a file over 64 MiB (`bad_input`).
- At most 5 redirects; 15 seconds to connect, 2 minutes in all.

## Getting a file

- **Google Calendar**: calendar.google.com → Settings → Import & export → Export. You get a `.zip` with one `.ics` file per calendar.
- **Apple Calendar**: on a Mac, select a calendar → File → Export → Export.
- **Outlook**: save a calendar as an iCalendar file.

Then: `ipse import agenda.ics --vault ~/Ipse`. Each event becomes a file in `events/`, its guests linked to the people already imported when their email matches exactly one of them.

## Records

Each event is read as soon as it ends in the file, so a file of any size uses little memory.

| Event | Records |
| --- | --- |
| On its own | One `upsert`, `external_id` = its `UID`. |
| Repeating (`RRULE`) | One `upsert` for the series, with its `rule`, `external_id` = its `UID`. Then one `upsert` per occurrence from **one year back to 90 days ahead**, `external_id` = `UID#<recurrence id>`. |
| One occurrence moved or changed (`RECURRENCE-ID`) | One `upsert` with its own content, `external_id` = `UID#<recurrence id>`, sent whatever its date. The series does not send that occurrence again. |
| One occurrence removed (`EXDATE`) | One `delete`, `external_id` = `UID#<recurrence id>`, inside the window, so that a re-import removes an occurrence imported before. |
| Cancelled (`STATUS:CANCELLED`) | Sent with `status: cancelled`. |

An event without `UID` gets a hash of its content instead, so importing the same file twice gives the same ids.

The **recurrence id** is the occurrence's original start: `2026-11-17T17:30:00Z` (in UTC) for a timed event, `2026-11-17` for a whole-day one. It is written the same way whatever time zone the file used, so that the same invitation in two calendars gives the same id (decision 0011).

`data`, fields left out when empty:

| Field | Type | From |
| --- | --- | --- |
| `uid` | text | `UID`, as written |
| `recurrence_id` | text | for an occurrence: its original start, as above |
| `title` | text | `SUMMARY` |
| `start` | `YYYY-MM-DD`, or RFC 3339 with offset | `DTSTART` |
| `end` | same | `DTEND`, else `DTSTART` + `DURATION`; a whole day without an end lasts one day. Exclusive. |
| `time_zone` | IANA name | the `TZID` of `DTSTART`, when it is one |
| `location` | text | `LOCATION` |
| `description` | text | `DESCRIPTION` |
| `status` | `confirmed`, `tentative`, `cancelled` | `STATUS` |
| `organizer` | `{name, email}` | `ORGANIZER` |
| `attendees` | list of `{name, email, response}` | `ATTENDEE`; `response` from `PARTSTAT`: `accepted`, `declined`, `tentative`, `needs-action` |
| `rule` | text | for a series: `RRULE`, as written |

## Times

- A time with a `TZID` stays in that time zone: `2026-11-14T20:00:00+01:00`.
- A UTC time (`…Z`) and a time without time zone are shown in the vault's time zone.
- A `TZID` is read as an IANA name (`Europe/Paris`), as the end of a longer name (`/mozilla.org/…/Europe/Paris`), as a common Windows name (`Romance Standard Time`), then from the file's own `VTIMEZONE`. A time zone the file never defines is read as the vault's.
- A repeating event keeps its local time across daylight saving changes; one written in UTC keeps its UTC time.

## Repeating events

Supported: `FREQ` daily, weekly, monthly and yearly; `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` (with positions such as `2TU` or `-1FR`), `BYMONTHDAY`, `BYMONTH`, `BYSETPOS`, `WKST`; extra dates in `RDATE`.

Rules that repeat within a day (`FREQ=HOURLY`, `BYHOUR`…) or by week number or day of the year are rare in calendars: only their first date is imported, and a final `progress` message says how many.

## Skipped

Events with no start, or a start that cannot be read, are skipped and counted in a final `progress` message. Alarms and to-dos are not read.

## Not yet

Syncing with CalDAV or the Google Calendar API, which send only what changed and can read private calendars without a secret link: later milestones.
