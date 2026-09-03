#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/app/backend/Cargo.toml"
DEFAULTS_RS="$ROOT/app/backend/src/defaults.rs"
CMAKE_LISTS="$ROOT/app/qt/CMakeLists.txt"
INFO_PLIST="$ROOT/app/qt/Info.plist"
README="$ROOT/README.md"

version="$(sed -n '/^\[package\]$/,/^\[/s/^version = "\([^"]*\)"/\1/p' "$CARGO_TOML" | head -n 1)"
[[ -n "$version" ]] || { echo "cannot read package version from $CARGO_TOML" >&2; exit 1; }

identifier="$(sed -n 's/^pub const APP_IDENTIFIER: &str = "\([^"]*\)";/\1/p' "$DEFAULTS_RS" | head -n 1)"
[[ -n "$identifier" ]] || { echo "cannot read APP_IDENTIFIER from $DEFAULTS_RS" >&2; exit 1; }

deployment_target="$(sed -n 's/.*set(CMAKE_OSX_DEPLOYMENT_TARGET "\([^"]*\)".*/\1/p' "$CMAKE_LISTS" | head -n 1)"
[[ -n "$deployment_target" ]] || { echo "cannot read deployment target from $CMAKE_LISTS" >&2; exit 1; }

plist_value() {
  local key="$1"
  awk -v key="$key" '
    $0 ~ "<key>" key "</key>" {
      if (getline > 0) {
        value=$0
        sub(/^[[:space:]]*<string>/, "", value)
        sub(/<\/string>[[:space:]]*$/, "", value)
        print value
        exit
      }
    }
  ' "$INFO_PLIST"
}

plist_short_version="$(plist_value CFBundleShortVersionString)"
plist_bundle_version="$(plist_value CFBundleVersion)"
plist_identifier="$(plist_value CFBundleIdentifier)"
plist_min_system="$(plist_value LSMinimumSystemVersion)"

[[ "$plist_short_version" == "$version" ]] \
  || { echo "Info.plist short version '$plist_short_version' does not match Cargo.toml ($version)" >&2; exit 1; }
[[ "$plist_bundle_version" == "$version" ]] \
  || { echo "Info.plist bundle version '$plist_bundle_version' does not match Cargo.toml ($version)" >&2; exit 1; }
grep -Fq '| Version | '"$version"' |' "$README" \
  || { echo "README version does not match Cargo.toml ($version)" >&2; exit 1; }
grep -Fq '## Capabilities ('"$version"')' "$README" \
  || { echo "README capabilities version does not match Cargo.toml ($version)" >&2; exit 1; }

[[ "$plist_identifier" == "$identifier" ]] \
  || { echo "Info.plist bundle id '$plist_identifier' does not match defaults.rs ($identifier)" >&2; exit 1; }
grep -Fq '| Bundle ID | `'$identifier'` |' "$README" \
  || { echo "README bundle id does not match defaults.rs ($identifier)" >&2; exit 1; }

[[ "$plist_min_system" == "$deployment_target" ]] \
  || { echo "Info.plist minimum system '$plist_min_system' does not match CMake deployment target ($deployment_target)" >&2; exit 1; }
readme_macos_major="${deployment_target%%.*}"
grep -Fq '| macOS '"$readme_macos_major"'+ | arm64 |' "$README" \
  || { echo "README macOS minimum does not match deployment target ($deployment_target)" >&2; exit 1; }

printf 'release metadata: version=%s bundle_id=%s macos=%s+\n' \
  "$version" "$identifier" "$deployment_target"
