# 0013 — The interface is rendered by the core, with htmx

- **Status:** Accepted
- **Date:** 2026-11-14
- **Refines:** 0012 (local web interface)

## Context

Decision 0012 makes the interface a web app, served by `ipse ui` at milestone 2 and packaged with Tauri at milestone 3, and asks for the framework to be chosen from measurements. What the interface must be:

- the same in a browser and in a Tauri window;
- embedded whole in the `ipse` binary, nothing fetched from the network;
- accessible with the keyboard and a screen reader, in French and English at least;
- reading through the query layer and writing through the same paths as the command line (decision 0012);
- installable with `cargo install`, and simple to work on.

Four candidates were built as the same screen: the review inbox with 1 000 proposals, and one proposal's changes opened beside it. The three client-side frameworks fetch the proposals as JSON and render them in the browser. The fourth renders HTML in Rust (maud templates), and htmx swaps the opened proposal in without reloading the page.

Measured on 2026-11-14: Chromium 1.63 through Playwright, a 2-core Linux machine, median of 15 fresh page loads, then with the processor slowed 4 times to stand for an older laptop.

| | Svelte 5.57 | Solid 1.9 | Preact 10 | htmx 2.0 + maud |
| --- | --- | --- | --- | --- |
| Code sent to the browser (JS + CSS, gzip) | 14.6 KB | 6.3 KB | 6.7 KB | 17.3 KB |
| List of 1 000 on screen | 73 ms | 47 ms | 66 ms | 23 ms |
| … processor slowed ×4 | 256 ms | 204 ms | 233 ms | 229 ms |
| Proposal opened | 9 ms | 7 ms | 14 ms | 7 ms |
| … processor slowed ×4 | 35 ms | 27 ms | 39 ms | 32 ms |
| Memory (JS heap after GC) | 4.1 MB | 2.7 MB | 2.5 MB | 1.9 MB |
| Build of the interface | 0.7 s, Node | 0.6 s, Node | 0.5 s, Node | 0.4 s, with `cargo build` |
| npm packages needed to build it | 39 (44 MB) | 74 (58 MB) | 88 (59 MB) | none: 1 vendored file |

Every candidate is fast enough: the differences are well under what a person notices, and the server is on the same computer. The choice rests on the other criteria.

## Decision

- **The core renders the interface as HTML**, with type-checked templates in Rust ([maud](https://maud.lambda.xyz)), and **htmx** updates parts of the page (open a proposal, accept it) without reloading it.
- Pages are made of real links and forms, so that the browser's own keyboard handling, history and accessibility work; htmx only spares the full reload.
- htmx is one JavaScript file, vendored in the repository at a fixed version and embedded in the binary, with the stylesheet. A screen that later needs more (the graph view of milestone 3) adds its own small script there, without changing the rest.
- Words on screen go through the same translations as the rest of Ipse, in the vault's language (French or English), with a switch.
- In Tauri (milestone 3), the window shows the same local server, protected as decision 0012 says.

## Why not a client-side framework

- **No Node to build or install Ipse.** With Svelte, Solid or Preact, either everyone building Ipse needs Node, which breaks `cargo install` as it is documented today, or the built files are committed and checked in CI, which makes every interface change two diffs and opens a gap where the committed files and the sources differ. Rendering in Rust needs neither.
- **One place for the rules.** The query layer, the rights and the write paths are already in Rust. Rendering there means no second copy of the data model in TypeScript and no JSON API to secure in addition to the pages: what a page shows is exactly what the query layer returned to the owner.
- **Less to supply.** One vendored file instead of 39 to 88 npm packages: fewer updates, a smaller attack surface, and a reviewable dependency.
- **Testable without a browser.** Most of the interface is checked by Rust tests on the rendered HTML; browser tests (Playwright, in CI) cover what htmx does.

Solid was the strongest of the client-side frameworks: the least code sent, the fastest client rendering. It stays the option if the interface later needs much more client-side behaviour than htmx gives (editing offline, heavy drag and drop).

Leptos (Rust compiled to WebAssembly) was considered and not measured, because the WebAssembly target could not be installed where the prototypes were built. It keeps everything in Rust too, but it needs its own build tools (the WebAssembly target and a bundler), and it still ships the application to the browser, where rendering in the core ships only the page.

## Consequences

- Building Ipse still needs only Rust; `cargo install --path crates/ipse` installs the interface too.
- Every action is a request to the local server. This is instant on the same computer; it rules out an interface that works while the core is stopped, which Ipse does not need.
- Contributors write the interface in Rust and HTML; contributors used to React or Svelte have something new to learn, though the templates are plain HTML in a Rust macro.
- Forms that change something use `POST`, with the session cookie and the `Origin` check of decision 0012 against cross-site requests; templates escape every value by default.
- The prototypes used for the measurements are not kept in the repository.
