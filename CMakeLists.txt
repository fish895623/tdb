cmake_minimum_required(VERSION 3.20)
project(tdb LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

include(cmake/Spdlog.cmake)
add_subdirectory(bin)

include(CTest)
if(BUILD_TESTING)
  add_subdirectory(tests)
endif()

option(ENABLE_COVERAGE "Build with code coverage instrumentation" OFF)
if(ENABLE_COVERAGE)
  include(cmake/Coverage.cmake)
endif()
