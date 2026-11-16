# 0015 — Prepare the public launch before the desktop app

- **Status:** Accepted
- **Date:** 2026-11-16
- **Refines:** 0006 (first sources), 0012 (the interface, packaged with Tauri later)

## Context

After milestone 2, Ipse does what sets it apart from other "local memory for AI" projects: it imports the person's real data, enforces rights per AI and per category in code, and turns every change an AI wants into a proposal the person accepts, refuses or undoes. Nobody outside the project can see it yet: installing means installing Rust, there is nothing to try without importing one's own life, and the README describes features in the future tense.

Milestone 3, the desktop app, is large: installers for three systems, code signing, updates, a tray icon. It is the product for people who are not technical (decision 0007), but technical people, who are the first to try a project like this, do not need it: they need a binary, a demo and an honest README.

Two security and honesty gaps also came up in a review of the project on 2026-11-16: imported text (an invitation's title, a contact's notes) reaches the person's AI with nothing that marks it as third-party data, and the architecture promised a "semantic search" index that no code implements.

## Decision

- **A "Launch readiness" stage comes between milestones 2 and 3.** It holds:
  - binaries for Windows, macOS and Linux, and an AI app connected in one command;
  - a demo memory, to try Ipse and to film it without real data;
  - imported data treated as untrusted in everything an AI reads (decision 0014, to be written in #102), with a threat model page;
  - location history from the Google Maps Timeline export, moved forward from milestone 4: it is the source decision 0006 named next, and the one that shows what Ipse adds (where was I, with whom, that evening);
  - a README built around what makes Ipse different, with only shipped features presented as such.
- **The public launch comes at the end of this stage**, before the desktop app. The milestone numbers stay as they are, so that earlier decisions still read correctly.
- **The desktop app (milestone 3) follows unchanged, and it remains the product.** It is how the person sees their memory, their people, their days and what their AI proposes, without a terminal and without a browser tab: one app that opens, stays in the background to keep sources in sync, and updates itself. `ipse ui` in the browser is the way to build and test that interface, and the way technical people use it until the app exists; it is not a replacement. The first connectors to synchronized and self-hosted services come alongside (CalDAV and CardDAV, then services such as Jellyfin).
- **Semantic search is no longer promised as part of the index.** It is planned as an option, with a model that runs on the person's computer, once full-text search shows its limits.

## Consequences

- Decisions 0001, 0007 and 0012 stand: the desktop app is still the product, and the only way most people will use Ipse; only its place in the order of work changes. The launch speaks to technical people first; the app is what makes Ipse usable by everyone else.
- Location history leaves milestone 4, which keeps the other connectors.
- The roadmap in the README marks each stage as shipped or planned, so that nothing planned reads as done.
- A launch can only be done once; the stage is finished when someone who does not know the project installs Ipse and connects an AI on their own.
