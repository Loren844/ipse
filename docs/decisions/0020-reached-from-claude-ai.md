# 0020 — Reached from claude.ai, over OAuth

- **Status:** Accepted
- **Date:** 2026-11-28
- **Refines:** 0019 (reached from the network)
- **Cites:** the threat model (`docs/guide/threat-model.md`), the MCP specification's authorization (OAuth 2.1, RFC 8414, RFC 7591, RFC 9728, PKCE)

## Context

Most conversations with an AI happen on claude.ai or in its phone app. They reach only remote MCP servers, over HTTPS, added as custom connectors that sign in with OAuth: an `ipse_…` code pasted in a header is not offered there (issue #270). Ipse runs on the person's own machine and has no account system to sign in to.

## Decision

- **`ipse serve` is its own authorization server**, as the MCP specification describes, for `/mcp` beside it: `/.well-known/oauth-protected-resource` and `/.well-known/oauth-authorization-server` say where its endpoints are, and `/mcp` answers a request without a code with `401` and `WWW-Authenticate: Bearer resource_metadata="…"`. Only `ipse serve` does it; `ipse mcp --http` and `ipse ui` stay on this computer, without OAuth.
- **Any app may register itself** (dynamic client registration), as a public client: no secret, `https` redirect addresses only (or this computer's). It gains nothing by it. Ipse keeps the 50 last registered, in `.ipse/oauth-clients.json`, which holds no secret.
- **Only the person approves**, on Ipse's own page, behind the interface's access code (decision 0019): they name the AI and tick what it may read, the sensitive categories closed unless they tick them. Approving gives the app a one-time code, valid 10 minutes, bound to it, its redirect address and its PKCE challenge (`S256` only). The page is never framed, and its form may send the browser only to that app's redirect address.
- **The access token is the AI's own access code.** Exchanging the code with its PKCE verifier connects the AI, as `ipse client add` does, and its code becomes the token. It does not expire, and there is no refresh token: like every other AI's code, it works until the person disconnects that AI in **Connected AIs**, which revokes it from its next request. It is never stored by Ipse, only its hash, as for every AI.
- **The AI is a connected AI like the others**: its rights per category, its proposals in the inbox, its reads in the access log, changed or disconnected in one click.
- **Reaching it from outside is the person's tunnel**: Tailscale Funnel or Cloudflare Tunnel, which give an HTTPS address without opening a port, as the self-hosting guide shows. `--public-url` (or `IPSE_PUBLIC_URL`) gives the address the apps see, otherwise read from each request's `Host`.

## Consequences

- With a tunnel, the memory is reached **from the internet**: by anyone who has the interface's code or an AI's code, and by the AI's provider (Anthropic for claude.ai), within the rights given to that AI. The threat model says so.
- A leaked token works until the AI is disconnected, as a leaked `ipse_…` code does today; short-lived tokens would need a refresh token kept by Ipse, for little gain while the person can see and cut each AI's access.
- The approval page is in the memory's language, apart from the interface; it uses no script.
