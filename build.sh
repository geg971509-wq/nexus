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
  NEXUS_CORE_BIN    override core path at runtime (absolute file only)
  NEXUS_CORE_TAGS   override go build -tags (default: Throne-aligned sing-box features)
  MACOSX_DEPLOYMENT_TARGET  default 12.0

Core deps (all linked into bin/NexusCore via go build -tags; no separate installers):
  sing-box     → go.mod replace Throneproj/sing-box
  xray-core    → go.mod replace throneproj/xray-core
  sing-tun     → go.mod replace ./third_party/sing-tun (required in tree)
  wireguard-go → go.mod replace throneproj/wireguard-go
  Feature tags: clash_api gvisor quic wireguard utls dhcp tailscale naive_outbound
  Post-build: go version -m verifies modules + tags; rejects <50MB stubs
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

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
TRIPLE="$(target_triple)"
log "root=$ROOT triple=$TRIPLE debug=$DEBUG"

# --- 1) NexusCore (sing-box + xray + sing-tun + WG fully linked in one static-ish Go binary) ---
# Throne oracle: script/build_go.sh (darwin) + build.sh build_core tags/ldflags.
# Without these -tags, gVisor/WG/QUIC/uTLS/Tailscale/Naive are compile-time stubs or missing.
# All deps come from go.mod + replace pins (no separate sing-box/xray installers).
CORE_TAGS="${NEXUS_CORE_TAGS:-with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_naive_outbound,badlinkname,tfogo_checklinkname0}"
# Required feature tags that must appear in the built binary (subset check; order free)
CORE_REQUIRED_TAGS=(with_clash_api with_gvisor with_quic with_wireguard with_utls with_dhcp with_tailscale with_naive_outbound badlinkname tfogo_checklinkname0)
mkdir -p "$BIN_DIR" "$BINARIES_DIR"

verify_core_binary() {
  # Fail loud if someone reused a stripped/incomplete Core without sing-box stack.
  local bin="$1"
  [[ -f "$bin" && -x "$bin" ]] || die "NexusCore not executable: $bin"
  local sz
  sz="$(stat -f%z "$bin" 2>/dev/null || stat -c%s "$bin")"
  # Full-tag Core is ~65–80MB on arm64; bare CheckConfig-only builds were ~40MB.
  if [[ "$sz" -lt 50000000 ]]; then
    die "NexusCore too small (${sz} bytes) — sing-box feature tags likely missing. Rebuild without a stub binary."
  fi
  local meta
  meta="$(go version -m "$bin" 2>/dev/null || true)"
  [[ -n "$meta" ]] || die "go version -m failed on $bin (not a Go binary?)"
  # modules (path may show replace arrow on next line; match dep path)
  echo "$meta" | grep -q 'github.com/sagernet/sing-box' || die "NexusCore missing module: sing-box"
  echo "$meta" | grep -q 'github.com/xtls/xray-core' || die "NexusCore missing module: xray-core"
  echo "$meta" | grep -q 'github.com/sagernet/sing-tun' || die "NexusCore missing module: sing-tun"
  echo "$meta" | grep -qE 'gvisor|github.com/sagernet/gvisor' || die "NexusCore missing gvisor (with_gvisor tag?)"
  # build tags line
  local tagline
  tagline="$(echo "$meta" | grep -E '^\s*build\s+-tags=' | head -1 || true)"
  [[ -n "$tagline" ]] || die "NexusCore has no build -tags= metadata"
  local t
  for t in "${CORE_REQUIRED_TAGS[@]}"; do
    echo "$tagline" | grep -q "$t" || die "NexusCore missing required -tag: $t (got: $tagline)"
  done
  ok "NexusCore verified · $(numfmt --to=iec "$sz" 2>/dev/null || echo "${sz}B") · tags+sing-box+xray+sing-tun linked"
}

if $SKIP_CORE && [[ -f "$CORE_OUT" ]]; then
  log "reuse $CORE_OUT (will still verify tags/modules)"
  verify_core_binary "$CORE_OUT"
else
  [[ -d "$CORE_SRC" ]] || die "core source missing: $CORE_SRC"
  [[ -f "$CORE_SRC/go.mod" ]] || die "missing $CORE_SRC/go.mod"
  # go.mod replaces (must be in tree / resolvable):
  #   sing-box  → github.com/Throneproj/sing-box
  #   xray-core → github.com/throneproj/xray-core
  #   sing-tun  → ./third_party/sing-tun
  #   wireguard-go → github.com/throneproj/wireguard-go
  [[ -d "$CORE_SRC/third_party/sing-tun" ]] || die "missing local replace path: $CORE_SRC/third_party/sing-tun"
  [[ -f "$CORE_SRC/third_party/sing-tun/go.mod" ]] || die "incomplete sing-tun vendored tree"
  # generated protos must exist (Nexus ships pre-generated gen/; Throne may regen via protoc)
  [[ -f "$CORE_SRC/gen/libcore.pb.go" ]] || die "missing $CORE_SRC/gen/libcore.pb.go (pre-generated protobuf)"

  log "building NexusCore (go · tags=$CORE_TAGS)…"
  (
    cd "$CORE_SRC"
    # Pull full module graph into module cache (sing-box, xray, quic, gvisor, tailscale, …)
    go mod download
    go mod verify

    VERSION_SINGBOX="$(go list -m -f '{{.Version}}' github.com/sagernet/sing-box)"
    [[ -n "$VERSION_SINGBOX" ]] || die "could not resolve github.com/sagernet/sing-box version from go.mod"
    log "sing-box: $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/sagernet/sing-box)"
    log "xray-core: $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/xtls/xray-core)"
    log "sing-tun:  $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}}{{end}}' github.com/sagernet/sing-tun)"
    log "wireguard: $(go list -m -f '{{.Path}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/sagernet/wireguard-go 2>/dev/null || echo 'via sing-box')"

    # darwin CGO for tun/stack — align with Throne build.sh (SDK + deployment target) + build_go.sh weak UTTypes
    export CGO_ENABLED="${CGO_ENABLED:-1}"
    if [[ "$(uname -s)" == "Darwin" ]]; then
      SDKROOT="$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"
      CLANG="$(xcrun --sdk macosx --find clang 2>/dev/null || true)"
      [[ -n "$CLANG" ]] && export CC="$CLANG"
      if [[ -n "$SDKROOT" ]]; then
        export CGO_CFLAGS="${CGO_CFLAGS:+$CGO_CFLAGS }-isysroot $SDKROOT -mmacosx-version-min=${MACOSX_DEPLOYMENT_TARGET}"
        export CGO_LDFLAGS="${CGO_LDFLAGS:--weak_framework UniformTypeIdentifiers} -isysroot $SDKROOT -mmacosx-version-min=${MACOSX_DEPLOYMENT_TARGET}"
      else
        export CGO_LDFLAGS="${CGO_LDFLAGS:--weak_framework UniformTypeIdentifiers}"
      fi
    fi

    # -checklinkname=0 required for gVisor / badlinkname tags on modern Go
    go build -trimpath \
      -tags "$CORE_TAGS" \
      -ldflags "-s -w -X 'github.com/sagernet/sing-box/constant.Version=${VERSION_SINGBOX}' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0" \
      -o "$CORE_OUT" \
      .
  )
  ok "NexusCore → $CORE_OUT (sing-box+xray+sing-tun compiled in)"
  verify_core_binary "$CORE_OUT"
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
echo "  note: connect = Start(LoadConfigReq); needs real share link on node"

if $OPEN_AFTER; then
  open "$DEST_APP"
fi
