# Vault format

- **Status:** Accepted
- **Version:** `vault-format: 1`
- **Applies to:** milestone 1 onward. From 1.0, this format is a compatibility promise.

The vault is the **source of truth** of a person's memory: a folder of Markdown files they can read and edit in any editor, Obsidian included. Everything else Ipse keeps (index, proposals, caches) is derived from it or sits beside it, and can be rebuilt or thrown away.

The words **must**, **must not**, **should** and **may** are used in their usual specification sense.

## Choices settled by the project owner

Settled on 2026-11-12. These choices are hard to undo after 1.0.

| # | Question | Choice | Why |
| --- | --- | --- | --- |
| Q1 | Encryption | The operating system's disk encryption; secrets (connector tokens) in the system keychain. An optional encrypted vault may come later. | Encrypted files cannot be read in Obsidian, which would break the "open formats" principle. See decision 0009. |
| Q2 | Language of section headings in files | The vault's language, chosen at creation (`## Faits` in French). Front-matter keys stay English in every language. | People read these files; machines read the keys. |
| Q3 | Where calendar events live | One file per event. Day summaries list and link them. | Events need structured fields (time, place, participants) and stable links. |
| Q4 | How facts are written | One bullet line per fact in the body, with a short identifier. Well-known attributes (birthday, email, phone) live in the front matter. | A person adds a fact by typing a line. |

## 1. Layout

```text
<vault>/
├── people/                 one file per person
├── places/                 one file per place
├── organizations/          companies, schools, clubs…
├── projects/
├── things/                 objects and devices: a car, a lamp, a laptop
├── topics/                 interests and themes: "jazz", "climbing"
├── works/                  films, series, books, albums
├── events/<YYYY>/          one file per dated event
├── states/<topic>/<YYYY>.md   "right now" states, one file per topic and year
├── summaries/
│   ├── day/<YYYY>/<YYYY-MM-DD>.md
│   ├── week/<YYYY>/<YYYY>-W<ww>.md
│   ├── month/<YYYY>/<YYYY-MM>.md
│   └── year/<YYYY>.md
├── journal/                raw journal — specified in journal.md, not part of this document
└── .ipse/                  Ipse's own data — never memory
    ├── vault.toml          vault settings (format version, language, freshness windows)
    ├── index/              derived index: may be deleted, Ipse rebuilds it
    └── proposals/          AI proposals waiting for review
```

- Folder names are fixed and in English in every language, so that tools and documentation stay the same everywhere.
- Ipse **must not** fail on files or folders it does not know. A person may keep their own notes in the vault; Ipse leaves them alone and does not index them as entities.
- `journal/` and `.ipse/index/` are excluded from history (section 9).

## 2. Common rules for every memory file

### 2.1 Encoding

- UTF-8, no byte order mark, `\n` line endings on write. Ipse **must** read `\r\n` files without error.
- Markdown follows [CommonMark](https://commonmark.org) with Obsidian conventions: YAML front matter, `[[wikilinks]]`, `^block-ids`.

### 2.2 Front matter

Every memory file starts with a YAML front matter block. These keys are common to all of them:

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | string | yes | Stable identifier, a [ULID](https://github.com/ulid/spec) in lowercase. Never changes, even when the file is renamed or moved. |
| `type` | string | yes | `person`, `place`, `organization`, `project`, `thing`, `topic`, `work`, `event`, `state-log`, `summary`. |
| `category` | string | yes | Rights category (see `rights.md`). Defaults per type are listed there. |
| `created` | datetime | yes | RFC 3339 with offset, e.g. `2026-11-12T19:30:00+01:00`. |
| `updated` | datetime | yes | Last change, same format. |
| `sources` | list of strings | no | Where the content came from, as `<connector>:<external-id>` or `person` (typed by the person) or `proposal:<id>`. |

Unknown keys **must** be preserved untouched when Ipse rewrites a file. People and plug-ins may add their own.

### 2.3 File names

- The file stem is `to_file_stem(name)` (see `crates/ipse-core/src/slug.rs`). When nothing usable is left, the stem is the `id`.
- On collision, Ipse appends `-2`, `-3`… The `id`, not the file name, identifies an entity.
- When a name changes, Ipse renames the file and rewrites every wikilink that points to it, in the same change (section 9).

### 2.4 Links

- An entity is referenced with a wikilink to its file stem: `[[marie-dupont]]`, or with a display text: `[[marie-dupont|Maman]]`.
- Ipse resolves links by stem first, then by `aliases` (section 3). A link that resolves to nothing is kept as written and reported in the index, never deleted.

### 2.5 Section headings

Sections are level-2 headings (`##`) whose text comes from the vault's language (Q2). Ipse recognizes the headings of **every** supported language, so a vault whose language is changed keeps working. Supported in format version 1:

| Section | `en` | `fr` |
| --- | --- | --- |
| Facts | `## Facts` | `## Faits` |
| Relations | `## Relations` | `## Relations` |
| Notes | `## Notes` | `## Notes` |
| Events | `## Events` | `## Événements` |
| Summary | `## Summary` | `## Résumé` |

Text outside known sections, and the `Notes` section, is free text: Ipse indexes it for search but never parses or rewrites it.

## 3. Entities

Entities are the files under `people/`, `places/`, `organizations/`, `projects/`, `things/`, `topics/` and `works/`.

### 3.1 Entity front matter

In addition to the common keys:

| Key | Type | Applies to | Meaning |
| --- | --- | --- | --- |
| `name` | string | all | Display name. |
| `aliases` | list of strings | all | Other names the person uses: `Maman`, `Mum`. |
| `birthday` | date or `--MM-DD` | person | `1965-03-12`, or `--03-12` when the year is unknown. |
| `emails` | list of strings | person, organization | |
| `phones` | list of strings | person, organization | E.164 when known: `+33612345678`. |
| `address` | string | place, person, organization | Free text. |
| `geo` | `[lat, lon]` | place | WGS 84, decimal degrees. |
| `located_in` | wikilink | place, thing | Containing place: a room in a home, a city. |

Well-known attributes live here because connectors fill them and duplicate detection compares them.

### 3.2 Facts

A fact is one bullet line in the `Facts` section:

```markdown
## Faits

- Allergique aux noix. ^f-3k9w
- Travaille comme infirmière à l'hôpital Pasteur depuis 2019. ^f-8m2p
- Préfère être appelée le soir.
```

- A fact is one sentence of free text, written for a human.
- The trailing `^f-xxxx` is an Obsidian block identifier. Ipse adds one to every fact it indexes that has none, so that proposals, history and AIs can point at a single fact. It is 4 characters of lowercase Crockford base 32 after `f-`, unique within the file.
- A fact the person typed without an identifier is valid; Ipse adds the identifier on its next write to that file, without changing the text.
- Provenance of each fact is kept in history (section 9), not on the line, so lines stay readable.

### 3.3 Relations

A relation is one bullet line in the `Relations` section: a relation word, a colon, a wikilink.

```markdown
## Relations

- mère : [[marie-dupont]]
- travaille chez : [[hopital-pasteur]]
- vit à : [[lyon]]
```

- The relation word is free text in the vault's language. Ipse maps common words to a fixed vocabulary for the index (`parent`, `child`, `sibling`, `partner`, `friend`, `colleague`, `works_at`, `lives_in`, `member_of`, `owns`, `located_in`, `part_of`); unknown words are kept as free relations.
- A relation is written once, on the file where the person wrote it. The index makes it visible from both ends; Ipse **must not** write the reverse line automatically.

## 4. Events

An event is something that happened, or will happen, at a date: a meeting, a trip, a film seen.

Path: `events/<YYYY>/<YYYY-MM-DD>-<stem>.md`, where the date is the start date in the event's own time zone.

```markdown
---
id: 01jcx8m2p4q6r8t0v2w4y6z8a0
type: event
category: agenda
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
sources: [calendar:5f2a9c1e@google.com]
title: Dîner chez Julie
start: 2026-11-14T20:00:00+01:00
end: 2026-11-14T23:00:00+01:00
place: "[[chez-julie]]"
participants: ["[[julie-dupont]]", "[[marc-leroy]]"]
---

## Notes

Apporter le dessert.
```

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `title` | string | yes | |
| `start` | datetime or date | yes | A date alone means an all-day event. |
| `end` | datetime or date | no | Exclusive. Absent for a point in time ("film seen"). |
| `place` | wikilink or text | no | A link when the place has a file; the address as the calendar wrote it otherwise. |
| `participants` | list of wikilinks or texts | no | Without the vault's owner, who is implied. A guest is linked only when their email matches exactly one person file (decision 0011); otherwise their name, or else their email, is kept as text. |
| `status` | string | no | `planned` (default for future events), `done`, `cancelled`. An event removed from its calendar before it happened becomes `cancelled`; past events stay as they are. |

Recurring events are expanded: one file per occurrence the person actually has, generated up to 90 days ahead. The recurrence rule itself stays in the raw journal.

An event is found again by its calendar identifier (`UID`, and `RECURRENCE-ID` for one occurrence), whatever calendar it came from: the same invitation in two calendars is one file with two `sources` (decision 0011).

## 5. States ("right now")

A state is true only while it is recent: listening to an artist, watching a series, being at home. States are what makes Ipse know "what is going on now" without polluting durable facts.

### 5.1 Files

Path: `states/<topic>/<YYYY>.md`, one file per topic and year. Topics in format version 1: `music`, `watching`, `reading`, `home`, `location`. Connectors may add topics; a topic name matches `[a-z0-9-]+`.

```markdown
---
id: 01jcx9a1b2c3d4e5f6g7h8j9k0
type: state-log
category: media
created: 2026-01-03T10:00:00+01:00
updated: 2026-11-12T18:42:00+01:00
topic: music
year: 2026
---

| Subject | First seen | Last seen | Count | Source |
| --- | --- | --- | --- | --- |
| [[nina-simone]] | 2026-10-20 | 2026-11-12 | 64 | device |
| [[justice]] | 2026-11-08 | 2026-11-11 | 21 | device |
| [[severance]] | 2026-10-02 | 2026-10-30 | 9 | netflix |
```

- One row per subject. A subject is a wikilink when an entity exists (an artist under `people/` or `organizations/`, a series under `works/`), plain text otherwise.
- `First seen` and `Last seen` are dates in the vault's time zone (or datetimes for topics whose window is under a day, such as `home`).
- `Count` is how many raw observations support the row.
- Cells are trimmed. A row with another number of columns, or a value of the wrong form, is reported with its line and never guessed. A `|` inside plain text is written `\|`.
- Rows are ordered by `Last seen`, most recent first. Ipse rewrites the table from the raw journal; a person may delete a row, and Ipse then ignores those observations for that row.

### 5.2 Freshness

The status of a state is **computed when it is read, never stored**, because it depends on the current time:

```text
age = now − last_seen
status = current   if age ≤ window(topic)
         recent    if age ≤ 3 × window(topic)
         past      otherwise
```

When `last_seen` is a date, its age is counted in whole days in the vault's time zone: seen today is 0 days old, yesterday 1 day (24 hours). A state seen in the future is current.

Default windows, which the person may change in `.ipse/vault.toml`:

| Topic | Window |
| --- | --- |
| `music` | 21 days |
| `watching` | 30 days |
| `reading` | 30 days |
| `location` | 1 day |
| `home` | 15 minutes |

Rules for what leaves Ipse (the exact tools are in `rights.md`):

- By default an AI receives **current** states only, each with its `last_seen`.
- **Recent** and **past** states are returned only to a request that explicitly asks about a past period.
- Ipse decides the status; it is never left to the AI.

### 5.3 From states to facts

When a subject stays **current** across 3 consecutive monthly summaries, Ipse proposes a durable fact on the subject's entity or on the owner's file ("Aime Nina Simone"), through the review inbox. It is never written without the person's acceptance (decision 0005).

## 6. The owner

The person the vault belongs to has a file `people/<stem>.md` with `owner: true` in its front matter. Exactly one file **must** have it; Ipse reports an error otherwise and does not guess. "Me" in relations and participants refers to this file and is never written explicitly.

## 7. Summaries

Path and period as in section 1. Each summary has `type: summary`, `period: day | week | month | year`, `start` and `end` (dates, `end` exclusive), and two sections:

- `Summary`: free text written by the model, then corrected by the person if they wish.
- `Events`: wikilinks to the events of the period, generated.

A summary of a longer period is built from the summaries of the shorter periods inside it, not from raw data. A summary the person edited is never regenerated over their edit; Ipse proposes changes instead.

## 8. Settings: `.ipse/vault.toml`

```toml
format = 1                    # vault format version; Ipse refuses a higher version it does not know
language = "fr"               # language of section headings written by Ipse
time_zone = "Europe/Paris"    # IANA name, used for dates and file paths

[freshness]                   # state windows, any duration accepted by humantime: "21d", "15m"
music = "21d"
watching = "30d"
```

## 9. History and who changed what

The vault is a git repository, invisible to the person: no git word appears in the interface (decision 0007).

- Every change Ipse makes is **one commit**, whatever the number of files it touches (a rename with its link updates is one change).
- Every commit carries a trailer saying where the change came from: `Ipse-Source: connector:calendar`, `Ipse-Source: proposal:<id>`, `Ipse-Source: person`, or `Ipse-Source: ipse` for what Ipse does on its own (creating the memory, upgrading its format). Commits are made in pure Rust; the `git` program is not needed.
- Changes the person makes in another editor are detected (file watching, then, before Ipse changes anything, a comparison of the folder with the last commit) and committed as `Ipse-Source: person`. **The person always wins**: Ipse never overwrites an edit it did not make; a connector update that conflicts becomes a proposal.
    - From a connector, Ipse writes only the keys it takes from the source (a person's name, emails, phones and birthday; an event's title, times, place, guests and status), and only while they are as it last wrote them. The raw journal keeps every version, and `.ipse/materialized.toml` remembers which one is in the vault.
    - If the person changed one of those keys, the update is skipped and counted, to become a proposal in milestone 2. What the person added (facts, notes, other keys) never stops an update.
    - A file the person deleted is not created again.
- `ipse init` in a folder that already holds notes records them first, as the person's, then its own files as one change. A vault created before its history starts one, its files recorded as `ipse`.
- The vault's `.gitignore` lists what is not memory: `journal/`, `.ipse/materialized.toml`, `.ipse/index/`, `.ipse/proposals/`, `.ipse/access/`, `.ipse/clients.toml`, and editors' scratch files (`.obsidian/workspace*`, `.trash/`, `.DS_Store`). Lines the person added are kept.
- After each commit the git index is rewritten, so that a person who opens the folder with git sees a clean state.
- "Undo" in the interface reverts one change by creating a new commit; history is never rewritten.

## 10. Robustness

- A file that cannot be parsed (broken YAML, missing required key) is **reported and skipped**: it stays searchable as plain text, Ipse never rewrites it, and the error says in plain words what to fix and on which line.
- Ipse **must** write files atomically: write to a temporary file in the same folder, then rename.
- Ipse **must** keep the person's formatting outside the parts it owns (front matter keys it manages, the `Facts` identifiers, the `States` tables, generated `Events` lists).
- Personal data **must not** appear in logs. Errors name the file and line, not its content.

## 11. Versioning

- `format` in `.ipse/vault.toml` is an integer, bumped only for changes an older Ipse cannot read.
- Ipse migrates older vaults forward in one recorded change, after a backup of the files it will touch.
- Until 1.0 the format may change between releases; each change is listed in the release notes with its migration.
