# 0017 — Standing consent for low-risk changes

- **Status:** Accepted
- **Date:** 2026-11-23
- **Refines:** 0005 (who writes to the memory)
- **Cites:** 0010 (the person's own AI does the work), 0014 (imported data is untrusted)

## Context

Decision 0005 makes anything an AI infers a proposal the person accepts. Since decision 0010, the person's own AI writes a summary of every day, week, month and year, and gives events a kind (decision 0016). The owner reviewed these one by one for weeks and found the inbox heavy: summaries and kinds are many, they are almost always accepted as they are, and a wrong one harms little. The owner asked to give the OK once, for one AI and one kind of change, and agreed that it must stay theirs to give and to take back.

Decision 0014 is the risk: text written outside Ipse (an invitation's title, a contact's note) reaches the AI, and could ask it to write something. A standing consent must not let such text change who the person knows or what the memory says about them.

## Decision

- **The person may accept in advance, for one AI, two kinds of change only:**
  - **summaries** of a day, week, month or year, in a proposal made only of summaries, for a `summary` task Ipse gave that AI;
  - **event kinds**, in a proposal made only of the `kind` key of events, for a `classify` task Ipse gave that AI.
- **Never** for anything else: people, places or any file created, facts, relations, merges, other keys, notes, and nothing the AI did not get from a task. A proposal mixing an allowed change with any other waits as before.
- **Off by default, per AI, turned on by the person** in **IA connectées**, one switch per kind, saying what it does and that each change can be undone. An AI cannot turn it on, and no command line or file does it for the person without their act. The interface offers it once the person accepted the last three such proposals of that AI one by one, never before; a "no thanks", or turning it off, is remembered and not offered again.
- **The same path as an accepted proposal**: the proposal is saved, then accepted at once by the code that accepts the person's choice: one change of the history, `Ipse-Source: proposal:<id>`, the proposal marked `automatic`, its task done. It can be undone in one click; the inbox does not wait on it; the home screen says how many were accepted automatically in the last days, with a link.
- Decision 0005's "letting an AI write on its own in some categories" stays for later: this is not a category opened to an AI, but two kinds of change it makes for Ipse's own tasks.

## Consequences

- `.ipse/clients.toml` keeps, per AI, `auto_accept` (the kinds on) and `declined` (the kinds not to offer again).
- The `propose` tool answers `accepted` instead of `waiting` for such a proposal (`docs/spec/rights.md`, section 4.2).
- Text from outside that steers a summary can now reach the memory without the person reading it first, but only as a summary or a kind, which says nothing on its own about anyone's file; the person sees it on the home screen and in the summaries, and undoes it in one click.
- A later kind of change is added only by a new decision.
