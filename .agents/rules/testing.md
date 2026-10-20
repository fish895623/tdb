---
paths:
  - "tests/**"
  - "src/**"
---

# Testing rules

- Framework: the built-in test harness (`cargo test`). No test dependencies
  beyond `tempfile`.
- Integration tests live in `tests/`, one file per library module
  (`tests/types.rs` for `types`, etc.) and use only the public API.
  `tests/smoke.rs` runs the `tdbd` program end to end.
- Name tests `<module>_<behaviour>` in snake_case, e.g.
  `record_round_trip_int32_truncates_fraction`.
- Tests that touch disk use `tempfile` and run inside the temp directory. Never
  write into the source tree or the current directory.
- Keep the hand-built byte fixtures in `tests/db.rs`: they pin the on-disk
  format (`TDB1`, little-endian) byte for byte.
- Malformed input gets a test that asserts the specific `Error` variant, not
  just "is an error".
- Changing a module's behavior means updating its test file in the same change.
  Run `cargo test` before committing.
