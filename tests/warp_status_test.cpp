#include "include/sys/Process.hpp"

#include <cstdlib>

using Configs_sys::ParseWarpRuntimeInfo;
using Configs_sys::ParseWarpStatusOutput;
using Configs_sys::WarpStatus;

int main() {
    if (ParseWarpStatusOutput("status: down (no state)\n") != WarpStatus::Down) return EXIT_FAILURE;
    const auto masque = ParseWarpRuntimeInfo("status: interface=utun5 pid=123 alive=true health=healthy transport=masque endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n");
    if (masque.status != WarpStatus::Alive || masque.transport != "masque") return EXIT_FAILURE;
    const auto wireguard = ParseWarpRuntimeInfo("status: interface=utun5 pid=123 alive=true health=recovering transport=wg endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n");
    if (wireguard.status != WarpStatus::Recovering || wireguard.transport != "wg") return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=true health=healthy endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Alive) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=true health=starting endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Recovering) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=true health=recovering endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Recovering) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=false health=healthy endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Stale) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=true health=broken endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Unknown) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=true health= endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Unknown) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123 alive=true endpoint_ip=1.1.1.1 old_gw=192.168.1.1 routes=[]\n")
        != WarpStatus::Alive) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("garbage") != WarpStatus::Unknown) return EXIT_FAILURE;
    if (ParseWarpStatusOutput("status: interface=utun5 pid=123\n") != WarpStatus::Unknown) return EXIT_FAILURE;
    if (Configs_sys::FormatWarpExitError("panic: bad offset\n", "ignored", 1) != "panic: bad offset") return EXIT_FAILURE;
    if (Configs_sys::FormatWarpExitError("", "route failed\n", 1) != "route failed") return EXIT_FAILURE;
    if (Configs_sys::FormatWarpExitError("", "", 7) != "WARP helper exited with code 7") return EXIT_FAILURE;
    return EXIT_SUCCESS;
}
