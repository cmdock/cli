# Glossary

This glossary defines the repo-local terms used by `cmdock-admin`.

This file is the local authority for CLI-specific operator terminology.

- **Admin API**
  The HTTPS `cmdock/server` operator API under `/admin/*` that
  `cmdock-admin` calls.

- **Backup staging directory**
  The server-local `backup_dir` where snapshot backups are written before the
  operator copies them off-host.

- **Doctor**
  The CLI command that checks operator-facing server health such as admin API
  reachability, TLS expiry, user enumeration, sync activity visibility, and
  backup visibility when the server exposes those endpoints.

- **Operator token**
  The bearer token used to authenticate `cmdock-admin` to the server admin API.
  This is not the same as a user API token used by an end-user client.

- **Safety snapshot**
  An automatic `pre-restore-*` snapshot created by the server before a restore
  attempt so it can roll back if the restore fails partway through.

- **Server truth**
  The rule that the CLI should render server-owned contract responses rather
  than recompute policy locally.

- **Standalone admin CLI**
  The `cmdock-admin` Rust binary shipped from this repo. It is separate from
  `cmdock-server`, speaks only HTTPS to the admin API, and does not access
  SQLite files directly.
