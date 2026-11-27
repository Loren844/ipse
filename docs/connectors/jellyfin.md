# Jellyfin connector

Reads, from your own Jellyfin, the films and episodes you watched, so that your memory knows the series and films of the moment ("watching Les Archives") and when you watched them.

| | |
| --- | --- |
| Id | `jellyfin` |
| Rung (decision 0008) | 1: Jellyfin's REST API, the interface it offers other apps, with an API key you create. |
| Modes | `sync` |
| Access | the network, only to the Jellyfin you give (`network = ["*"]`); one secret, `key` (an API key), kept in the system keychain |
| Config | `url` (its address), `user` (the id of the user followed) |
| Sync | every hour by default: what you watch is "current" for 30 days (`docs/spec/vault.md`, section 5.2) |
| Category | `media` |
| Kinds | `media.watched` |

Read only: Ipse never writes to Jellyfin and never plays anything.

## Connecting

In **Sources → Connect Jellyfin**, give its address and an **API key**, then choose whom to follow when it has several users; its only user is followed at once. On the command line: `ipse jellyfin add nas.local:8096 --vault ~/Ipse`, which asks the key and follows the only user, or the one given with `--user <id>`. `ipse jellyfin list` shows it, `ipse jellyfin remove jellyfin-2` stops it (what it brought stays).

- **The address** is the one you open in your browser: often `<your server>:8096` at home, or your `https://` address from outside.
- **The API key**: in Jellyfin, as an administrator, **Dashboard → API Keys → +**, named "Ipse". It can be revoked there at any time.

The key is kept as the Home Assistant token is ([Home Assistant](homeassistant.md#connecting)): in the system keychain, never in the memory, its journal, its history or a message. `https://` by default; plain `http://` only for an address of the home network, said on screen and on the command line. No redirect is followed. The key goes in the `Authorization: MediaBrowser … Token="…"` header.

## Syncing

1. `GET /Users` lists the users, with their names, for the person to choose from. The first session of a new source, before anyone is chosen, only does this.
2. `GET /Users/<id>/Items?Recursive=true&IncludeItemTypes=Episode,Movie&Filters=IsPlayed&SortBy=DatePlayed&SortOrder=Descending`, 200 items at a time, then the same with `Filters=IsResumable` for what is under way: the items played after the last play read, or over the last year the first time, 5 000 at most per session.

Each play is one `upsert`:

**`media.watched`**. `external_id`: `play:<item id>:<last played, Unix seconds>`, so that watching again is a new play.

| Field | Type | From |
| --- | --- | --- |
| `item` | text | Jellyfin's item id |
| `type` | `movie` or `episode` | `Type` |
| `title` | text | `Name`: the film's title, or the episode's |
| `series`, `season`, `episode` | text, numbers | `SeriesName`, `ParentIndexNumber`, `IndexNumber`, for an episode |
| `year` | number | `ProductionYear`, when given |
| `played_at` | RFC 3339 timestamp | `UserData.LastPlayedDate` |
| `finished` | boolean | `UserData.Played`: watched to the end |

The cursor holds the users found, with their names, and the time of the last play read. Nothing else.

## In the vault

`states/watching/<year>.md`, in the `media` category, holds one row per series or film: the days of the first and the last play of the year, and how many plays. A series is one row, whatever the season. The subject links to a file of `works/` with that name or alias (`[[les-archives]]`), and is plain text otherwise. The file is rewritten from every play at each sync (`docs/spec/vault.md`, section 5). Through `now`, an AI that reads `media` sees the series and films watched in the last 30 days.

## Failures

As for [Home Assistant](homeassistant.md#failures), in the person's language, never quoting the address or the key: `unavailable` when Jellyfin cannot be reached, `rate_limited` on 429, `auth` on 401 or 403 (create a new key), `bad_input` for an answer that is not Jellyfin's or an `http://` address outside the home network. 15 seconds to connect, 2 minutes per request.
