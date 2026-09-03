#!/usr/bin/env bash
# Nexus macOS product build: NexusCore + Qt Quick .app
# Always full rebuild. No flags. The product target is Apple Silicon macOS.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$ROOT/app"
BACKEND_DIR="$APP_DIR/backend"
ICON_DIR="$APP_DIR/assets/icons"
CORE_SRC="$ROOT/core/server"
BIN_DIR="$ROOT/bin"
CORE_OUT="$BIN_DIR/NexusCore"

export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

# Homebrew does not link Qt into PATH by default. Allow CI and non-Homebrew
# installations to override the prefix without editing this script.
if [[ -n "${NEXUS_QT_HOME:-}" ]]; then
  QT_HOME="$NEXUS_QT_HOME"
elif command -v brew >/dev/null 2>&1; then
  QT_HOME="$(brew --prefix)"
else
  QT_HOME="/opt/homebrew/opt/qt"
fi
export PATH="$QT_HOME/bin:$PATH"

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
    *) die "Nexus release builds support Apple Silicon only (got: $arch)" ;;
  esac
  os="$(uname -s)"
  case "$os" in
    Darwin) echo "${arch}-apple-darwin" ;;
    *) die "host build.sh is macOS-only (got $os)" ;;
  esac
}

[[ $# -eq 0 ]] || die "usage: ./build.sh  (no flags — always full release rebuild)"

[[ "$(uname -s)" == "Darwin" ]] || die "macOS only host"
bash "$ROOT/script/check-release-metadata.sh" \
  || die "release metadata is inconsistent"
need go
need cargo
need cmake
need python3
need rustc
need macdeployqt
need codesign
need ditto
need otool
need install_name_tool
need plutil
need spctl
need xcrun
# Both sides generate from core/server/gen/libcore.proto: Go here, Rust in build.rs.
need protoc
if ! xcode-select -p >/dev/null 2>&1; then
  die "Xcode CLT not configured (xcode-select -p)"
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-26.0}"
DEPLOYMENT_MAJOR="${MACOSX_DEPLOYMENT_TARGET%%.*}"
[[ "$DEPLOYMENT_MAJOR" =~ ^[0-9]+$ ]] \
  || die "invalid MACOSX_DEPLOYMENT_TARGET: $MACOSX_DEPLOYMENT_TARGET"
(( DEPLOYMENT_MAJOR >= 26 )) \
  || die "Nexus release builds require macOS 26+ (got MACOSX_DEPLOYMENT_TARGET=$MACOSX_DEPLOYMENT_TARGET)"
TRIPLE="$(target_triple)"
log "root=$ROOT triple=$TRIPLE (mac release rebuild)"

# Feature tags required in NexusCore (stubs if missing).
CORE_TAGS_BASE="with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,with_naive_outbound,badlinkname,tfogo_checklinkname0"
CORE_TAGS_MAC="${NEXUS_CORE_TAGS:-$CORE_TAGS_BASE}"
IFS=',' read -ra CORE_REQUIRED_TAGS <<< "$CORE_TAGS_BASE"
mkdir -p "$BIN_DIR"

verify_core_binary() {
  local bin="$1"
  local label="${2:-NexusCore}"
  [[ -f "$bin" ]] || die "$label missing: $bin"
  local sz
  sz="$(stat -f%z "$bin" 2>/dev/null || stat -c%s "$bin")"
  local meta
  meta="$(go version -m "$bin" 2>/dev/null || true)"
  [[ -n "$meta" ]] || die "go version -m failed on $bin (not a Go binary?)"
  echo "$meta" | grep -q 'github.com/sagernet/sing-box' || die "$label missing module: sing-box"
  echo "$meta" | grep -q 'github.com/sagernet/sing-tun' || die "$label missing module: sing-tun"
  echo "$meta" | grep -qE 'gvisor|github.com/sagernet/gvisor' || die "$label missing gvisor (with_gvisor tag?)"
  local tagline
  tagline="$(echo "$meta" | grep -E '^\s*build\s+-tags=' | head -1 || true)"
  [[ -n "$tagline" ]] || die "$label has no build -tags= metadata"
  local t
  for t in "${CORE_REQUIRED_TAGS[@]}"; do
    echo "$tagline" | grep -q "$t" || die "$label missing required -tag: $t (got: $tagline)"
  done
  ok "$label verified · $(numfmt --to=iec "$sz" 2>/dev/null || echo "${sz}B") · tags+sing-box+sing-tun"
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

# --- 2) Qt Quick host (mac .app) ---
QT_DIR="$APP_DIR/qt"
QT_BUILD="$QT_DIR/build"
[[ -f "$QT_DIR/CMakeLists.txt" ]] || die "missing Qt host: $QT_DIR/CMakeLists.txt"
[[ -f "$QT_DIR/Info.plist" ]] || die "missing $QT_DIR/Info.plist"
log "cmake Qt host…"
cmake -S "$QT_DIR" -B "$QT_BUILD" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET" \
  -DCMAKE_PREFIX_PATH="$QT_HOME"
cmake --build "$QT_BUILD" --target nexus
QT_BIN="$QT_BUILD/nexus"
[[ -x "$QT_BIN" ]] || die "qt host missing: $QT_BIN"
ok "qt host → $QT_BIN"

log "building nexusfwd…"
(cd "$BACKEND_DIR" && cargo build --locked --release --bin nexusfwd)
FWD_BIN="$BACKEND_DIR/target/release/nexusfwd"
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
plutil -replace LSMinimumSystemVersion -string "$MACOSX_DEPLOYMENT_TARGET" \
  "$DEST_APP/Contents/Info.plist"
cp -f "$ICON_DIR/icon.icns" "$DEST_APP/Contents/Resources/icon.icns"

# Include the notices a recipient needs with the exact binary they received.
NOTICE_DIR="$DEST_APP/Contents/Resources/licenses"
mkdir -p "$NOTICE_DIR"
cp -f "$ROOT/LICENSE" "$NOTICE_DIR/NEXUS-LICENSE.txt"
cp -f "$ROOT/THIRD_PARTY_NOTICES.md" "$NOTICE_DIR/THIRD_PARTY_NOTICES.md"
cp -f "$ROOT/licenses/GPL-3.0.txt" "$NOTICE_DIR/GPL-3.0.txt"
cp -f "$ROOT/licenses/LGPL-3.0.txt" "$NOTICE_DIR/LGPL-3.0.txt"
cp -f "$ROOT/licenses/MPL-2.0.txt" "$NOTICE_DIR/MPL-2.0.txt"

# Copy Qt frameworks, QML runtime modules and plugins, then rewrite install names.
# Product QML and tray frames are compiled into the host as qrc resources.
macdeployqt "$DEST_APP" \
  -qmldir="$QT_DIR/qml" \
  -libpath="$QT_HOME/lib" \
  -always-overwrite \
  -verbose=1

for framework in QtQuickTimeline QtQuickTimelineBlendTrees; do
  target="$DEST_APP/Contents/Frameworks/$framework.framework"
  [[ -d "$target" ]] && continue
  source="$QT_HOME/lib/$framework.framework"
  [[ -e "$source" ]] || die "missing Qt runtime framework: $source"
  source="$(python3 -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$source")"
  ditto "$source" "$target"
done

plutil -lint "$DEST_APP/Contents/Info.plist" >/dev/null

while IFS= read -r -d '' file; do
  /usr/bin/file -b "$file" | grep -q 'Mach-O' || continue
  current_id="$(otool -D "$file" 2>/dev/null | sed -n '2p')"
  [[ -n "$current_id" ]] || continue
  rel="${file#"$DEST_APP/Contents/Frameworks/"}"
  wanted_id="@executable_path/../Frameworks/$rel"
  [[ "$current_id" == "$wanted_id" ]] || install_name_tool -id "$wanted_id" "$file"
done < <(find "$DEST_APP/Contents/Frameworks" -type f -print0)

while IFS= read -r -d '' file; do
  /usr/bin/file -b "$file" | grep -q 'Mach-O' || continue
  while IFS= read -r dep; do
    case "$dep" in
      @rpath/*) framework_rel="${dep#@rpath/}" ;;
      */lib/*) framework_rel="${dep##*/lib/}" ;;
      *) continue ;;
    esac
    bundled="$DEST_APP/Contents/Frameworks/$framework_rel"
    [[ -e "$bundled" ]] || continue
    install_name_tool -change "$dep" \
      "@executable_path/../Frameworks/$framework_rel" "$file"
  done < <(otool -L "$file" | tail -n +2 | awk '{print $1}')
done < <(find "$DEST_APP/Contents" -type f -print0)

# macdeployqt can leave build-host-only LC_RPATH entries on Homebrew binaries.
# They are unnecessary in a self-contained bundle and can make missing bundle
# dependencies appear to work on the build machine, so remove them before the
# dependency audit below.
while IFS= read -r -d '' file; do
  /usr/bin/file -b "$file" | grep -q 'Mach-O' || continue
  while IFS= read -r rpath; do
    [[ -n "$rpath" ]] || continue
    case "$rpath" in
      @*|/System/Library/*|/usr/lib/*) ;;
      *) install_name_tool -delete_rpath "$rpath" "$file" ;;
    esac
  done < <(
    otool -l "$file" 2>/dev/null \
      | awk '$1 == "cmd" && $2 == "LC_RPATH" { want = 1; next }
             want && $1 == "path" { print $2; want = 0 }'
  )
done < <(find "$DEST_APP/Contents" -type f -print0)

INVENTORY="$NOTICE_DIR/BUNDLE_INVENTORY.txt"
{
  echo "Nexus.app runtime inventory"
  echo "Generated by build.sh from the staged application bundle."
  echo
  find "$DEST_APP/Contents/Frameworks" \
       "$DEST_APP/Contents/PlugIns" \
       "$DEST_APP/Contents/Resources/qml" \
       -type f -print 2>/dev/null \
    | sed "s#^$DEST_APP/Contents/##" \
    | LC_ALL=C sort
} > "$INVENTORY"

verify_macho_dependencies() {
  local file dep rel rpath candidate resolved count=0 bad=0
  while IFS= read -r -d '' file; do
    /usr/bin/file -b "$file" | grep -q 'Mach-O' || continue
    count=$((count + 1))
    rel="${file#"$DEST_APP/Contents/"}"

    while IFS= read -r rpath; do
      [[ -n "$rpath" ]] || continue
      case "$rpath" in
        @*|/System/Library/*|/usr/lib/*) ;;
        *) err "non-system absolute LC_RPATH: $rel -> $rpath"; bad=1 ;;
      esac
    done < <(
      otool -l "$file" 2>/dev/null \
        | awk '$1 == "cmd" && $2 == "LC_RPATH" { want = 1; next }
               want && $1 == "path" { print $2; want = 0 }'
    )

    while IFS= read -r dep; do
      [[ -n "$dep" ]] || continue
      case "$dep" in
        @executable_path/*)
          candidate="$DEST_APP/Contents/MacOS/${dep#@executable_path/}"
          [[ -e "$candidate" ]] \
            || { err "unresolved @executable_path dependency: $rel -> $dep"; bad=1; }
          ;;
        @loader_path/*)
          candidate="$(dirname "$file")/${dep#@loader_path/}"
          [[ -e "$candidate" ]] \
            || { err "unresolved @loader_path dependency: $rel -> $dep"; bad=1; }
          ;;
        @rpath/*)
          resolved=0
          while IFS= read -r rpath; do
            [[ -n "$rpath" ]] || continue
            case "$rpath" in
              @loader_path/*)
                candidate="$(dirname "$file")/${rpath#@loader_path/}/${dep#@rpath/}"
                ;;
              @executable_path/*)
                candidate="$DEST_APP/Contents/MacOS/${rpath#@executable_path/}/${dep#@rpath/}"
                ;;
              /*)
                candidate="$rpath/${dep#@rpath/}"
                ;;
              *) continue ;;
            esac
            if [[ -e "$candidate" ]]; then
              resolved=1
              break
            fi
          done < <(
            otool -l "$file" 2>/dev/null \
              | awk '$1 == "cmd" && $2 == "LC_RPATH" { want = 1; next }
                     want && $1 == "path" { print $2; want = 0 }'
          )
          (( resolved == 1 )) \
            || { err "unresolved @rpath dependency: $rel -> $dep"; bad=1; }
          ;;
        @*)
          err "unsupported loader dependency form: $rel -> $dep"
          bad=1
          ;;
        /System/Library/*|/usr/lib/*) ;;
        *) err "non-system absolute dependency: $rel -> $dep"; bad=1 ;;
      esac
    done < <(otool -L "$file" | tail -n +2 | awk '{print $1}')
  done < <(find "$DEST_APP/Contents" -type f -print0)
  (( count > 0 )) || die "no Mach-O files found in staged app"
  (( bad == 0 )) || die "staged app has unresolved or build-machine Mach-O dependencies"
  ok "Mach-O dependency audit passed · $count files · all loader paths resolve inside bundle/system"
}

verify_macho_dependencies

verify_macos_minimum() {
  python3 - "$DEST_APP" "$MACOSX_DEPLOYMENT_TARGET" <<'PY'
import os
import subprocess
import sys

contents = os.path.join(sys.argv[1], "Contents")
declared = tuple(int(part) for part in sys.argv[2].split("."))
counts = {}
bad = []

for directory, _, files in os.walk(contents):
    for name in files:
        path = os.path.join(directory, name)
        if os.path.islink(path):
            continue
        try:
            kind = subprocess.check_output(
                ["/usr/bin/file", "-b", path],
                text=True,
                stderr=subprocess.DEVNULL,
            )
        except subprocess.CalledProcessError:
            continue
        if "Mach-O" not in kind:
            continue

        lines = subprocess.check_output(
            ["otool", "-l", path],
            text=True,
            stderr=subprocess.DEVNULL,
        ).splitlines()
        minos = None
        for index, line in enumerate(lines):
            command = line.strip()
            if command == "cmd LC_BUILD_VERSION":
                key = "minos"
                window = lines[index + 1:index + 8]
            elif command == "cmd LC_VERSION_MIN_MACOSX":
                key = "version"
                window = lines[index + 1:index + 6]
            else:
                continue
            for candidate in window:
                parts = candidate.split()
                if len(parts) >= 2 and parts[0] == key:
                    minos = parts[1]
                    break
            if minos is not None:
                break

        rel = os.path.relpath(path, contents)
        if minos is None:
            bad.append(f"{rel}: missing macOS minimum load command")
            continue
        counts[minos] = counts.get(minos, 0) + 1
        required = tuple(int(part) for part in minos.split("."))
        if required > declared:
            bad.append(f"{rel}: requires macOS {minos}, declared {sys.argv[2]}")

if bad:
    for item in bad:
        print(f"[ERR] {item}", file=sys.stderr)
    sys.exit(1)

distribution = ", ".join(
    f"{version}:{counts[version]}"
    for version in sorted(counts, key=lambda value: tuple(int(part) for part in value.split(".")))
)
print(f"Mach-O minimum audit passed · declared {sys.argv[2]} · {distribution}")
PY
}

verify_macos_minimum \
  || die "staged app contains binaries requiring a newer macOS than declared"

# A Developer ID identity creates a distributable hardened-runtime build.
# Without one, ad-hoc signing still produces a coherent local test artifact.
SIGN_IDENTITY="${NEXUS_SIGN_IDENTITY:--}"
NOTARY_PROFILE="${NEXUS_NOTARY_PROFILE:-}"
if [[ -n "$NOTARY_PROFILE" && "$SIGN_IDENTITY" == "-" ]]; then
  die "NEXUS_NOTARY_PROFILE requires NEXUS_SIGN_IDENTITY (Developer ID Application)"
fi

sign_code_item() {
  local path="$1"
  if [[ "$SIGN_IDENTITY" == "-" ]]; then
    codesign --force --sign - "$path"
  else
    codesign --force --options runtime --timestamp \
      --sign "$SIGN_IDENTITY" "$path"
  fi
}

# Apple requires nested code to be signed from the inside out; --deep is a
# verification aid, not a release signing strategy. Sign standalone Mach-O
# leaves first, then nested code bundles deepest-first, and the .app last.
while IFS= read -r -d '' file; do
  /usr/bin/file -b "$file" | grep -q 'Mach-O' || continue
  rel="${file#"$DEST_APP/Contents/"}"
  case "$rel" in
    *.framework/*|*.bundle/*|*.plugin/*|*.xpc/*|*.appex/*|*.app/*) continue ;;
  esac
  sign_code_item "$file"
done < <(find "$DEST_APP/Contents" -type f -print0)

while IFS= read -r -d '' bundle; do
  sign_code_item "$bundle"
done < <(
  python3 - "$DEST_APP/Contents" <<'PY'
import os
import sys

root = sys.argv[1]
suffixes = (".framework", ".bundle", ".plugin", ".xpc", ".appex", ".app")
paths = []
for directory, dirs, _ in os.walk(root):
    for name in dirs:
        if name.endswith(suffixes):
            paths.append(os.path.join(directory, name))
for path in sorted(paths, key=lambda p: p.count(os.sep), reverse=True):
    sys.stdout.buffer.write(os.fsencode(path) + b"\0")
PY
)

sign_code_item "$DEST_APP"
if [[ "$SIGN_IDENTITY" == "-" ]]; then
  ok "ad-hoc signed inside-out (local testing; set NEXUS_SIGN_IDENTITY for distribution)"
else
  ok "signed inside-out with Developer ID: $SIGN_IDENTITY"
fi
codesign --verify --deep --strict --verbose=2 "$DEST_APP"

if [[ -n "$NOTARY_PROFILE" ]]; then
  NOTARY_ZIP="$BIN_DIR/Nexus-notarize.zip"
  rm -f "$NOTARY_ZIP"
  ditto -c -k --keepParent "$DEST_APP" "$NOTARY_ZIP"
  xcrun notarytool submit "$NOTARY_ZIP" \
    --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DEST_APP"
  xcrun stapler validate "$DEST_APP"
  spctl --assess --type execute --verbose=4 "$DEST_APP"
  ok "notarized and stapled"
fi

ok "staged self-contained $DEST_APP"

echo
ok "build complete"
echo "  mac app:  $DEST_APP"
echo "  mac core: $CORE_OUT"
echo "  run mac:  open \"$DEST_APP\""
