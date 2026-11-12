# 0010 — The person's own AI does the work

- **Status:** Accepted
- **Date:** 2026-11-12
- **Supersedes:** [0004](0004-model-independence.md)

## Context

Decision 0004 had Ipse call a language model the person configures, to extract facts, write summaries and ask review questions. That keeps Ipse independent of any provider, but it still asks the person to set up a model inside Ipse (an API key or a local model to install), which goes against decision 0007. Most people already use an AI they like.

## Decision

- **Ipse has no AI of its own by default.** It is the memory, not the assistant.
- **Reading:** any AI that speaks MCP reads the memory, within its rights (`rights.md`). Ipse does not know or care which AI it is.
- **Processing:** the work that needs a language model is exposed to the person's AI as **tasks** through MCP: new data to turn into memory, questions to ask, summaries to write. The AI reads a task and submits **proposals**; the person accepts or refuses them in the review inbox (decision 0005). The person simply asks their AI to "tidy up my Ipse".
- **When the AI app supports MCP sampling**, Ipse may ask the person's model to write a text (a summary) directly, if the person enabled it for that AI.
- **Optional:** a person who wants processing to run on its own, every night, may plug a model directly into Ipse (any provider, local or online, through one common interface). An online model receives data only after the person explicitly enables it.
- **Building blocks for automation:** the same operations are available as MCP tools and as `ipse` commands, so a person can have an agent or a script run them on a schedule.

## Consequences

- Ipse works with zero AI configuration; nothing leaves the machine unless the person's own AI asks for it, within its rights.
- Without a model plugged into Ipse, processing happens when the person asks their AI, not in the background.
- Proposal quality depends on the person's AI; the review inbox stays the safeguard.
- Task tools must be designed for any AI to understand: short descriptions, structured items, examples of good proposals.
