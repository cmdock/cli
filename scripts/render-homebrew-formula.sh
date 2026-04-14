#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "usage: $0 <version> <amd64-url> <amd64-sha256> <arm64-url> <arm64-sha256>" >&2
  exit 2
fi

VERSION="$1"
AMD64_URL="$2"
AMD64_SHA="$3"
ARM64_URL="$4"
ARM64_SHA="$5"

sed \
  -e "s|@VERSION@|${VERSION}|g" \
  -e "s|@AMD64_URL@|${AMD64_URL}|g" \
  -e "s|@AMD64_SHA256@|${AMD64_SHA}|g" \
  -e "s|@ARM64_URL@|${ARM64_URL}|g" \
  -e "s|@ARM64_SHA256@|${ARM64_SHA}|g" \
  packaging/homebrew/cmdock-admin.rb.template
