# Packaging

This repo ships a small standalone Rust binary. The packaging pipeline is split
into two parts:

- `.gitea/workflows/ci.yml` runs format, test, install, and package-readiness checks
- `.gitea/workflows/release-artifacts.yml` builds tagged release artifacts for the
  supported OS/arch matrix and uploads them as workflow artifacts

## Release Artifacts

Tagged builds (`v*`) produce:

- `cmdock-admin-v<version>-linux-amd64.tar.gz`
- `cmdock-admin-v<version>-linux-arm64.tar.gz`
- `cmdock-admin-v<version>-macos-amd64.tar.gz`
- `cmdock-admin-v<version>-macos-arm64.tar.gz`
- `cmdock-admin-v<version>-windows-amd64.tar.gz`

Each archive is accompanied by a `.sha256` file.

The workflow currently expects runners for:

- `ubuntu-latest`
- `ubuntu-24.04-arm64`
- `macos-13`
- `macos-14`
- `windows-latest`

If a runner label is not available yet, the corresponding matrix job will stay
pending until ops adds that runner.

## Manual Packaging

To package the current local build:

```bash
cargo build --release --locked
./scripts/package-release.sh 0.1.0 linux-amd64 target/release/cmdock-admin
```

## Homebrew Formula

The Homebrew formula in this repo is a template for the tap repo. Render it with:

```bash
./scripts/render-homebrew-formula.sh \
  0.1.0 \
  https://example.invalid/cmdock-admin-v0.1.0-macos-amd64.tar.gz \
  <amd64-sha256> \
  https://example.invalid/cmdock-admin-v0.1.0-macos-arm64.tar.gz \
  <arm64-sha256>
```

That prints a ready-to-commit formula for `cmdock/tap`.
