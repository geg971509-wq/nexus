#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BUILD_TYPE="${BUILD_TYPE:-Release}"
QT_VERSION="${QT_VERSION:-6.12.0-beta2}"
PROTOC_GEN_GO_VERSION="${PROTOC_GEN_GO_VERSION:-v1.36.11}"
PROTOC_GEN_GO_GRPC_VERSION="${PROTOC_GEN_GO_GRPC_VERSION:-1.6.2}"
DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-26.0}"
ARCH="$(uname -m)"
QT_BUILD_DIR="$ROOT_DIR/qt6/build"
QT_CORE_ARCHIVE="$QT_BUILD_DIR/lib/QtCore.framework/Versions/A/QtCore"
CLANG_FORMAT_PATH=""
SDKROOT=""
SDK_VERSION=""
UPDATE=false
BUILD=true
BOOTSTRAP=false

export GOPATH="${GOPATH:-$HOME/go}"
export PATH="$GOPATH/bin:$PATH"

color_output() { printf '\033[%sm%s\033[0m\n' "$2" "$1"; }
log_success() { color_output "[SUCCESS] $1" "32"; }
log_error() { color_output "[ERROR] $1" "31"; }
log_warn() { color_output "[WARNING] $1" "33"; }
log_info() { color_output "[INFO] $1" "36"; }
cmd_exists() { command -v "$1" >/dev/null 2>&1; }

usage() {
    cat <<'EOF'
Usage: ./build.sh [--update] [--update-only] [--bootstrap]

  --update       Update pinned core forks, then build
  --update-only  Update pinned core forks without building
  --bootstrap    Prepare the macOS build environment before building
  --help         Show this help
EOF
}

require_command() {
    if ! cmd_exists "$1"; then
        log_error "Missing required command: $1 (run ./build.sh --bootstrap)"
        exit 1
    fi
}

resolve_openssl() {
    if [[ -n "${OPENSSL_ROOT_DIR:-}" && -d "$OPENSSL_ROOT_DIR" ]]; then
        return
    fi
    if cmd_exists brew && brew list openssl@3 >/dev/null 2>&1; then
        OPENSSL_ROOT_DIR="$(brew --prefix openssl@3)"
        export OPENSSL_ROOT_DIR
        return
    fi
    log_error "OpenSSL 3 was not found (run ./build.sh --bootstrap)"
    exit 1
}

preflight() {
    local command_name version
    for command_name in cmake ninja go protoc protoc-gen-go protoc-gen-go-grpc clang-format xcrun codesign dsymutil strip; do
        require_command "$command_name"
    done
    if ! xcode-select -p >/dev/null 2>&1; then
        log_error "Xcode Command Line Tools are not configured"
        exit 1
    fi

    version="$(protoc-gen-go --version)"
    case "$version" in
        *"$PROTOC_GEN_GO_VERSION") ;;
        *) log_error "Expected protoc-gen-go $PROTOC_GEN_GO_VERSION, got: $version"; exit 1 ;;
    esac
    version="$(protoc-gen-go-grpc --version)"
    case "$version" in
        *"$PROTOC_GEN_GO_GRPC_VERSION") ;;
        *) log_error "Expected protoc-gen-go-grpc $PROTOC_GEN_GO_GRPC_VERSION, got: $version"; exit 1 ;;
    esac

    if [[ ! -f "$QT_CORE_ARCHIVE" ]]; then
        log_error "Static Qt is missing at $QT_BUILD_DIR (run ./build.sh --bootstrap)"
        exit 1
    fi
    if [[ ! -x "$QT_BUILD_DIR/bin/macdeployqt" ]]; then
        log_error "macdeployqt is missing at $QT_BUILD_DIR/bin/macdeployqt"
        exit 1
    fi

    SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
    SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
    case "$SDK_VERSION" in
        26.*) ;;
        *) log_error "macOS 26 SDK required, found $SDK_VERSION"; exit 1 ;;
    esac
    export SDKROOT MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"
    CLANG_FORMAT_PATH="$(command -v clang-format)"
    resolve_openssl

    local expected_qt_stamp qt_source_commit qt_tag_commit
    qt_source_commit="$(git -C "$ROOT_DIR/qt6" rev-parse HEAD)"
    qt_tag_commit="$(git -C "$ROOT_DIR/qt6" rev-parse "v$QT_VERSION^{commit}")"
    if [[ "$qt_source_commit" != "$qt_tag_commit" ]]; then
        log_error "Qt source is not at v$QT_VERSION (run ./build.sh --bootstrap)"
        exit 1
    fi
    expected_qt_stamp="$(printf 'qt=%s\ncommit=%s\nsdk=%s\ntarget=%s\narch=%s' "$QT_VERSION" "$qt_source_commit" "$SDK_VERSION" "$DEPLOYMENT_TARGET" "$ARCH")"
    if [[ ! -f "$QT_BUILD_DIR/.throne-static-qt-stamp" ]] ||
        [[ "$(<"$QT_BUILD_DIR/.throne-static-qt-stamp")" != "$expected_qt_stamp" ]]; then
        log_error "Static Qt cache is not built for macOS $DEPLOYMENT_TARGET (run ./build.sh --bootstrap)"
        exit 1
    fi
}

build_core() {
    local server_dir="$ROOT_DIR/core/server"
    local gen_dir="$server_dir/gen"
    local build_tags version_singbox ldflags

    export CGO_ENABLED=1
    export CC="$(xcrun --sdk macosx --find clang)"
    export CGO_CFLAGS="${CGO_CFLAGS:+$CGO_CFLAGS }-isysroot $SDKROOT -mmacosx-version-min=$DEPLOYMENT_TARGET"
    export CGO_LDFLAGS="${CGO_LDFLAGS:+$CGO_LDFLAGS }-isysroot $SDKROOT -mmacosx-version-min=$DEPLOYMENT_TARGET"

    rm -f "$gen_dir"/*.pb.go "$gen_dir"/*_grpc.pb.go "$gen_dir"/*.pb.protorpc.go
    rm -rf "$gen_dir/gen"
    (
        cd "$gen_dir"
        protoc -I . \
            --plugin=protoc-gen-go="$GOPATH/bin/protoc-gen-go" \
            --plugin=protoc-gen-go-grpc="$GOPATH/bin/protoc-gen-go-grpc" \
            --go_out=. --go-grpc_out=. libcore.proto
    )
    if [[ -d "$gen_dir/gen" ]]; then
        mv "$gen_dir/gen"/*.go "$gen_dir/"
        rmdir "$gen_dir/gen"
    fi
    if [[ ! -f "$gen_dir/libcore.pb.go" || ! -f "$gen_dir/libcore_grpc.pb.go" ]]; then
        log_error "Protocol Buffer generation did not produce both Go outputs"
        exit 1
    fi

    version_singbox="$(cd "$server_dir" && go list -m -f '{{.Version}}' github.com/sagernet/sing-box)"
    build_tags="with_clash_api,with_gvisor,with_quic,with_wireguard,with_utls,with_dhcp,with_tailscale,badlinkname,tfogo_checklinkname0"
    ldflags="-w -s -X 'github.com/sagernet/sing-box/constant.Version=$version_singbox' -X 'internal/godebug.defaultGODEBUG=multipathtcp=0' -checklinkname=0"
    (cd "$server_dir" && go build -v -trimpath -ldflags "$ldflags" -tags "$build_tags" -o ThroneCore .)
    log_success "ThroneCore built"
}

configure_project() {
    local build_dir="$ROOT_DIR/build"
    local qt_cmake_dir="$QT_BUILD_DIR/lib/cmake"
    local cmake_args=(
        -G Ninja
        -DCMAKE_BUILD_TYPE="$BUILD_TYPE"
        -DBUILD_TESTING=OFF
        -DNKR_PACKAGE_MACOS=ON
        -DCMAKE_PREFIX_PATH="$qt_cmake_dir"
        -DCMAKE_OSX_ARCHITECTURES="$ARCH"
        -DOPENSSL_ROOT_DIR="$OPENSSL_ROOT_DIR"
        -DCLANG_FORMAT:FILEPATH="$CLANG_FORMAT_PATH"
        -DCMAKE_OSX_SYSROOT="$SDKROOT"
        -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"
        -DWARP_CLIENT_ENABLE=ON
    )

    if [[ -f "$build_dir/CMakeCache.txt" ]] && ! grep -Fq "$ROOT_DIR" "$build_dir/CMakeCache.txt"; then
        log_warn "Removing a CMake cache created for another source directory"
        rm -rf "$build_dir"
    fi
    cmake -S "$ROOT_DIR" -B "$build_dir" "${cmake_args[@]}"
}

build_project() {
    cmake --build "$ROOT_DIR/build" --config "$BUILD_TYPE" --parallel
    log_success "Throne.app and warp-client built"
}

package_app() {
    local throne_app="$ROOT_DIR/build/Throne.app"
    local throne_core="$ROOT_DIR/core/server/ThroneCore"
    local warp_client="$ROOT_DIR/build/warp-client-src/bin/warp-client"
    local bin_dir="$ROOT_DIR/bin"

    if [[ ! -d "$throne_app" ]]; then
        log_error "Missing release artifact: $throne_app"
        exit 1
    fi
    if [[ ! -x "$throne_core" ]]; then
        log_error "Missing release artifact: $throne_core"
        exit 1
    fi
    if [[ ! -x "$warp_client" ]]; then
        log_error "Missing release artifact: $warp_client"
        exit 1
    fi

    rm -rf "$throne_app/Contents/MacOS/config"
    "$QT_BUILD_DIR/bin/macdeployqt" "$throne_app" -no-plugins -verbose=2
    cp "$throne_core" "$throne_app/Contents/MacOS/"
    cp "$warp_client" "$throne_app/Contents/MacOS/warp-client"
    chmod +x "$throne_app/Contents/MacOS/warp-client"
    if [[ ! -x "$throne_app/Contents/MacOS/warp-client" ]]; then
        log_error "warp-client is not executable in the app bundle"
        exit 1
    fi
    dsymutil "$throne_app/Contents/MacOS/Throne" 2>/dev/null || true
    strip -S "$throne_app/Contents/MacOS/Throne" 2>/dev/null || true
    codesign --force --deep --sign - "$throne_app"
    codesign --verify --deep --strict --verbose=2 "$throne_app"

    mkdir -p "$bin_dir"
    rm -rf "$bin_dir/Throne.app"
    cp -R "$throne_app" "$bin_dir/"
    log_success "Release app copied to $bin_dir/Throne.app"
}

build_release() {
    log_info "Building Throne for macOS $ARCH ($BUILD_TYPE)"
    preflight
    build_core
    configure_project
    build_project
    package_app
}

while (( $# > 0 )); do
    case "$1" in
        --update)
            UPDATE=true
            ;;
        --update-only)
            UPDATE=true
            BUILD=false
            ;;
        --bootstrap)
            BOOTSTRAP=true
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown option: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [[ "$BUILD" == true || "$BOOTSTRAP" == true ]] && [[ "$(uname -s)" != Darwin ]]; then
    printf 'The local release entry currently targets macOS; cross-platform releases use .github/workflows/build.yml.\n' >&2
    exit 1
fi

if [[ "$BOOTSTRAP" == true ]]; then
    "$ROOT_DIR/script/bootstrap_macos.sh"
fi
if [[ "$UPDATE" == true ]]; then
    "$ROOT_DIR/script/update_core_forks.sh"
fi
if [[ "$BUILD" == true ]]; then
    build_release
fi
