cmake_minimum_required(VERSION 3.20)
project(nanite_clip_platform_service LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(Qt6 REQUIRED COMPONENTS Core Gui DBus)
find_package(KF6GlobalAccel REQUIRED)
find_package(KF6CoreAddons REQUIRED)
find_package(KF6DBusAddons REQUIRED)

add_executable(nanite-clip-platform-service main.cpp)
target_link_libraries(
    nanite-clip-platform-service
    PRIVATE
        Qt6::Core
        Qt6::DBus
        Qt6::Gui
        KF6::GlobalAccel
        KF6::CoreAddons
        KF6::DBusAddons
)

install(TARGETS nanite-clip-platform-service RUNTIME DESTINATION bin)
