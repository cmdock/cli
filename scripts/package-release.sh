#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <version> <artifact-suffix> <binary-path>" >&2
  exit 2
fi

VERSION="$1"
ARTIFACT_SUFFIX="$2"
BINARY_PATH="$3"
OUT_DIR="dist"
ARCHIVE_BASENAME="cmdock-admin-v${VERSION}-${ARTIFACT_SUFFIX}"
STAGE_DIR="${OUT_DIR}/${ARCHIVE_BASENAME}"

if [ ! -f "$BINARY_PATH" ]; then
  echo "binary not found: $BINARY_PATH" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

cp README.md "$STAGE_DIR/"
cp LICENSE "$STAGE_DIR/"
cp "$BINARY_PATH" "$STAGE_DIR/$(basename "$BINARY_PATH")"

tar -C "$OUT_DIR" -czf "${OUT_DIR}/${ARCHIVE_BASENAME}.tar.gz" "$ARCHIVE_BASENAME"

(
  cd "$OUT_DIR"
  sha256sum "${ARCHIVE_BASENAME}.tar.gz" > "${ARCHIVE_BASENAME}.sha256"
)

echo "${OUT_DIR}/${ARCHIVE_BASENAME}.tar.gz"
