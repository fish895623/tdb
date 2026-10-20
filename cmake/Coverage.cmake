# gcov-style coverage: instruments the bin/ targets (+ tdb_tests if present)
# and adds a `coverage` target that runs the tests and collects results.

if(NOT CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang|AppleClang")
  message(FATAL_ERROR
          "ENABLE_COVERAGE is not supported by ${CMAKE_CXX_COMPILER_ID}")
endif()

set(_cov_targets ${TDB_BIN_TARGETS})
if(TARGET tdb_tests)
  list(APPEND _cov_targets tdb_tests)
endif()

foreach(t IN LISTS _cov_targets)
  target_compile_options(${t} PRIVATE --coverage)
  target_link_options(${t} PRIVATE --coverage)
endforeach()

# The engine is header-only, so the tests are what exercise it. Instrument only
# the engine headers: GoogleTest's headers make gcov print "nan% of 0" lines,
# which CTest reports as a coverage error.
if(TARGET tdb_tests)
  target_compile_options(tdb_tests PRIVATE
    -fprofile-filter-files=${PROJECT_SOURCE_DIR}/include/tdb/)
endif()

add_custom_target(coverage
  COMMAND ${CMAKE_CTEST_COMMAND} --output-on-failure
  COMMAND ${CMAKE_CTEST_COMMAND} -T Coverage
  WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
  USES_TERMINAL
  COMMENT "Running tests and collecting coverage"
)
add_dependencies(coverage ${_cov_targets})
