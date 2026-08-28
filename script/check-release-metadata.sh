#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/app/backend/Cargo.toml"
INFO_PLIST="$ROOT/app/qt/Info.plist"
README="$ROOT/README.md"

version="$(sed -n '/^\[package\]$/,/^\[/s/^version = "\([^"]*\)"/\1/p' "$CARGO_TOML" | head -n 1)"
[[ -n "$version" ]] || { echo "cannot read package version from $CARGO_TOML" >&2; exit 1; }

grep -Fq '<string>'"$version"'</string>' "$INFO_PLIST" \
  || { echo "Info.plist version does not match Cargo.toml ($version)" >&2; exit 1; }
grep -Fq '| Version | '"$version"' |' "$README" \
  || { echo "README version does not match Cargo.toml ($version)" >&2; exit 1; }
grep -Fq '## Capabilities ('"$version"')' "$README" \
  || { echo "README capabilities version does not match Cargo.toml ($version)" >&2; exit 1; }

printf 'release metadata: %s\n' "$version"
