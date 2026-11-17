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
| `app` | The desktop app (Tauri), its own Cargo workspace: a window that runs the `ipse` command it ships with. |
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

### The desktop app

The app in `app/` is its own Cargo workspace, so that Tauri and the system libraries it needs stay out of the core's builds. It runs the `ipse` command for everything (`ipse ui --app` for the window, `ipse init` from its first screen): build that first.

- **Linux** needs WebKitGTK and a few libraries: `sudo apt install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.
- **macOS** needs the Xcode command line tools; **Windows** needs WebView2, already there on Windows 10 and 11.

```sh
cargo build -p ipse      # the command the app runs, from the repository's root
cd app
cargo run                # opens the app; a development build finds ../target/debug/ipse
cargo clippy --all-targets -- -D warnings
cargo test
```

The app looks for `ipse` next to its own program first (where the installers put it), then in `IPSE_BIN`. Its first screen is plain HTML in `app/welcome/`; the memory's interface is `ui/`, as in the browser.

## Releasing

Releases are built by [cargo-dist](https://opensource.axo.dev/cargo-dist/) (`.github/workflows/release.yml`, settings in `dist-workspace.toml`) when a `v*` tag is pushed. **Merging a release pull request is the decision to release**: `.github/workflows/tag.yml` makes the tag.

1. A pull request sets the version in `Cargo.toml` (`[workspace.package]`) and dates its CHANGELOG section: `## 0.2.0 - 2026-12-01`. A pre-release has its own version and section: `0.2.0-rc.1`. The tag must match the version exactly, or `release.yml` refuses it.
2. The owner merges it. On `main`, `tag.yml` sees a version without a tag and a dated section, and pushes `v0.2.0`.
3. `release.yml` builds the binaries for every system, then publishes a GitHub release with the installers, checksums and the changelog section as notes; a version like `0.2.0-rc.1` makes a pre-release.

Set up once by the owner: a [fine-grained token](https://github.com/settings/personal-access-tokens/new) with access to this repository only and the permission **Contents: read and write**, saved as the repository secret `RELEASE_TOKEN` (Settings → Secrets and variables → Actions). Without it, `tag.yml` only says the version is ready, and the owner pushes the tag by hand: `git tag v0.2.0 && git push origin v0.2.0`.
