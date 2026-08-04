// The WARP underlay is a root helper driven by an unelevated GUI. Three things
// about its paths and its elevated command line must hold, or the tunnel can
// never come up.
//
// 1. The operational log must NOT live inside the WARP data directory. The
//    helper runs as root and Go's EnsureDir chowns that directory back to the
//    user, but a directory left root-owned by an older build makes the GUI's own
//    QFile::open on the log fail with EACCES -- and that happens before
//    osascript is launched, so the app can never obtain the root it needs to
//    repair itself. Keeping the log one level up, in a directory the user always
//    owns, breaks that deadlock and lets EnsureDir do the repair.
//
// 2. Every elevated command needs its environment inlined. osascript's
//    `do shell script ... with administrator privileges` spawns a fresh root
//    shell that inherits nothing from QProcess::setProcessEnvironment, so a bare
//    command resolves a different data directory and cannot read state.json.
//
// 3. The data directory must not collide with the warp-client binary. Non-package
//    builds leave the base path at the app dir, where <appdir>/warp-client is the
//    binary itself; mkdir then returns EEXIST and open returns ENOTDIR.

#include "include/sys/Process.hpp"

#include <QString>
#include <cstdio>

using Configs_sys::BuildElevatedShell;
using Configs_sys::WarpDataDirCollides;
using Configs_sys::WarpDataDirIn;
using Configs_sys::WarpEnvPrefix;
using Configs_sys::WarpLogPathIn;
using Configs_sys::WarpShellQuote;

namespace {

int failures = 0;

void check(bool ok, const char *what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        ++failures;
    }
}

void checkContains(const QString &haystack, const QString &needle, const char *what) {
    if (!haystack.contains(needle)) {
        std::fprintf(stderr, "FAIL: %s\n  wanted: %s\n  in: %s\n",
                     what, qUtf8Printable(needle), qUtf8Printable(haystack));
        ++failures;
    }
}

} // namespace

int main() {
    const QString base = "/Users/king/Library/Preferences/Throne";

    // --- 1. log lives outside the data dir ---
    {
        const auto dataDir = WarpDataDirIn(base);
        const auto logPath = WarpLogPathIn(base);
        check(!logPath.startsWith(dataDir + "/"),
              "log must NOT be inside the data dir: a root-owned data dir would "
              "make the unelevated GUI fail before it can ask for root");
        check(logPath.startsWith(base + "/"), "log still lives under the base path");
        // The data dir name is load-bearing: renaming it orphans the user's
        // account.json, and the helper has no deregister path to reclaim the
        // Cloudflare device it registered.
        check(dataDir == base + "/warp-client", "data dir name is unchanged");
    }

    // --- 2. shell quoting is injection-safe ---
    {
        check(WarpShellQuote("plain") == "'plain'", "plain value is single-quoted");
        check(WarpShellQuote("a'b") == "'a'\\''b'", "embedded single quote is escaped");
        check(!WarpShellQuote("/tmp/x; rm -rf /").contains("'; rm"),
              "semicolon cannot escape the quoting");
        check(WarpShellQuote("/has space/x") == "'/has space/x'", "spaces stay inside quotes");
    }

    // --- 3. env prefix carries every variable the helper needs ---
    {
        const auto dataDir = WarpDataDirIn(base);
        const auto logPath = WarpLogPathIn(base);
        const auto env = WarpEnvPrefix(dataDir, logPath, 501, 20);
        checkContains(env, "THRONE_WARP_DATA_DIR=" + WarpShellQuote(dataDir), "data dir inlined");
        checkContains(env, "THRONE_WARP_LOG_PATH=" + WarpShellQuote(logPath), "log path inlined");
        // Go's openOperationalLog hard-fails with "WARP log owner mismatch" when
        // the log's uid is not this value, so it is not optional.
        checkContains(env, "THRONE_WARP_OWNER_UID='501'", "owner uid inlined");
        checkContains(env, "THRONE_WARP_OWNER_GID='20'", "owner gid inlined");
        check(env.startsWith("env "), "prefix is a usable env(1) invocation");
    }

    // --- 4. both up and down carry that env ---
    {
        const QString binary = "/Applications/Throne.app/Contents/MacOS/warp-client";
        const auto dataDir = WarpDataDirIn(base);
        const auto logPath = WarpLogPathIn(base);

        for (const char *cmd : {"up", "down"}) {
            const auto shell = BuildElevatedShell(binary, cmd, dataDir, logPath, 501, 20);
            checkContains(shell, "THRONE_WARP_DATA_DIR=", QString("%1 carries the data dir").arg(cmd).toUtf8().constData());
            checkContains(shell, WarpShellQuote(binary), "binary path is quoted");
            checkContains(shell, WarpShellQuote(QString(cmd)), "command is quoted");
            // No mkdir/chown prologue: running those as root before Go's
            // Lstat symlink check would let a pre-planted symlink at the data dir
            // turn this into a root file-creation primitive.
            check(!shell.contains("mkdir"), "no mkdir prologue (symlink escalation)");
            check(!shell.contains("chown"), "no chown prologue (symlink escalation)");
        }
    }

    // --- 5. binary collision is detected ---
    {
        const QString appDir = "/Applications/Throne.app/Contents/MacOS";
        check(WarpDataDirCollides(WarpDataDirIn(appDir), appDir + "/warp-client"),
              "data dir equal to the binary path is a collision");
        check(!WarpDataDirCollides(WarpDataDirIn(base), appDir + "/warp-client"),
              "a data dir elsewhere is not a collision");
    }

    // --- 6. status classification cannot be spoofed by error payloads ---
    {
        using Configs_sys::ParseWarpStatusOutput;
        using Configs_sys::WarpStatus;

        // An absent state file is the only thing that means "down".
        check(ParseWarpStatusOutput("status: down (no state)\n") == WarpStatus::Down,
              "absent state file classifies as Down");

        // An unreadable state file must NOT classify as Down. WarpProcess::Down()
        // returns success early on Down, so misclassifying here leaves a live
        // tunnel's routes installed while the UI reports WARP off.
        check(ParseWarpStatusOutput(
                  "status: error=\"open /x/state.json: permission denied\"\n") == WarpStatus::Unknown,
              "unreadable state file does not classify as Down");

        // The error payload carries a filesystem path, and a path may contain the
        // literal phrase. Classification must key on the line, not on a substring
        // found anywhere in the output.
        check(ParseWarpStatusOutput(
                  "status: error=\"open /tmp/status: down/state.json: permission denied\"\n")
                  == WarpStatus::Unknown,
              "a path containing the down phrase cannot spoof Down");
        // Every field must be whitespace-terminated for this to discriminate: the
        // health regex is greedy, so "health=healthy/state.json:" never equals
        // "healthy" and the case would pass against a substring parser too.
        check(ParseWarpStatusOutput(
                  "status: error=\"open /tmp/status: interface=utun9 alive=true health=healthy /x: denied\"\n")
                  == WarpStatus::Unknown,
              "a path containing the interface phrase cannot spoof a live tunnel");

        // Real status lines still classify correctly.
        check(ParseWarpStatusOutput(
                  "status: interface=utun172 pid=42 alive=true health=healthy transport=wg\n")
                  == WarpStatus::Alive,
              "a live tunnel still classifies as Alive");
    }

    // --- 7. status poll gate ---
    {
        using Configs_sys::ShouldPollWarpStatus;
        using Configs_sys::WarpStatus;

        check(ShouldPollWarpStatus(true, WarpStatus::Down),
              "enable_warp on always polls");
        check(ShouldPollWarpStatus(false, WarpStatus::Unknown),
              "startup Unknown still gets one probe");
        check(ShouldPollWarpStatus(false, WarpStatus::Alive),
              "Alive with enable_warp off still watches teardown");
        check(!ShouldPollWarpStatus(false, WarpStatus::Down),
              "enable_warp off + Down stops the empty fork");
        check(ShouldPollWarpStatus(false, WarpStatus::Down, true),
              "pinned underlay still polls even when enable_warp is off");
    }

    // --- 8. readiness wait is abortable ---
    {
        using Configs_sys::WarpStatus;
        using Configs_sys::WarpWaitAction;
        using Configs_sys::WarpWaitStep;

        check(WarpWaitStep(WarpStatus::Alive, false, false) == WarpWaitAction::Ready,
              "a live tunnel is ready");
        // The readiness poll runs up to a minute. Without this the user's Cancel
        // click cannot land until the whole timeout elapses, which is the same
        // as having no Cancel at all.
        check(WarpWaitStep(WarpStatus::Recovering, true, false) == WarpWaitAction::Abort,
              "cancel aborts the wait instead of running out the timeout");
        // Cancel must not discard a tunnel that already came up: the caller
        // still has to see it so the toolbar and the config pin stay truthful.
        check(WarpWaitStep(WarpStatus::Alive, true, false) == WarpWaitAction::Ready,
              "a tunnel that came up wins over a late cancel");
        check(WarpWaitStep(WarpStatus::Down, false, true) == WarpWaitAction::HelperGone,
              "an existing helper that went down ends the wait");
        check(WarpWaitStep(WarpStatus::Stale, false, true) == WarpWaitAction::HelperGone,
              "an existing helper left stale ends the wait");
        check(WarpWaitStep(WarpStatus::Down, false, false) == WarpWaitAction::KeepWaiting,
              "a fresh up is still starting, keep waiting");
        check(WarpWaitStep(WarpStatus::Unknown, false, true) == WarpWaitAction::KeepWaiting,
              "an unreadable status is not proof the helper died");
    }

    if (failures > 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    std::printf("warp_paths_test: all checks passed\n");
    return 0;
}
