# 0016 — A first version shows the whole memory

- **Status:** Accepted
- **Date:** 2026-11-16
- **Refines:** 0015 (get ready to launch, then launch with the desktop app)

## Context

The owner installed the pre-release `0.1.0-rc.1` on macOS on 2026-11-16. It works, but it is not yet a first version: the interface reads as a directory of people, while people are only a small part of a memory. The vault already holds places, organizations, projects, things, works, topics, events, states ("right now") and summaries (`docs/spec/vault.md`); the interface shows people, the agenda and summaries. The person cannot change a file themselves, choose a theme, see how things link, or look at their events other than as a list.

Decision 0015 put the graph view and editing with the desktop app (milestone 3). They are interface work, which the app packages unchanged (decision 0012): nothing is lost by doing them first, in the browser.

## Decision

- **A "First version" milestone comes before milestone 3, and ends with the release of 0.1.0.** Until then, versions are pre-releases (`0.1.0-rc.N`). In this order:
  1. **Settings**: theme (system, light, dark), language, time zone, where the memory is, the version.
  2. **The whole memory**: every kind of file, searchable, with a page for any file and what links to it; "right now" from current states.
  3. **Editing and creating files** in the interface, as the person's own changes (decision 0005): never overwritten by a source, undoable.
  4. **A graph view** around any file.
  5. **Event kinds** (below), so that the data is there for the views that come later.
- **Other views of events come with milestone 3, after the app**: a calendar by month and week, views by kind of event, a timeline of one's life by years and months, and a map of the places one has been (after location history). They present what the first version already holds; the agenda stays a list until then.
- **Events get a kind** (trip, meal, appointment…). The person's own AI proposes it, through tasks, and the person accepts it, as for summaries (decision 0010); the person can also set it by hand. Ipse does not guess it on its own.
- **The launch readiness stage keeps its remaining work** (connecting an AI app in one command, location history), after the interface work; the map waits for location history.
- **Milestone 3 keeps the packaging** (the desktop app with Tauri, installer, signature, updates, background sync), **then the views of events** above. The public launch still comes after it (decision 0015).

## Consequences

- 0.1.0 means a memory the person can see whole, change and explore through its links, not only query through an AI.
- The README roadmap shows the First version milestone before the desktop app.
- The demo memory grows with each screen, so that every screen has something to show.
