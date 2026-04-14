#!/bin/bash
# deploy.sh -- build-and-push helper for cmdock-admin
#
# Builds the release binary locally, copies it to a target host via scp, and
# installs it at /usr/local/bin/cmdock-admin. The script owns only the
# standalone CLI artifact flow; it does not manage server runtime, ingress,
# tokens, or any sibling services.
#
# Target hostnames are sourced from environment variables so the script is
# safe to publish:
#
#   CMDOCK_CLI_STAGING_SSH  SSH host for --target staging (default: staging.example.com)
#   CMDOCK_CLI_DOGFOOD_SSH  SSH host for --target dogfood (default: dogfood.example.com)
#
# Both variables are typically sourced from a gitignored .env.local so the
# real hostnames never hit version control.
#
# Usage:
#   ./scripts/deploy.sh local              # Build + push cmdock-admin to staging
#   ./scripts/deploy.sh local dogfood      # Build + push cmdock-admin to dogfood
#   ./scripts/deploy.sh status             # Show staging cmdock-admin status
#   ./scripts/deploy.sh status dogfood     # Show dogfood cmdock-admin status

set -euo pipefail

# Load repo-local overrides (gitignored). .env.local defines the real
# staging/dogfood SSH hostnames; the placeholder values below keep this
# script safe to publish.
if [[ -f ".env.local" ]]; then
  # shellcheck disable=SC1091
  set -a; source ./.env.local; set +a
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

LOCAL_BINARY="$PROJECT_DIR/target/release/cmdock-admin"
INSTALL_PATH="/usr/local/bin/cmdock-admin"

GREEN='\033[0;32m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${BOLD}==> ${NC}$*"; }
ok()    { echo -e "  ${GREEN}✓${NC} $*"; }
fail()  { echo -e "  ${RED}✗${NC} $*"; }

target_ssh_host() {
    case "${1:-staging}" in
        staging) echo "${CMDOCK_CLI_STAGING_SSH:-staging.example.com}" ;;
        dogfood) echo "${CMDOCK_CLI_DOGFOOD_SSH:-dogfood.example.com}" ;;
        *)
            fail "Unknown target: ${1:-}"
            exit 1
            ;;
    esac
}

SSH_OPTS=(-o ControlMaster=no)
SCP_OPTS=(-o ControlMaster=no)

build_binary() {
    info "Building cmdock-admin release binary..."
    cd "$PROJECT_DIR"
    cargo build --release --locked
    ok "Binary built: $LOCAL_BINARY"
}

push_local() {
    local target="${1:-staging}"
    local ssh_host
    ssh_host="$(target_ssh_host "$target")"

    build_binary

    info "Pushing cmdock-admin to $target via scp..."
    scp "${SCP_OPTS[@]}" "$LOCAL_BINARY" "$ssh_host:/tmp/cmdock-admin"
    ssh "${SSH_OPTS[@]}" "$ssh_host" \
        "sudo install -m 0755 /tmp/cmdock-admin '$INSTALL_PATH' && rm -f /tmp/cmdock-admin"
    ok "Binary installed on $target: $INSTALL_PATH"

    show_status "$target"
}

show_status() {
    local target="${1:-staging}"
    local ssh_host
    ssh_host="$(target_ssh_host "$target")"

    info "$target cmdock-admin status:"
    ssh "${SSH_OPTS[@]}" "$ssh_host" "
        set -e
        command -v cmdock-admin
        cmdock-admin --version
        sha256sum '$INSTALL_PATH'
    "
}

case "${1:-help}" in
    local)
        push_local "${2:-staging}"
        ;;
    status)
        show_status "${2:-staging}"
        ;;
    *)
        echo "Usage: $0 <command> [options]"
        echo ""
        echo "Deploy commands:"
        echo "  local [target]     Build + push cmdock-admin to target via scp/install (default: staging)"
        echo ""
        echo "Inspection commands:"
        echo "  status [target]    Show installed cmdock-admin path, version, and sha256"
        ;;
esac
