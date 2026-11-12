# 0009 — Encryption: disk and keychain

- **Status:** Accepted
- **Date:** 2026-11-12

## Context

A vault holding a whole life is a target. Encrypting every file would protect it at rest, but encrypted files can no longer be read or edited in Obsidian or any other editor, which breaks the "open formats" principle and the promise that the person can always read their memory without Ipse.

## Decision

- The vault's files are **plain Markdown**. Protection at rest relies on the **operating system's disk encryption** (FileVault, BitLocker, LUKS); Ipse checks whether it is on and, if not, explains in plain words how to turn it on.
- **Secrets** (connector tokens, API keys for online models) are never written in the vault: they go to the **system keychain** (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux).
- An **optional encrypted vault** may come after 1.0, for people who accept losing direct editing.

## Consequences

- Someone with access to the unlocked computer can read the vault, as with any document on it.
- Data that leaves the machine (sync, online models) is covered by its own rules: decision 0004 for models, end-to-end encryption for future sync.
