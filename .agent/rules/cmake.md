# CMake rules

## Keep the root `CMakeLists.txt` minimal
- The root file only: sets the project and C++ standard, defines the `tdb`
  executable, adds `tests/` under `BUILD_TESTING`, and includes optional
  feature modules. Target ~20 lines.
- Do not inline feature logic there. Put it in `cmake/<Feature>.cmake` and
  `include()` it behind an `option()`. Test setup lives in `tests/CMakeLists.txt`.
- Order matters: `include(cmake/Coverage.cmake)` must come **after**
  `add_subdirectory(tests)`, because it checks `TARGET tdb_tests`.
- Wrap lines at 80 columns, indent 2 spaces, keep comments short and about
  *why* (not what).

## Tests (`tests/`)
- Framework: GoogleTest. `find_package(GTest)` first, `FetchContent` fallback.
- One test file per header in `include/tdb/` (`types_test.cpp` for `types.h`,
  etc.). Add new files to the `tdb_tests` source list in `tests/CMakeLists.txt`.
- Register cases with `gtest_discover_tests`. Do not `add_test` per case.
  Non-GoogleTest checks (e.g. the `tdb_smoke_test` of the executable) go in
  the same file with `add_test`.
- Tests that touch disk use a per-test temp file and delete it in `TearDown`.
  Never write into the source tree.

## Coverage (`cmake/Coverage.cmake`)
- The engine is header-only, so coverage comes from `tdb_tests`.
- Instrument `tdb_tests` only for `include/tdb/` via `-fprofile-filter-files`.
  Instrumenting GoogleTest headers makes gcov print `nan% of 0` lines, and
  `ctest -T Coverage` then exits with code 32.

## Verify before committing a CMake change
```sh
cmake --preset debug && cmake --build --preset debug && ctest --preset debug
rm -rf build/coverage && cmake --preset coverage && cmake --build --preset coverage
cmake -S . -B /tmp/tdb_off -G Ninja -DBUILD_TESTING=OFF   # must still configure
```
- All tests must pass and the `coverage` target must exit 0.
- `build/` is git-ignored; never commit it. Use `/tmp` for throwaway build dirs.
