# 0004 — Any LLM, local or online

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

Ipse needs a language model to extract facts from raw data and to consolidate summaries. Fully local models protect privacy but need strong hardware for reliable structured output; online models are better and cheaper to start with but receive personal data.

## Decision

- Ipse talks to models through **one common interface**, so any LLM can be plugged in, local or online.
- **Local by default** when a local model is configured.
- An **online model receives data only after the person explicitly enables it**, and can be limited to some categories.
- AIs that *read* the memory connect through MCP, which is already model-independent.

## Consequences

- No feature may depend on a specific provider's API.
- Development can start with an online model without breaking the "local first" principle.
- The settings must make clear, in plain words, what leaves the machine.
