#!/usr/bin/env bash
# Nexus macOS app build — Go NexusCore + Tauri 2 shell
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$ROOT/app"
TAURI_DIR="$APP_DIR/src-tauri"
CORE_SRC="$ROOT/core/server"
BIN_DIR="$ROOT/bin"
CORE_OUT="$BIN_DIR/NexusCore"
# Tauri externalBin: binaries/<name>-<target-triple>
BINARIES_DIR="$TAURI_DIR/binaries"

export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

SKIP_CORE=false
SKIP_NPM=false
DEBUG=false
OPEN_AFTER=false

usage() {
  cat <<'EOF'
Usage: ./build.sh [options]

  Build Nexus.app (macOS): NexusCore (Go) + Tauri shell.

  --debug       cargo/tauri debug profile (default: release)
  --skip-core   reuse existing bin/NexusCore
  --skip-npm    skip npm install if node_modules present (still runs if missing)
  --open        open the built .app when done
  -h, --help    this help

Env:
  NEXUS_CORE_BIN   override core path at runtime (absolute file only)
  MACOSX_DEPLOYMENT_TARGET  default 11.0
EOF
}

log()  { printf '\033[36m[INFO]\033[0m %s\n' "$*"; }
ok()   { printf '\033[32m[OK]\033[0m %s\n' "$*"; }
err()  { printf '\033[31m[ERR]\033[0m %s\n' "$*" >&2; }
die()  { err "$*"; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing command: $1"
}

target_triple() {
  local arch os
  arch="$(uname -m)"
  case "$arch" in
    arm64|aarch64) arch="aarch64" ;;
    x86_64) arch="x86_64" ;;
    *) die "unsupported arch: $arch" ;;
  esac
  os="$(uname -s)"
  case "$os" in
    Darwin) echo "${arch}-apple-darwin" ;;
    *) die "this build.sh is macOS-only (got $os)" ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug) DEBUG=true; shift ;;
    --skip-core) SKIP_CORE=true; shift ;;
    --skip-npm) SKIP_NPM=true; shift ;;
    --open) OPEN_AFTER=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done

[[ "$(uname -s)" == "Darwin" ]] || die "macOS only"
need go
need cargo
need npm
need rustc
if ! xcode-select -p >/dev/null 2>&1; then
  die "Xcode CLT not configured (xcode-select -p)"
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
TRIPLE="$(target_triple)"
log "root=$ROOT triple=$TRIPLE debug=$DEBUG"

# --- 1) NexusCore ---
mkdir -p "$BIN_DIR" "$BINARIES_DIR"
if $SKIP_CORE && [[ -f "$CORE_OUT" ]]; then
  log "reuse $CORE_OUT"
else
  log "building NexusCore (go)…"
  (
    cd "$CORE_SRC"
    # CGO often needed for tun/stack bits on macOS
    CGO_ENABLED="${CGO_ENABLED:-1}" go build -trimpath -ldflags="-s -w" -o "$CORE_OUT" .
  )
  ok "NexusCore → $CORE_OUT"
fi
[[ -f "$CORE_OUT" ]] || die "NexusCore missing at $CORE_OUT"
chmod +x "$CORE_OUT"

# Stage for Tauri externalBin (name must match tauri.conf externalBin entry)
STAGED="$BINARIES_DIR/NexusCore-${TRIPLE}"
cp -f "$CORE_OUT" "$STAGED"
chmod +x "$STAGED"
ok "staged $STAGED"

# --- 2) frontend deps ---
if [[ ! -d "$APP_DIR/node_modules" ]] || ! $SKIP_NPM; then
  if [[ -d "$APP_DIR/node_modules" ]] && $SKIP_NPM; then
    log "node_modules present, --skip-npm"
  else
    log "npm install…"
    (cd "$APP_DIR" && npm install)
  fi
else
  log "skip npm install"
fi

# Prefer local CLI
TAURI_CLI=(npx --no-install tauri)
if ! (cd "$APP_DIR" && npx --no-install tauri --version >/dev/null 2>&1); then
  if command -v cargo-tauri >/dev/null 2>&1 || cargo tauri --version >/dev/null 2>&1; then
    TAURI_CLI=(cargo tauri)
  else
    log "installing @tauri-apps/cli in app/…"
    (cd "$APP_DIR" && npm install --save-dev @tauri-apps/cli@^2)
  fi
fi

# --- 3) tauri build ---
log "tauri build…"
BUILD_ARGS=(build)
if $DEBUG; then
  BUILD_ARGS+=(--debug)
fi
(
  cd "$APP_DIR"
  # ensure shell can find core during any build-time checks
  export NEXUS_CORE_BIN="$CORE_OUT"
  "${TAURI_CLI[@]}" "${BUILD_ARGS[@]}"
)

# --- 4) locate .app ---
PROFILE="release"
$DEBUG && PROFILE="debug"
APP_CANDIDATES=(
  "$TAURI_DIR/target/${PROFILE}/bundle/macos/Nexus.app"
  "$TAURI_DIR/target/${TRIPLE}/${PROFILE}/bundle/macos/Nexus.app"
)
APP_PATH=""
for c in "${APP_CANDIDATES[@]}"; do
  if [[ -d "$c" ]]; then
    APP_PATH="$c"
    break
  fi
done
if [[ -z "$APP_PATH" ]]; then
  APP_PATH="$(find "$TAURI_DIR/target" -type d -name 'Nexus.app' 2>/dev/null | head -1 || true)"
fi
[[ -n "$APP_PATH" && -d "$APP_PATH" ]] || die "Nexus.app not found under $TAURI_DIR/target"

# Ensure NexusCore sits next to the main binary (MacOS/) even if externalBin naming differs
MACOS_DIR="$APP_PATH/Contents/MacOS"
if [[ -d "$MACOS_DIR" ]]; then
  cp -f "$CORE_OUT" "$MACOS_DIR/NexusCore"
  chmod +x "$MACOS_DIR/NexusCore"
  ok "embedded $MACOS_DIR/NexusCore"

  # Bundle official Cloudflare warp-cli (not full WARP.app)
  WARP_STAGE="$ROOT/third_party/cloudflare-warp/warp-cli"
  WARP_SYS="/Applications/Cloudflare WARP.app/Contents/Resources/warp-cli"
  if [[ ! -f "$WARP_STAGE" && -f "$WARP_SYS" ]]; then
    mkdir -p "$(dirname "$WARP_STAGE")"
    cp -f "$WARP_SYS" "$WARP_STAGE"
    chmod +x "$WARP_STAGE"
    log "staged warp-cli from system WARP.app → $WARP_STAGE"
  fi
  if [[ -f "$WARP_STAGE" ]]; then
    cp -f "$WARP_STAGE" "$MACOS_DIR/warp-cli"
    chmod +x "$MACOS_DIR/warp-cli"
    ok "embedded $MACOS_DIR/warp-cli"
  else
    log "warp-cli not staged (third_party/cloudflare-warp or system WARP.app missing)"
  fi
fi

# Convenience copy under repo bin/
DEST_APP="$BIN_DIR/Nexus.app"
rm -rf "$DEST_APP"
cp -R "$APP_PATH" "$DEST_APP"
ok "copied → $DEST_APP"

echo
ok "build complete"
echo "  app:  $DEST_APP"
echo "  also: $APP_PATH"
echo "  core: $CORE_OUT"
echo "  run:  open \"$DEST_APP\""
echo "  note: connect is still CheckConfig-only until Start() is wired"

if $OPEN_AFTER; then
  open "$DEST_APP"
fi
