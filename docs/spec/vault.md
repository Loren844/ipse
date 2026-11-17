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
    ├── index/              derived index: may be deleted, Ipse rebuilds it (search.bin: the search index)
    ├── proposals/          AI proposals waiting for review (section 12)
    └── tasks/              work left for the person's AI (section 13)
```

- Folder names are fixed and in English in every language, so that tools and documentation stay the same everywhere.
- Ipse **must not** fail on files or folders it does not know. A person may keep their own notes in the vault; Ipse leaves them alone and does not index them as entities.
- `journal/` and `.ipse/index/` are excluded from history (section 9).
- The search index covers entities (name, other names, facts, relations, notes), events (title, place, participants, notes) and summaries (their text). Words are matched without case or accents, and plurals as their singular, by the rules of the vault's language (`chevaux` finds `cheval`, `cities` finds `city`). Ipse saves it in `.ipse/index/search.bin` and, when it opens, reads again only the files whose size or date changed.

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
| `sources` | list of strings | no | Where the content came from, as `<connector>:<external-id>` or `person` (typed by the person) or `proposal:<id>`. A file without sources was written by the person. AIs get its kinds as `origin` (decision 0014). |

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
- A fact the person typed without an identifier is valid; Ipse adds the identifier on its next write to that file, at the end of the fact's last line, without changing the text or any other byte.
- **The identifier is for Ipse, not for the person.** The interface never shows it: it displays and edits the fact's text alone and keeps the identifier attached to the fact. AIs receive facts without it. Obsidian already hides `^…` block identifiers when reading.
- An identifier used by two facts of a file is reported by `ipse check`, which says which file to fix; Ipse never renumbers a fact on its own.
- Provenance of each fact is kept in history (section 9), not on the line, so lines stay readable.

### 3.3 Relations

A relation is one bullet line in the `Relations` section: a relation word, a colon, a wikilink.

```markdown
## Relations

- mère : [[marie-dupont]]
- travaille chez : [[hopital-pasteur]]
- vit à : [[lyon]]
```

- The relation word is free text in the vault's language. Ipse maps common French and English words to a fixed vocabulary for the index (`parent`, `child`, `sibling`, `partner`, `friend`, `colleague`, `works_at`, `lives_in`, `member_of`, `owns`, `located_in`, `part_of`), ignoring case and accents: `mère` and `father` are `parent`, `travaille chez` is `works_at`. Unknown words, like `parrain`, are kept as free relations. AIs receive the kind next to the word.
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
| `organizer` | wikilink or text | no | Who sent the invitation, when it is not the vault's owner, linked or written as for `participants`. Its title, place and notes were written by them (decision 0014). Set by the calendar, never by a proposal. |
| `kind` | string | no | What kind of event it is: `meal` (lunch, dinner, a coffee), `celebration` (a birthday, a party, a wedding), `outing` (drinks, a walk, a visit), `culture` (a concert, a film, a play, a museum), `sport`, `trip` (a stay away from home), `travel` (a flight, a train, a long drive), `work` (a meeting, a conference), `study` (a class, an exam, a training), `appointment` (the bank, the hairdresser, a repair), `health` (a doctor, a dentist, a therapist), `other`. Absent until someone says: the person, or their AI through a `classify` task (section 13). Never set by a calendar, and kept when the calendar changes the event. The interface names each kind in its language. |

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
- In the `location` topic, a subject is a place (`places/`), `First seen` and `Last seen` are days, and `Count` is how many stays; the log is rewritten from every visit of the year (`docs/connectors/location.md`).
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

A summary of a longer period is built from the summaries of the shorter periods inside it, not from raw data: a week and a month from the summaries of their days, a year from those of its months. A summary the person edited is never regenerated over their edit; Ipse proposes changes instead.

- The person's AI writes them, as `summary` tasks (section 13) answered with a `write_summary` change (section 12). The front matter also has `period`, `start` and `end`; the category is `summaries`; `sources` names the proposal that created it.
- **Day tasks:** each past day with events, looked at over the last 30 days (a new memory gets its recent days, then one a day). The items are the day's events, the people and places they link to, and the day's summary if there is one.
- **Longer periods:** a week, a month or a year that has ended gets a task once every shorter period inside it that holds something has its summary (or its task was closed with nothing worth keeping). Only the periods since Ipse first asked count.
- A period is asked again only when what it holds changed: a new or changed event, a summary inside it rewritten. `.ipse/tasks/summaries.json` remembers what each held when asked. The current summary is then one of the items, and the task asks to keep the person's words: the answer is an update proposal, showing their text and the new one.
- The tasks are planned when an AI lists them (`tasks`) and by `ipse tasks list`.
- The interface (`ipse ui`, **Résumés**) lists them by period, newest first, shows a day's summary in the agenda and the latest on the home screen. Correcting one there changes only its `Summary` text, as one change of history by the person (`Correct a summary`).

## 8. Settings: `.ipse/vault.toml`

```toml
format = 1                    # vault format version; Ipse refuses a higher version it does not know
language = "fr"               # language of section headings written by Ipse
time_zone = "Europe/Paris"    # IANA name, used for dates and file paths

[freshness]                   # state windows, any duration accepted by humantime: "21d", "15m"
music = "21d"
watching = "30d"
```

The person changes `language` and `time_zone` with `ipse settings --language en --time-zone America/Toronto`, or in the interface's **Settings**: Ipse rewrites those two lines and keeps every other line, comments included, as one change of the history by the person ("Change the settings"). Files already written keep their section headings; Ipse reads them in every supported language. The interface's theme and language are not settings of the memory: they stay in the browser.

## 9. History and who changed what

The vault is a git repository, invisible to the person: no git word appears in the interface (decision 0007).

- Every change Ipse makes is **one commit**, whatever the number of files it touches (a rename with its link updates is one change).
- Every commit carries a trailer saying where the change came from: `Ipse-Source: connector:calendar`, `Ipse-Source: proposal:<id>`, `Ipse-Source: person`, or `Ipse-Source: ipse` for what Ipse does on its own (creating the memory, upgrading its format). Commits are made in pure Rust; the `git` program is not needed.
- Changes the person makes in another editor are detected (file watching, then, before Ipse changes anything, a comparison of the folder with the last commit) and committed as `Ipse-Source: person`. **The person always wins**: Ipse never overwrites an edit it did not make; a connector update that conflicts becomes a proposal.
    - From a connector, Ipse writes only the keys it takes from the source (a person's name, emails, phones, birthday and address; an event's title, times, place, guests and status), and only while they are as it last wrote them. The raw journal keeps every version, and `.ipse/materialized.toml` remembers which one is in the vault.
    - This holds **key by key**: a key the person changed keeps their value, and the other keys still update. When the source then changes that key too, the person's value stays, and the source's value becomes a proposal by `connector:<instance>` (section 12): one per file, which a later import updates instead of adding another. Refusing it means "keep mine": the same value of the source is not proposed again, a newer one is. The import says how many files this concerns. What the person added (facts, notes, other keys) never stops an update.
    - A name the person gave to a contact is theirs: the address book's name (often a nickname such as "Maman") becomes one of the person's other names, so that it is still understood.
    - A guest first written as text, then linked once their contact is imported, is not a change by the person.
    - A file the person deleted is not created again.
    - The owner's own contact card completes the owner's file instead of making a second one: Ipse only adds what the card brought since it last read it, never replaces or removes anything.
    - Relations taken from the owner's own contact card are added once: `.ipse/added-relations.toml` lists them, so that one the person removes is not added again (`docs/connectors/contacts.md`).
- `ipse init` in a folder that already holds notes records them first, as the person's, then its own files as one change. A vault created before its history starts one, its files recorded as `ipse`.
- The vault's `.gitignore` lists what is not memory: `journal/`, `.ipse/connectors/`, `.ipse/materialized.toml`, `.ipse/added-relations.toml`, `.ipse/sync.toml`, `.ipse/index/`, `.ipse/proposals/`, `.ipse/tasks/`, `.ipse/access/`, `.ipse/clients.toml`, and editors' scratch files (`.obsidian/workspace*`, `.trash/`, `.DS_Store`). Lines the person added are kept.
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

## 12. Proposals

Decision 0005: anything an AI infers, merges or changes is a **proposal**, which the person accepts or refuses. A proposal is a list of changes to memory files that waits in `.ipse/proposals/<id>.json`, one file per proposal, written atomically. Proposals are not memory: they are left out of history (section 9).

```json
{
  "format": 1,
  "id": "01jd3k8m2p4q6r8t0v2w4y6z8a",
  "by": "client:claude",
  "created": "2026-11-14T19:02:11+01:00",
  "reason": "Marie mentioned her new number in our conversation.",
  "task": "01jd3k7…",
  "categories": ["people"],
  "changes": [
    { "kind": "set_key", "file": "people/marie-durand.md", "key": "phones",
      "from": ["+33612345678"], "to": ["+33698765432"] },
    { "kind": "add_fact", "file": "people/marie-durand.md", "text": "Allergic to nuts." }
  ],
  "status": { "state": "waiting" }
}
```

- `id` is time-ordered, like a file's `id`. `by` is `client:<id>` for a connected AI, `connector:<instance>` for a source that changed what the person had changed (section 9), or `ipse`. `reason` is the author's own words, shown to the person. `task` is the task the proposal answers, if any. `categories` are those of the files it touches, for rights (`rights.md`).
- Changes apply in order. Paths are relative to the vault, with `/`, in an entity or event folder:
    - `create`: a new entity or event: `type`, optional `category` (the type's default otherwise), `keys` (`name` for an entity; `title` and `start` for an event), optional `facts`, `relations` (`word`, `target`, optional `display`) and `notes`. Its `file` is chosen when the proposal is submitted. Its `sources` is `proposal:<id>`.
    - `set_key`: one front-matter key, with its value when the proposal was made (`from`) and its new value (`to`): a string, a list of strings, or `null` for none. Entities: `name`, `aliases`, `birthday`, `emails`, `phones`, `address`, `located_in`. Events: `title`, `start`, `end`, `place`, `participants`, `status`, `kind`. Other keys are not proposed: `id`, `type`, `category`, `owner`… stay as they are.
    - `add_fact` (`text`), `edit_fact` (`fact`: its `f-xxxx` identifier, `from`, `to`), `remove_fact` (`fact`, `text`).
    - `add_relation` (`word`, `target`: the linked file's stem, optional `display`), `remove_relation` (`word`, `target`).
    - `write_summary` (`period`: `day`, `week`, `month` or `year`; `start`: its first day, a Monday for a week; `text`): writes the period's summary (section 7). Its `file` and its `from` (the summary's text when the proposal was made, none for a new one) are set when it is submitted. Accepting it writes the `Summary` text and the generated `Events` list, and keeps everything else in the file; if the person changed the text since, the proposal is stale.
    - `merge` (`keep`, `remove`, optional `clues`: `email`, `phone`, `name`, `title_start`): merges two files of the same type (decision 0011). Accepting it brings into `keep` the other's sources, other names (its name too), emails, phones (compared on their last 9 digits), birthday and address when `keep` has none, facts and relations it lacks, and its notes after `keep`'s; for events, participants and place. Every `[[remove]]` link in other memory files is redirected to `keep`, and `remove` is deleted, all in one change. The owner's file is never `remove`. Refusing it records the pair, by id, in `.ipse/not-duplicates.toml`: it is never proposed again.
- `status.state`: `waiting`; `accepted` (`at`, `change`: the history change it made); `refused` (`at`, optional `comment`, kept for the AI to try again); `undone` (`at`, `change`); `stale` (`at`, `reason`).
- **Submitting** checks every change against the files as they are: a key that cannot be proposed, a file that does not exist, a value of the wrong form, a `from` that is no longer true, or a proposal that changes nothing is refused with a message saying what to fix, without quoting values.
- **Reviewing** shows each file's changes in plain words: before and after, per key, fact and relation. Fact identifiers and front-matter syntax are never shown.
- **Accepting** first records the person's own edits (section 9), then applies every change in **one change** of history, `Ipse-Source: proposal:<id>`, through the same writing rules as everything else (fact identifiers, atomic writes, layout kept). If any change no longer fits the files (a key or fact that changed since, a file removed or renamed), nothing is written and the proposal becomes `stale`; a stale proposal can only be refused.
- **Undoing** an accepted proposal reverts its change as a new change by the person; history is never rewritten. It is refused while one of its files changed since, so that the later change is never lost.
- **The review inbox** of `ipse ui` lists the waiting proposals newest first, with who made them, why, and the files they touch; one that no longer applies is marked so. Several can be accepted in one action, each still accepted on its own (one change of history each). A refusal takes an optional comment of at most 2 000 characters. The last 20 decided proposals stay listed, to read them again or undo an accepted one. A decided or stale proposal is shown as it was written; a waiting one, as it would change the files now.
- **Possible duplicates** (decision 0011) are proposed by Ipse (`by: ipse`) after each import and sync, and by `ipse duplicates`: people sharing an email (very likely) or a phone (likely), the same name only adding weight; events with the same title and start in two calendars, one proposal per pair of calendars. A value shared by more than 8 files (a switchboard) is no clue. Pairs waiting or said to be different are left out.
- `ipse proposals list|show|accept|refuse|undo` does the same from the command line.
- **The person's own edits** in `ipse ui` (decision 0016) go through the same changes and the same writing code, without waiting: a file's form is read with a version (a hash of the file), and saving turns what differs into `set_key`, fact, relation and `set_notes` changes, applied at once as **one change** by the person (`Edit a file`, `Create a file`, `Delete a file`), which can be undone from the history. Saving with an older version is refused: the file changed meanwhile. `set_notes` (`from`, `to`) rewrites the `Notes` section; it is the person's only, never accepted from an AI. A file the person creates has no `sources`; created from a link to a file that does not exist yet, it takes the link's name when that name is free. Deleting leaves links to the file as they are, and the owner's file cannot be deleted. A waiting proposal made from the old text becomes stale; a source that changes the file again proposes instead of writing over the person (section 9).

## 13. Tasks

Decision 0010: the work that needs a language model is left for the person's own AI, as **tasks** it reads through MCP (`rights.md`, section 4.2) and answers with a proposal (section 12). Tasks wait in `.ipse/tasks/<id>.json`, one file per task; they are not memory and are left out of history.

```json
{
  "format": 1,
  "id": "01jd3k7m2p4q6r8t0v2w4y6z8a",
  "kind": "extract",
  "created": "2026-11-14T19:02:11+01:00",
  "description": "Read the notes of 20 files and propose the facts and relations they hold.",
  "categories": ["people"],
  "items": [
    { "file": "people/marie-durand.md", "category": "people", "hash": "9f2c…" }
  ],
  "status": { "state": "open" },
  "comment": "Too vague: say which hospital."
}
```

- `kind` is `extract`, `merge`, `question`, `summary` or `classify`. `description` is one line without personal data. `items` (at most 100) name the files the task covers, with the hash of what in them it is about; the text itself is read from the files when an AI opens the task, so that it is never copied and always current. `categories` are those of the items, for rights.
- An AI opening a task gets what is expected in plain words, the items as they are now (file, name, type, category, text) and an example of a good proposal.
- `status.state` is `open`, `proposed` (with the `proposal` waiting for the person) or `done` (with the accepted `proposal`, or none when the AI found nothing worth keeping). One proposal at a time answers a task. When the person refuses it, the task is open again with their `comment`; when it no longer applies, or the person undoes it, the task is open again saying so.
- **`extract`:** after each import or sync, the notes of the files it wrote (a contact's notes, an event's description) that are new or changed since a task last covered them become tasks of 20 items. `.ipse/tasks/extract.json` remembers the hash of the notes each file had then, so the same notes are never asked twice. The AI is asked for lasting facts and relations, in the vault's language.
- **`summary`:** a period to summarize (section 7). The task has its `period` (`day:2026-11-14`); what is expected says how long and how personal the text should be, with an example, in the vault's language.
- **`question`:** someone saved under a first name, a nickname or a family word ("Jo plombier", "Léa", "Maman"): fewer than two words that start with a capital letter, or a family word in a supported language, with no other name that is a full one; never the owner. Only the person can name them, so their AI asks them, in one short question, when it fits a conversation, using what the item says (phones, emails, the latest events with them). It answers with `set_key` `name` (the old name is kept in `aliases`) and, when the person says how they know them, `add_relation` on the owner's file ("plombier : [[jo]]"); or with no change when the person does not know. The person can also answer in the review inbox, as their own change. Each person is asked about once; at most 3 questions wait at a time and at most 3 are made a day, so that questions never feel like a chore. `.ipse/tasks/questions.json` remembers who was asked, the questions waiting and how many were made each day.
- **`classify`:** events without a `kind`, nearest today first, 30 to a task and one category to a task, so that an AI that may not read a category never holds back the others. Each item gives the event's fields, each in its own key (title, start, end, place, people, status, who invited, notes cut at 500 characters), never mixed with Ipse's words (decision 0014). The AI answers with `set_key` `kind` from `null` for each event it can tell, `other` when none fits; an event given a kind since the task was made is left out of it. Each event is asked about once; at most 2 tasks wait at a time and at most 3 are made a day. `.ipse/tasks/classify.json` remembers which events were asked about, the tasks waiting and how many were made each day.
- The other kind comes with its feature: possible duplicates (`merge`).
- `ipse tasks list|show` shows them from the command line, as an AI reads them.
