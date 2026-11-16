# 0015 — Get ready to launch, then launch with the desktop app

- **Status:** Accepted
- **Date:** 2026-11-16
- **Refines:** 0006 (first sources), 0012 (the interface, packaged with Tauri later)

## Context

After milestone 2, Ipse does what sets it apart from other "local memory for AI" projects: it imports the person's real data, enforces rights per AI and per category in code, and turns every change an AI wants into a proposal the person accepts, refuses or undoes. Nobody outside the project can see it yet: installing means installing Rust, there is nothing to try without importing one's own life, and the README describes features in the future tense.

Milestone 3 is the desktop app, the product itself (decision 0007): the way to see one's memory, people, days and what one's AI proposes without a terminal and without a browser tab. Today `ipse ui` serves the same interface on the person's computer, but it has to be started from a terminal and kept open in a browser: fine for developers, not for most people.

A launch can only be done once. Launching before the app would show Ipse to most people in a form they cannot use.

Two security and honesty gaps also came up in a review of the project on 2026-11-16: imported text (an invitation's title, a contact's notes) reaches the person's AI with nothing that marks it as third-party data, and the architecture promised a "semantic search" index that no code implements.

## Decision

- **A "Launch readiness" stage comes between milestones 2 and 3.** It holds what the launch needs and the app builds on:
  - binaries for Windows, macOS and Linux, and an AI app connected in one command (the app reuses both);
  - a demo memory, to try Ipse and to film it without real data;
  - imported data treated as untrusted in everything an AI reads (decision 0014, to be written in #102), with a threat model page;
  - location history from the Google Maps Timeline export, moved forward from milestone 4: it is the source decision 0006 named next, and the one that shows what Ipse adds (where was I, with whom, that evening).
- **Then milestone 3, the desktop app, unchanged**: installer, signature, updates, sync in the background, graph view. The first connectors to synchronized and self-hosted services come alongside (CalDAV and CardDAV, then services such as Jellyfin). `ipse ui` in the browser stays the way to build and test the interface, and the way developers use it; it does not replace the app.
- **The public launch comes after milestone 3**, with the app. Before it, the binaries can go to a few testers invited privately, to find what blocks a first install; nothing is announced publicly.
- **The README is rewritten for the launch**, with only shipped features presented as such.
- **Semantic search is no longer promised as part of the index.** It is planned as an option, with a model that runs on the person's computer, once full-text search shows its limits.

## Consequences

- Decisions 0001, 0007 and 0012 stand, and are reinforced: the desktop app is the product, and the launch waits for it.
- Location history leaves milestone 4, which keeps the other connectors.
- The milestone numbers stay as they are, so that earlier decisions still read correctly.
- The roadmap in the README marks each stage as shipped or planned, so that nothing planned reads as done.
