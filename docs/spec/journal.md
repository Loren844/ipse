# Raw journal

- **Status:** Accepted
- **Version:** `journal-format: 1`
- **Applies to:** milestone 1 onward.

The raw journal keeps **everything connectors bring back, exactly as they brought it**, timestamped and append-only. It is the ground truth Ipse rebuilds from: states, summaries and facts can be recomputed from it at any time, for example when extraction improves.

The vault holds what a person reads; the journal holds what machines read. It is never shown as such in the interface and never sent to an AI (see `rights.md`).

## Choices settled by the project owner

Settled on 2026-11-12.

| # | Question | Choice | Why |
| --- | --- | --- | --- |
| Q1 | How long raw data is kept | Forever by default; the person can shorten it per source. | Old raw data is what lets Ipse rebuild a better memory later. A year of GPS points at one per minute is about 20 MB raw and 3 to 5 MB compressed. |
| Q2 | What "forget this source" does | Raw records are deleted at once. What the vault learned from them is listed in the review inbox ("312 events, 4 facts and 2 people come from this source"), and the person removes all of it in one action or keeps what they choose. | Nothing the person can read, and may have edited, disappears without being shown first (decision 0005). |

## 1. Layout

```text
<vault>/journal/
├── <source>/
│   ├── <YYYY>/
│   │   ├── <YYYY-MM>.jsonl          current month: open, appended to
│   │   ├── <YYYY-MM>.jsonl.zst      past months: sealed and compressed
│   │   └── <YYYY-MM>.jsonl.zst.sha256
│   └── cursor.json                  the connector's resume point (section 6)
└── journal.toml                     journal format version
```

- `<source>` is the connector's identifier: `calendar`, `contacts`, `location`, `home-assistant`… It matches `[a-z0-9-]+`. Two installations of the same connector (two calendars) use `calendar`, `calendar-2`…
- A record goes in the segment of the month of its `observed_at`, in UTC.
- The journal is excluded from the vault's history (vault spec, section 9): it is already append-only, and its volume would bloat history.

## 2. Format

Segments are [JSON Lines](https://jsonlines.org): one JSON object per line, UTF-8, `\n`-terminated. Sealed segments are compressed with [Zstandard](https://facebook.github.io/zstd/). Both are open formats readable with standard tools (`zstdcat`, `jq`).

## 3. Records

Every line is one record:

```json
{"v":1,"id":"01jcxb7k2m4p6r8t0v2w4y6z8a","source":"calendar","kind":"calendar.event","op":"upsert","external_id":"5f2a9c1e@google.com","observed_at":"2026-11-12T18:30:00Z","ingested_at":"2026-11-12T18:31:04Z","hash":"b3:9f2c…","data":{"title":"Dîner chez Julie","start":"2026-11-14T20:00:00+01:00"}}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `v` | integer | yes | Record format version, `1`. |
| `id` | string | yes | ULID, lowercase, assigned by Ipse on write. Sorts by write time. |
| `source` | string | yes | Same as the folder name. |
| `kind` | string | yes | What the data is, `<domain>.<type>`: `calendar.event`, `contact.card`, `location.points`, `media.playing`, `home.state`… Each connector documents its kinds and their `data` shape. |
| `op` | string | yes | `upsert` (the item exists with this content), `delete` (the item no longer exists at the source), or `observe` (a one-off observation with no identity, such as a song being played). |
| `external_id` | string | for `upsert` and `delete` | The item's identifier at the source. Together with `source` and `kind`, it identifies an item across records. |
| `observed_at` | datetime | yes | When the fact was true at the source, RFC 3339 in UTC. For a batch of points, the first point's time. |
| `ingested_at` | datetime | yes | When Ipse wrote the record, RFC 3339 in UTC. |
| `hash` | string | for `upsert` | `b3:` + BLAKE3 of the canonical JSON of `data` (keys sorted, no spaces), in hex. Used to skip unchanged items (section 5). |
| `data` | object | yes, except for `delete` | The connector's payload, as the connector sent it. Ipse never edits it. |

The current state of an item is its **latest record** by `id`: its last `upsert`, unless a later `delete` follows.

## 4. Time series

High-volume measurements (GPS points, heart rate, home sensor readings) are stored as **batches**, one record per batch, with columns instead of one object per point:

```json
{"v":1,"id":"01jcxc…","source":"location","kind":"location.points","op":"observe","observed_at":"2026-11-12T07:00:00Z","ingested_at":"2026-11-12T08:00:02Z","data":{"t":[1794466800,1794466860,1794466920],"lat":[45.7578,45.7581,45.7585],"lon":[4.8320,4.8324,4.8331],"acc":[12,9,10]}}
```

- `t` is Unix time in seconds, UTC; other columns have the same length as `t`.
- A batch covers at most one hour and 10 000 points.
- Columns of numbers compress far better than repeated objects, and a reader can skip a whole batch by its `observed_at` without parsing the columns.

## 5. Writing

- **Only Ipse writes to the journal.** Connectors send their data through the connector protocol (`connectors.md`); they never touch these files.
- **Append-only.** A written record is never changed. The only exceptions are recovery (below) and forgetting (section 8).
- **Unchanged items are skipped.** Before appending an `upsert`, Ipse compares its `hash` with the latest record of the same item; if they are equal, nothing is written. This makes a full re-sync from a connector cheap.
- **Durability.** Records arrive in batches. A batch is appended, then flushed to disk (`fsync`) before Ipse acknowledges it to the connector. A batch acknowledged is never lost.
- **Recovery.** If the last line of an open segment is incomplete after a crash, Ipse truncates that line before appending to it again, and notes that it did, without the content.
- **Sealing.** On the first write of a session to a source, Ipse compresses (zstd) every open segment of a past month, writes its SHA-256 checksum next to it (the `sha256sum` format), then deletes the uncompressed file. The checksum is written before the compressed file, so a crash between the two leaves a segment that fails its check and an open file that is still there: nothing is lost.
- **Late records.** A record observed in a month already sealed goes to a new open segment of that month. At the next sealing, Ipse merges it with the sealed segment, if that one matches its checksum.
- **Damaged segments.** A sealed segment whose checksum does not match is reported (by readers and by `ipse check`) and kept, never overwritten nor merged into: it is to be restored from a backup.

## 6. Resume points

`cursor.json` holds what a connector needs to resume where it stopped: a sync token, a last-modified date, a file offset. Its content is opaque to Ipse; the connector reads it on start and returns an updated one with each batch (`connectors.md`). Ipse writes the cursor only after the batch is flushed, so a crash never skips data.

## 7. Reading

- Readers go **segment by segment, line by line**, without loading a segment in memory.
- To serve "the current state of every item" quickly, Ipse keeps a derived table per source in `.ipse/index/journal/<source>.bin` (item → latest record id and hash, with the id of the last record it has seen). It can be deleted; when it is missing, damaged or behind the journal, Ipse rebuilds it by reading the journal once.
- A period can be read alone: only the segments of its months are opened.
- Records with an unknown `v` or `kind` are skipped and counted, never an error: an older Ipse must be able to read a journal written by a newer connector.

## 8. Forgetting

The person can forget a whole source, or a period of a source:

- Ipse rewrites the affected segments without those records, writing each new segment beside the old one, then swapping them, so that a crash leaves either the old or the new segment, never a mix.
- What the vault learned from the forgotten records is listed in the review inbox (Q2); nothing in the vault is removed without the person's action.
- Forgetting is the **only** operation that removes raw data, and it always comes from an explicit action of the person.

## 9. Settings: `journal.toml`

```toml
format = 1        # journal format version; Ipse refuses a higher version it does not know

[retention]       # optional, per source; absent means forever (Q1)
location = "730d"
```

When a retention is set, records older than it are forgotten (section 8) once a day.
