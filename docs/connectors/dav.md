# CalDAV and CardDAV connector

Keeps your agenda and your contacts in sync with an account on a server that speaks CalDAV and CardDAV: Nextcloud, iCloud, Fastmail, Radicale, Baïkal, mailbox.org, Synology and most self-hosted servers. Changes on the server (an event moved, a contact edited) reach your memory at the next sync, without exporting a file again.

| | |
| --- | --- |
| Id | `dav` |
| Rung (decision 0008) | 1: CalDAV (RFC 4791) and CardDAV (RFC 6352) are the open standards these servers offer other apps. |
| Modes | `sync` |
| Access | the network, only to the server the person gives, over HTTPS (`network = ["*"]`); one secret, `password` (an app password), kept in the system keychain |
| Config | `url` (the server's address), `user` (the user name), `collections` (the calendars and address books chosen) |
| Sync | every 15 minutes by default |
| Category | `agenda` for events, `people` for contacts |
| Kinds | `calendar.event`, `contact.card`: the same records as the [calendar](calendar.md) and [contacts](contacts.md) connectors, so that the vault code is shared |

Read only: Ipse never writes to the server.

## Connecting an account

In **Sources → Connect an account**, give the server's address, your user name and an **app password**, then choose which calendars and address books to follow. On the command line: `ipse dav add <address> --user <name> --vault ~/Ipse`, which asks the password and follows every calendar and address book found.

- **iCloud**: the address is `caldav.icloud.com` (contacts are found at `contacts.icloud.com`). Your Apple Account password does not work here: create an app password at account.apple.com → Sign-In and Security → App-Specific Passwords. The user name is your Apple Account email.
- **Fastmail**: the address is `fastmail.com`. Create an app password in Settings → Privacy & Security → Manage app passwords, with access to "Calendars and contacts (CalDAV/CardDAV)". The user name is your Fastmail address.
- **Nextcloud**: the address of your Nextcloud (`cloud.example.org`). Create an app password in Personal settings → Security → Devices & sessions.
- **Radicale, Baïkal and others**: the server's address and your account there.

The user name stays in `.ipse/connectors.toml`, with the address; the password goes to the system keychain and is never written to the memory, its journal, its history or a message. Only `https://` addresses are accepted, except to this computer (a server on your own machine, tests).

## Discovery

1. The connector asks `<address>/.well-known/caldav` and `<address>/.well-known/carddav` (RFC 6764) for the user's principal (`current-user-principal`), following up to 5 redirects. When there is none, it asks the address itself. iCloud's and Fastmail's own addresses for contacts are known.
2. From the principal, the homes: `calendar-home-set` and `addressbook-home-set`.
3. In each home, at depth 1, the collections: a **calendar** (`resourcetype` `calendar`, holding events: `supported-calendar-component-set` includes `VEVENT` or is absent) or an **address book** (`resourcetype` `addressbook`), with its `displayname`, `getctag` and `sync-token`.

The credentials are sent over HTTPS only, and a redirect is followed with them only to the same host or another host of the same domain (`caldav.icloud.com` to `p42-caldav.icloud.com`). Any other redirect is refused (`bad_input`).

The first session of an account, before any collection is chosen, only discovers: it sends no record, keeps what it found in its cursor and ends with a `need` of `config`. The Sources screen then lists the calendars and address books found, to choose from. A collection that appears later is listed too, but not followed until chosen.

## Syncing

Each session lists the homes again (one request per home), which gives every collection's `sync-token` and `getctag`:

- **Nothing changed** (every chosen collection has the token or ctag of the cursor, and the day did not change): nothing is sent, and `complete` is left out, as for a calendar link that answers 304.
- **A collection changed**, with a `sync-token`: a `sync-collection` report (RFC 6578) from the cursor's token gives what changed and what was removed.
- **A collection changed**, without a `sync-token`: the `getetag` of every item, at depth 1, compared with the cursor's.
- What changed is read with `calendar-multiget` or `addressbook-multiget`, 100 items per request, and sent as `upsert` records.
- **Something was removed**, a token was refused (`valid-sync-token`), or **the first sync of the day**: every chosen collection is read whole and the session ends with `done.complete`, so that the core removes what the server no longer holds (`docs/spec/connectors.md`, section 5.8). A removed event that had not happened is marked cancelled; a past one stays as it happened, and a contact removed from the server leaves its person file alone. Reading whole once a day also expands repeating events for the days that came since, as the calendar connector does.
- A collection no longer chosen is not read, and its items go at the next whole read.

The cursor holds the day of the last whole read and, per chosen collection, its `sync-token`, its `getctag` and, without a token, each item's `getetag`, plus the collections found, with their names. Nothing personal is in it but the names the person gave their calendars and address books.

## Records

Each item of a calendar is an iCalendar object, read as the [calendar connector](calendar.md#records) reads a file: one `upsert` per event, `external_id` its `UID`, repeating events with their occurrences from one year back to 90 days ahead. Each item of an address book is a vCard, read as the [contacts connector](contacts.md) reads a file: one `contact.card` record, `external_id` its `UID`.

An event or a contact already imported from a file is **found again, not doubled** (decision 0011): an event by its `UID` (and recurrence id), as for two calendars that share an invitation; a contact by its `UID`, when the file and the server gave the same card. The person's file then lists both sources. Two cards of the same person with different `UID`s are not merged: they are proposed as possible duplicates, as always.

## Failures

In the person's language, never quoting the address, the user name or the password:

| What | Code |
| --- | --- |
| The server cannot be reached, or answers 5xx | `unavailable`, tried again with the backoff |
| 429 | `rate_limited`, with `Retry-After` in seconds when given |
| 401 or 403 | `auth`, then a `need` of `reauth`: check the user name and the app password |
| No calendar and no address book found | `bad_input`: check the address |
| A redirect elsewhere, an answer that is not WebDAV, an item over 16 MiB | `bad_input` |

15 seconds to connect, 2 minutes per request.
