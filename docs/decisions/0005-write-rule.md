# 0005 — Who writes to the memory

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

The person must keep control of their memory, but validating every calendar event by hand would make the app unusable.

## Decision

- Data a connector produces **deterministically** (a calendar event, a contact card) is written directly.
- Anything an **AI infers, merges or changes** is a **proposal** the person accepts or refuses, with a diff and undo.
- Letting an AI write on its own in some categories may come at milestone 4, as an explicit opt-in per category.

## Consequences

- Every write is traceable to its origin: a connector or an accepted proposal.
- The review inbox (milestone 2) is the single place where proposals wait.
