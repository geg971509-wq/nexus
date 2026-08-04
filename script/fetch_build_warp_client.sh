#!/usr/bin/env bash
# Fetch a pinned warp-client tag from its own repo and build the companion binary.
# warp-client was extracted from Throne into a standalone repo; Throne no longer
# vendors its source. Pin the version here (one line) to upgrade.
set -euo pipefail

WARP_REPO="${WARP_CLIENT_REPO:-http://192.168.8.2:3000/osaka/warp-client.git}"
WARP_REF="${WARP_CLIENT_REF:-v0.1.0}"

# Args from CMake: <src_dir> <out_binary>
SRC_DIR="$1"
OUT_BIN="$2"

if [[ -d "$SRC_DIR/.git" ]]; then
    # Offline-tolerant: if the fetch fails (no network/repo), proceed as long as
    # the pinned ref is already available locally.
    git -C "$SRC_DIR" fetch --tags --quiet origin || true
else
    rm -rf "$SRC_DIR"
    git clone --quiet "$WARP_REPO" "$SRC_DIR"
fi
git -C "$SRC_DIR" -c advice.detachedHead=false checkout --quiet "$WARP_REF"

# Skip the rebuild when the existing binary was already built from this ref.
STAMP_FILE="$SRC_DIR/bin/.built-ref"
if [[ -x "$OUT_BIN" && -f "$STAMP_FILE" && "$(cat "$STAMP_FILE")" == "$WARP_REF" ]]; then
    exit 0
fi

mkdir -p "$(dirname "$OUT_BIN")"
CGO_ENABLED=0 go -C "$SRC_DIR" build -trimpath -o "$OUT_BIN" ./cmd/warp-client
echo "$WARP_REF" > "$STAMP_FILE"
