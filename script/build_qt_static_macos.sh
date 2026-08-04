#!/bin/bash
set -euo pipefail

QT_VERSION="${1:-6.12.0-beta2}"
DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-26.0}"

echo "=========================================="
echo "Building Qt $QT_VERSION Static for macOS"
echo "=========================================="

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
QT_ROOT="${QT_ROOT:-$PROJECT_DIR/qt6}"
QT_BUILD_DIR="${QT_BUILD_DIR:-$QT_ROOT/build}"
QT_STAMP="$QT_BUILD_DIR/.throne-static-qt-stamp"
QT_CORE_ARCHIVE="$QT_BUILD_DIR/lib/QtCore.framework/Versions/A/QtCore"

ARCH="${ARCH:-$(uname -m)}"
echo "[INFO] Building for architecture: $ARCH"

SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
case "$SDK_VERSION" in
    26.*) ;;
    *) echo "ERROR: macOS 26 SDK required, found $SDK_VERSION" >&2; exit 1 ;;
esac
export SDKROOT="$SDK_PATH"
export MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"
echo "[INFO] Using macOS SDK $SDK_VERSION: $SDK_PATH"

# Check if Qt source already exists and is valid
if [ -f "$QT_ROOT/qtbase/configure" ]; then
    echo "Qt source already exists, skipping clone..."
else
    # Remove incomplete qt6 directory if it exists
    if [ -d "$QT_ROOT" ]; then
        echo "Removing incomplete qt6 directory..."
        rm -rf "$QT_ROOT"
        if [ -d "$QT_ROOT" ]; then
            echo "ERROR: Cannot remove qt6 directory. Please manually delete it and try again."
            exit 1
        fi
    fi

    # Clone Qt repository
    echo "Cloning Qt repository..."
    git clone https://code.qt.io/qt/qt5.git "$QT_ROOT"

fi

echo "Switching to branch/tag $QT_VERSION..."
git -C "$QT_ROOT" fetch --depth 1 origin "refs/tags/v$QT_VERSION:refs/tags/v$QT_VERSION"
git -C "$QT_ROOT" checkout --detach "v$QT_VERSION"

echo "Initializing Qt submodules..."
echo "This may take a while..."
git -C "$QT_ROOT" submodule update --init --recursive qtbase || { echo "Failed to init qtbase submodule"; exit 1; }
git -C "$QT_ROOT" submodule update --init --recursive qtimageformats || echo "Warning: Failed to init qtimageformats submodule, continuing..."
git -C "$QT_ROOT" submodule update --init --recursive qtsvg || echo "Warning: Failed to init qtsvg submodule, continuing..."
git -C "$QT_ROOT" submodule update --init --recursive qttools || echo "Warning: Failed to init qttools submodule, continuing..."
echo "Submodules initialized successfully"

QT_SOURCE_COMMIT="$(git -C "$QT_ROOT" rev-parse HEAD)"
QT_TAG_COMMIT="$(git -C "$QT_ROOT" rev-parse "v$QT_VERSION^{commit}")"
if [ "$QT_SOURCE_COMMIT" != "$QT_TAG_COMMIT" ]; then
    echo "ERROR: Qt source is not at v$QT_VERSION" >&2
    exit 1
fi
EXPECTED_STAMP="$(printf 'qt=%s\ncommit=%s\nsdk=%s\ntarget=%s\narch=%s' "$QT_VERSION" "$QT_SOURCE_COMMIT" "$SDK_VERSION" "$DEPLOYMENT_TARGET" "$ARCH")"
if [ -f "$QT_CORE_ARCHIVE" ] &&
    [ -f "$QT_STAMP" ] &&
    [ "$(cat "$QT_STAMP")" = "$EXPECTED_STAMP" ]; then
    echo "[INFO] Static Qt cache matches macOS $DEPLOYMENT_TARGET"
    exit 0
fi
if [ "${QT_CACHE_CHECK_ONLY:-0}" = "1" ]; then
    echo "[INFO] Static Qt cache requires a macOS $DEPLOYMENT_TARGET rebuild" >&2
    exit 2
fi

echo "[INFO] Invalidating static Qt cache for the previous SDK/target"
rm -rf "$QT_BUILD_DIR" "$QT_ROOT/CMakeFiles" "$QT_ROOT/.qt" "$QT_ROOT/qtbase/include"
rm -f "$QT_ROOT/CMakeCache.txt" "$QT_ROOT/build.ninja" "$QT_ROOT/.ninja_deps" "$QT_ROOT/.ninja_log"
find "$QT_ROOT/config.tests" "$QT_ROOT/qtbase/config.tests" -name CMakeCache.txt -delete 2>/dev/null || true
find "$QT_ROOT/config.tests" "$QT_ROOT/qtbase/config.tests" -type d -name CMakeFiles -prune -exec rm -rf {} + 2>/dev/null || true
cd "$QT_ROOT"

echo "=========================================="
echo "Configuring Qt..."
echo "=========================================="

mkdir -p "$QT_BUILD_DIR"

# Set OpenSSL path (from Homebrew)
if [ -z "${OPENSSL_ROOT_DIR:-}" ]; then
    if [ -d "$(brew --prefix openssl@3 2>/dev/null)" ]; then
        OPENSSL_ROOT_DIR="$(brew --prefix openssl@3)"
    elif [ -d "$(brew --prefix openssl@1.1 2>/dev/null)" ]; then
        OPENSSL_ROOT_DIR="$(brew --prefix openssl@1.1)"
    elif [ -d "/opt/homebrew/opt/openssl" ]; then
        OPENSSL_ROOT_DIR="/opt/homebrew/opt/openssl"
    elif [ -d "/usr/local/opt/openssl" ]; then
        OPENSSL_ROOT_DIR="/usr/local/opt/openssl"
    fi
fi
echo "Using OpenSSL from: ${OPENSSL_ROOT_DIR:-not found}"

# Configure Qt for static build
CONFIGURE_ARGS=(
    -openssl-linked
    -no-dtls
    -no-ocsp
    -release
    -static
    -prefix "$QT_BUILD_DIR"
    -submodules qtbase,qtimageformats,qtsvg,qttools
    -skip tests
    -skip examples
    -no-opengl
    -gui
    -widgets
)

# Build CMake passthrough arguments
CMAKE_EXTRA_ARGS=()
if [ -n "${OPENSSL_ROOT_DIR:-}" ]; then
    CMAKE_EXTRA_ARGS+=(-D "OPENSSL_ROOT_DIR=$OPENSSL_ROOT_DIR")
    CMAKE_EXTRA_ARGS+=(-D "OPENSSL_USE_STATIC_LIBS=ON")
fi
CMAKE_EXTRA_ARGS+=(-D "CMAKE_OSX_SYSROOT=$SDK_PATH")
CMAKE_EXTRA_ARGS+=(-D "CMAKE_OSX_DEPLOYMENT_TARGET=$DEPLOYMENT_TARGET")
CMAKE_EXTRA_ARGS+=(-D "CMAKE_OSX_ARCHITECTURES=$ARCH")
# Suppress SDK version check warnings
CMAKE_EXTRA_ARGS+=(-D "QT_NO_APPLE_SDK_MAX_VERSION_CHECK=ON")

if [ ${#CMAKE_EXTRA_ARGS[@]} -gt 0 ]; then
    ./configure "${CONFIGURE_ARGS[@]}" -- "${CMAKE_EXTRA_ARGS[@]}"
else
    ./configure "${CONFIGURE_ARGS[@]}"
fi

if [ $? -ne 0 ]; then
    echo "Qt configuration failed"
    exit 1
fi

echo "=========================================="
echo "Building Qt (this may take 2-4 hours)..."
echo "=========================================="
cmake --build . --parallel
if [ $? -ne 0 ]; then
    echo "Qt build failed"
    exit 1
fi

echo "=========================================="
echo "Installing Qt..."
echo "=========================================="
cmake --install . || ninja install
printf '%s\n' "$EXPECTED_STAMP" > "$QT_STAMP"

echo "=========================================="
echo "Qt $QT_VERSION static build complete!"
echo "Installed to: $QT_BUILD_DIR"
echo "=========================================="

cd "$PROJECT_DIR"
exit 0
