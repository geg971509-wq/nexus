#include "include/sys/Process.hpp"

#include <cstdlib>

using Configs_sys::ShouldContinuePreIpcWatch;
using Configs_sys::ShouldRelaunchPreIpcCore;

int main() {
    // Given a detached core that died before first IPC, matching launch token.
    if (!ShouldRelaunchPreIpcCore(/*prepare_exit=*/false,
                                  /*core_running=*/false,
                                  /*current_generation=*/3,
                                  /*watch_generation=*/3,
                                  /*watch_pid=*/4242,
                                  /*current_launched_pid=*/4242,
                                  /*pid_alive=*/false)) {
        return EXIT_FAILURE;
    }

    // Preparing exit must not relaunch.
    if (ShouldRelaunchPreIpcCore(true, false, 3, 3, 4242, 4242, false)) return EXIT_FAILURE;

    // IPC already up: disconnect recovery owns the lifecycle.
    if (ShouldRelaunchPreIpcCore(false, true, 3, 3, 4242, 4242, false)) return EXIT_FAILURE;

    // Superseded launch (recycle bumped generation).
    if (ShouldRelaunchPreIpcCore(false, false, 4, 3, 4242, 4242, false)) return EXIT_FAILURE;

    // Different pid than the one this watch was armed for.
    if (ShouldRelaunchPreIpcCore(false, false, 3, 3, 4242, 9999, false)) return EXIT_FAILURE;
    if (ShouldRelaunchPreIpcCore(false, false, 3, 3, 4242, 4242, true)) return EXIT_FAILURE;
    if (ShouldRelaunchPreIpcCore(false, false, 3, 3, 0, 0, false)) return EXIT_FAILURE;

    // Keep watching only while the same launched child is still alive and IPC is down.
    if (!ShouldContinuePreIpcWatch(false, false, 3, 3, 4242, 4242, true)) return EXIT_FAILURE;
    if (ShouldContinuePreIpcWatch(false, false, 3, 3, 4242, 4242, false)) return EXIT_FAILURE;
    if (ShouldContinuePreIpcWatch(false, true, 3, 3, 4242, 4242, true)) return EXIT_FAILURE;
    if (ShouldContinuePreIpcWatch(true, false, 3, 3, 4242, 4242, true)) return EXIT_FAILURE;
    if (ShouldContinuePreIpcWatch(false, false, 4, 3, 4242, 4242, true)) return EXIT_FAILURE;

    // Rate-limit: second exit inside 10s is rejected; later exits are allowed.
    if (!Configs_sys::AllowPreIpcRelaunch(/*now_ms=*/1000, /*last_relaunch_ms=*/0)) return EXIT_FAILURE;
    if (Configs_sys::AllowPreIpcRelaunch(/*now_ms=*/5000, /*last_relaunch_ms=*/1000)) return EXIT_FAILURE;
    if (!Configs_sys::AllowPreIpcRelaunch(/*now_ms=*/12000, /*last_relaunch_ms=*/1000)) return EXIT_FAILURE;

    return EXIT_SUCCESS;
}
