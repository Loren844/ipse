# 0013 — The interface is written with Svelte

- **Status:** Accepted
- **Date:** 2026-11-14
- **Refines:** 0012 (local web interface)

## Context

Decision 0012 makes the interface a web app, served by `ipse ui` at milestone 2 and packaged with Tauri at milestone 3, and asks for the framework to be chosen from measurements. The same interface becomes the desktop app, which is the product for people who are not technical (decision 0007): it must be **polished and pleasant to use**, not only functional, with the motion, feedback and keyboard comfort people expect from a modern app. It must also:

- work the same in a browser and in a Tauri window;
- be embedded whole in the `ipse` binary, nothing fetched from the network;
- be accessible with the keyboard and a screen reader, in French and English at least;
- read through the query layer and write through the same paths as the command line (decision 0012);
- keep `cargo install` working.

Four candidates were built as the same screen: the review inbox with 1 000 proposals, and one proposal's changes opened beside it. Svelte, Solid and Preact fetch the proposals as JSON and render them in the browser; the fourth renders HTML in Rust (maud) and swaps parts of the page with htmx.

Measured on 2026-11-14: Chromium through Playwright, a 2-core Linux machine, median of 15 fresh page loads, then with the processor slowed 4 times to stand for an older laptop.

| | Svelte 5.57 | Solid 1.9 | Preact 10 | htmx 2.0 + maud |
| --- | --- | --- | --- | --- |
| Code sent to the browser (JS + CSS, gzip) | 14.6 KB | 6.3 KB | 6.7 KB | 17.3 KB |
| List of 1 000 on screen | 73 ms | 47 ms | 66 ms | 23 ms |
| … processor slowed ×4 | 256 ms | 204 ms | 233 ms | 229 ms |
| Proposal opened | 9 ms | 7 ms | 14 ms | 7 ms |
| … processor slowed ×4 | 35 ms | 27 ms | 39 ms | 32 ms |
| Memory (JS heap after GC) | 4.1 MB | 2.7 MB | 2.5 MB | 1.9 MB |
| Build of the interface | 0.7 s | 0.6 s | 0.5 s | 0.4 s, with `cargo build` |
| npm packages needed to build it | 39 (44 MB) | 74 (58 MB) | 88 (59 MB) | none: 1 vendored file |

Every candidate is fast enough: the differences are well under what a person notices. The choice rests on how well each lets us build a refined app.

## Decision

- **Svelte 5, with SvelteKit as a single-page app** (`adapter-static`, no server rendering), in TypeScript. This is how Tauri's own guide sets SvelteKit up, so milestone 3 packages the same build unchanged.
- **Motion and feedback are built in:** Svelte's transitions and animations (`transition:`, `animate:`, springs) make lists that settle smoothly, panels that slide, and changes that show where they went, without an animation library.
- **Accessible building blocks:** [Bits UI](https://bits-ui.com) for the components that are hard to get right (dialogs, menus, comboboxes, tooltips, tabs), unstyled, so that Ipse has its own look. Ipse's colours, type, spacing and dark mode are design tokens in CSS, written once.
- **Words on screen** come from translation files, French and English first, in the vault's language with a switch.
- **The core serves a JSON API** for the interface, on the same local server and with the same protections as decision 0012 (one-time code, session cookie, `Host` and `Origin` checks). It answers through the query layer with the owner's rights and writes through the same paths as the command line. The TypeScript types of the API are generated from the Rust types, so the two never drift.
- **`cargo install` keeps working without Node.** The interface's source lives in `ui/`; its built files are committed in `ui/build/` and embedded in the binary at compile time. CI builds the interface again and fails if the committed files differ from the sources. Only people who change the interface need Node.

## Why Svelte

- **htmx** keeps everything in Rust and needs no Node, but every interaction goes back to the server, and rich behaviour (smooth transitions between states, drag and drop, editing that reacts as one types, optimistic updates) means writing JavaScript by hand outside any framework. For the desktop app this decision prepares, that is the wrong trade.
- **Solid** sends the least code and renders fastest, but its ecosystem of accessible components and examples is the smallest of the three.
- **Preact** brings React's way of working in a small size, but most React component libraries need compatibility layers, and it has no built-in motion.
- **Svelte** has motion in the language, mature accessible components (Bits UI, the base of shadcn-svelte), a first-class Tauri setup, and components that read like HTML, which keeps them easy to review. Its 14.6 KB is loaded once from the computer itself.

## Consequences

- Two languages: Rust for the core, TypeScript and Svelte for the interface.
- Building the interface needs Node, for those who work on it; installing Ipse does not.
- Committed build files make interface changes two diffs; CI keeps them in step with the sources.
- npm packages are build-time dependencies, pinned by the lockfile and updated deliberately. What runs in the window is Ipse's code and Svelte's runtime only.
- The prototypes used for the measurements are not kept in the repository.
