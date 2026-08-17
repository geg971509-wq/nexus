#!/usr/bin/env python3
"""Throneproj sing-box darwin process ownership patches (re-applied after go mod verify).

1) ProcessID: throng searcher had pid but left ConnectionOwner.ProcessID=0 when path fails.
2) TCP local-port fallback: exact 4-tuple often misses (Tun / NAT / dest rewrite); local
   IP+port is unique per host socket — same idea UDP already used.

Never touch sagernet/sing-box@version published modules.

Usage: sing-box-darwin-process-id.py <staged-sing-box-module-dir>
The dir is a writable copy staged by build.sh; the shared module cache under
~/go/pkg/mod is read-only and must stay byte-identical for `go mod verify`.
"""
import pathlib
import sys

if len(sys.argv) != 2:
    print("usage: sing-box-darwin-process-id.py <staged-sing-box-module-dir>")
    sys.exit(2)

target = pathlib.Path(sys.argv[1]) / "common" / "process" / "searcher_darwin_shared.go"
if not target.is_file():
    print("no throng searcher_darwin_shared.go at", target)
    sys.exit(1)
files = [target]

# --- patch A: set ProcessID before path lookup ---
old_pid = (
    "\t\tprocessPath, err := getExecPathFromPID(entry.pid)\n"
    "\t\tif err == nil {\n"
    "\t\t\towner.ProcessPath = processPath\n"
    "\t\t\treturn owner, nil\n"
    "\t\t}"
)
new_pid = (
    "\t\towner.ProcessID = entry.pid\n"
    "\t\tprocessPath, err := getExecPathFromPID(entry.pid)\n"
    "\t\tif err == nil {\n"
    "\t\t\towner.ProcessPath = processPath\n"
    "\t\t\treturn owner, nil\n"
    "\t\t}"
)

# --- patch B: allow TCP local-port fallback (remove UDP-only gate) ---
# Tabs match throng source (3 tabs inside for-loop body for if network).
old_match = (
    "\t\tif network != N.NetworkUDP {\n"
    "\t\t\tcontinue\n"
    "\t\t}\n"
    "\t\tif !hasLocalFallback && entry.localAddr == sourceAddr {\n"
    "\t\t\thasLocalFallback = true\n"
    "\t\t\tlocalFallback = entry\n"
    "\t\t}\n"
    "\t\tif !hasWildcardFallback && entry.localAddr.IsUnspecified() {\n"
    "\t\t\thasWildcardFallback = true\n"
    "\t\t\twildcardFallback = entry\n"
    "\t\t}"
)
new_match = (
    "\t\t// TCP+UDP: local IP+port uniquely identifies the socket when remote\n"
    "\t\t// dest in metadata does not match PCB (common under Tun / rewrite).\n"
    "\t\tif !hasLocalFallback && entry.localAddr == sourceAddr {\n"
    "\t\t\thasLocalFallback = true\n"
    "\t\t\tlocalFallback = entry\n"
    "\t\t}\n"
    "\t\tif network == N.NetworkUDP && !hasWildcardFallback && entry.localAddr.IsUnspecified() {\n"
    "\t\t\thasWildcardFallback = true\n"
    "\t\t\twildcardFallback = entry\n"
    "\t\t}"
)


def ensure_writable(p: pathlib.Path) -> None:
    try:
        p.chmod(p.stat().st_mode | 0o200)
    except OSError:
        pass


missed = []

for p in files:
    t = p.read_text()
    changed = False

    if "owner.ProcessID = entry.pid" not in t:
        if old_pid not in t:
            print("pattern miss (ProcessID)", p)
            missed.append("ProcessID")
        else:
            ensure_writable(p)
            t = t.replace(old_pid, new_pid, 1)
            changed = True
            print("patched ProcessID", p)
    else:
        print("ok ProcessID", p)

    match_ok = (
        "local IP+port uniquely identifies" in t
        or "if network == N.NetworkUDP && !hasWildcardFallback" in t
    )
    if match_ok:
        print("ok TCP-local-fallback", p)
    elif old_match not in t:
        print("pattern miss (TCP-local-fallback)", p)
        missed.append("TCP-local-fallback")
        # debug first miss only
        if "if network != N.NetworkUDP" in t:
            i = t.find("if network != N.NetworkUDP")
            print(" nearby:", repr(t[i - 40 : i + 200]))
    else:
        ensure_writable(p)
        t = t.replace(old_match, new_match, 1)
        changed = True
        print("patched TCP-local-fallback", p)

    if changed:
        p.write_text(t)

# A miss means upstream moved the code: the shipped Core would silently lose
# per-process routing. Fail the build instead of shipping a half-patched core.
if missed:
    print("FAILED to apply:", ", ".join(missed), file=sys.stderr)
    sys.exit(1)
