# Calendar connector

Imports your events from a calendar file (`.ics`).

| | |
| --- | --- |
| Id | `calendar` |
| Rung (decision 0008) | 4, file export: every calendar app exports iCalendar files; no account or network access is needed. |
| Modes | `import` |
| Access | the files the person picks; no network; no secret |
| Category | `agenda` |
| Kind | `calendar.event` |

## Getting the file

- **Google Calendar**: calendar.google.com → Settings → Import & export → Export. You get a `.zip` with one `.ics` file per calendar.
- **Apple Calendar**: on a Mac, select a calendar → File → Export → Export.
- **Outlook**: save a calendar as an iCalendar file.

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

Syncing with CalDAV or Google Calendar: later milestones.
