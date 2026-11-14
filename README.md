# Ipse

**Your memory, on your machine, for any AI.**

Ipse (Latin for *oneself*) gathers what makes up a person's memory — facts, memories, habits, and the data already sitting in their services — into a local graph they can see, correct and control. Any AI, local or online, can then plug into it with the rights the person chooses, and act as a truly personal assistant without being told everything again.

> **Status: early.** The first milestone works from the command line: import your contacts and your calendar, connect your AI app, and ask it about the people you know and your week. [Getting started](docs/guide/getting-started.md) shows how. The app comes with the next milestones.

## What it will do

- **Local storage.** Your memory lives in a folder you pick, on your machine.
- **Open files.** Everything is Markdown, readable in Obsidian or any editor.
- **Connectors.** Calendar, contacts, location history and more fill your memory on their own.
- **Graph view.** People, places, projects and memories, and how they relate, in one navigable view.
- **You stay in charge.** The AI proposes, you accept or refuse. Every change is visible and can be undone.
- **Per-AI rights.** Each AI you connect gets read and write access category by category, enforced by the software.
- **Any model.** Works with a local model or an online one, through the open [Model Context Protocol](https://modelcontextprotocol.io).

## Principles

1. **Simple to pick up.** Install in one step, useful without any setup, no technical word in the interface.
2. **Local first.** Nothing leaves your machine without an explicit action from you.
3. **Open formats.** No proprietary format, ever.
4. **The human keeps control.** Every AI change is a proposal: visible, reversible, historized.
5. **Rights enforced by the software**, never by a line in a prompt.
6. **Model-independent.** Plug in any LLM, local or online.

## Roadmap

1. **Core without interface** — vault format, raw journal, index and graph, command line, read-only MCP server with per-category rights. First connectors: calendar and contacts.
2. **Review and proposals** — the interface starts, in the browser (`ipse ui`): daily summary, review inbox, proposals with diff and undo.
3. **Desktop app** — the same interface as an app for Windows, macOS and Linux: installer, updates, graph view, editing, connector and rights management.
4. **Widening** — more connectors (location history first), weekly to yearly summaries, AI writes.
5. **Mobile** — sync, quick capture, questions of the day.

## Repository layout

| Path | What it holds |
| --- | --- |
| `crates/ipse` | The `ipse` program: command line, MCP server, built-in connectors. |
| `crates/ipse-core` | The core library: vault, journal, index, rights. |
| `crates/ipse-protocol` | Messages of the connector protocol, shared by the core and connectors. |
| `docs/guide` | How to use Ipse, starting with [Getting started](docs/guide/getting-started.md). |
| `docs/spec` | The specification: vault, journal, connectors, rights. |
| `docs/decisions` | Architecture decisions, one dated file each. |
| `AGENTS.md` | Rules every contributor and coding agent follows. |
| `.github` | CI, issue and pull request templates. |

## Building from source

Requires [Rust](https://rustup.rs) (the exact version is pinned in `rust-toolchain.toml` and installed automatically).

To use Ipse, follow [Getting started](docs/guide/getting-started.md). To work on it:

```sh
cargo test --workspace     # run the tests
cargo bench --workspace    # run the benchmarks
```

## Contributing

The project is not open to outside contributions yet, while the specification settles. Read [`AGENTS.md`](AGENTS.md) for how work is organized.

## License

[Apache License 2.0](LICENSE).
