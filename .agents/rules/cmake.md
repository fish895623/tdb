---
paths:
  - "CMakeLists.txt"
  - "**/CMakeLists.txt"
  - "cmake/**"
  - "CMakePresets.json"
---

# CMake rules

## Keep the root `CMakeLists.txt` minimal
- The root file only: sets the project and C++ standard, includes
  `cmake/Executables.cmake`, adds `tests/` under `BUILD_TESTING`, and includes
  optional feature modules. Target ~20 lines.
- Do not inline feature logic there. Put it in `cmake/<Feature>.cmake` and
  `include()` it behind an `option()`. Test setup lives in
  `tests/CMakeLists.txt`.
- Order matters: `include(cmake/Coverage.cmake)` must come **after**
  `add_subdirectory(tests)`, because it checks `TARGET tdb_tests`.
- Wrap lines at 80 columns, indent 2 spaces, keep comments short and about
  *why* (not what).

## Executables (`cmake/Executables.cmake`)
- Every `bin/*.cpp` becomes its own executable, named after the file
  (`bin/tdb.cpp` -> `tdb`), like Rust's `src/bin/`. To add a program, drop a
  `.cpp` with its own `main()` in `bin/`; no CMake edit needed (the glob uses
  `CONFIGURE_DEPENDS`).
- Only `bin/` files get executables. Shared code goes in `include/tdb/`, never
  in a `bin/` file, since each one is a standalone program.
- The module sets `TDB_BIN_TARGETS`; `Coverage.cmake` instruments that list.
- Keep `bin/tdb.cpp`: the `tdb_smoke_test` runs the `tdb` target.

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
