# Home Assistant connector

Reads, from your own Home Assistant, when you are at home and in which zones, so that your memory knows "right now" whether you are home, at work or at the gym, and when you were. Your Home Assistant already knows it from the companion app on your phone, or from your router; Ipse only asks it.

| | |
| --- | --- |
| Id | `homeassistant` |
| Rung (decision 0008) | 1: Home Assistant's REST API, the interface it offers other apps, with an access token you create. |
| Modes | `sync` |
| Access | the network, only to the Home Assistant you give (`network = ["*"]`); one secret, `token` (a long-lived access token), kept in the system keychain |
| Config | `url` (its address), `person` (the person followed, like `person.ines`), `positions` (`true` when the person asked for their phone's positions too) |
| Sync | every 5 minutes by default: being at home is "current" for 15 minutes (`docs/spec/vault.md`, section 5.2) |
| Category | `home`, and `location` for the positions; both **closed to AIs by default** (`docs/spec/rights.md`) |
| Kinds | `home.presence`; `location.visit` when the positions are read |

Read only: Ipse never writes to Home Assistant and never controls a device.

## Connecting

In **Sources → Connect Home Assistant**, give its address and a **long-lived access token**, then choose whom to follow when it has several people. On the command line: `ipse homeassistant add homeassistant.local:8123 --vault ~/Ipse`, which asks the token and follows the only person, or the one given with `--person person.ines`. `ipse homeassistant list` shows it, `ipse homeassistant remove homeassistant-2` stops it (what it brought stays).

- **The address** is the one you open in your browser: often `homeassistant.local:8123` at home, or your `https://` address from outside.
- **The token**: in Home Assistant, your profile (bottom left) → **Security** → **Long-lived access tokens** → **Create token**. Home Assistant shows it once. It can be revoked there at any time.

The address stays in `.ipse/connectors.toml`; the token goes to the system keychain and is never written to the memory, its journal, its history or a message.

**`https://` by default.** Plain `http://` is accepted only for an address of the home network: this computer, a private address (`192.168.x.x`, `10.x.x.x`, `172.16–31.x.x`, IPv6 `fd…` and `fe80…`), or a local name (one without a dot, or ending in `.local`, `.lan`, `.home`, `.home.arpa`, `.internal`). The token then goes unencrypted over that network, and both the Sources screen and the command line say so. Such an address is reached directly, never through a proxy. No redirect is followed.

## Syncing

1. `GET /api/states` lists the `person.*` entities, with their names, for the person to choose from. The first session of a new source, before anyone is chosen, only does this: it sends no record and keeps what it found in its cursor.
2. `GET /api/history/period/<from>?filter_entity_id=<person>&end_time=<now>&minimal_response&no_attributes` gives each change of the chosen person's state since `<from>`: the start of the stay going on at the last session, or 10 days back the first time (what Home Assistant keeps by default).

The person's state is `home`, `not_home`, or the name of a zone of Home Assistant (`Travail`, `Salle de sport`). Each stay is one `upsert`:

**`home.presence`**. `external_id`: `presence:<entity>:<start, Unix seconds>`.

| Field | Type | From |
| --- | --- | --- |
| `entity` | text | the person followed, like `person.ines` |
| `state` | text | `home`, `not_home`, `unknown`, `unavailable` or a zone's name, as Home Assistant says |
| `start` | RFC 3339 timestamp | when the state began (`last_changed`) |
| `end` | RFC 3339 timestamp | when the next state began; absent for the stay going on |
| `seen` | RFC 3339 timestamp | for the stay going on: the time of the session |

The stay going on is sent again at each session with a new `seen`, so that the memory knows it still holds; a stay that ended does not change, and the journal skips it when it is sent again. The first state of an answer is the one at `<from>`: when it goes on the stay of the last session, that stay keeps its own start.

The cursor holds the people found, with their names, the state and start of the stay going on, and, when positions are read, where the positions of the place the person is at start. Nothing else.

### The phone's positions, when the person asks

Off until the person turns it on: **Lire aussi où va mon téléphone** (**Also read where my phone goes**) on the source in **Sources**, or `ipse homeassistant positions <id>` (`--off` to stop). It needs the Home Assistant app on the phone, with its location sent to Home Assistant: the `person.*` entity then carries the phone's GPS position. It replaces the export of the Google Timeline (issue #208), and the positions go nowhere but the person's own Home Assistant and Ipse.

3. `GET /api/history/period/<from>?filter_entity_id=<person>&end_time=<now>&significant_changes_only=0` gives every update of the person, with its attributes: `latitude`, `longitude`, `gps_accuracy`, at `last_updated`, and the zone the person was in as the state. `<from>` is where the positions of the place the person is at start, or 10 days back the first time.

The positions become stays (`connectors/stays.rs`): positions within 100 meters of their middle, for at least 10 minutes, are one stay. A stay ends at the first position away from it, when that one comes within 12 hours, else at its last position. A lone position counts only in a zone of Home Assistant, as a phone on the way can be silent for a while. Positions less precise than 250 meters are left out. The stay going on is not sent: it is read again at the next session, from its first position.

Each stay that ended is one `upsert` of **`location.visit`**, with the fields of the location history connector (`docs/connectors/location.md`): `start`, `end`, `lat`, `lon`, `semantic` = `home` for a stay in the zone `home`, `name` = the zone's name for a stay in another zone. `external_id`: `visit:<start, Unix seconds>:<lat>,<lon>`. Positions themselves are not kept.

The stays then become places, visit events and the `location` state log, as the Google Timeline's do, in the `location` category: a place found within 75 meters is the same place, a new one is named after its zone, or "Lieu près de …" outside zones; a stay at home only counts in the state log.

## In the vault

`states/home/<year>.md`, in the `home` category, holds one row per place: **Domicile** (or **Home**, in the vault's language) for `home`, and each zone by its name. First seen is when the first stay of the year began, last seen when the last one ended or was last seen, with the time; the count is how many stays. Being away (`not_home`), `unknown` and `unavailable` are not rows. The file is rewritten from every stay at each sync (`docs/spec/vault.md`, section 5).

An AI the person opened the `home` category to sees, through `now`, whether they are at home or in a zone right now, with when they were last seen there.

## Failures

In the person's language, never quoting the address or the token:

| What | Code |
| --- | --- |
| Home Assistant cannot be reached, or answers 5xx | `unavailable`, tried again with the backoff |
| 429 | `rate_limited`, with `Retry-After` in seconds when given |
| 401 or 403 | `auth`: create a new token |
| An answer that is not Home Assistant's, a redirect, an `http://` address outside the home network | `bad_input` |

15 seconds to connect, 2 minutes per request.
