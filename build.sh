#!/usr/bin/env bash
# Nexus dual product build: macOS arm64 (.app) + Windows x86_64 (exe package)
# Always full rebuild. No flags.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$ROOT/app"
TAURI_DIR="$APP_DIR/src-tauri"
CORE_SRC="$ROOT/core/server"
BIN_DIR="$ROOT/bin"
CORE_OUT="$BIN_DIR/NexusCore"
CORE_WIN_OUT="$BIN_DIR/NexusCore-windows-x86_64.exe"
BINARIES_DIR="$TAURI_DIR/binaries"
WIN_DIST="$BIN_DIR/windows-x86_64"

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
    *) die "host build.sh is macOS-only (got $os); use Windows host for native win shell" ;;
  esac
}

[[ $# -eq 0 ]] || die "usage: ./build.sh  (no flags — always full release rebuild)"

[[ "$(uname -s)" == "Darwin" ]] || die "macOS only host for dual product build"
need go
need cargo
need npm
need rustc
# Both sides generate from core/server/gen/libcore.proto: Go here, Rust in build.rs.
need protoc
if ! xcode-select -p >/dev/null 2>&1; then
  die "Xcode CLT not configured (xcode-select -p)"
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
TRIPLE="$(target_triple)"
log "root=$ROOT triple=$TRIPLE (full dual release rebuild)"

# Feature tags required in NexusCore (stubs if missing)
# Windows Core: with_purego + with_naive_outbound (CGO off cross-build)
CORE_TAGS_MAC="${NEXUS_CORE_TAGS:-with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_naive_outbound,badlinkname,tfogo_checklinkname0}"
CORE_TAGS_WIN="${NEXUS_CORE_TAGS_WIN:-with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_naive_outbound,with_purego,badlinkname,tfogo_checklinkname0}"
CORE_REQUIRED_TAGS=(with_clash_api with_gvisor with_quic with_wireguard with_utls with_dhcp with_tailscale with_naive_outbound badlinkname tfogo_checklinkname0)
mkdir -p "$BIN_DIR" "$BINARIES_DIR" "$WIN_DIST"

verify_core_binary() {
  local bin="$1"
  local label="${2:-NexusCore}"
  [[ -f "$bin" ]] || die "$label missing: $bin"
  # Windows .exe may not be +x on macOS host
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
# Both core builds below compile against the patched copy.
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

# --- 1b) NexusCore Windows amd64 (cross from mac; CGO=0) ---
log "building NexusCore windows/amd64 (go · tags=$CORE_TAGS_WIN)…"
(
  cd "$CORE_SRC"
  VERSION_SINGBOX="$(go list -m -f '{{.Version}}' github.com/sagernet/sing-box)"
  export CGO_ENABLED=0
  export GOOS=windows
  export GOARCH=amd64
  # clear mac CGO env
  unset CC CGO_CFLAGS CGO_LDFLAGS || true
  # -H=windowsgui: PE subsystem GUI — no black console when GUI/Core spawn helpers.
  # stdout/stderr still work when parent redirects (session core_stdio_sinks).
  go build -trimpath \
    -tags "$CORE_TAGS_WIN" \
    -ldflags "-s -w -H=windowsgui -X 'github.com/sagernet/sing-box/constant.Version=${VERSION_SINGBOX}' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0" \
    -o "$CORE_WIN_OUT" \
    .
)
ok "NexusCore windows → $CORE_WIN_OUT"
verify_core_binary "$CORE_WIN_OUT" "NexusCore(win)"

# stage for tauri externalBin on windows target name
cp -f "$CORE_WIN_OUT" "$BINARIES_DIR/NexusCore-x86_64-pc-windows-msvc.exe"
cp -f "$CORE_WIN_OUT" "$BINARIES_DIR/NexusCore-x86_64-pc-windows-gnu.exe"
# cronet dll for naive outbound on Windows
if command -v curl >/dev/null 2>&1; then
  log "fetching libcronet.dll (windows amd64)…"
  if curl -fLso "$WIN_DIST/libcronet.dll" \
    "https://github.com/SagerNet/cronet-go/releases/latest/download/libcronet-windows-amd64.dll"; then
    ok "libcronet.dll → $WIN_DIST/libcronet.dll"
  else
    err "libcronet.dll download failed (naive outbound may need it at runtime)"
  fi
fi
cp -f "$CORE_WIN_OUT" "$WIN_DIST/NexusCore.exe"
ok "windows core package seed → $WIN_DIST"

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

# --- 2.5) UI staging: page + extracted css/i18n + assets ---
UI_SRC="$APP_DIR/ui/index.html"
[[ -f "$UI_SRC" ]] || die "missing UI source: $UI_SRC"
[[ -f "$APP_DIR/ui/app.css" ]] || die "missing UI css: $APP_DIR/ui/app.css"
[[ -f "$APP_DIR/ui/i18n.js" ]] || die "missing UI i18n: $APP_DIR/ui/i18n.js"
UI_STAGE="$TAURI_DIR/ui-release-dist"
rm -rf "$UI_STAGE"
mkdir -p "$UI_STAGE"
cp "$UI_SRC" "$UI_STAGE/index.html"
cp "$APP_DIR/ui/app.css" "$UI_STAGE/app.css"
cp "$APP_DIR/ui/i18n.js" "$UI_STAGE/i18n.js"
if [[ -d "$APP_DIR/ui/assets" ]]; then
  cp -R "$APP_DIR/ui/assets" "$UI_STAGE/assets"
fi
UI_CONF_OVERRIDE="$TAURI_DIR/tauri.release-ui.json"
cat > "$UI_CONF_OVERRIDE" <<EOF
{
  "build": {
    "frontendDist": "./ui-release-dist"
  },
  "bundle": {
    "targets": ["app"]
  }
}
EOF
ok "UI release staging · $UI_STAGE"

# --- 3) tauri release build (mac host .app) ---
log "tauri build (release · macOS app)…"
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

# --- 5) Windows shell via remote host (Tauri GUI needs native Windows toolchain) ---
# Remote Windows build is opt-in: set NEXUS_WIN_HOST / NEXUS_WIN_USER + pass file.
# Do not bake LAN hosts or usernames into the tree.
WIN_HOST="${NEXUS_WIN_HOST:-}"
WIN_USER="${NEXUS_WIN_USER:-}"
WIN_PASS_FILE="${NEXUS_WIN_PASS_FILE:-/tmp/nexus-win-ssh.pass}"
SSH_BASE=(sshpass -f "$WIN_PASS_FILE" ssh -o StrictHostKeyChecking=no -o PreferredAuthentications=password -o PubkeyAuthentication=no)
SCP_BASE=(sshpass -f "$WIN_PASS_FILE" scp -o StrictHostKeyChecking=no -o PreferredAuthentications=password -o PubkeyAuthentication=no)
if [[ -n "$WIN_HOST" && -n "$WIN_USER" && -f "$WIN_PASS_FILE" ]] && command -v sshpass >/dev/null 2>&1; then
  log "Windows host $WIN_USER@$WIN_HOST — sync + remote tauri build…"
  REMOTE_DIR="C:/Users/${WIN_USER}/NexusBuild"
  # Pack only product sources (avoid exclude bin eating app/src-tauri/src/bin).
  # Exclude macOS AppleDouble (._*) / .DS_Store / .omc — Windows tauri-build reads
  # every file under permissions/ and dies on non-UTF-8 `._nexus.toml`.
  PACK=/tmp/nexus-win-src.tgz
  COPYFILE_DISABLE=1 tar -C "$ROOT" -czf "$PACK" \
    --exclude='._*' --exclude='.DS_Store' --exclude='.omc' --exclude='**/.omc/**' \
    app/package.json app/package-lock.json app/ui \
    app/src-tauri/src app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock \
    app/src-tauri/tauri.conf.json app/src-tauri/build.rs app/src-tauri/windows \
    app/src-tauri/capabilities app/src-tauri/permissions app/src-tauri/icons \
    script/build_windows_remote.ps1 script/install_rust_windows.ps1
  "${SCP_BASE[@]}" "$PACK" "${WIN_USER}@${WIN_HOST}:C:/Users/${WIN_USER}/nexus-win-src.tgz"
  "${SSH_BASE[@]}" "${WIN_USER}@${WIN_HOST}" \
    "powershell -NoProfile -Command \"New-Item -ItemType Directory -Force -Path '$REMOTE_DIR' | Out-Null; tar -xzf C:/Users/${WIN_USER}/nexus-win-src.tgz -C '$REMOTE_DIR'; Get-ChildItem -Path '$REMOTE_DIR' -Recurse -Force -Filter '._*' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue; Get-ChildItem -Path '$REMOTE_DIR' -Recurse -Force -Filter '.DS_Store' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue\""
  # ship prebuilt Core
  "${SSH_BASE[@]}" "${WIN_USER}@${WIN_HOST}" \
    "powershell -NoProfile -Command \"New-Item -ItemType Directory -Force -Path '$REMOTE_DIR/bin' | Out-Null\""
  "${SCP_BASE[@]}" "$CORE_WIN_OUT" \
    "${WIN_USER}@${WIN_HOST}:${REMOTE_DIR}/bin/NexusCore.exe"
  if ! "${SSH_BASE[@]}" "${WIN_USER}@${WIN_HOST}" \
    "powershell -NoProfile -ExecutionPolicy Bypass -File $REMOTE_DIR/script/build_windows_remote.ps1 -NexusRoot $REMOTE_DIR"; then
    err "remote Windows shell build failed — Core package still at $WIN_DIST"
  else
    "${SCP_BASE[@]}" \
      "${WIN_USER}@${WIN_HOST}:${REMOTE_DIR}/app/src-tauri/target/release/nexus.exe" \
      "$WIN_DIST/nexus.exe" && ok "pulled nexus.exe"
  fi
else
  log "skip remote Windows shell (need NEXUS_WIN_HOST/USER + sshpass + $WIN_PASS_FILE); Core-only package at $WIN_DIST"
fi

echo
ok "build complete"
echo "  mac app:  $DEST_APP"
echo "  mac core: $CORE_OUT"
echo "  win core: $CORE_WIN_OUT"
echo "  win dist: $WIN_DIST"
echo "  run mac:  open \"$DEST_APP\""
