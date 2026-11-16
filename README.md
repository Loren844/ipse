# Ipse

**Your memory, on your machine, for any AI.**

Ipse (Latin for *oneself*) gathers what makes up a person's memory — facts, memories, habits, and the data already sitting in their services — into a local graph they can see, correct and control. Any AI, local or online, can then plug into it with the rights the person chooses, and act as a truly personal assistant without being told everything again.

> **Status: early, not released yet.** Today you can import your contacts and your calendar, connect your AI app, and ask it about the people you know and your week ([Getting started](docs/guide/getting-started.md)). Your AI can tidy up your memory: it proposes facts from your notes, merges of duplicates, the full names of people saved under a nickname, and summaries of your days; you review its proposals in `ipse ui`, in your browser ([Review what it proposes](docs/guide/review.md)). Installation still needs Rust; ready-made binaries come next, then the desktop app.

## What it will do

- **Local storage.** Your memory lives in a folder you pick, on your machine.
- **Open files.** Everything is Markdown, readable in Obsidian or any editor.
- **Connectors.** Calendar, contacts, location history and more fill your memory on their own.
- **Graph view.** People, places, projects and events, and how they relate, around any file of your memory, one or two steps away; filtered by kind and category, and readable as a list.
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

✅ shipped · 🔜 planned

1. ✅ **Core without interface** — vault format, raw journal, index, command line, read-only MCP server with per-category rights. First connectors: calendar and contacts.
2. ✅ **Review and proposals** — the interface starts, in the browser (`ipse ui`): home, people, agenda, review inbox with diff and undo, history, connected AIs and their rights; tasks for your AI (facts from notes, duplicates, questions about nicknames, day to year summaries).
- 🔜 **First version, 0.1.0** ([decision 0016](docs/decisions/0016-a-first-version-shows-the-whole-memory.md)) — settings and themes; the whole memory, not only people (places, organizations, projects, works, what is going on right now); editing and creating files yourself; a graph view; kinds of events (trips, meals, appointments…), proposed by your AI. Binaries for Windows, macOS and Linux and a demo memory are already there ([0.1.0-rc.1](https://github.com/Loren844/ipse/releases/tag/v0.1.0-rc.1)); imported data is treated as untrusted.
- 🔜 **Launch readiness** ([decision 0015](docs/decisions/0015-launch-with-the-desktop-app.md)) — an AI app connected in one command, location history (Google Maps Timeline).
3. 🔜 **Desktop app** — Ipse as an app for Windows, macOS and Linux, to see your memory without a terminal or a browser: installer, updates, sync in the background, connector management. Synchronized calendars and contacts (CalDAV, CardDAV). Then views of events: by month and week, by kind, on a timeline of your life and on a map. Then the public launch.
4. 🔜 **Widening** — more connectors, self-hosted services first (Jellyfin, Immich, Home Assistant…); a kit for anyone to write one; semantic search with a local model, as an option.
5. 🔜 **Mobile** — sync, quick capture.

## Install

Ready-made binaries for Windows, macOS and Linux come with each [release](https://github.com/Loren844/ipse/releases); [Getting started](docs/guide/getting-started.md) shows how to install one and go from there.

## Contributing

The project is not open to outside code contributions yet; bug reports are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for the repository layout and building from source, and [SECURITY.md](SECURITY.md) to report a flaw.

## License

[Apache License 2.0](LICENSE).
