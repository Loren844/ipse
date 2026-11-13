# 0001 — Rust core, Tauri desktop, performance first

- **Status:** Accepted; refined by 0012 (the interface is served locally first)
- **Date:** 2026-11-12

## Context

Ipse indexes a whole life: thousands of entities, years of events, large time series such as GPS points. It ships as a local app and as an MCP server that people add to their AI client. A desktop app with a graph view comes at milestone 3, mobile at milestone 5. Code is written by coding agents and reviewed before merge.

## Decision

- The core (vault, raw journal, index, command line, MCP server) is written in **Rust**.
- The desktop app uses **Tauri**: its backend is Rust, so it uses the core directly, and its interface is web technology, best suited to an interactive graph.
- **Performance comes first.** The code aims for the best possible speed and memory use, verified by benchmarks that CI compiles on every pull request.
- The MCP server is built on **rmcp**, the official Rust SDK of the protocol.

## Consequences

- One self-contained binary, no runtime to install.
- The same core can later serve mobile through Tauri's iOS and Android support.
- Coding agents are less fluent in Rust than in TypeScript: tasks must be small, and the compiler, Clippy and tests act as the first reviewers.
- Slower compilation makes CI a little longer.
- Optimizations are guided by benchmarks (see `AGENTS.md`), so that complexity is only added where it measurably pays.
