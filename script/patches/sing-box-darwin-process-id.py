#!/usr/bin/env python3
"""Ensure throng sing-box darwin ConnectionOwner sets ProcessID from entry.pid."""
import glob
import os
import pathlib
import sys

patterns = [
    os.path.expanduser(
        "~/go/pkg/mod/github.com/!throneproj/sing-box@*/common/process/searcher_darwin_shared.go"
    ),
    os.path.expanduser(
        "~/go/pkg/mod/github.com/*/sing-box@*/common/process/searcher_darwin_shared.go"
    ),
]
files = []
seen = set()
for pat in patterns:
    for p in glob.glob(pat):
        if p not in seen and os.path.isfile(p):
            seen.add(p)
            files.append(pathlib.Path(p))

if not files:
    print("no throng searcher_darwin_shared.go found; skip")
    sys.exit(0)

old = (
    "\t\tprocessPath, err := getExecPathFromPID(entry.pid)\n"
    "\t\tif err == nil {\n"
    "\t\t\towner.ProcessPath = processPath\n"
    "\t\t\treturn owner, nil\n"
    "\t\t}"
)
new = (
    "\t\towner.ProcessID = entry.pid\n"
    "\t\tprocessPath, err := getExecPathFromPID(entry.pid)\n"
    "\t\tif err == nil {\n"
    "\t\t\towner.ProcessPath = processPath\n"
    "\t\t\treturn owner, nil\n"
    "\t\t}"
)

for p in files:
    t = p.read_text()
    if "owner.ProcessID = entry.pid" in t:
        print("ok", p)
        continue
    if old not in t:
        print("pattern miss", p)
        continue
    try:
        p.chmod(p.stat().st_mode | 0o200)
    except OSError:
        pass
    p.write_text(t.replace(old, new, 1))
    print("patched", p)
