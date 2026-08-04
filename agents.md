# Throne Engineering Direction

## Architecture

- The Qt/C++ application owns desktop UI, configuration translation, and SQLite persistence.
- `core/server` owns proxy runtime execution. `core/server/gen/libcore.proto` is the RPC boundary between the application and the Go core.
- Sing-box is the primary runtime and configuration path. New features target sing-box first; Xray changes are compatibility work and require a concrete compatibility case plus focused tests.
- Repository code reports domain and persistence results. UI callers own presentation and active-profile orchestration.

## Lifecycle Constraint

- MainWindow-owned operations must reject work during teardown and join before the UI is deleted.
- Legacy TrafficLooper and connection producers are detached infinite loops that call global MainWindow callbacks. Keep those callbacks and the global pointer valid until the producers gain cooperative stop and join; do not describe that cleanup as complete.

## Code Knowledge Graph

- `.cbmignore` is the repository policy for excluding vendored code, local Qt sources, build output, and agent state.
- Use the canonical project name `ThronePrivate`. A private graph must contain no `File` nodes under `3rdparty/`, `qt6/`, `bin/`, `build/`, `.codegraph/`, or `.omo/`.
- The installed codebase-memory-mcp binary is a pinned custom build. Running its upstream updater replaces the custom exclusion default.
- Pinned build: `v0.9.0+private.1` (upstream commit `b637e3330c96cfe452da623db068c241aaa3ec01` plus moving `third_party`/`thirdparty`/`3rdparty`/`external` from `FAST_SKIP_DIRS` to `ALWAYS_SKIP_DIRS` in `src/discover/discover.c`). Source tree lives at `~/tools/codebase-memory-mcp`; installed binary SHA-256 is `6202193c87f290773b486be18f5a9d5ed06537f5c3f6a7283a59a46f37f6b424` (rebuilt 2026-07-18 after the upstream updater overwrote it).

## macOS Build

1. Run `script/bootstrap_macos.sh` once to install tools and build static Qt.
2. Run `build.sh` for repeat release builds. It installs nothing and verifies pinned generated inputs and release artifacts.

## Verification

- C++: configure with `-DBUILD_TESTING=ON`, build the named test targets, then run `ctest --output-on-failure`.
- Go: generate `core/server/gen` from `libcore.proto`, then run race-enabled tests on the changed private packages.
- Release tooling: run `bash -n build.sh script/bootstrap_macos.sh script/build_qt_static_macos.sh`.
