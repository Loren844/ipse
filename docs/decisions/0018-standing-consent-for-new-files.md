# 0018 — Standing consent for new files

- **Status:** Accepted
- **Date:** 2026-11-24
- **Refines:** 0017 (standing consent for low-risk changes)
- **Cites:** 0005 (who writes to the memory), 0011 (no silent merge), 0014 (imported data is untrusted)

## Context

The person wants to tell their AI *"remember my PC setup, my dev environments"* and find a real file of their memory afterwards: a `thing` "Mon PC" with its facts, a `topic` "Environnements de dev". An AI can already propose such a `create`, but it waits in the inbox, and decision 0017 explicitly kept "any file created" out of standing consent. The owner asked to let an AI create these files without validating each, to keep things fluid (issue #207).

Decision 0014 is still the risk: text from outside (an invitation, a contact's note) reaches the AI and could ask it to write something. A file about a thing or a topic says nothing about who the person knows, where they go or when; a person, a place or an event does.

## Decision

- **A third standing consent, `creations`**, per AI, off by default, turned on by the person in **IA connectées** like the two of decision 0017, and offered after the person accepted three such proposals of that AI one by one.
- **It covers a proposal made only of:**
  - `create` of a `thing`, `topic`, `project`, `organization` or `work`;
  - `add_fact` and `add_relation` on a file of those types that this same AI created, by a proposal still accepted (not undone).
- **Reminders too** (issue #206): `create` of a `reminder`, and `set_key` of `done` or `at` on any reminder, as "remind me" and "done" are the person's own words to their AI.
- **Only in a known category that is not sensitive.** A sensitive category, or one the vault does not declare, waits for the person. The AI still needs the `propose` right on the category, as for any proposal.
- **Never without the person:** people, places, events; facts on a file the AI did not create; any other change to its own files (a key, an edited or removed fact, notes); merges (decision 0011), deletions, renames, summaries outside their task. A proposal mixing an allowed change with any other waits as before.
- **The same path as 0017:** saved, then accepted at once by the code that accepts the person's choice; one change of the history, marked `automatic`, undone in one click. The home screen says how many files the AI created in the last days, with a link.

## Consequences

- `Standing::Creations`, written `creations` in `auto_accept` of `.ipse/clients.toml`.
- No task is needed: the person asks their AI in conversation, and it calls `propose`. The server's instructions tell the AI to `search` first, and to add a fact to the file it finds rather than create a second one.
- The AI cannot create a category. A proposal into a new category is left for later: it would wait for the person, who would create the category by accepting it.
- Text from outside could now make a file about a thing or a topic appear without the person reading it first; it names no one in the person's life, sits in a category the person opened to that AI, and shows on the home screen and in the history, to be undone in one click.
