// Throne pins the standalone warp-client's device as sing-box's
// route.default_interface so proxy egress descends through the WARP underlay.
// That binding is what makes the stack two-layer, and it is also a liability: the
// device name must be parsed correctly, and if it changes the running config is
// dialing into nothing. These tests cover both halves.

#include "include/sys/Process.hpp"

#include <QString>
#include <cstdlib>

using Configs_sys::ParseWarpRuntimeInfo;
using Configs_sys::WarpStatus;

namespace {

// True when a config pinned to `pinned` must be rebuilt because the live device
// reported by warp-client is no longer the same one. Mirrors the rule in
// MainWindow::refreshWarpRuntimeStatus.
bool needsRebuild(const QString& pinned, const QString& live) {
    return !pinned.isEmpty() && live != pinned;
}

} // namespace

int main() {
    // --- device name reaches config generation ---
    {
        const auto alive = ParseWarpRuntimeInfo(
            "status: interface=utun171 pid=42 alive=true health=healthy transport=wg "
            "endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n");
        if (alive.status != WarpStatus::Alive) return EXIT_FAILURE;
        if (alive.interfaceName != "utun171") return EXIT_FAILURE;
    }
    {
        // Recovering still pins: the device exists and its name is stable across
        // rebuilds, so generation can use it without waiting for it to settle.
        const auto recovering = ParseWarpRuntimeInfo(
            "status: interface=utun171 pid=42 alive=true health=recovering transport=masque "
            "endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n");
        if (recovering.status != WarpStatus::Recovering) return EXIT_FAILURE;
        if (recovering.interfaceName != "utun171") return EXIT_FAILURE;
    }
    {
        // Stale tunnels still name their device: callers need it to tear down.
        const auto stale = ParseWarpRuntimeInfo(
            "status: interface=utun171 pid=42 alive=false health=healthy "
            "endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n");
        if (stale.status != WarpStatus::Stale) return EXIT_FAILURE;
        if (stale.interfaceName != "utun171") return EXIT_FAILURE;
    }
    {
        const auto down = ParseWarpRuntimeInfo("status: down (no state)\n");
        if (down.status != WarpStatus::Down) return EXIT_FAILURE;
        // Must be empty: a name here would be pinned as default_interface and
        // bind every dial to a device that does not exist.
        if (!down.interfaceName.isEmpty()) return EXIT_FAILURE;
    }
    {
        const auto garbage = ParseWarpRuntimeInfo("garbage");
        if (!garbage.interfaceName.isEmpty()) return EXIT_FAILURE;
    }
    {
        // Truncated line: no alive= field, so status is unknown, but the name must
        // still not be invented.
        const auto truncated = ParseWarpRuntimeInfo("status: interface=utun171 pid=42\n");
        if (truncated.status != WarpStatus::Unknown) return EXIT_FAILURE;
        if (truncated.interfaceName != "utun171") return EXIT_FAILURE;
    }
    {
        // The name must stop at whitespace, not swallow the rest of the line.
        const auto info = ParseWarpRuntimeInfo(
            "status: interface=utun9 pid=1 alive=true health=healthy transport=wg\n");
        if (info.interfaceName != "utun9") return EXIT_FAILURE;
    }

    // --- stale-pin detection ---
    // Nothing pinned: never rebuild, whatever the underlay is doing.
    if (needsRebuild("", "utun171")) return EXIT_FAILURE;
    if (needsRebuild("", "")) return EXIT_FAILURE;
    // Pinned and unchanged: no rebuild, or the poll would restart the core forever.
    if (needsRebuild("utun171", "utun171")) return EXIT_FAILURE;
    // Pinned but the device is gone: rebuild, else every dial targets a dead
    // interface. This is the case that only the poll can catch -- WARP dying on
    // its own never goes through the toggle.
    if (!needsRebuild("utun171", "")) return EXIT_FAILURE;
    // Pinned but renamed: same problem.
    if (!needsRebuild("utun171", "utun4")) return EXIT_FAILURE;

    // End to end: a Down report against a live pin must trigger a rebuild.
    {
        const auto down = ParseWarpRuntimeInfo("status: down (no state)\n");
        if (!needsRebuild("utun171", down.interfaceName)) return EXIT_FAILURE;
    }
    // ...and an unchanged alive report must not.
    {
        const auto alive = ParseWarpRuntimeInfo(
            "status: interface=utun171 pid=42 alive=true health=healthy transport=wg\n");
        if (needsRebuild("utun171", alive.interfaceName)) return EXIT_FAILURE;
    }

    // --- pure underlay pin decision (no RuntimeInfo() probe) ---
    using Configs_sys::WarpUnderlayInterfaceForConfig;
    if (WarpUnderlayInterfaceForConfig(true, true, WarpStatus::Alive, "utun172") != "utun172")
        return EXIT_FAILURE;
    if (WarpUnderlayInterfaceForConfig(true, true, WarpStatus::Recovering, "utun172") != "utun172")
        return EXIT_FAILURE;
    // Tun off / WARP off / wrong status → empty (must not pin a dead device).
    if (!WarpUnderlayInterfaceForConfig(false, true, WarpStatus::Alive, "utun172").isEmpty())
        return EXIT_FAILURE;
    if (!WarpUnderlayInterfaceForConfig(true, false, WarpStatus::Alive, "utun172").isEmpty())
        return EXIT_FAILURE;
    if (!WarpUnderlayInterfaceForConfig(true, true, WarpStatus::Down, "utun172").isEmpty())
        return EXIT_FAILURE;
    if (!WarpUnderlayInterfaceForConfig(true, true, WarpStatus::Stale, "utun172").isEmpty())
        return EXIT_FAILURE;
    if (!WarpUnderlayInterfaceForConfig(true, true, WarpStatus::Unknown, "utun172").isEmpty())
        return EXIT_FAILURE;

    return EXIT_SUCCESS;
}
