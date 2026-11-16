# Contributing

The project is not open to outside code contributions yet, while the specification settles. Contributions of connectors will open first, with a kit to write them ([decision 0015](docs/decisions/0015-launch-with-the-desktop-app.md)). Until then, bug reports and questions are welcome in the issues, with invented data only. To report a security flaw, see [SECURITY.md](SECURITY.md).

Everyone follows the [code of conduct](CODE_OF_CONDUCT.md). Read [`AGENTS.md`](AGENTS.md) for how work is organized: it applies to people and coding agents alike.

## Repository layout

| Path | What it holds |
| --- | --- |
| `crates/ipse` | The `ipse` program: command line, MCP server, built-in connectors. |
| `crates/ipse-core` | The core library: vault, journal, index, rights. |
| `crates/ipse-protocol` | Messages of the connector protocol, shared by the core and connectors. |
| `ui` | The interface (Svelte), served by `ipse ui`; its build is embedded in the binary. |
| `docs/guide` | How to use Ipse, starting with [Getting started](docs/guide/getting-started.md). |
| `docs/spec` | The specification: vault, journal, connectors, rights. |
| `docs/decisions` | Architecture decisions, one dated file each. |
| `AGENTS.md` | Rules every contributor and coding agent follows. |
| `.github` | CI, release, issue and pull request templates. |

## Building from source

Requires [Rust](https://rustup.rs) (the exact version is pinned in `rust-toolchain.toml` and installed automatically).

```sh
cargo install --path crates/ipse   # the ipse command, from this folder
cargo test --workspace             # run the tests
cargo bench --workspace            # run the benchmarks
```

The interface needs [Node.js](https://nodejs.org) only to change it: its build in `ui/build` is committed and embedded in the binary.

```sh
cd ui
npm ci
npm run check    # types
npm run build    # rebuild ui/build, then commit it
npx playwright test
```

## Releasing

Releases are built by [cargo-dist](https://opensource.axo.dev/cargo-dist/) (`.github/workflows/release.yml`, settings in `dist-workspace.toml`) when the owner pushes a tag:

1. Set the version in `Cargo.toml` (`[workspace.package]`) and give the CHANGELOG section its date: `## 0.2.0 - 2026-12-01`.
2. Merge, then tag the merge commit: `git tag v0.2.0 && git push origin v0.2.0`.
3. The workflow builds the binaries for every system, then publishes a GitHub release with the installers, checksums and the changelog section as notes. A tag like `v0.2.0-rc.1` makes a pre-release.
