# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.2.0] - 2026-04-27

### Added
- `--scheme` flag on `connect config` for staging-specific URL schemes
  (e.g. `cmdock-staging://`). Required for the iOS staging build, which
  registers a dedicated URL scheme separate from the production app. Mirrors
  `cmdock/server#80`.

### Changed
- Reworked the root documentation set to match the shared documentation standards.
- Added a contribution guide and aligned the README with the standard open-source template.

## [0.1.0] - 2026-04-06

### Added
- Initial release of the standalone `cmdock-admin` operator CLI.
- Setup, doctor, backup, restore, user management, and connect workflows over the admin HTTPS API.
- Local docs set for the standalone CLI, including ADR and glossary coverage.
