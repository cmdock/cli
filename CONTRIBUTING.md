# Contributing

Thanks for contributing to `cmdock-admin`.

## Build

Prerequisite:

- Rust toolchain
- `just` for the common local build and verification commands

Build locally with:

```bash
cargo build
cargo build --release
just build-release
```

## Test

Run the normal local checks before opening a PR:

```bash
cargo test --offline
cargo fmt --check
just check
```

If you change CLI behavior, update the docs and examples in the same change.
Deployed end-to-end testing of the standalone binary is owned by
`cmdock/server` staging verification rather than a separate CLI-local harness.

## Code Style

- Format with `cargo fmt`
- Keep warnings and lint noise low
- Follow the local ADR in [docs/adr/ADR-0001-cli-renders-server-truth.md](docs/adr/ADR-0001-cli-renders-server-truth.md)
- Keep the CLI thin: render server truth rather than re-implementing server policy locally

## PR Process

- Keep PRs scoped to one coherent behavior or documentation change
- Update [CHANGELOG.md](CHANGELOG.md) for user-visible changes under `[Unreleased]`
- Include tests or command-level verification where practical
- Expect review on CLI UX, contract alignment, and docs impact
- If you are preparing a public GitHub publication, follow
  [docs/reference/publication-workflow.md](docs/reference/publication-workflow.md)
  instead of mirroring internal `main` directly
- Treat internal release-engineering material as excluded from the public
  branch by default unless there is an explicit decision to publish it

## Issue Reporting

- Use the repository issue tracker for bugs, docs gaps, and contract-alignment work
- Include the command you ran, expected behavior, actual behavior, and any
  relevant server response details
- Link to the admin CLI contract when the issue is about API or UX expectations

## Licence

By contributing to this repo, you agree that your contributions are provided
under the repository licence: [MIT](LICENSE).
