# 0011 — People and events are never merged silently

- **Status:** Accepted
- **Date:** 2026-11-13

## Context

The same person often appears several times: in a Google export and an Apple export, in a contact card and as a calendar attendee. Same email, same phone or same name are strong clues, but not proofs: a family shares a landline, two different people share a name. A wrong merge mixes two people's lives and is hard to notice.

## Decision

- **Same source, same identifier** (a re-import of the same contacts): the existing file is updated. This is certain and automatic.
- **Linking is not merging.** When an event names an attendee by an email that matches exactly one person, the event links to that person; no person file changes. When it matches none or several people, the attendee stays plain text.
- **Two person files are never merged silently.** Possible duplicates become proposals in the review inbox, each saying why ("same email", "same phone number") and how likely it is. The person answers *merge* or *these are two people*; the second answer is remembered and never asked again.
- A name alone is too weak: it is proposed only together with another clue.
- A merge keeps one file, brings in the other's sources, aliases, emails, phones and facts, redirects every link, and can be undone.

## Events

Added on 2026-11-13. The same rule applies to events, with one more certain case:

- **Same calendar identifier** (iCalendar `UID`, plus `RECURRENCE-ID` for one occurrence): the identifier is meant to be unique in the world, so an invitation received in two calendars is **one event file with two sources**. Automatic.
- **Same title and same start, different identifiers** (an event copied by hand into two calendars): a proposal. Such proposals are **grouped per pair of calendars** ("12 events look duplicated between Perso and Pro: merge them all?"), so that one action settles them, each one still listed.
- Same title at different times (a weekly class) is not a duplicate.

## Consequences

- Before the review inbox exists (milestone 2), similar people stay as separate files (`marie-dupont.md`, `marie-dupont-2.md`): nothing is lost, nothing is mixed.
- Importing the same contacts from two apps creates duplicates until then.
- Detection must index emails and phones so that it stays fast on large vaults.
