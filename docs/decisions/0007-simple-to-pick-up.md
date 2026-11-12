# 0007 — Simple to pick up

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

Ipse is meant for people, not only developers. Its competitors are either developer building blocks or tools that require editing files by hand. The inside will be complex; the outside must not be.

## Decision

Ease of use is a product requirement, checked like a feature:

- **Install in one step**, with no runtime, database or account to set up.
- **Useful without configuration**: sensible defaults everywhere, every setting optional.
- **One connector at a time**: each connector works on its own and explains in one sentence what it brings.
- **No technical word in the interface**: no "git", "commit", "index" or "MCP" shown to the person; "history", "undo" and "connect an AI" instead.
- **Connecting an AI takes one action**: Ipse shows exactly what to copy or click for common AI clients.
- **Every error says what to do next**, in plain words.

## Consequences

- When ease of use and a feature conflict, the feature waits until it can be made simple.
- Each user-facing issue states how a non-technical person will use it.
