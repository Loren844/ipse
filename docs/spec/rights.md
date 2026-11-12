# Rights and the AI gateway

- **Status:** Accepted
- **Version:** `rights: 1`
- **Applies to:** milestone 1 onward.

Every AI connected to Ipse sees only the categories the person allowed it. Rights are **enforced by the software**, in the code path that returns data, never by an instruction in a prompt. An AI that is denied a category gets the same answer as if that data did not exist.

## Choices settled by the project owner

Settled on 2026-11-12.

| # | Question | Choice | Why |
| --- | --- | --- | --- |
| Q1 | What a newly connected AI may read | Every category except sensitive ones, shown on the connection screen with one switch per category. | Connecting an AI stays one action (decision 0007); sensitive categories stay off until the person turns them on. |
| Q2 | Smallest unit a category applies to | A whole file (an entity, an event) for milestone 1. To hide one fact, the person moves it to a file of a private category. | Simple to understand and to enforce. Per-fact categories can be added later without breaking files. |

## 1. Categories

Every vault file has one `category` (vault spec, section 2.2). Format version 1 defines these categories:

| Category | Contains | Default for | Sensitive |
| --- | --- | --- | --- |
| `identity` | The owner's own file: name, birthday, languages, preferences | the owner's file | no |
| `people` | People and organizations of personal life, and their relations | `person`, `organization` | no |
| `agenda` | Events: meetings, appointments, trips | `event` | no |
| `places` | Places the person knows and uses | `place` | no |
| `media` | Music, films, series, books, interests | `work`, `topic`, states `music`, `watching`, `reading` | no |
| `work` | Professional projects, colleagues, employers | `project` | no |
| `things` | Objects and devices | `thing` | no |
| `summaries` | Day to year summaries | `summary` | no |
| `location` | Where the person is and has been | states `location` | **yes** |
| `home` | Rooms, presence, home devices' states | states `home`, Home Assistant | **yes** |
| `health` | Health data and facts | health connectors | **yes** |
| `private` | Anything the person wants hidden from every AI by default | chosen by the person | **yes** |

- A category is a **label**, not a folder: it groups any number of files of any type. A `work` category can hold projects, colleagues, an employer and the events of a work trip.
- A connector's manifest sets the default category of what it produces; the person may change the category of any file.
- A file created as part of another (a sub-project linked with `part_of`, a room `located_in` a home) takes the category of its parent by default.
- **Sensitive** categories are denied to every AI until the person allows them for that AI, one by one.

### 1.1 Adding categories

- **The project** may add categories in later versions of this spec. Existing vaults keep working: files never change category on their own.
- **The person** may create their own categories, for example `studies` or `association`, in `.ipse/vault.toml`:

  ```toml
  [categories.studies]
  name = "Études"         # shown in the interface
  sensitive = false       # true: denied to every AI until allowed one by one
  ```

  A new non-sensitive category is **not** added to existing clients automatically: Ipse asks, for each connected AI, whether it may read it.
- A category used in a file but declared nowhere is treated as `private`.

## 2. Connected AIs

A connected AI is a **client**. Clients are listed in `.ipse/clients.toml`, outside the vault's memory:

```toml
[[client]]
id = "claude-desktop"
name = "Claude (ordinateur)"
created = 2026-11-12T20:40:00+01:00
token_hash = "b3:4e1a…"        # BLAKE3 of the token; the token itself is never stored
read = ["identity", "people", "agenda", "places", "media", "work", "things", "summaries"]
propose = []                    # categories where it may submit proposals (milestone 2)
```

- **Connecting an AI** creates a client, generates a random 256-bit token, and shows the person exactly what to paste or click for their AI app (decision 0007). The token appears once and is stored only by that app.
- **Removing** a client deletes its entry; its token stops working immediately.
- Access levels per category:
    - `read`: the client can read the category.
    - `propose` (milestone 2): the client can submit proposals touching that category; the person accepts or refuses them in the review inbox.
    - Writing without review is out of scope until decision 0005 is revisited at milestone 4.

## 3. Transport

- Ipse is an MCP server built on `rmcp` (decision 0001).
- **Local AI apps** start `ipse mcp` themselves, over standard input and output. The token is passed in the environment variable `IPSE_TOKEN` of that command, set in the AI app's configuration.
- **Online AIs** that cannot start a local program connect over Streamable HTTP on `127.0.0.1` only, with the token as a bearer token. Exposing Ipse beyond the computer is out of scope for 1.0.
- A request without a valid token is refused before any data is read.

## 4. Tools

In milestone 1, the gateway is **read-only** and exposes these MCP tools. Every tool filters by the client's categories before anything else.

| Tool | Arguments | Returns |
| --- | --- | --- |
| `about_me` | — | The owner's file: name, well-known attributes, facts. Requires `identity`. |
| `search` | `query`, `limit` (default 10, max 50) | Matching entities, events and facts, best first, each with its `id`, type and a short excerpt. |
| `get` | `id`, or `name` (matched against names and aliases) | One entity or event in full: front matter, facts, relations, notes. |
| `events` | `from`, `to` (dates), optional `with` (an entity id) | Events in that period, ordered by start, at most 200. |
| `now` | optional `topics` | **Current** states only (vault spec, section 5.2), each with its `last_seen`. |
| `states_history` | `topic`, `from`, `to` | States of that topic in a past period, including recent and past ones. The only way to get them. |

Rules every tool follows:

- **Invisible means absent.** Hidden data is never returned, counted, or hinted at: no "3 results hidden", no different error.
- **Links to hidden files are masked.** A wikilink to a file the client cannot read is returned as `[masqué]` (in the vault's language), without its id or display text.
- **No raw data.** The raw journal is never exposed, in any form.
- **Bounded answers.** Every list is capped as above and says when it was cut, so an AI asks for more instead of receiving a whole life at once.
- **Times** are returned in RFC 3339 with the vault's time zone offset.

## 5. Access log

Every request is logged in `.ipse/access/<YYYY-MM>.jsonl`, one line per call:

```json
{"at":"2026-11-12T20:41:03+01:00","client":"claude-desktop","tool":"events","categories":["agenda"],"results":7}
```

- The log records **which categories were read and how many results**, never the arguments' text or the content returned.
- The interface turns it into plain words: "Claude a consulté votre agenda 12 fois aujourd'hui."
- The access log is kept 12 months, then deleted.

## 6. What is checked where

- Category filtering happens in the **query layer of the core**, the same code every tool and every future interface goes through. A tool cannot return data without passing through it.
- Tests for each tool **must** cover: a denied category returns nothing; a link to a denied file is masked; an unknown category is treated as `private`; a missing or wrong token is refused before any read.
