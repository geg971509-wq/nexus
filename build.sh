#!/usr/bin/env bash
# Nexus macOS product build: NexusCore + Qt Quick .app
# Always full rebuild. No flags. Windows is not a product this round.
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
    *) die "host build.sh is macOS-only (got $os)" ;;
  esac
}

[[ $# -eq 0 ]] || die "usage: ./build.sh  (no flags — always full release rebuild)"

[[ "$(uname -s)" == "Darwin" ]] || die "macOS only host"
need go
need cargo
need cmake
need rustc
# Both sides generate from core/server/gen/libcore.proto: Go here, Rust in build.rs.
need protoc
if ! xcode-select -p >/dev/null 2>&1; then
  die "Xcode CLT not configured (xcode-select -p)"
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
TRIPLE="$(target_triple)"
log "root=$ROOT triple=$TRIPLE (mac release rebuild)"

# Feature tags required in NexusCore (stubs if missing).
CORE_TAGS_BASE="with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_naive_outbound,badlinkname,tfogo_checklinkname0"
CORE_TAGS_MAC="${NEXUS_CORE_TAGS:-$CORE_TAGS_BASE}"
IFS=',' read -ra CORE_REQUIRED_TAGS <<< "$CORE_TAGS_BASE"
mkdir -p "$BIN_DIR" "$BINARIES_DIR"

verify_core_binary() {
  local bin="$1"
  local label="${2:-NexusCore}"
  [[ -f "$bin" ]] || die "$label missing: $bin"
  local sz
  sz="$(stat -f%z "$bin" 2>/dev/null || stat -c%s "$bin")"
  if [[ "$sz" -lt 50000000 ]]; then
    die "$label too small (${sz} bytes) — sing-box feature tags likely missing."
  fi
  local meta
  meta="$(go version -m "$bin" 2>/dev/null || true)"
  [[ -n "$meta" ]] || die "go version -m failed on $bin (not a Go binary?)"
  echo "$meta" | grep -q 'github.com/sagernet/sing-box' || die "$label missing module: sing-box"
  echo "$meta" | grep -q 'github.com/xtls/xray-core' || die "$label missing module: xray-core"
  echo "$meta" | grep -q 'github.com/sagernet/sing-tun' || die "$label missing module: sing-tun"
  echo "$meta" | grep -qE 'gvisor|github.com/sagernet/gvisor' || die "$label missing gvisor (with_gvisor tag?)"
  local tagline
  tagline="$(echo "$meta" | grep -E '^\s*build\s+-tags=' | head -1 || true)"
  [[ -n "$tagline" ]] || die "$label has no build -tags= metadata"
  local t
  for t in "${CORE_REQUIRED_TAGS[@]}"; do
    echo "$tagline" | grep -q "$t" || die "$label missing required -tag: $t (got: $tagline)"
  done
  ok "$label verified · $(numfmt --to=iec "$sz" 2>/dev/null || echo "${sz}B") · tags+sing-box+xray+sing-tun"
}

[[ -d "$CORE_SRC" ]] || die "core source missing: $CORE_SRC"
[[ -f "$CORE_SRC/go.mod" ]] || die "missing $CORE_SRC/go.mod"
[[ -f "$CORE_SRC/gen/libcore.proto" ]] || die "missing $CORE_SRC/gen/libcore.proto"

# --- 0) protobuf: generate Go stubs from the same .proto the Rust build.rs uses.
log "generating Go protobuf stubs…"
bash "$ROOT/script/gen-proto.sh" || die "protobuf generation failed"
ok "protobuf stubs generated"

# --- 0b) sing-box patch: stage a writable copy, patch it, replace via overlay modfile.
# The shared cache under ~/go/pkg/mod must stay byte-identical or `go mod verify`
# fails for every other project on this machine. Staging keeps the patch local to
# this build; go.patched.mod is gitignored so the committed go.mod stays upstream.
SINGBOX_STAGE="$BIN_DIR/singbox-patched"
log "staging patched sing-box…"
(
  cd "$CORE_SRC"
  go mod download github.com/sagernet/sing-box
  src="$(go list -m -f '{{if .Replace}}{{.Replace.Dir}}{{else}}{{.Dir}}{{end}}' github.com/sagernet/sing-box)"
  [[ -d "$src" ]] || die "cannot locate sing-box module dir"
  rm -rf "$SINGBOX_STAGE"
  mkdir -p "$SINGBOX_STAGE"
  # Cache dirs are 0555; copy then restore write permission on the copy only.
  cp -R "$src/." "$SINGBOX_STAGE/"
  chmod -R u+w "$SINGBOX_STAGE"
  python3 "$ROOT/script/patches/sing-box-darwin-process-id.py" "$SINGBOX_STAGE" \
    || die "sing-box patch failed to apply (upstream moved? see script/patches/)"

  cp -f go.mod go.patched.mod
  cp -f go.sum go.patched.sum
  # -replace overwrites the existing Throneproj replace; appending would conflict.
  go mod edit -modfile=go.patched.mod \
    -replace "github.com/sagernet/sing-box=$SINGBOX_STAGE"
)
export GOFLAGS="${GOFLAGS:+$GOFLAGS }-modfile=go.patched.mod"
ok "sing-box patched → $SINGBOX_STAGE"

# --- 1a) NexusCore macOS host ---
log "building NexusCore macOS (go · tags=$CORE_TAGS_MAC)…"
(
  cd "$CORE_SRC"
  go mod download
  go mod verify || die "go mod verify failed (module cache is corrupt or tampered)"

  VERSION_SINGBOX="$(go list -m -f '{{.Version}}' github.com/sagernet/sing-box)"
  [[ -n "$VERSION_SINGBOX" ]] || die "could not resolve github.com/sagernet/sing-box version from go.mod"
  log "sing-box: $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/sagernet/sing-box)"
  log "xray-core: $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/xtls/xray-core)"
  log "sing-tun:  $(go list -m -f '{{.Path}} {{.Version}}{{if .Replace}} => {{.Replace.Path}}{{end}}' github.com/sagernet/sing-tun)"
  log "wireguard: $(go list -m -f '{{.Path}}{{if .Replace}} => {{.Replace.Path}} {{.Replace.Version}}{{end}}' github.com/sagernet/wireguard-go 2>/dev/null || echo 'via sing-box')"

  export CGO_ENABLED="${CGO_ENABLED:-1}"
  SDKROOT="$(xcrun --sdk macosx --show-sdk-path 2>/dev/null || true)"
  CLANG="$(xcrun --sdk macosx --find clang 2>/dev/null || true)"
  [[ -n "$CLANG" ]] && export CC="$CLANG"
  if [[ -n "$SDKROOT" ]]; then
    export CGO_CFLAGS="${CGO_CFLAGS:+$CGO_CFLAGS }-isysroot $SDKROOT -mmacosx-version-min=${MACOSX_DEPLOYMENT_TARGET}"
    export CGO_LDFLAGS="${CGO_LDFLAGS:--weak_framework UniformTypeIdentifiers} -isysroot $SDKROOT -mmacosx-version-min=${MACOSX_DEPLOYMENT_TARGET}"
  else
    export CGO_LDFLAGS="${CGO_LDFLAGS:--weak_framework UniformTypeIdentifiers}"
  fi

  go build -trimpath \
    -tags "$CORE_TAGS_MAC" \
    -ldflags "-s -w -X 'github.com/sagernet/sing-box/constant.Version=${VERSION_SINGBOX}' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0" \
    -o "$CORE_OUT" \
    .
)
ok "NexusCore → $CORE_OUT"
verify_core_binary "$CORE_OUT" "NexusCore(mac)"
chmod +x "$CORE_OUT"

STAGED="$BINARIES_DIR/NexusCore-${TRIPLE}"
cp -f "$CORE_OUT" "$STAGED"
chmod +x "$STAGED"
ok "staged $STAGED"

# --- 2) Qt Quick host (mac .app) ---
QT_DIR="$APP_DIR/qt"
QT_BUILD="$QT_DIR/build"
[[ -f "$QT_DIR/CMakeLists.txt" ]] || die "missing Qt host: $QT_DIR/CMakeLists.txt"
[[ -f "$QT_DIR/Info.plist" ]] || die "missing $QT_DIR/Info.plist"
log "cmake Qt host…"
cmake -S "$QT_DIR" -B "$QT_BUILD" -DCMAKE_BUILD_TYPE=Release
cmake --build "$QT_BUILD" --target nexus
QT_BIN="$QT_BUILD/nexus"
[[ -x "$QT_BIN" ]] || die "qt host missing: $QT_BIN"
ok "qt host → $QT_BIN"

log "building nexusfwd…"
(cd "$TAURI_DIR" && cargo build --release --bin nexusfwd)
FWD_BIN="$TAURI_DIR/target/release/nexusfwd"
[[ -x "$FWD_BIN" ]] || die "nexusfwd missing: $FWD_BIN"
ok "nexusfwd → $FWD_BIN"

DEST_APP="$BIN_DIR/Nexus.app"
rm -rf "$DEST_APP"
mkdir -p "$DEST_APP/Contents/MacOS" "$DEST_APP/Contents/Resources"
cp -f "$QT_BIN" "$DEST_APP/Contents/MacOS/nexus"
chmod +x "$DEST_APP/Contents/MacOS/nexus"
cp -f "$CORE_OUT" "$DEST_APP/Contents/MacOS/NexusCore"
chmod +x "$DEST_APP/Contents/MacOS/NexusCore"
cp -f "$FWD_BIN" "$DEST_APP/Contents/MacOS/nexusfwd"
chmod +x "$DEST_APP/Contents/MacOS/nexusfwd"
cp -f "$QT_DIR/Info.plist" "$DEST_APP/Contents/Info.plist"
cp -f "$TAURI_DIR/icons/icon.icns" "$DEST_APP/Contents/Resources/icon.icns"
ok "staged $DEST_APP"

echo
ok "build complete"
echo "  mac app:  $DEST_APP"
echo "  mac core: $CORE_OUT"
echo "  run mac:  open \"$DEST_APP\""
