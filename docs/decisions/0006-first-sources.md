# 0006 — First sources: calendar, contacts, location

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

The first milestone needs real data to prove that an AI connected to Ipse knows its user. Strava was considered first, but its API agreement forbids using data obtained through its API in AI models or similar applications, which is exactly what Ipse does.

## Decision

- Milestone 1 ships two connectors: **calendar** and **contacts**.
- **Location history** is the next source: an import of the Google Maps Timeline export (now stored on the phone and exported as JSON), with live tracking later.
- **Strava is set aside.** Sport data may come back later through files the person owns (their Strava archive, GPX or FIT files), not through the Strava API.

## Consequences

- Contacts create "person" entities early, so duplicate detection (one of the main risks) is faced from the start.
- File imports avoid authentication work in the first milestone.
- The first milestone is reached when a connected AI correctly answers questions about the person's calendar and contacts, without seeing the categories it is denied.
