#!/bin/bash

set -euo pipefail

QT_VERSION="${QT_VERSION:-6.12.0-beta2}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-26.0}"
PROTOC_GEN_GO_VERSION="v1.36.11"
PROTOC_GEN_GO_GRPC_VERSION="v1.6.2"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ARCH="$(uname -m)"

color_output() { printf "\033[%sm%s\033[0m\n" "$2" "$1"; }
log_success() { color_output "[SUCCESS] $1" "32"; }
log_warn() { color_output "[WARNING] $1" "33"; }
log_info() { color_output "[INFO] $1" "36"; }
cmd_exists() { command -v "$1" >/dev/null 2>&1; }

install_brew_if_needed() {
    if cmd_exists brew; then
        log_success "Homebrew already installed"
        return
    fi

    log_info "Installing Homebrew"
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    if [ "$ARCH" = "arm64" ]; then
        eval "$(/opt/homebrew/bin/brew shellenv)"
    else
        eval "$(/usr/local/bin/brew shellenv)"
    fi
}

install_xcode_cli_if_needed() {
    if xcode-select -p >/dev/null 2>&1; then
        log_success "Xcode Command Line Tools already installed"
        return
    fi

    xcode-select --install
    log_warn "Complete the Xcode Command Line Tools installation, then rerun this script"
    exit 1
}

install_formula_for_command() {
    local command_name="$1"
    local formula="$2"
    if cmd_exists "$command_name"; then
        log_success "$command_name already installed"
    else
        log_info "Installing $formula"
        brew install "$formula"
    fi
}

install_go_plugins() {
    export GOPATH="${GOPATH:-$HOME/go}"
    export PATH="$GOPATH/bin:$PATH"
    log_info "Installing protoc-gen-go $PROTOC_GEN_GO_VERSION"
    go install "google.golang.org/protobuf/cmd/protoc-gen-go@$PROTOC_GEN_GO_VERSION"
    log_info "Installing protoc-gen-go-grpc $PROTOC_GEN_GO_GRPC_VERSION"
    go install "google.golang.org/grpc/cmd/protoc-gen-go-grpc@$PROTOC_GEN_GO_GRPC_VERSION"
}

install_openssl_if_needed() {
    if brew list openssl@3 >/dev/null 2>&1; then
        log_success "OpenSSL 3 already installed"
    else
        log_info "Installing OpenSSL 3"
        brew install openssl@3
    fi
}

build_static_qt() {
    local qt_core="$PROJECT_DIR/qt6/build/lib/QtCore.framework/Versions/A/QtCore"
    log_info "Checking static Qt $QT_VERSION for macOS $MACOSX_DEPLOYMENT_TARGET"
    bash "$SCRIPT_DIR/build_qt_static_macos.sh" "$QT_VERSION"
    if [ ! -f "$qt_core" ]; then
        printf 'Static Qt build did not create %s\n' "$qt_core" >&2
        exit 1
    fi
}

main() {
    install_xcode_cli_if_needed
    install_brew_if_needed
    install_formula_for_command cmake cmake
    install_formula_for_command ninja ninja
    install_formula_for_command go go
    install_formula_for_command protoc protobuf
    install_formula_for_command clang-format clang-format
    install_openssl_if_needed
    install_go_plugins
    build_static_qt
    log_success "macOS build environment is ready"
}

main "$@"
