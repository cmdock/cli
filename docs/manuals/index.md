# cmdock-admin Docs

This repo is intentionally small. The local docs set covers repo-local
behavior and terminology for the standalone operator CLI.

`cmdock-admin` is the recommended routine operator tool for live self-hosted
deployments. It does not replace `cmdock-server admin`, which remains the
local self-sufficiency and break-glass surface owned by `cmdock/server`.

## Read This First

1. [README](../../README.md)
   Install, configure, and run the shipped commands.
2. [Glossary](../reference/glossary.md)
   Repo-local operator and contract terms used throughout the CLI.
3. [ADR-0001: CLI Renders Server Truth](../adr/ADR-0001-cli-renders-server-truth.md)
   Local design rule for keeping the CLI thin and contract-driven.

## Contract Boundary

`cmdock-admin` follows the admin API shape exposed by `cmdock/server`. This
local docs set explains the CLI-side behavior and terminology without assuming
access to private cross-repo design material.

## Testing Boundary

This repo owns Rust-level CLI verification such as `cargo test`.

Deployed end-to-end verification of the shipped `cmdock-admin` binary is owned
by `cmdock/server` staging verification, which installs this binary onto the
operator host and exercises it against a live server.
