cmake_minimum_required(VERSION 3.21)
if(CMAKE_HOST_APPLE AND NOT DEFINED CMAKE_OSX_DEPLOYMENT_TARGET)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "13.0" CACHE STRING "Minimum macOS version")
endif()
project(nexus-qt LANGUAGES C CXX)
if(APPLE)
    if(CMAKE_OSX_DEPLOYMENT_TARGET VERSION_LESS "13.0")
        message(FATAL_ERROR "Qt 6.11 requires CMAKE_OSX_DEPLOYMENT_TARGET 13.0 or newer")
    endif()
    enable_language(OBJCXX)
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

list(PREPEND CMAKE_PREFIX_PATH /opt/homebrew /opt/homebrew/opt/qtbase /opt/homebrew/opt/qtdeclarative /opt/homebrew/opt/qtsvg)

find_package(Qt6 6.11 REQUIRED COMPONENTS Quick Widgets Svg Gui QuickControls2)

set(NEXUS_TAURI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../src-tauri")
set(NEXUS_CARGO_TARGET_DIR "${NEXUS_TAURI_DIR}/target")
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(NEXUS_CARGO_PROFILE "")
    set(NEXUS_LIB_DIR "${NEXUS_CARGO_TARGET_DIR}/debug")
else()
    set(NEXUS_CARGO_PROFILE "--release")
    set(NEXUS_LIB_DIR "${NEXUS_CARGO_TARGET_DIR}/release")
endif()
set(NEXUS_LIB "${NEXUS_LIB_DIR}/libnexus_lib.a")

file(GLOB_RECURSE NEXUS_ENGINE_SOURCES CONFIGURE_DEPENDS
    "${NEXUS_TAURI_DIR}/src/*.rs"
)
list(APPEND NEXUS_ENGINE_SOURCES
    "${NEXUS_TAURI_DIR}/Cargo.toml"
    "${NEXUS_TAURI_DIR}/Cargo.lock"
    "${NEXUS_TAURI_DIR}/build.rs"
    "${CMAKE_CURRENT_SOURCE_DIR}/../../core/server/gen/libcore.proto"
)

add_custom_command(
    OUTPUT "${NEXUS_LIB}"
    COMMAND cargo build --lib ${NEXUS_CARGO_PROFILE} --manifest-path "${NEXUS_TAURI_DIR}/Cargo.toml"
    WORKING_DIRECTORY "${NEXUS_TAURI_DIR}"
    DEPENDS ${NEXUS_ENGINE_SOURCES}
    COMMENT "Building nexus_lib"
    VERBATIM
)
add_custom_target(nexus_lib_a DEPENDS "${NEXUS_LIB}")

add_executable(nexus
    src/main.cpp
    src/bridge.cpp
    src/host.cpp
    src/qr_decoder.cpp
    src/tray.cpp
    third_party/quirc/decode.c
    third_party/quirc/identify.c
    third_party/quirc/quirc.c
    third_party/quirc/version_db.c
)

file(GLOB NEXUS_QML_FILES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/qml/*.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/qml/qmldir"
)
qt_add_resources(nexus "nexus_qml"
    PREFIX "/nexus"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    FILES ${NEXUS_QML_FILES}
)

file(GLOB NEXUS_TRAY_FILES CONFIGURE_DEPENDS
    "${NEXUS_TAURI_DIR}/icons/tray/frame_*.png"
)
qt_add_resources(nexus "nexus_tray"
    PREFIX "/nexus/tray"
    BASE "${NEXUS_TAURI_DIR}/icons/tray"
    FILES ${NEXUS_TRAY_FILES}
)
if(APPLE)
    target_sources(nexus PRIVATE src/macos.mm)
    set_source_files_properties(src/macos.mm PROPERTIES COMPILE_FLAGS "-fobjc-arc")
endif()
add_dependencies(nexus nexus_lib_a)

target_include_directories(nexus PRIVATE src third_party)
target_link_libraries(nexus PRIVATE
    Qt6::Quick
    Qt6::QuickControls2
    Qt6::Widgets
    Qt6::Svg
    Qt6::Gui
    "${NEXUS_LIB}"
)

if(APPLE)
    target_link_options(nexus PRIVATE "LINKER:-force_load,${NEXUS_LIB}")
    target_link_libraries(nexus PRIVATE
        "-framework AppKit"
        "-framework ApplicationServices"
        "-framework Carbon"
        "-framework ColorSync"
        "-framework CoreFoundation"
        "-framework CoreGraphics"
        "-framework CoreServices"
        "-framework CoreText"
        "-framework CoreVideo"
        "-framework DiskArbitration"
        "-framework Foundation"
        "-framework IOKit"
        "-framework ImageIO"
        "-framework Metal"
        "-framework Network"
        "-framework QuartzCore"
        "-framework Security"
        "-framework SystemConfiguration"
        "-framework UniformTypeIdentifiers"
        "-framework UserNotifications"
        "-framework WebKit"
        objc
        z
        iconv
        resolv
    )
endif()
