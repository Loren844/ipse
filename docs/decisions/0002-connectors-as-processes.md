# 0002 — Connectors run as separate processes

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

Connectors are where most outside contributions will come from, and fewer people write Rust than other languages. A connector also handles untrusted data from third-party services and can fail in many ways.

## Decision

Each connector is a **separate program** that the core starts and talks to through a **JSON protocol over standard input and output**. The exact protocol is defined in the specification.

## Consequences

- A connector can be written in any language; first-party connectors are written in Rust.
- A crashing or misbehaving connector cannot take the core down, and can be given only the access it needs.
- Data crosses a process boundary, which costs some serialization; connectors therefore send batches, not single records.
