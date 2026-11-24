# Location history connector

Imports where you have been from your phone's location history (Google Maps Timeline), so that your memory knows the places you go and when you were there.

Without exporting a file: connect your own Home Assistant and turn on its phone's positions (`docs/connectors/homeassistant.md`); its stays are the same `location.visit` records, and land in the vault in the same way.

| | |
| --- | --- |
| Id | `google-timeline` |
| Rung (decision 0008) | 4, file export: the phone writes the file; no account or network access is needed. |
| Modes | `import` |
| Access | the file the person picks; no network; no secret |
| Category | `location`, **closed to AIs by default** (`docs/spec/rights.md`) |
| Kinds | `location.visit`, `location.points`, `location.place` |

## Getting the file

Since 2024, Google keeps the Timeline on the phone only. Export it from the phone:

- **Android**: Settings → Location → Location services → Timeline → **Export Timeline data**. The file is `Timeline.json`.
- **iPhone**: Google Maps → your picture → Your Timeline → ⋯ → Location and privacy settings → **Export Timeline data**. The file is `location-history.json`.
- **Google Takeout** (an older export, before the Timeline moved to the phone): the `Semantic Location History/<year>/<year>_<MONTH>.json` files and `Records.json` are read too.

Then: `ipse import Timeline.json --vault ~/Ipse`. Ipse recognizes the file by its content, whatever its name; a JSON file that is none of these is refused with a message that says how to export one.

## What is read

| Format | Visits | Points | Places |
| --- | --- | --- | --- |
| Android (`semanticSegments`) | `visit`: start, end, `topCandidate.placeId`, `semanticType`, position, probability | `timelinePath`, and `rawSignals` positions with their accuracy | `userLocationProfile.frequentPlaces` (home, work) |
| iPhone (a top-level array) | `visit`: start, end, `topCandidate.placeID`, `semanticType`, `placeLocation` (`geo:lat,lon`) | `timelinePath` (offsets in minutes from the segment's start) | — |
| Takeout semantic history | `placeVisit`: `duration`, `location.placeId`, **name and address**, `semanticType` | — (see `Records.json`) | — |
| Takeout `Records.json` | — | `locations`, coordinates in E7, with their accuracy | — |

The file is read as a stream: a year of history (about 40 MB, 400 000 points) is never held in memory whole. Activities (walking, driving), their distances and their paths in the Takeout semantic history are left out; so are segments the file cannot place in time.

## Records

All records are `upsert`, so importing the same file again changes nothing, and importing a newer export adds only what is new.

**`location.visit`**, one per stay. `external_id`: `visit:<start, Unix seconds>:<place id, or lat,lon>`.

| Field | Type | From |
| --- | --- | --- |
| `start`, `end` | RFC 3339 timestamps | the segment's times |
| `lat`, `lon` | degrees, 6 decimals | the place's position |
| `place_id` | text | Google's place identifier, when there is one |
| `name`, `address` | text | Takeout only; the phone exports have none |
| `semantic` | `home` or `work` | `semanticType` (`HOME`, `INFERRED_HOME`, `WORK`…); absent for any other place |
| `probability` | 0–1, 2 decimals | how sure Google is of the place |

**`location.points`**, the path between stays and the phone's raw positions, in batches of at most one hour and 10 000 points. `external_id`: `points:<first time>:<size>`. `data`: `{"t": [...], "lat": [...], "lon": [...], "acc": [...]}`, columns of the same length (`acc` in meters, `null` when unknown). Points stay in the journal: they are never written to the vault.

**`location.place`**, one per frequent place of the Android profile. `external_id`: `place:<place id>`. `data`: `place_id`, `label` (`home`, `work`), `lat`, `lon`.

A final `progress` message says how many visits and positions were read.

## In the vault

**Places.** A visited place is found again, in this order:

1. by Google's identifier, kept in the place file's `sources` (`google-timeline:place:<id>`);
2. by its position: an existing place within 75 meters is the same place, whoever wrote it;
3. by the agenda: the place an overlapping event already links to, which then gets the position and Google's identifier.

Otherwise Ipse creates a file in `places/`, in the `location` category, named as the export says (Takeout), else "Domicile" / "Travail" for home and work, else the address of an overlapping agenda event, else "Lieu près de 48.8566, 2.3522". The person can rename it; Ipse finds it again by its identifier. A place the person deleted is not created again by an older visit, only by a visit newer than the deletion.

**Visits.**

- A stay at home or at work is not an event: it only counts in the state log.
- A stay that overlaps an event of the agenda (by at least half of the shorter of the two) gives that event its place, when the event has none or only an address. The calendar stays the source of the event.
- Any other stay becomes an event in `events/`, in the `location` category, titled with the place's name, with the place as its `place`. It is updated when a newer export changes the stay.

**State log.** `states/location/<year>.md` holds one row per place: first and last day seen, how many stays (`docs/spec/vault.md`, section 5). It is rewritten from every visit of the year at each import.

The person's page shows the places where they meet someone ("Où vous vous voyez"), from the events they share that have a place.

## Rights

Everything this connector writes is in the `location` category, which AIs cannot see until the person opens it (`docs/spec/rights.md`). Opened, an AI can find the visits among the events, and ask for events `with` a place as well as with a person. An agenda event that only received its place keeps its own category: the place's name becomes visible through it, the place's file does not.

## Volumes

A year of history from an Android phone (1 825 stays, 394 200 points, 39 MB) is imported in about 1.1 s end to end, connector, journal and vault together, and imported again in about 1.1 s (`cargo bench -p ipse --bench timeline`). Reading the file dominates both.
