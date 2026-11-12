# 0008 — Automatic, within the rules

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

Most of a person's data sits in proprietary services. If filling the memory takes manual work, people give up. But several services forbid using their API data with AI (Strava, Spotify), and many forbid automated access to their site (Netflix forbids robots, spiders and scrapers).

## Decision

**No manual entry, no broken rule.** For each source, use the highest rung that is possible:

1. **Official API that allows our use** — fully automatic connector (calendar, contacts).
2. **Reading on the person's own device** — what the operating system already exposes, such as "now playing" media on Windows, Linux and Android, read locally without calling the service's API.
3. **A relay service the person chose** — for example a scrobbler (Last.fm, ListenBrainz) that already records their listening. Ipse states plainly what that service makes public.
4. **A file export reduced to one click** — Ipse reminds the person with a direct link, detects the downloaded file and imports it on its own.

Ipse **never** signs in to a service on the person's behalf, scrapes their account, or uses an API against its terms.

## Consequences

- Each connector's documentation names its rung and why a higher one is not possible.
- Rung 2 depends on the desktop app (milestone 3) and later on mobile; it cannot see playback on devices where Ipse does not run, and iOS does not allow it.
- Terms of service change: a connector found to break a rule is disabled until it is fixed.
