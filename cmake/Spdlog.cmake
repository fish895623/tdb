# Logging. Use a system spdlog when present, otherwise fetch the same version.

find_package(spdlog QUIET)

if(NOT spdlog_FOUND)
  message(STATUS "spdlog not found locally; fetching it")
  include(FetchContent)
  # Silence the URL-extract timestamp warning (CMake 3.24+).
  if(POLICY CMP0135)
    cmake_policy(SET CMP0135 NEW)
  endif()
  FetchContent_Declare(
    spdlog
    URL https://github.com/gabime/spdlog/archive/refs/tags/v1.17.0.tar.gz
  )
  FetchContent_MakeAvailable(spdlog)
endif()
