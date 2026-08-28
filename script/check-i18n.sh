#!/usr/bin/env bash
# Fail if the four I18N packs in app/qt/qml/I18n.qml drift apart.
#
# t() falls back to zh-CN on a missing key, so a dropped translation is invisible
# in testing and ships as Chinese text in the English UI. A duplicate key is worse:
# the later literal silently wins and the earlier one is unreachable.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/app/qt/qml/I18n.qml"

python3 - "$SRC" <<'PY'
from collections import Counter
from pathlib import Path
import re
import sys

src = Path(sys.argv[1]).read_text(encoding="utf-8")
start = src.find("readonly property var dict")
if start < 0:
    raise SystemExit("I18n dict not found")

packs: dict[str, list[str]] = {}
current = None
closed = False
for line in src[start:].splitlines():
    if re.fullmatch(r"    \}\)\s*", line):
        closed = True
        break
    match = re.fullmatch(r'        "([a-zA-Z-]+)":\s*\{\s*', line)
    if match:
        current = match.group(1)
        packs[current] = []
        continue
    match = re.fullmatch(r'            "([^"]+)":.*', line)
    if current and match:
        packs[current].append(match.group(1))
    elif current and re.fullmatch(r"        \},?\s*", line):
        current = None

if not closed:
    raise SystemExit("I18n dict never closed")
required = ["zh-CN", "en", "ru", "zh-TW"]
if list(packs) != required:
    raise SystemExit(f"expected language packs {required}, got {list(packs)}")

failed = False
for name, keys in packs.items():
    duplicates = sorted(key for key, count in Counter(keys).items() if count > 1)
    if duplicates:
        print(f"{name}: duplicate keys: {', '.join(duplicates)}", file=sys.stderr)
        failed = True

base = packs["zh-CN"]
for name, keys in packs.items():
    if name == "zh-CN":
        continue
    missing = sorted(set(base) - set(keys))
    extra = sorted(set(keys) - set(base))
    if missing:
        print(f"{name}: missing {len(missing)}: {', '.join(missing)}", file=sys.stderr)
    if extra:
        print(f"{name}: not in zh-CN {len(extra)}: {', '.join(extra)}", file=sys.stderr)
    failed = failed or bool(missing or extra)

root = Path(sys.argv[1]).parents[3]
ui_source = "\n".join(
    path.read_text(encoding="utf-8")
    for folder, patterns in (
        (root / "app/qt/qml", ("*.qml",)),
        (root / "app/qt/src", ("*.cpp", "*.h", "*.mm")),
    )
    for pattern in patterns
    for path in folder.glob(pattern)
    if path != Path(sys.argv[1])
)
unused = sorted(
    key for key in base
    if f'"{key}"' not in ui_source and f"'{key}'" not in ui_source
)
if unused:
    print(f"unused translation keys: {', '.join(unused)}", file=sys.stderr)
    failed = True

if failed:
    raise SystemExit("i18n packs are out of sync")
print(f"i18n ok: {len(packs)} packs x {len(base)} keys")
PY
