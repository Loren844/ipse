# AGENTS.md

Rules for anyone writing code in this repository, human or AI agent. Read this file in full before every task.

## The project in three lines

Ipse is a local, open memory that any AI can plug into. Rust core, Markdown vault as source of truth, MCP server enforcing per-category rights. The product must be **simple to pick up** and the code must be **as fast as possible**.

## How work is organized

- **One issue, one pull request.** Do only what the issue asks. Anything else you notice goes in the PR description under "Follow-ups", not in the code.
- **The issue is the contract.** Meet every acceptance criterion and write every test it lists. If a criterion is unclear or impossible, stop and say so in the PR instead of guessing.
- **Decisions live in `docs/decisions/`.** Never contradict one. If your task seems to require it, stop and explain why in the PR.
- **The project owner merges.** No pull request is merged by an AI, and no AI approves another AI's work.

## Before you open a pull request

All of these must pass locally. CI runs the same commands and blocks the merge otherwise.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench --workspace --no-run
```

## Code rules

### Correctness first

- No `unwrap()` outside tests. Use `?` and typed errors; `expect()` only with a message proving it cannot fail.
- No panic on user data: a malformed file, a missing field or a broken connector returns an error, never crashes.
- Every public item has a doc comment saying what it does and what it returns on failure.
- Every behaviour change comes with tests. Bug fixes start with a failing test that reproduces the bug.

### Performance

The code aims for the best possible performance, guided by measurement.

- **Measure before and after.** Any change on a hot path (parsing, indexing, search, the raw journal, the MCP server) adds or updates a benchmark in `benches/` and states the before/after numbers in the PR.
- **Allocate deliberately.** Prefer borrowing (`&str`, slices) over cloning, reuse buffers in loops, size collections with `with_capacity` when the size is known.
- **Stream, don't load.** Read large files and the raw journal incrementally; never load a whole dataset into memory when it can be processed in a stream.
- **Pick the right structure.** State in the PR why a data structure fits the access pattern.
- **`unsafe` is denied by default.** It may be allowed locally only with a `// SAFETY:` comment proving soundness and a benchmark showing the gain.
- A clever optimization that makes code much harder to read needs a benchmark showing a real gain; otherwise keep the clear version.

### Style

- Edition 2024, formatting by `rustfmt` defaults, lints from the workspace `Cargo.toml`.
- Small modules with one job each. File and function names say what they do.
- Code, comments and commit messages in English.
- The command line speaks English, whatever the vault's language; the app will offer a choice of languages. Words Ipse writes into vault files (section headings, hints) follow the vault's language.
- The command line speaks English, whatever the vault's language; the app will offer a choice of languages. Words Ipse writes into vault files (section headings, hints) follow the vault's language.

### Dependencies

- Add a dependency only when it saves real work, and say why in the PR.
- Prefer well-maintained crates with permissive licenses compatible with Apache 2.0 (MIT, Apache-2.0, BSD, ISC, Zlib). No GPL or AGPL dependency.

## Architecture in brief

Data flows through five layers:

1. **Connectors** — one per source, each a separate process speaking a JSON protocol to the core, so they can be written in any language.
2. **Raw journal** — everything connectors bring back, timestamped, append-only, never modified.
3. **Markdown vault** — entities, facts and summaries. The source of truth, editable by the person.
4. **Derived index** — graph, full-text and semantic search, rebuildable at any time from the vault.
5. **AI gateway** — an MCP server that enforces each connected AI's rights.

Write rule: data a connector produces deterministically is written directly; anything an AI infers or changes is a proposal the person must accept.

## Security and privacy

- Personal data never appears in logs, error messages, test fixtures or commits. Test data is invented.
- No network call from the core unless a connector or a model provider the person enabled requires it.
- Rights are checked in the code path that returns data, never left to the caller.

## Commits and pull requests

- Branch names: `issue-<number>-<short-slug>`.
- Commit messages: imperative mood, one line under 72 characters, details below if needed.
- PR description: what changed, how it was tested, benchmark numbers if relevant, follow-ups.
