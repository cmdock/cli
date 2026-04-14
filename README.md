# cmdock-admin

Standalone admin CLI for self-hosted `cmdock/server` deployments.

**Status:** Alpha. Core operator flows are shipped for setup, doctor, backup,
restore, connect, user management, and admin webhook management over the admin
HTTPS API.

This repo owns the standalone operator CLI surface for the server admin API.
The server owns runtime behavior and validation; this CLI renders and drives
that API over HTTPS.

## Quick Start

```bash
cargo install --path .
cmdock-admin --server https://tasks.example.com --token "$CMDOCK_ADMIN_TOKEN" doctor
cmdock-admin --server https://tasks.example.com --token "$CMDOCK_ADMIN_TOKEN" user list
```

The CLI is intentionally standalone:

- Rust binary
- HTTPS admin API only
- no SQLite
- no TaskChampion
- no dependency on the server binary

## Installation

Install from a local checkout:

```bash
cargo install --path .
```

Tagged builds produce release archives through the packaging pipeline. See
[packaging/README.md](packaging/README.md) for the release-artifact workflow and
Homebrew formula handoff.

Local verification in this repo is `cargo test`. Deployed end-to-end operator
testing for the shipped standalone binary runs through the staging verification
owned by `cmdock/server`, which installs and exercises `cmdock-admin` against a
live server.

## Configuration

`cmdock-admin` reads configuration in this order:

1. `--server` / `--token`
2. `CMDOCK_ADMIN_SERVER` / `CMDOCK_ADMIN_TOKEN`
3. `~/.config/cmdock-admin/config.toml`

Example:

```toml
server_url = "https://tasks.example.com"
admin_token = "your-admin-token"
```

## Usage

Current first slice:

- `cmdock-admin setup`
- `cmdock-admin doctor`
- `cmdock-admin backup`
- `cmdock-admin backup list`
- `cmdock-admin backup restore <timestamp>`
- `cmdock-admin user list`
- `cmdock-admin user create <name>`
- `cmdock-admin user delete <name>`
- `cmdock-admin connect --taskwarrior`
- `cmdock-admin setup --qr`
- `cmdock-admin connect <user> --qr`
- `cmdock-admin webhook list`
- `cmdock-admin webhook create --url <url> --secret <secret> --events <events>`
- `cmdock-admin webhook test <id>`
- `cmdock-admin webhook deliveries <id>`
- `cmdock-admin webhook enable <id>`
- `cmdock-admin webhook disable <id>`

QR notes:

- `setup --qr` works because setup already has the created/reused `user_id`
- `connect --qr` accepts either a username or a user ID
- username lookup uses `GET /admin/users`

```bash
cmdock-admin setup
cmdock-admin doctor
cmdock-admin backup
cmdock-admin backup --include-secrets
cmdock-admin backup list
cmdock-admin backup restore 2026-04-06T10-00-00
cmdock-admin user list
cmdock-admin user create simon
cmdock-admin user delete simon --yes
cmdock-admin connect simon --taskwarrior
cmdock-admin setup --qr
cmdock-admin connect simon --qr
cmdock-admin webhook list
cmdock-admin webhook create --url https://hooks.example.com/cmdock --secret whsec_abcdefghijklmnopqrstuvwxyz012345 --events task.created,task.completed
cmdock-admin webhook deliveries awh_123
cmdock-admin webhook test awh_123
```

## Documentation

- [Docs index](docs/manuals/index.md)
- [Glossary](docs/reference/glossary.md)
- [CLI simplicity ADR](docs/adr/ADR-0001-cli-renders-server-truth.md)
- Server-side admin API behavior is documented by `cmdock/server`

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for build, test, style, and PR
expectations.

## Licence

MIT — see [LICENSE](LICENSE).
