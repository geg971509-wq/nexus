find_library(SECURITY_FRAMEWORK Security)
find_library(SERVICEMANAGEMENT_FRAMEWORK ServiceManagement)
set(PLATFORM_SOURCES src/sys/macos/MacOS.cpp src/sys/macos/AutoRun.mm src/sys/macos/UrlScheme.cpp)
set(PLATFORM_LIBRARIES ${SECURITY_FRAMEWORK} ${SERVICEMANAGEMENT_FRAMEWORK})
# AutoRun uses SMAppService (ObjC). Keep it out of UNITY CXX amalgamation.
set_source_files_properties(src/sys/macos/AutoRun.mm PROPERTIES
    LANGUAGE OBJCXX
    SKIP_UNITY_BUILD_INCLUSION ON)
if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|AppleClang")
    set_property(SOURCE src/sys/macos/AutoRun.mm APPEND PROPERTY
        COMPILE_OPTIONS "-Wno-deprecated-declarations")
endif()
