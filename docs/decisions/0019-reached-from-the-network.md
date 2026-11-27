# 0019 — Reached from the network, for a home server

- **Status:** Accepted
- **Date:** 2026-11-24
- **Refines:** 0009 (encryption: disk and keychain), 0012 (local web interface)
- **Cites:** the threat model (`docs/guide/threat-model.md`)

## Context

People who self-host want Ipse on their NAS or home server, without a desktop: the memory there, the sources syncing in the background, their AI apps and their browser reaching it from their other machines (issue #224). Until now `ipse ui` and `ipse mcp --http` refuse any address but `127.0.0.1`, on purpose: nothing could reach the memory but this computer.

A container has no keychain, and no person at its terminal to open a one-time link: the interface's one-time code and the system keychain, which protect the desktop, do not fit there.

## Decision

- **One command for a server, `ipse serve`:** the interface at `/`, MCP at `/mcp`, and the background sync of the followed sources, in one process, on one port (7420 by default).
- **Listening beyond this computer is off by default.** `ipse serve` listens on `127.0.0.1` unless it is given another address with `--listen`, like `--listen 0.0.0.0:7420`; it then says at start that the memory can be reached from the network. `ipse ui` and `ipse mcp --http` stay on this computer only.
- **The interface asks for Ipse's own access code**, as MCP asks each AI for its own. `ipse access-code` makes it, shows it once, and keeps only its BLAKE3 hash in `.ipse/interface.toml`; running it again replaces it and closes every session opened with the old one. The interface shows a login page; the code entered there opens a session cookie (`HttpOnly`, `SameSite=Strict`, `Secure` behind HTTPS, 30 days). A wrong code waits a second before its answer. No code made yet: the login page says how to make one, and nothing else of the interface opens.
- **The `Host` a request names is not checked** by `ipse serve`, as the person's own address for it (a name on their network, their proxy's domain) is not known; DNS rebinding gains nothing, as every page needs the session and every MCP request an AI's code. A browser's `Origin` must be the address the request was sent to; MCP refuses any browser's `Origin`.
- **HTTPS is left to the person's reverse proxy** (Caddy, Traefik), with a guide. Ipse does not serve it. Over plain HTTP the codes cross the network readable by anyone on it: the guide says so, and keeps plain HTTP for a network the person trusts.
- **Secrets in a file, on a server only.** When `IPSE_SECRETS_FILE` names a file, the secrets of the sources (calendar links, app passwords, tokens) are kept there, readable by its owner only, instead of in the system keychain. The Docker image sets it to `/secrets/secrets.json`, a volume of its own, outside the memory folder: a copy of the memory carries no secret.
- **An image `ghcr.io/loren844/ipse`** for `linux/amd64` and `linux/arm64`, built with each release: the static binary alone on a distroless base, as a user who is not root, the memory as the volume `/vault`.

## Consequences

- The threat model says what changes: with `--listen`, anyone on the network who has the interface's code or an AI's code reaches the memory as that code allows; without HTTPS, anyone on the network can read what is sent.
- The disk the server keeps the memory and the secrets file on is the person's to encrypt (decision 0009 still holds for the files).
- Sessions live in the server's memory: restarting it closes them, and the person logs in again.
- `ipse serve` answers every route of the interface's API as `ipse ui` does, with the owner's rights: the interface's code is the owner's key, to keep like a password.
