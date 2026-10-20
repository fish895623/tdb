cmake_minimum_required(VERSION 3.20)

project(tdb LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_executable(${PROJECT_NAME} main.cpp)
target_include_directories(${PROJECT_NAME} PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)

include(CTest)

if(BUILD_TESTING)
  add_test(NAME tdb_smoke_test COMMAND $<TARGET_FILE:${PROJECT_NAME}>)
  set_tests_properties(tdb_smoke_test PROPERTIES
    PASS_REGULAR_EXPRESSION "Hello, World!"
  )

  # Unit tests (GoogleTest). Each test case is registered with CTest.
  add_subdirectory(tests)
endif()

option(ENABLE_COVERAGE "Build with code coverage instrumentation" OFF)

if(ENABLE_COVERAGE)
  if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang|AppleClang")
    target_compile_options(${PROJECT_NAME} PRIVATE --coverage)
    target_link_options(${PROJECT_NAME} PRIVATE --coverage)

    # The engine is header-only, so the unit tests are what exercise it.
    # Only instrument the engine headers: instrumenting GoogleTest's headers
    # makes gcov emit "nan% of 0" lines, which CTest reports as a coverage error.
    if(TARGET tdb_tests)
      target_compile_options(tdb_tests PRIVATE
        --coverage
        -fprofile-filter-files=${CMAKE_CURRENT_SOURCE_DIR}/include/tdb/)
      target_link_options(tdb_tests PRIVATE --coverage)
    endif()

    add_custom_target(coverage
      COMMAND ${CMAKE_CTEST_COMMAND} --output-on-failure
      COMMAND ${CMAKE_CTEST_COMMAND} -T Coverage
      WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
      USES_TERMINAL
      COMMENT "Running tests and collecting coverage"
    )
    add_dependencies(coverage ${PROJECT_NAME})
    if(TARGET tdb_tests)
      add_dependencies(coverage tdb_tests)
    endif()
  else()
    message(FATAL_ERROR "ENABLE_COVERAGE is not supported by ${CMAKE_CXX_COMPILER_ID}")
  endif()
endif()