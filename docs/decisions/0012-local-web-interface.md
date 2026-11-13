# 0012 — The interface is a local web app, packaged with Tauri later

- **Status:** Accepted
- **Date:** 2026-11-13
- **Refines:** 0001 (Tauri desktop)

## Context

Milestone 2 brings the review inbox: the person's AI proposes, the person accepts or refuses. That needs a screen; doing it on the command line would be thrown away. Decision 0001 chose Tauri for the desktop app at milestone 3, and a Tauri app is a web interface in a native window, so the interface itself is the same work either way.

What Tauri adds is packaging: installers for three systems, code signing (paid on macOS, required to avoid warnings on Windows and macOS), automatic updates, a tray icon, starting with the computer. That work improves nothing while the interface is still taking shape and Ipse is installed with `cargo install` by technical users.

A version hosted by the project, where people would put their memory online, was also considered.

## Decision

- **One interface, written as a web app**, in the repository.
- **Milestone 2: served locally.** `ipse ui` serves it on this computer and opens the browser. It is the way to build and test the interface quickly, in a real browser, in CI.
- **Milestone 3: packaged with Tauri.** The same interface becomes the desktop app, for everyone: installer, signature, updates, tray icon, background sync. The desktop app is the product for non-technical people (decision 0007); the browser version stays for developers and for computers where the app is not installed.
- **Never hosted by the project.** Ipse does not run a service that holds people's memories. Reaching one's own Ipse from another device (a home server, a phone) is for after 1.0, with real authentication and encryption, like remote access for online AIs (`docs/spec/rights.md`, section 3).

### Protecting the local server

A server on `127.0.0.1` can still be reached by any site open in the same browser. From the first version:

- it listens on `127.0.0.1` only, on a port chosen at start;
- `ipse ui` opens the browser with a one-time code in the address, exchanged for a session cookie (`HttpOnly`, `SameSite=Strict`); every request without it is refused;
- the `Host` and `Origin` headers are checked, against DNS rebinding and cross-site requests;
- the interface goes through the same query layer as AIs, with the owner's own rights, and writes only through the same paths as the command line.

## Consequences

- The interface is written once and works in both containers.
- The packaging work moves to milestone 3 without losing anything.
- At milestone 2, using the interface means running `ipse ui`; acceptable while installation is still `cargo install`.
- The choice of web framework for the interface is made at the start of milestone 2, in its own decision, with bundle size and speed measured.
