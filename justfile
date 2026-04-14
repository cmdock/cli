# cmdock-admin justfile
#
# These recipes own only the standalone admin CLI artifact flow — build,
# test, and deploy-to-target. Target hostnames are loaded from .env.local
# (gitignored) via direnv so this file is safe to publish. Copy
# .env.local.example → .env.local and fill in real values for deploy
# recipes that need a real target.

set dotenv-filename := ".env.local"
set dotenv-required := false

staging_ssh := env_var_or_default("CMDOCK_CLI_STAGING_SSH", "staging.example.com")
dogfood_ssh := env_var_or_default("CMDOCK_CLI_DOGFOOD_SSH", "dogfood.example.com")

# Default: show available recipes
default:
    @just --list

# Build debug
build:
    cargo build

# Build release
build-release:
    cargo build --release --locked

# Run tests
test:
    cargo test --offline

# Format code
fmt:
    cargo fmt

# Check formatting
fmt-check:
    cargo fmt -- --check

# Run all quality checks
check: fmt-check test

# Deploy current local cmdock-admin binary to staging
deploy-staging:
    ./scripts/deploy.sh local staging

# Deploy current local cmdock-admin binary to dogfood
deploy-dogfood:
    ./scripts/deploy.sh local dogfood

# Show deployed staging cmdock-admin status
status-staging:
    ./scripts/deploy.sh status staging

# Show deployed dogfood cmdock-admin status
status-dogfood:
    ./scripts/deploy.sh status dogfood
