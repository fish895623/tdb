---
paths:
  - "tests/**"
  - "include/tdb/**"
---

# Testing rules

- Framework: GoogleTest. `find_package(GTest)` first, `FetchContent` fallback.
- One test file per header in `include/tdb/` (`types_test.cpp` for `types.h`,
  etc.). Add new files to the `tdb_tests` source list in
  `tests/CMakeLists.txt`.
- Register cases with `gtest_discover_tests`. Do not `add_test` per case.
  Non-GoogleTest checks (e.g. the `tdb_smoke_test` of the executable) go in
  the same file with `add_test`.
- Tests that touch disk use a per-test temp file and delete it in `TearDown`.
  Never write into the source tree.
- Changing a header's behavior means updating its test file in the same
  change. Run `ctest --preset debug` before committing.
