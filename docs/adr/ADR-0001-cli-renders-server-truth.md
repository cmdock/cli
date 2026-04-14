---
created: 2026-04-06
status: accepted
tags: [architecture, principles, quality]
---

# ADR-0001: CLI Renders Server Truth

## Status

**Accepted**

## Context

`cmdock-admin` is a thin standalone operator CLI for `cmdock/server`.

Its job is to:

- call the admin HTTP API
- present operator-facing output
- provide light interaction such as confirmations and tables

It is not a second implementation of server policy. Simplicity drift in this
repo usually shows up when the CLI starts inferring or recomputing server truth
instead of rendering the contract it receives.

## Decision

Within `cmdock/cli`, simplicity means:

- prefer server-owned response shapes over client-side inference
- keep command modules thin and feature-oriented
- avoid local domain rules when the server can own them once
- keep shared helpers generic; keep feature logic in the feature module

### Specific rules

1. The CLI should not compute compatibility judgments that the server already
   validates.
2. Confirmation prompts may use snapshot metadata when available, but should
   degrade gracefully instead of rebuilding server policy locally.
3. If the CLI needs a richer summary, prefer a simpler server response contract
   over adding local heuristics or multiple preflight calls.
4. `main.rs` is command wiring, not the home for feature-specific reporting
   systems.

## Consequences

- Backup and doctor logic should live in dedicated modules.
- Future admin features should be added as feature modules rather than growing
  `main.rs`.
- When review finds the CLI narrating server semantics instead of rendering
  them, that is considered simplicity drift.
