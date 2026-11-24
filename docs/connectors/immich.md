# Immich connector

Reads, from your own Immich, the people its face recognition found on your photos and you named, with how many photos show them and when the first and the last were taken. Your memory then knows whom you see, and when you last took a photo together.

| | |
| --- | --- |
| Id | `immich` |
| Rung (decision 0008) | 1: Immich's REST API, the interface it offers other apps, with an API key you create. |
| Modes | `sync` |
| Access | the network, only to the Immich you give (`network = ["*"]`); one secret, `key` (an API key), kept in the system keychain |
| Config | `url` (its address), `user` (the user the key belongs to, found on connecting) |
| Sync | every 6 hours by default |
| Category | `people` |
| Kinds | `photos.person` |

Read only: Ipse never writes to Immich, never downloads a photo, and reads no position.

## Connecting

In **Sources → Connect Immich**, give its address and an **API key**. The key belongs to one user of Immich, who is followed at once. On the command line: `ipse immich add nas.local:2283 --vault ~/Ipse`, which asks the key. `ipse immich list` shows it, `ipse immich remove immich-2` stops it (what it brought stays).

- **The address** is the one you open in your browser: often `<your server>:2283` at home, or your `https://` address from outside.
- **The API key**: in Immich, your picture (top right) → **Account Settings** → **API Keys** → **New API Key**, named "Ipse". It can be revoked there at any time.

The key is kept as the Home Assistant token is ([Home Assistant](homeassistant.md#connecting)): in the system keychain, never in the memory, its journal, its history or a message. `https://` by default; plain `http://` only for an address of the home network, said on screen and on the command line. No redirect is followed. The key goes in the `x-api-key` header.

## Syncing

1. `GET /api/users/me`: the user the key belongs to.
2. `GET /api/people?withHidden=false`, 500 at a time: the people of the photos, 5 000 at most. Only those you named are read; hidden ones are left out.
3. For each: `GET /api/people/<id>/statistics` (how many photos), and `POST /api/search/metadata` with `personIds`, `size: 1` and `order` `asc` then `desc` (the first and the last photo). These searches only read.

Each named person is one `upsert`, sent again at each sync; the journal skips those that did not change.

**`photos.person`**. `external_id`: `person:<Immich's person id>`.

| Field | Type | From |
| --- | --- | --- |
| `person` | text | Immich's person id |
| `name` | text | the name you gave them in Immich |
| `photos` | number | how many photos show them |
| `first`, `last` | RFC 3339 timestamps | when the first and the last photo were taken (`fileCreatedAt`) |

The cursor holds the user the key belongs to. Nothing else.

## In the vault

`states/photos/<year>.md`, in the `people` category and in the year of the last photo, holds one row per person of the photos: the days of the first and the last photo that shows them, and how many photos (`docs/spec/vault.md`, section 5). Through `now`, an AI that reads `people` sees whom you took photos with in the last 30 days.

Who they are in the memory is never guessed (decision 0011):

- **Sure**: a file of `people/` whose name or an alias is the name Immich gives, and only one. The row links to it (`[[gaspard-lemoine]]`).
- **Not sure**: Immich gives one word ("Paul") that is the first name of one person of the memory only ("Paul Martin"). The row stays plain text, and Ipse proposes, in **À revoir**, to add "Paul" to Paul Martin's other names. Accepted, the next sync links the row; refused, it is not proposed again.
- **Anyone else** stays plain text: two people with that name, nobody, or a name of several words that matches no one.

You, the owner, are on most of your own photos: you are no row.

## Failures

As for [Home Assistant](homeassistant.md#failures), in the person's language, never quoting the address or the key: `unavailable` when Immich cannot be reached, `rate_limited` on 429, `auth` on 401 or 403 (create a new key), `bad_input` for an answer that is not Immich's or an `http://` address outside the home network. 15 seconds to connect, 2 minutes per request.
