#!/usr/bin/env bash
# Nexus macOS app build — always full rebuild: NexusCore (Go) + Tauri 2 shell
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$ROOT/app"
TAURI_DIR="$APP_DIR/src-tauri"
CORE_SRC="$ROOT/core/server"
BIN_DIR="$ROOT/bin"
CORE_OUT="$BIN_DIR/NexusCore"
BINARIES_DIR="$TAURI_DIR/binaries"

export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

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

[[ $# -eq 0 ]] || die "usage: ./build.sh  (no flags — always full release rebuild)"

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
log "root=$ROOT triple=$TRIPLE (full release rebuild)"

# Feature tags required in NexusCore (stubs if missing)
CORE_TAGS="${NEXUS_CORE_TAGS:-with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_naive_outbound,badlinkname,tfogo_checklinkname0}"
CORE_REQUIRED_TAGS=(with_clash_api with_gvisor with_quic with_wireguard with_utls with_dhcp with_tailscale with_naive_outbound badlinkname tfogo_checklinkname0)
mkdir -p "$BIN_DIR" "$BINARIES_DIR"

verify_core_binary() {
  local bin="$1"
  [[ -f "$bin" && -x "$bin" ]] || die "NexusCore not executable: $bin"
  local sz
  sz="$(stat -f%z "$bin" 2>/dev/null || stat -c%s "$bin")"
  if [[ "$sz" -lt 50000000 ]]; then
    die "NexusCore too small (${sz} bytes) — sing-box feature tags likely missing."
  fi
  local meta
  meta="$(go version -m "$bin" 2>/dev/null || true)"
  [[ -n "$meta" ]] || die "go version -m failed on $bin (not a Go binary?)"
  echo "$meta" | grep -q 'github.com/sagernet/sing-box' || die "NexusCore missing module: sing-box"
  echo "$meta" | grep -q 'github.com/xtls/xray-core' || die "NexusCore missing module: xray-core"
  echo "$meta" | grep -q 'github.com/sagernet/sing-tun' || die "NexusCore missing module: sing-tun"
  echo "$meta" | grep -qE 'gvisor|github.com/sagernet/gvisor' || die "NexusCore missing gvisor (with_gvisor tag?)"
  local tagline
  tagline="$(echo "$meta" | grep -E '^\s*build\s+-tags=' | head -1 || true)"
  [[ -n "$tagline" ]] || die "NexusCore has no build -tags= metadata"
  local t
  for t in "${CORE_REQUIRED_TAGS[@]}"; do
    echo "$tagline" | grep -q "$t" || die "NexusCore missing required -tag: $t (got: $tagline)"
  done
  ok "NexusCore verified · $(numfmt --to=iec "$sz" 2>/dev/null || echo "${sz}B") · tags+sing-box+xray+sing-tun linked"
}

# --- 1) NexusCore ---
[[ -d "$CORE_SRC" ]] || die "core source missing: $CORE_SRC"
[[ -f "$CORE_SRC/go.mod" ]] || die "missing $CORE_SRC/go.mod"
[[ -d "$CORE_SRC/third_party/sing-tun" ]] || die "missing local replace path: $CORE_SRC/third_party/sing-tun"
[[ -f "$CORE_SRC/third_party/sing-tun/go.mod" ]] || die "incomplete sing-tun vendored tree"
[[ -f "$CORE_SRC/gen/libcore.pb.go" ]] || die "missing $CORE_SRC/gen/libcore.pb.go (pre-generated protobuf)"

log "building NexusCore (go · tags=$CORE_TAGS)…"
(
  cd "$CORE_SRC"
  go mod download
  go mod verify

  VERSION_SINGBOX="$(go list -m -f '{{.Version}}' github.com/sagernet/sing-box)"
  [[ -n "$VERSION_SINGBOX" ]] || die "could not resolve github.com/sagernet/sing-box version from go.mod"
  log "sing-box: $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/sagernet/sing-box)"
  log "xray-core: $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/xtls/xray-core)"
  log "sing-tun:  $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}}{{end}}' github.com/sagernet/sing-tun)"
  log "wireguard: $(go list -m -f '{{.Path}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/sagernet/wireguard-go 2>/dev/null || echo 'via sing-box')"

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

  go build -trimpath \
    -tags "$CORE_TAGS" \
    -ldflags "-s -w -X 'github.com/sagernet/sing-box/constant.Version=${VERSION_SINGBOX}' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0" \
    -o "$CORE_OUT" \
    .
)
ok "NexusCore → $CORE_OUT"
verify_core_binary "$CORE_OUT"
chmod +x "$CORE_OUT"

STAGED="$BINARIES_DIR/NexusCore-${TRIPLE}"
cp -f "$CORE_OUT" "$STAGED"
chmod +x "$STAGED"
ok "staged $STAGED"

# --- 2) frontend deps ---
log "npm install…"
(cd "$APP_DIR" && npm install)

TAURI_CLI=(npx --no-install tauri)
if ! (cd "$APP_DIR" && npx --no-install tauri --version >/dev/null 2>&1); then
  if command -v cargo-tauri >/dev/null 2>&1 || cargo tauri --version >/dev/null 2>&1; then
    TAURI_CLI=(cargo tauri)
  else
    log "installing @tauri-apps/cli in app/…"
    (cd "$APP_DIR" && npm install --save-dev @tauri-apps/cli@^2)
  fi
fi

# --- 2.5) UI staging: index.html + assets ---
UI_SRC="$APP_DIR/ui/index.html"
[[ -f "$UI_SRC" ]] || die "missing UI source: $UI_SRC"
UI_STAGE="$TAURI_DIR/ui-release-dist"
rm -rf "$UI_STAGE"
mkdir -p "$UI_STAGE"
cp "$UI_SRC" "$UI_STAGE/index.html"
if [[ -d "$APP_DIR/ui/assets" ]]; then
  cp -R "$APP_DIR/ui/assets" "$UI_STAGE/assets"
fi
UI_CONF_OVERRIDE="$TAURI_DIR/tauri.release-ui.json"
cat > "$UI_CONF_OVERRIDE" <<EOF
{
  "build": {
    "frontendDist": "./ui-release-dist"
  }
}
EOF
ok "UI release staging · $UI_STAGE"

# --- 3) tauri release build ---
log "tauri build (release)…"
(
  cd "$APP_DIR"
  export NEXUS_CORE_BIN="$CORE_OUT"
  "${TAURI_CLI[@]}" build --config "$UI_CONF_OVERRIDE"
)

# --- 4) locate .app ---
APP_CANDIDATES=(
  "$TAURI_DIR/target/release/bundle/macos/Nexus.app"
  "$TAURI_DIR/target/${TRIPLE}/release/bundle/macos/Nexus.app"
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

MACOS_DIR="$APP_PATH/Contents/MacOS"
if [[ -d "$MACOS_DIR" ]]; then
  cp -f "$CORE_OUT" "$MACOS_DIR/NexusCore"
  chmod +x "$MACOS_DIR/NexusCore"
  ok "embedded $MACOS_DIR/NexusCore"
fi

DEST_APP="$BIN_DIR/Nexus.app"
rm -rf "$DEST_APP"
cp -R "$APP_PATH" "$DEST_APP"
ok "copied → $DEST_APP"

html_count="$(find "$DEST_APP" -name '*.html' 2>/dev/null | wc -l | tr -d ' ')"
[[ "$html_count" -le 1 ]] || die "release app has unexpected HTML count=$html_count"
ok "bundle UI clean · html_count=$html_count"

echo
ok "build complete"
echo "  app:  $DEST_APP"
echo "  also: $APP_PATH"
echo "  core: $CORE_OUT"
echo "  run:  open \"$DEST_APP\""
