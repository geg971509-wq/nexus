#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_DIR="$ROOT_DIR/core/server"

update_replace() {
    local module="$1"
    local fork="$2"
    local ref="$3"
    local current_version
    local target_version

    current_version="$(go list -m -f '{{with .Replace}}{{.Version}}{{end}}' "$module")"
    target_version="$(go list -m -f '{{.Version}}' "$fork@$ref")"

    if [[ -z "$current_version" || -z "$target_version" ]]; then
        printf 'Failed to resolve %s replacement versions\n' "$module" >&2
        return 1
    fi
    if [[ "$current_version" == "$target_version" ]]; then
        printf '%s is current at %s\n' "$module" "$current_version"
        return
    fi

    printf 'Updating %s: %s -> %s\n' "$module" "$current_version" "$target_version"
    go mod edit "-replace=$module=$fork@$target_version"
}

cd "$SERVER_DIR"
update_replace github.com/sagernet/sing-box github.com/Throneproj/sing-box stable
update_replace github.com/xtls/xray-core github.com/throneproj/xray-core main

go mod tidy
