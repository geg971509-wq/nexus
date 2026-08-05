cmake_minimum_required(VERSION 3.20)

if (APPLE)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "26.0" CACHE STRING "Minimum macOS version" FORCE)
endif ()

project(Throne VERSION 0.1)
if(APPLE)
    enable_language(OBJCXX)
endif()

include(CTest)

set(CMAKE_INCLUDE_CURRENT_DIR ON)
include_directories("${CMAKE_SOURCE_DIR}/include/global")

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
if(MSVC)
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
    string(REPLACE "/Zi" "/Z7" CMAKE_CXX_FLAGS_RELWITHDEBINFO "${CMAKE_CXX_FLAGS_RELWITHDEBINFO}")
    string(REPLACE "/Zi" "/Z7" CMAKE_C_FLAGS_RELWITHDEBINFO "${CMAKE_C_FLAGS_RELWITHDEBINFO}")
endif()

find_package(Qt6 REQUIRED COMPONENTS Widgets Network LinguistTools DBus)

#### Platform Variables ####
if (APPLE)
    include("cmake/macos/macos.cmake")
elseif (WIN32)
    include("cmake/windows/windows.cmake")
else ()
    include("cmake/linux/linux.cmake")
endif ()

#### For Dumps ####
if (NOT MSVC)
    add_compile_options(-fno-omit-frame-pointer)
endif()

#### Compiler Warnings ####
if (MSVC)
    add_compile_options(/W4)
else()
    add_compile_options(-Wall -Wextra)
endif()
# Note: 3rdparty targets added via add_subdirectory inherit these but can override with target_compile_options

#### default prefix path ####

message("[CMAKE_PREFIX_PATH] ${CMAKE_PREFIX_PATH}")

# for some cross toolchain
list(APPEND CMAKE_FIND_ROOT_PATH ${CMAKE_PREFIX_PATH})
message("[CMAKE_FIND_ROOT_PATH] ${CMAKE_FIND_ROOT_PATH}")

include("cmake/print.cmake")
include("cmake/nkr.cmake")
include("cmake/QHotkey.cmake")

find_package(Threads)

if (NKR_PACKAGE OR NKR_PACKAGE_MACOS)
    nkr_add_compile_definitions(NKR_CPP_USE_APPDATA)
endif ()

include("cmake/myproto.cmake")
list(APPEND NKR_EXTERNAL_TARGETS myproto)

set(BUILD_SHARED_LIBS OFF)
list(APPEND NKR_EXTERNAL_TARGETS qhotkey)

# Sources
set(PROJECT_SOURCES
        ${PLATFORM_SOURCES}

        src/main.cpp
        src/global/Configs.cpp
        include/global/CorePath.hpp
        src/global/Utils.cpp
        src/global/Version.cpp
        include/global/PeriodicRunner.hpp
        src/global/PeriodicRunner.cpp
        src/global/HTTPRequestHelper.cpp
        src/global/DeviceDetailsHelper.cpp
        src/global/CountryHelper.cpp

        3rdparty/qrcodegen.cpp
        3rdparty/QtExtKeySequenceEdit.cpp
        3rdparty/QrDecoder.cpp

        3rdparty/qv2ray/v2/ui/LogHighlighter.cpp
        3rdparty/qv2ray/v2/ui/QvAutoCompleteTextEdit.cpp
        3rdparty/qv2ray/v2/ui/widgets/common/QJsonModel.cpp
        3rdparty/qv2ray/v2/ui/widgets/editors/w_JsonEditor.cpp
        3rdparty/qv2ray/v2/ui/widgets/editors/w_JsonEditor.hpp
        3rdparty/qv2ray/v2/ui/widgets/editors/w_JsonEditor.ui
        3rdparty/qv2ray/v2/ui/widgets/speedchart/SpeedWidget.cpp
        3rdparty/qv2ray/v2/ui/widgets/speedchart/SpeedWidget.hpp
        3rdparty/qv2ray/v2/proxy/QvProxyConfigurator.cpp

        3rdparty/quirc/decode.c
        3rdparty/quirc/identify.c
        3rdparty/quirc/quirc.c
        3rdparty/quirc/version_db.c

        3rdparty/SQLiteCpp/src/Backup.cpp
        3rdparty/SQLiteCpp/src/Column.cpp
        3rdparty/SQLiteCpp/src/Database.cpp
        3rdparty/SQLiteCpp/src/Exception.cpp
        3rdparty/SQLiteCpp/src/Savepoint.cpp
        3rdparty/SQLiteCpp/src/sqlite3.c
        3rdparty/SQLiteCpp/src/Statement.cpp
        3rdparty/SQLiteCpp/src/Transaction.cpp

        src/api/RPC.cpp

        src/stats/traffic/TrafficLooper.cpp
        include/stats/traffic/TrafficStatsManager.hpp
        src/stats/traffic/TrafficStatsManager.cpp

        include/configs/sub/GroupUpdater.hpp
        src/configs/sub/GroupUpdater.cpp
        include/configs/sub/RouteUpdater.hpp
        src/configs/sub/RouteUpdater.cpp
        src/configs/sub/clash.cpp

        include/sys/Process.hpp
        src/sys/Process.cpp

        include/sys/ProcessMetrics.hpp
        src/sys/ProcessMetrics.cpp

        include/sys/UrlScheme.hpp
        src/sys/UrlScheme.cpp

        include/ui/setting/ThemeManager.hpp
        src/ui/setting/ThemeManager.cpp
        src/ui/setting/Icon.cpp

        src/ui/mainwindow_rpc.cpp
        src/ui/mainwindow_warp.cpp
        src/ui/mainwindow.cpp
        include/ui/mainwindow.h
        include/ui/mainwindow.ui
        include/ui/widget/StartStopButton.hpp
        src/ui/widget/StartStopButton.cpp
        include/ui/widget/TrayProfileSelector.hpp
        src/ui/widget/TrayProfileSelector.cpp
        include/ui/utils/DataViewHtmlGenerator.h
        src/ui/utils/DataViewHtmlGenerator.cpp
        include/ui/utils/ProfilesTableModel.h
        src/ui/utils/ProfilesTableModel.cpp
        include/ui/utils/ProfilesFilterProxyModel.h
        src/ui/utils/ProfilesFilterProxyModel.cpp
        include/ui/utils/ProfilesTableView.h
        src/ui/utils/ProfilesTableView.cpp
        include/ui/utils/ProfilesTableVerticalHeader.h
        src/ui/utils/ProfilesTableVerticalHeader.cpp
        include/ui/utils/ProfilesTableFilterHeader.h

        include/ui/profile/dialog_edit_profile.h
        src/ui/profile/dialog_edit_profile.cpp
        src/ui/profile/dialog_edit_profile_xhttp.cpp
        include/ui/profile/dialog_edit_profile.ui
        include/ui/group/dialog_edit_group.h
        src/ui/group/dialog_edit_group.cpp
        include/ui/group/dialog_edit_group.ui

        include/ui/profile/edit_chain.h
        src/ui/profile/edit_chain.cpp
        include/ui/profile/edit_chain.ui
        include/ui/profile/edit_autoselector.h
        src/ui/profile/edit_autoselector.cpp
        include/ui/profile/edit_autoselector.ui
        include/ui/profile/edit_shadowsocks.h
        src/ui/profile/edit_shadowsocks.cpp
        include/ui/profile/edit_shadowsocks.ui
        include/ui/profile/edit_vmess.h
        src/ui/profile/edit_vmess.cpp
        include/ui/profile/edit_vmess.ui
        include/ui/profile/edit_vless.h
        src/ui/profile/edit_vless.cpp
        include/ui/profile/edit_vless.ui
        include/ui/profile/edit_anytls.h
        src/ui/profile/edit_anytls.cpp
        include/ui/profile/edit_anytls.ui
        include/ui/profile/edit_mieru.h
        src/ui/profile/edit_mieru.cpp
        include/ui/profile/edit_mieru.ui
        include/ui/profile/edit_tailscale.h
        src/ui/profile/edit_tailscale.cpp
        include/ui/profile/edit_tailscale.ui
        include/ui/profile/edit_trojan.h
        src/ui/profile/edit_trojan.cpp
        include/ui/profile/edit_trojan.ui
        include/ui/profile/edit_http.h
        src/ui/profile/edit_http.cpp
        include/ui/profile/edit_http.ui
        include/ui/profile/edit_socks.h
        src/ui/profile/edit_socks.cpp
        include/ui/profile/edit_socks.ui
        include/ui/profile/edit_hysteria.h
        src/ui/profile/edit_hysteria.cpp
        include/ui/profile/edit_hysteria.ui
        include/ui/profile/edit_tuic.h
        src/ui/profile/edit_tuic.cpp
        include/ui/profile/edit_tuic.ui
        include/ui/profile/edit_juicity.h
        src/ui/profile/edit_juicity.cpp
        include/ui/profile/edit_juicity.ui
        include/ui/profile/edit_trusttunnel.h
        src/ui/profile/edit_trusttunnel.cpp
        include/ui/profile/edit_trusttunnel.ui
        include/ui/profile/edit_shadowtls.h
        src/ui/profile/edit_shadowtls.cpp
        include/ui/profile/edit_shadowtls.ui
        include/ui/profile/edit_naive.h
        src/ui/profile/edit_naive.cpp
        include/ui/profile/edit_naive.ui

        include/ui/profile/edit_custom.h
        src/ui/profile/edit_custom.cpp
        include/ui/profile/edit_custom.ui

        include/ui/profile/edit_extra_core.h
        src/ui/profile/edit_extra_core.cpp
        include/ui/profile/edit_extra_core.ui

        include/ui/profile/edit_wireguard.h
        src/ui/profile/edit_wireguard.cpp
        include/ui/profile/edit_wireguard.ui

        src/ui/setting/dialog_basic_settings.cpp
        include/ui/setting/dialog_basic_settings.h
        include/ui/setting/dialog_basic_settings.ui

        src/ui/group/dialog_manage_groups.cpp
        include/ui/group/dialog_manage_groups.h
        include/ui/group/dialog_manage_groups.ui

        src/ui/setting/dialog_manage_routes.cpp
        include/ui/setting/dialog_manage_routes.h
        include/ui/setting/dialog_manage_routes.ui

        src/ui/setting/dialog_vpn_settings.cpp
        include/ui/setting/dialog_vpn_settings.h
        include/ui/setting/dialog_vpn_settings.ui

        src/ui/stats/dialog_traffic_stats.cpp
        include/ui/stats/dialog_traffic_stats.h
        include/ui/stats/dialog_traffic_stats.ui
        src/ui/stats/TrafficChartWidget.cpp
        include/ui/stats/TrafficChartWidget.h

        src/ui/stats/dialog_runtime_stats.cpp
        include/ui/stats/dialog_runtime_stats.h
        include/ui/stats/dialog_runtime_stats.ui
        src/ui/stats/MiniChartWidget.cpp
        include/ui/stats/MiniChartWidget.h

        src/ui/setting/dialog_hotkey.cpp
        include/ui/setting/dialog_hotkey.h
        include/ui/setting/dialog_hotkey.ui

        src/ui/profile/ProxyItem.cpp
        include/ui/profile/ProxyItem.h
        include/ui/profile/ProxyItem.ui
        src/ui/group/GroupItem.cpp
        include/ui/group/GroupItem.h
        include/ui/group/GroupItem.ui
        src/ui/setting/RouteItem.cpp
        include/ui/setting/RouteItem.h
        include/ui/setting/RouteItem.ui

        src/ui/setting/RawRouteItem.cpp
        include/ui/setting/RawRouteItem.h

        res/icons.qrc
        ${QV2RAY_RC}
        res/darkstyle.qrc
        res/theme/feiyangqingyun/qss.qrc
        src/ui/profile/edit_ssh.cpp
        include/ui/profile/edit_ssh.h
        include/ui/profile/edit_ssh.ui
        include/sys/macos/MacOS.h

        src/sys/linux/LinuxCap.cpp

        src/sys/Process.cpp

        src/sys/macos/MacOS.cpp

        include/ui/mainwindow_interface.h
        include/stats/connections/connectionLister.hpp
        src/stats/connectionLister/connectionLister.cpp
        include/sys/windows/eventHandler.h
        include/configs/baseConfig.h
        include/configs/common/DialFields.h
        include/configs/common/TLS.h
        include/configs/common/multiplex.h
        include/configs/common/transport.h
        include/configs/common/Outbound.h
        include/configs/common/OutboundFactory.h

        # common impl
        src/configs/common/DialFields.cpp
        src/configs/common/TLS.cpp
        src/configs/common/multiplex.cpp
        src/configs/common/transport.cpp
        src/configs/common/Outbound.cpp
        src/configs/common/OutboundFactory.cpp
        include/configs/outbounds/socks.h
        include/configs/outbounds/http.h
        include/configs/outbounds/shadowsocks.h
        include/configs/outbounds/vmess.h
        include/configs/outbounds/trojan.h
        include/configs/outbounds/hysteria.h
        include/configs/outbounds/vless.h
        include/configs/outbounds/tuic.h
        include/configs/outbounds/juicity.h
        include/configs/outbounds/trusttunnel.h
        include/configs/outbounds/naive.h
        include/configs/outbounds/shadowtls.h
        include/configs/outbounds/anyTLS.h
        include/configs/outbounds/mieru.h
        include/configs/outbounds/ssh.h
        
        # outbounds impl
        src/configs/outbounds/anyTLS.cpp
        src/configs/outbounds/mieru.cpp
        src/configs/outbounds/http.cpp
        src/configs/outbounds/hysteria.cpp
        src/configs/outbounds/shadowsocks.cpp
        src/configs/outbounds/socks.cpp
        src/configs/outbounds/ssh.cpp
        src/configs/outbounds/tailscale.cpp
        src/configs/outbounds/trojan.cpp
        src/configs/outbounds/tuic.cpp
        src/configs/outbounds/juicity.cpp
        src/configs/outbounds/trusttunnel.cpp
        src/configs/outbounds/naive.cpp
        src/configs/outbounds/shadowtls.cpp
        src/configs/outbounds/vless.cpp
        src/configs/outbounds/vmess.cpp
        src/configs/outbounds/wireguard.cpp
        src/configs/outbounds/custom.cpp
        include/configs/generate.h
        src/configs/generate.cpp
        include/configs/common/utils.h
        src/configs/common/utils.cpp
        include/configs/outbounds/custom.h
        include/configs/outbounds/chain.h
        include/configs/outbounds/autoselector.h
        src/configs/outbounds/autoselector.cpp
        include/configs/AutoSelectorPlan.h
        src/configs/AutoSelectorPlan.cpp
        include/stats/autoselector/AutoSelectorMonitor.hpp
        src/stats/autoselector/AutoSelectorMonitor.cpp
        include/ui/stats/dialog_auto_selector.h
        src/ui/stats/dialog_auto_selector.cpp
        include/configs/outbounds/extracore.h
        include/ui/profile/edit_advanced.h
        src/ui/profile/edit_advanced.cpp
        include/ui/profile/edit_advanced.ui
        include/configs/common/xrayStreamSetting.h
        include/configs/outbounds/xrayVless.h
        include/configs/common/xrayMultiplex.h
        src/configs/common/xrayMultiplex.cpp
        src/configs/common/xrayStreamSetting.cpp
        src/configs/outbounds/xrayVless.cpp
        include/ui/profile/edit_xrayvless.h
        src/ui/profile/edit_xrayvless.cpp
        include/ui/profile/edit_xrayvless.ui
        include/ui/profile/edit_direct.h
        include/ui/profile/edit_direct.ui
        src/ui/profile/edit_direct.cpp

        include/database/Database.h
        src/database/Database.cpp
        include/database/ProfilesRepo.h
        include/database/GroupsRepo.h
        include/database/RoutesRepo.h
        include/database/SettingsRepo.h
        include/database/TrafficStatsRepo.h

        include/database/entities/Profile.h
        include/database/entities/Group.h
        include/database/entities/RouteProfile.h
        include/database/entities/RouteRule.h

        src/database/entities/Profile.cpp
        src/database/entities/Group.cpp
        src/database/entities/RouteProfile.cpp
        src/database/entities/RouteRule.cpp

        src/database/ProfilesRepo.cpp
        src/database/GroupsRepo.cpp
        src/database/RoutesRepo.cpp
        src/database/TrafficStatsRepo.cpp

        include/database/DatabaseManager.h
        src/database/DatabaseManager.cpp

        src/database/SettingsRepo.cpp
        include/sys/linux/systemChecks.h
        src/sys/linux/systemChecks.cpp
        include/configs/sub/warp.h
        src/configs/sub/warp.cpp
        include/global/Common.h
        src/global/Common.cpp
)

if (NOT APPLE AND Qt6_VERSION VERSION_GREATER_EQUAL 6.9.0)
    list(APPEND PROJECT_SOURCES res/EmojiFont.qrc)
    if (WIN32)
        list(APPEND PROJECT_SOURCES res/EmojiFontWin.qrc)
    endif ()
endif ()

# Qt exe
qt_add_executable(Throne
        MANUAL_FINALIZATION
        ${PROJECT_SOURCES}
)

# Target

set_property(TARGET Throne PROPERTY AUTOUIC ON)
set_property(TARGET Throne PROPERTY AUTOMOC ON)
set_property(TARGET Throne PROPERTY AUTORCC ON)


# Target Source macOS

# Nexus: Throne brand icons removed; Qt shell orphaned. Product icons: app/src-tauri/icons/
set(MACOSX_ICON "")
if (APPLE)
    target_sources(Throne PRIVATE ${MACOSX_ICON})
endif ()

set_target_properties(Throne PROPERTIES
        MACOSX_BUNDLE_ICON_FILE ""
        RESOURCE ${MACOSX_ICON}
        MACOSX_BUNDLE_INFO_PLIST ${CMAKE_SOURCE_DIR}/res/MacOSXBundleInfo.plist
        MACOSX_BUNDLE_GUI_IDENTIFIER moe.Throne.macosx
        MACOSX_BUNDLE TRUE
        WIN32_EXECUTABLE TRUE
        UNITY_BUILD ON
)
set_property(SOURCE "3rdparty/SQLiteCpp/src/sqlite3.c" PROPERTY SKIP_UNITY_BUILD_INCLUSION ON)

# Target Source Translations

set(TS_FILES
        res/translations/zh_CN.ts
        res/translations/fa_IR.ts
        res/translations/ru_RU.ts
)
set(LUPDATE_OPTIONS
        -locations none -no-obsolete
)
qt_add_lupdate(Throne TS_FILES ${TS_FILES} OPTIONS ${LUPDATE_OPTIONS})
qt_add_lrelease(Throne TS_FILES ${TS_FILES} QM_FILES_OUTPUT_VARIABLE QM_FILES)

configure_file(res/translations/translations.qrc ${CMAKE_BINARY_DIR} COPYONLY)
target_sources(Throne PRIVATE ${CMAKE_BINARY_DIR}/translations.qrc)

# Target Link

target_link_libraries(Throne PRIVATE
        Qt6::Widgets Qt6::Network Qt6::DBus
        Threads::Threads
        ${NKR_EXTERNAL_TARGETS}
        ${PLATFORM_LIBRARIES}
)

qt_finalize_executable(Throne)

# Keep companion binaries next to the app binary for local/dev installs.
# Release packaging (build.sh) still does the authoritative copy.
if (APPLE)
    # warp-client lives in its own repo (osaka/warp-client). We do not vendor its
    # source; fetch a pinned tag and build it. Disable (e.g. in CI without access
    # to the warp-client repo) with -DWARP_CLIENT_ENABLE=OFF.
    option(WARP_CLIENT_ENABLE "Fetch and build the warp-client companion" ON)

    if (WARP_CLIENT_ENABLE)
        find_program(GO_EXECUTABLE go REQUIRED)
        # Pinned ref. Not a CACHE var, so bumping this line takes effect on
        # existing build trees. The NOT DEFINED guard still lets a command-line
        # -DWARP_CLIENT_REF=... win (it is defined before this runs).
        if (NOT DEFINED WARP_CLIENT_REF)
            set(WARP_CLIENT_REF "v0.3.2")
        endif ()
        set(WARP_CLIENT_SRC_DIR "${CMAKE_BINARY_DIR}/warp-client-src")
        set(WARP_CLIENT_BINARY "${WARP_CLIENT_SRC_DIR}/bin/warp-client")
        # No OUTPUT-level dependency tracking: the script itself decides whether
        # a rebuild is needed (ref stamp + missing binary), so bumping
        # WARP_CLIENT_REF takes effect on existing build trees without manual
        # cleanup. The script tolerates an offline fetch when the ref is already
        # checked out.
        add_custom_target(warp_client_companion ALL
                COMMAND ${CMAKE_COMMAND} -E env
                "WARP_CLIENT_REF=${WARP_CLIENT_REF}"
                bash "${CMAKE_SOURCE_DIR}/script/fetch_build_warp_client.sh"
                "${WARP_CLIENT_SRC_DIR}" "${WARP_CLIENT_BINARY}"
                COMMENT "Fetch (${WARP_CLIENT_REF}) and build warp-client companion"
                VERBATIM
        )
        add_dependencies(Throne warp_client_companion)
    endif ()

    add_custom_command(TARGET Throne POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${CMAKE_SOURCE_DIR}/core/server/ThroneCore"
            "$<TARGET_FILE_DIR:Throne>/ThroneCore"
            COMMAND chmod +x "$<TARGET_FILE_DIR:Throne>/ThroneCore"
            COMMENT "Copy ThroneCore into app bundle MacOS"
            VERBATIM
    )

    if (WARP_CLIENT_ENABLE)
        add_custom_command(TARGET Throne POST_BUILD
                COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${WARP_CLIENT_BINARY}"
                "$<TARGET_FILE_DIR:Throne>/warp-client"
                COMMAND chmod +x "$<TARGET_FILE_DIR:Throne>/warp-client"
                COMMENT "Copy warp-client into app bundle MacOS"
                VERBATIM
        )
    endif ()
endif ()

if (BUILD_TESTING)
    add_executable(version_test tests/version_test.cpp src/global/Version.cpp)
    target_include_directories(version_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(version_test PRIVATE Qt6::Core)
    add_test(NAME version_test COMMAND version_test)

    add_executable(core_path_test tests/core_path_test.cpp)
    target_include_directories(core_path_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(core_path_test PRIVATE Qt6::Core)
    add_test(NAME core_path_test COMMAND core_path_test)

    add_executable(parse_int_or_test tests/parse_int_or_test.cpp)
    target_include_directories(parse_int_or_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(parse_int_or_test PRIVATE Qt6::Core)
    add_test(NAME parse_int_or_test COMMAND parse_int_or_test)

    add_executable(warp_paths_test tests/warp_paths_test.cpp)
    target_include_directories(warp_paths_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(warp_paths_test PRIVATE Qt6::Core)
    add_test(NAME warp_paths_test COMMAND warp_paths_test)

    if (UNIX)
        add_executable(linux_cap_test tests/linux_cap_test.cpp src/sys/linux/LinuxCap.cpp)
        target_include_directories(linux_cap_test PRIVATE ${CMAKE_SOURCE_DIR})
        target_link_libraries(linux_cap_test PRIVATE Qt6::Core)
        add_test(NAME linux_cap_test COMMAND linux_cap_test)
    endif ()

    add_executable(settings_repo_test
            tests/settings_repo_test.cpp
            src/database/SettingsRepo.cpp
            src/database/Database.cpp
            3rdparty/SQLiteCpp/src/Backup.cpp
            3rdparty/SQLiteCpp/src/Column.cpp
            3rdparty/SQLiteCpp/src/Database.cpp
            3rdparty/SQLiteCpp/src/Exception.cpp
            3rdparty/SQLiteCpp/src/Savepoint.cpp
            3rdparty/SQLiteCpp/src/sqlite3.c
            3rdparty/SQLiteCpp/src/Statement.cpp
            3rdparty/SQLiteCpp/src/Transaction.cpp
    )
    target_include_directories(settings_repo_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(settings_repo_test PRIVATE
            Qt6::Core Qt6::Gui Qt6::Widgets Threads::Threads
    )
    add_test(NAME settings_repo_test COMMAND settings_repo_test)

    add_executable(db_batch_throw_test
            tests/db_batch_throw_test.cpp
            src/database/Database.cpp
            3rdparty/SQLiteCpp/src/Backup.cpp
            3rdparty/SQLiteCpp/src/Column.cpp
            3rdparty/SQLiteCpp/src/Database.cpp
            3rdparty/SQLiteCpp/src/Exception.cpp
            3rdparty/SQLiteCpp/src/Savepoint.cpp
            3rdparty/SQLiteCpp/src/sqlite3.c
            3rdparty/SQLiteCpp/src/Statement.cpp
            3rdparty/SQLiteCpp/src/Transaction.cpp
    )
    target_include_directories(db_batch_throw_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(db_batch_throw_test PRIVATE
            Qt6::Core Qt6::Gui Qt6::Widgets Threads::Threads
    )
    add_test(NAME db_batch_throw_test COMMAND db_batch_throw_test)

    add_executable(traffic_rate_snapshot_test tests/traffic_rate_snapshot_test.cpp)
    target_include_directories(traffic_rate_snapshot_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(traffic_rate_snapshot_test PRIVATE Qt6::Core Threads::Threads)
    add_test(NAME traffic_rate_snapshot_test COMMAND traffic_rate_snapshot_test)

    add_executable(profile_column_width_policy_test
            tests/profile_column_width_policy_test.cpp)
    target_compile_definitions(profile_column_width_policy_test PRIVATE
            THRONE_SOURCE_DIR="${CMAKE_SOURCE_DIR}")
    target_link_libraries(profile_column_width_policy_test PRIVATE Qt6::Core)
    add_test(NAME profile_column_width_policy_test
            COMMAND profile_column_width_policy_test)

    if (APPLE)
        add_executable(macos_core_permissions_test
                tests/macos_core_permissions_test.cpp)
        target_compile_definitions(macos_core_permissions_test PRIVATE
                THRONE_SOURCE_DIR="${CMAKE_SOURCE_DIR}")
        target_link_libraries(macos_core_permissions_test PRIVATE Qt6::Core)
        add_test(NAME macos_core_permissions_test
                COMMAND macos_core_permissions_test)
    endif ()

    add_executable(rpc_wg_keypair_test tests/rpc_wg_keypair_test.cpp src/api/RPC.cpp)
    target_include_directories(rpc_wg_keypair_test PRIVATE
            ${CMAKE_SOURCE_DIR}
            ${CMAKE_SOURCE_DIR}/3rdparty/simple-protobuf/include
    )
    target_link_libraries(rpc_wg_keypair_test PRIVATE Qt6::Core Qt6::Network Qt6::Widgets myproto)
    add_test(NAME rpc_wg_keypair_test COMMAND rpc_wg_keypair_test)

    add_executable(concurrency_lifecycle_test tests/concurrency_lifecycle_test.cpp)
    target_include_directories(concurrency_lifecycle_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(concurrency_lifecycle_test PRIVATE Qt6::Core Threads::Threads)
    add_test(NAME concurrency_lifecycle_test COMMAND concurrency_lifecycle_test)

    add_executable(warp_status_test tests/warp_status_test.cpp)
    target_include_directories(warp_status_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(warp_status_test PRIVATE Qt6::Core)
    add_test(NAME warp_status_test COMMAND warp_status_test)

    # The header sits in include/, so AUTOMOC needs it listed to find its Q_OBJECT.
    add_executable(theme_dark_test
            tests/theme_dark_test.cpp
            src/ui/setting/ThemeManager.cpp
            include/ui/setting/ThemeManager.hpp)
    set_property(TARGET theme_dark_test PROPERTY AUTOMOC ON)
    target_include_directories(theme_dark_test PRIVATE ${CMAKE_SOURCE_DIR})
    # Static Qt: link the offscreen platform plugin so the test needs no display.
    target_link_libraries(theme_dark_test PRIVATE
            Qt6::Core Qt6::Gui Qt6::Widgets Qt6::QOffscreenIntegrationPlugin)
    add_test(NAME theme_dark_test COMMAND theme_dark_test)

    add_executable(core_pre_ipc_recovery_test tests/core_pre_ipc_recovery_test.cpp)
    target_include_directories(core_pre_ipc_recovery_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(core_pre_ipc_recovery_test PRIVATE Qt6::Core)
    add_test(NAME core_pre_ipc_recovery_test COMMAND core_pre_ipc_recovery_test)

    add_executable(tun_private_bypass_test tests/tun_private_bypass_test.cpp)
    target_include_directories(tun_private_bypass_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(tun_private_bypass_test PRIVATE Qt6::Core Qt6::Network)
    add_test(NAME tun_private_bypass_test COMMAND tun_private_bypass_test)

    add_executable(routing_generation_contract_test
            tests/routing_generation_contract_test.cpp)
    target_compile_definitions(routing_generation_contract_test PRIVATE
            THRONE_SOURCE_DIR="${CMAKE_SOURCE_DIR}")
    set_property(TARGET routing_generation_contract_test PROPERTY AUTOMOC OFF)
    target_include_directories(routing_generation_contract_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(routing_generation_contract_test PRIVATE
            Qt6::Core Qt6::Network Qt6::Gui Qt6::Widgets)
    add_test(NAME routing_generation_contract_test
            COMMAND routing_generation_contract_test)

    add_executable(warp_underlay_pin_test tests/warp_underlay_pin_test.cpp)
    target_include_directories(warp_underlay_pin_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(warp_underlay_pin_test PRIVATE Qt6::Core Qt6::Network)
    add_test(NAME warp_underlay_pin_test COMMAND warp_underlay_pin_test)

    add_executable(backup_archive_test tests/backup_archive_test.cpp)
    target_include_directories(backup_archive_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(backup_archive_test PRIVATE Qt6::Core)
    add_test(NAME backup_archive_test COMMAND backup_archive_test)

    add_executable(backup_restore_test tests/backup_restore_test.cpp)
    target_include_directories(backup_restore_test PRIVATE ${CMAKE_SOURCE_DIR})
    target_link_libraries(backup_restore_test PRIVATE Qt6::Core)
    add_test(NAME backup_restore_test COMMAND backup_restore_test)
endif ()
