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

1. A pull request sets the version in `Cargo.toml` (`[workspace.package]`), `app/Cargo.toml` and `app/tauri.conf.json` (CI checks they agree) and dates its CHANGELOG section: `## 0.2.0 - 2026-12-01`. A pre-release has its own version and section: `0.2.0-rc.1`. The tag must match the version exactly, or `release.yml` refuses it.
2. The owner merges it. On `main`, `tag.yml` sees a version without a tag and a dated section, and pushes `v0.2.0`.
3. `release.yml` builds the binaries for every system, then publishes a GitHub release with the installers, checksums and the changelog section as notes; a version like `0.2.0-rc.1` makes a pre-release.
4. `app-release.yml` builds the desktop app's installers on the same tag (`.dmg` for Apple silicon and Intel, `.msi`, `.AppImage` and `.deb`), each with the `ipse` command beside the app, and attaches them to that release once it exists.

Set up once by the owner: a [fine-grained token](https://github.com/settings/personal-access-tokens/new) with access to this repository only and the permission **Contents: read and write**, saved as the repository secret `RELEASE_TOKEN` (Settings → Secrets and variables → Actions). Without it, `tag.yml` only says the version is ready, and the owner pushes the tag by hand: `git tag v0.2.0 && git push origin v0.2.0`.

Signing the app (issue #257), set up by the owner as repository secrets and variables (Settings → Secrets and variables → Actions; secrets never in a file, a log or a conversation). Only builds of a tag are signed. Once a system is set up, `app-release.yml` checks each signature (the app, the `ipse` command inside it and the installer) and fails rather than publish a half-signed installer; the release notes then stop warning about that system.

- **macOS**: an [Apple Developer Program](https://developer.apple.com/programs/) membership (99 $ a year). In the developer account, a **Developer ID Application** certificate, exported from Keychain Access as a `.p12` with a password; an [app-specific password](https://account.apple.com) for the Apple ID. Six secrets, all needed: `APPLE_CERTIFICATE` (the `.p12` in base64: `base64 -i certificate.p12 | pbcopy`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` (like `Developer ID Application: Name (TEAMID)`, from `security find-identity -v -p codesigning`), `APPLE_ID`, `APPLE_PASSWORD` (the app-specific password) and `APPLE_TEAM_ID`. Tauri signs the app and `ipse` with the hardened runtime and notarizes the app; the workflow then signs, notarizes and staples the `.dmg`. Without them the app is signed ad hoc and not notarized: macOS asks to open it anyway, once.
- **Windows**, one of three, the first set up wins:
  - **SignPath** ([free for open-source projects](https://signpath.org), after applying): the secret `SIGNPATH_API_TOKEN` (a CI user's token) and the variable `SIGNPATH_ORGANIZATION_ID`. In SignPath, the project `ipse` with GitHub as its trusted build system, the signing policy `release-signing` and the artifact configuration `msi`, pasted from [`.signpath/artifact-configuration.xml`](.signpath/artifact-configuration.xml); other names go in the variables `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG` and `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG`. The workflow sends the built `.msi`, SignPath signs it and the `.exe` files inside, and the update's `.sig` is made again on the signed file.
  - **Azure Trusted Signing** (about 10 $ a month, for an identity Microsoft can validate): an app registration with the role **Trusted Signing Certificate Profile Signer** on the account. Secrets `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`; variables `AZURE_SIGNING_ENDPOINT` (like `https://weu.codesigning.azure.net`), `AZURE_SIGNING_ACCOUNT` and `AZURE_SIGNING_PROFILE`. Tauri signs through [`trusted-signing-cli`](https://github.com/Levminer/trusted-signing-cli).
  - **A certificate** that can be exported as a `.pfx` (rare since 2023: most now live on a hardware token or in the cloud): `WINDOWS_CERTIFICATE` (the `.pfx` in base64) and `WINDOWS_CERTIFICATE_PASSWORD`.

  Without any, SmartScreen asks to run the installer anyway. A newly signed installer may still be held by SmartScreen for its first downloads, until it builds a reputation.
- **Updates**: a key made once with `npx @tauri-apps/cli@2 signer generate -w ipse-updates.key`, kept somewhere safe; losing it means the installed apps can no longer update themselves. `TAURI_SIGNING_PRIVATE_KEY` (the content of `ipse-updates.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the public key (`ipse-updates.key.pub`) goes in `app/tauri.conf.json`, `plugins.updater.pubkey`. With them, each file of the app gets its signature and the release gets `latest.json` (`.github/scripts/update_manifest.py`), which the installed apps read to offer the new version. Without them, or without the public key, the app is not offered updates. `app-update.yml` checks on every change to the app that an old version updates itself, with a key made for the test.
