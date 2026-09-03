#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/app/backend/Cargo.toml"
DEFAULTS_RS="$ROOT/app/backend/src/defaults.rs"
INFO_PLIST="$ROOT/app/qt/Info.plist"
README="$ROOT/README.md"

version="$(sed -n '/^\[package\]$/,/^\[/s/^version = "\([^"]*\)"/\1/p' "$CARGO_TOML" | head -n 1)"
[[ -n "$version" ]] || { echo "cannot read package version from $CARGO_TOML" >&2; exit 1; }

identifier="$(sed -n 's/^pub const APP_IDENTIFIER: &str = "\([^"]*\)";/\1/p' "$DEFAULTS_RS" | head -n 1)"
[[ -n "$identifier" ]] || { echo "cannot read APP_IDENTIFIER from $DEFAULTS_RS" >&2; exit 1; }

grep -Fq '<string>'"$version"'</string>' "$INFO_PLIST" \
  || { echo "Info.plist version does not match Cargo.toml ($version)" >&2; exit 1; }
grep -Fq '| Version | '"$version"' |' "$README" \
  || { echo "README version does not match Cargo.toml ($version)" >&2; exit 1; }
grep -Fq '## Capabilities ('"$version"')' "$README" \
  || { echo "README capabilities version does not match Cargo.toml ($version)" >&2; exit 1; }

grep -Fq '<string>'"$identifier"'</string>' "$INFO_PLIST" \
  || { echo "Info.plist bundle id does not match defaults.rs ($identifier)" >&2; exit 1; }
grep -Fq '| Bundle ID | `'$identifier'` |' "$README" \
  || { echo "README bundle id does not match defaults.rs ($identifier)" >&2; exit 1; }

printf 'release metadata: version=%s bundle_id=%s\n' "$version" "$identifier"
