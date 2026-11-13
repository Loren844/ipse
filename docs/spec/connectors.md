# Connector protocol

- **Status:** Accepted
- **Version:** `protocol: 1`
- **Applies to:** milestone 1 onward.

A connector brings data from one source into Ipse. It is a **separate program** (decision 0002) that the core starts, and that talks to the core through **JSON messages on its standard input and output**. The core alone writes to the raw journal (`journal.md`); a connector only sends records.

## Choices settled by the project owner

Settled on 2026-11-12.

| # | Question | Choice | Why |
| --- | --- | --- | --- |
| Q1 | How first-party connectors ship | Inside the main `ipse` program, each started as its own process with `ipse connector <name>`. Community connectors are separate programs. | One thing to install (decision 0007), while each connector still runs in its own process (decision 0002). |
| Q2 | Community connectors | Allowed from 1.0, installed by the person after a clear warning listing what the connector can access. An official list of reviewed connectors comes later. | Blocking them would slow the community the project relies on. |

## 1. Vocabulary

- **Core**: the Ipse process that starts connectors, writes the journal and the vault.
- **Connector**: a program implementing this protocol for one source.
- **Instance**: one configured use of a connector: "my Google calendar" and "my work calendar" are two instances of `calendar`. Each instance has its own folder in the journal (`calendar`, `calendar-2`). The bare id is for files the person imports; instances that sync are numbered from 2. They are listed in `.ipse/connectors.toml` (id, connector, name, `every`), which is part of the vault's history. Their secrets are never in the vault: they stay in the system keychain, under a key unique to the vault.

## 2. Manifest

Every connector has a manifest, `connector.toml`, read by the core before it starts the program. For first-party connectors it is built into Ipse.

```toml
protocol = 1
id = "calendar"                          # [a-z0-9-]+, unique
name = "Agenda"                          # shown to the person
summary = "Vos événements passés et à venir, avec lieux et participants."  # one sentence, shown to the person
version = "0.1.0"
command = ["ipse", "connector", "calendar"]   # how to start it
modes = ["import", "sync"]               # section 3
rung = 1                                 # decision 0008: 1 official API, 2 device, 3 relay, 4 file export
kinds = ["calendar.event"]               # record kinds it may send (journal.md, section 3)
category = "agenda"                      # default rights category of what it produces (rights.md)

[access]                                 # what it needs; shown to the person before installing
network = ["www.googleapis.com"]         # hosts it may contact; empty for none
files = "picked"                         # "none" | "picked" (only files the person picks) 
secrets = ["oauth"]                      # secrets the core keeps in the keychain for it

[sync]
default_every = "15m"                    # for mode "sync"
```

`summary` and `name` exist in the languages the connector supports (`summary.fr`, `summary.en`); the core picks the interface's language, then English.

## 3. Modes

| Mode | When | Lifetime | Example |
| --- | --- | --- | --- |
| `import` | The person gives a file | Runs once, then exits | A `.vcf` contacts export, a Netflix history CSV |
| `sync` | On a schedule, or on demand | Runs, sends what changed since its cursor, exits | Google Calendar, CardDAV |
| `watch` | While Ipse runs | Stays up and streams | Now-playing on the computer, Home Assistant |

A connector declares the modes it supports; the core never asks for another one.

## 4. Transport

- The core starts the connector with the manifest's `command`, in a working directory of its own, with **no inherited environment** except `PATH`, `HOME`, `LANG` and the variables the connector documents.
- Messages are **JSON Lines**: one JSON object per line, UTF-8, on `stdin` (core → connector) and `stdout` (connector → core). Nothing else may be written to `stdout`.
- `stderr` is for logs. The core keeps the last 1 000 lines per instance for diagnosis. Connectors **must not** log personal data.
- One message is at most **8 MiB**. Larger data is split across batches.
- Every message has a `type`. Unknown fields are ignored; an unknown `type` is answered with an `error` of code `unsupported` and does not stop the session.

## 5. Session

```text
core                                   connector
 │ ── hello ────────────────────────────▶ │
 │ ◀──────────────────────────── ready ── │
 │ ── start ────────────────────────────▶ │
 │ ◀──────────────────────────── batch ── │
 │ ── ack ──────────────────────────────▶ │
 │ ◀──────────────────────────── batch ── │   (up to 4 batches in flight)
 │ ── ack ──────────────────────────────▶ │
 │ ◀───────────────────────────── done ── │   (import, sync)
 │ ── bye ──────────────────────────────▶ │
```

### 5.1 `hello` (core → connector)

```json
{"type":"hello","protocol":1,"instance":"calendar-2","mode":"sync","language":"fr","time_zone":"Europe/Paris","cursor":{"sync_token":"CPDAlvWDx4wCEPDAlvWDx4wCGAU="},"config":{"calendar_id":"primary"},"secrets":{"oauth":"<token>"},"files":[]}
```

- `language` is the language the person reads messages in: the interface's language (`en` for the command line), not necessarily the vault's.
- `cursor` is the value the connector returned last time (`journal.md`, section 6), or `null` on the first run.
- `config` holds the instance's settings, as the connector declared them.
- `secrets` are read by the core from the system keychain for this session only. A connector **must not** write them anywhere.
- `files` lists absolute paths the person picked, for `import` mode.

### 5.2 `ready` (connector → core)

```json
{"type":"ready","protocol":1,"connector":"calendar","version":"0.1.0"}
```

Must arrive within **10 seconds** of `hello`. If the connector does not support the protocol version, it answers with an `error` of code `unsupported_protocol` and exits.

### 5.3 `start` (core → connector)

```json
{"type":"start","full":false}
```

`full: true` asks for everything, ignoring the cursor (after the person asks for a full re-sync). Thanks to hash comparison in the journal, a full sync only writes what changed.

### 5.4 `batch` (connector → core)

```json
{"type":"batch","seq":1,"records":[{"kind":"calendar.event","op":"upsert","external_id":"5f2a9c1e@google.com","observed_at":"2026-11-12T18:30:00Z","data":{"title":"Dîner chez Julie"}}],"cursor":{"sync_token":"CPDAlvWDx4wCEPDAlvWDx4wCGAY="}}
```

- `seq` starts at 1 and grows by 1 per batch in the session.
- Each record has the journal fields a connector knows: `kind`, `op`, `external_id`, `observed_at`, `data`. The core adds `v`, `id`, `source`, `ingested_at` and `hash`.
- A batch holds at most **1 000 records**, and a connector sends at least one batch per minute while it has data, so progress is never hidden.
- `cursor` is optional; when present, it is where the connector would resume if everything up to this batch is kept.
- A connector may have **at most 4 batches** waiting for `ack`; it waits before sending a fifth.

### 5.5 `ack` (core → connector)

```json
{"type":"ack","seq":1}
```

Sent once the batch is flushed to disk and its cursor saved. A batch without `ack` may be lost; a connector that restarts resumes from the last acknowledged cursor.

The core may answer `{"type":"ack","seq":1,"rejected":[3]}`: the records at those positions were malformed (wrong kind, missing field) and were not written. The rest of the batch is kept.

### 5.6 `progress` (connector → core, optional)

```json
{"type":"progress","done":350,"total":1200,"message":"Lecture de 2025"}
```

Lets the interface show progress in plain words. `total` may be absent when unknown.

### 5.7 `need` (connector → core)

When the connector cannot go on without the person:

```json
{"type":"need","what":"reauth","message":"Google demande de vous reconnecter à votre agenda."}
```

For `file`, the message also carries `url`, the direct link to the export page:

```json
{"type":"need","what":"file","message":"Téléchargez votre historique Netflix, Ipse l'importera tout seul.","url":"https://www.netflix.com/viewingactivity"}
```

| `what` | Meaning | The core… |
| --- | --- | --- |
| `reauth` | Access expired or was revoked | asks the person to reconnect, then starts a new session |
| `file` | A file export is needed (rung 4) | shows `message` and `url` (direct link to the export page), then watches for the downloaded file |
| `config` | A setting is missing or wrong | opens the instance's settings |

After `need`, the connector sends `done` or waits for `bye`.

### 5.8 `done` (connector → core)

```json
{"type":"done","summary":{"upserts":12,"deletes":1,"observes":0,"complete":true}}
```

`complete: true` says the session sent **every** item the source holds now, as for a calendar subscription, which can only give the whole calendar. Then, in mode `sync`, the core records the removal of every item of the instance that is still in the journal but was not sent this time: the connector keeps no list of what it sent before, the journal is that list. A connector leaves `complete` out when it sent only part of the source, when it sent only changes (with a cursor), or when the source said nothing changed (HTTP 304): sending nothing then must not empty the vault. In mode `import`, `complete` is ignored: importing a file never removes anything.

The connector exits after `bye`. `watch` connectors never send `done` unless they stop for good.

### 5.9 `error` (connector → core)

```json
{"type":"error","code":"rate_limited","retry_after":"10m","message":"Google limite les requêtes, nouvelle tentative dans 10 minutes."}
```

| Code | Retry | Meaning |
| --- | --- | --- |
| `rate_limited` | after `retry_after` | The source asks to slow down. |
| `unavailable` | with backoff: 1, 5, 15, 60 minutes | The source cannot be reached. |
| `auth` | no | Credentials are refused: followed by a `need` of `reauth`. |
| `bad_input` | no | The picked file or the source's data cannot be read. |
| `unsupported`, `unsupported_protocol` | no | |
| `internal` | once | Bug in the connector. |

`message` is written for the person, in the `language` of `hello`, and says what happens next. It **must not** contain personal data.

### 5.10 `bye` (core → connector)

```json
{"type":"bye"}
```

The connector exits within **5 seconds**. After that, the core stops it.

## 6. Watch mode

- A `watch` connector sends batches as observations come, at least every **60 seconds** while something changes, and nothing while nothing changes.
- It sends `{"type":"alive"}` every **5 minutes** when it has nothing to send. Without a message for 10 minutes, the core restarts it.
- When Ipse quits, the core sends `bye`.

## 7. Supervision

- An instance in mode `sync` runs when its `every` has passed since its last run, and after a failure when the backoff of `unavailable` has passed. When each instance last ran, and how many times in a row it failed, is in `.ipse/sync.toml`, left out of the vault's history.
- A connector that crashes is restarted with the backoff of `unavailable`. After 5 crashes in a row, the instance is paused and the person is told in plain words.
- A connector that sends a message that is not valid JSON, or breaks the rules above, is stopped; nothing from the unacknowledged batches is kept.
- A connector never runs twice for the same instance at the same time.
- Resource limits (memory, CPU time) may be added later without changing this protocol.

## 8. Writing a connector

- Any language can implement this protocol; the repository will provide a Rust helper crate for first-party connectors and a conformance test that any connector can run.
- A connector documents, in its own README: its rung and why a higher one is not possible (decision 0008), its kinds with the shape of their `data`, its settings, and what it sends where.
