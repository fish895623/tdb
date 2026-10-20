find_package(GTest QUIET)

if(NOT GTest_FOUND)
  message(STATUS "GoogleTest not found locally; fetching it")
  include(FetchContent)
  FetchContent_Declare(
    googletest
    URL https://github.com/google/googletest/archive/refs/tags/v1.17.0.tar.gz
  )
  set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
  FetchContent_MakeAvailable(googletest)
endif()

add_executable(tdb_tests
  types_test.cpp
  record_test.cpp
  table_test.cpp
  db_test.cpp
)
target_include_directories(tdb_tests PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_libraries(tdb_tests PRIVATE GTest::gtest_main)

if(NOT MSVC)
  target_compile_options(tdb_tests PRIVATE -Wall -Wextra)
endif()

include(GoogleTest)
gtest_discover_tests(tdb_tests DISCOVERY_MODE PRE_TEST)
