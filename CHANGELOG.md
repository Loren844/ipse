# Changelog

What changed in each version of Ipse, for the people who use it. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Before 1.0, a minor version may change the format of memory files; Ipse then says what to do.

## 0.1.0-rc.1 - 2026-11-16

A pre-release of 0.1.0, to try the binaries and the installers before the first version. What it holds is described under 0.1.0 below.

## 0.1.0 - Unreleased

Not released yet: 0.1.0 waits for the First version milestone (settings, the whole memory, editing, graph view, event kinds; [decision 0016](docs/decisions/0016-a-first-version-shows-the-whole-memory.md)). What is already done, from milestones 1 and 2 and the launch readiness stage ([decision 0015](docs/decisions/0015-launch-with-the-desktop-app.md)):

### Your memory

- A memory is a folder of Markdown files you own, readable in any editor or in Obsidian: people, places, events, summaries ([vault format](docs/spec/vault.md)). `ipse init` creates one; `ipse init --demo` an invented one, to try Ipse first.
- Every change is recorded in an invisible history, with who made it (you, a source, an AI's proposal you accepted), and can be undone.
- Search by name or by any word of what is known, in a few milliseconds on tens of thousands of files.

### Sources

- Contacts from vCard files (Google, Apple, Outlook, vCard 2.1 to 4): names, emails, phones in international form, birthdays, addresses, organizations, notes, and the related names on your own card ("mother: Mom") as relations.
- Calendars from iCalendar files, or kept in sync by subscribing to their secret link (kept in your system's keychain): guests linked to the people you know, repeating events, time zones, cancellations.
- A change you made yourself is never overwritten by a source: what the source says waits for your decision.
- Two files of the same person or event are never merged without you: possible duplicates are proposed.

### Your AI

- Any AI app that speaks MCP connects with its own code (`ipse client add`), locally or over HTTP on this computer, and reads only the categories you allowed; sensitive ones are closed by default. Every read is logged, without what was asked.
- With your permission, it tidies up your memory through tasks: facts and relations from notes, merges of duplicates, the full names of people saved under a nickname, summaries of your days, weeks, months and years. Everything it wants to change is a proposal you accept, refuse (saying why) or undo.
- Text imported from your apps reaches your AI as data, never as instructions, with where it came from and who sent an invitation ([decision 0014](docs/decisions/0014-imported-data-is-untrusted.md)).

### The interface

- `ipse ui` opens your memory in your browser, on this computer only: home, people, agenda, summaries, review inbox with what each proposal changes, connected AIs and their rights per category, history of changes, of AI reads and of imports. French and English.
- Edit any file yourself in `ipse ui`: names, phones, emails, birthday, address, an event's times, place and people, facts, relations and notes. Create a file of any kind, including the one a link already points to, and delete one after seeing what links to it. Each save is one change you can undo from the history; a file changed meanwhile is never written over.
- A graph around any file, yourself by default: what it links to and what links to it, one or two steps away, filtered by kind of file and category, the categories closed to AIs marked. Click a file to put it at the center. The same neighbourhood reads as a list, for the keyboard and screen readers.
- Kinds of events: meals, celebrations, outings, culture, sport, trips, travel, work, studies, appointments, health. Your AI proposes them, a few batches a day, and you accept them in the review inbox; you can also set one on the event's page. The agenda shows them.
- On a phone, the main sections (home, memory, agenda, review) sit in a bar at the bottom, the others behind More, and the search stays in the top bar on every screen.

### Installing

- Binaries for Windows, macOS (Apple silicon and Intel) and Linux (x64 and ARM), with an installer for each system ([Getting started](docs/guide/getting-started.md)).
