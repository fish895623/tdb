---
paths:
  - "crates/**"
---

# Testing rules

- Framework: the built-in test harness (`cargo test --workspace`). No test
  dependencies beyond `tempfile`.
- Integration tests live in each crate's `tests/`, one file per module, and use
  only that crate's public API:
  - `tdb-core`: `types.rs`, `record.rs`, `table.rs`, `db.rs`
  - `tdb-lexer`: one file per model plus shared behaviour: `kv.rs`, `table.rs`,
    `vector.rs`, `graph.rs`, `common.rs`, `keywords.rs`, and `models.rs` (the
    per-model entry point). Write them against
    `docs/dd-003-query-lexer.md`; when the lexer changes, change the spec and
    its tests in the same commit
  - `tdb-config`: `config.rs` (parsing and validation)
  - `tdb-cli`: `smoke.rs` runs `tdbd` end to end and clears `RUST_LOG` and
    `TDB_CONFIG` so results don't depend on the developer's shell
- Name tests `<module>_<behaviour>` in snake_case, e.g.
  `record_round_trip_int32_truncates_fraction`.
- Tests that touch disk use `tempfile` and run inside the temp directory. Never
  write into the source tree or the current directory.
- Keep the hand-built byte fixtures in `tdb-core/tests/db.rs`: they pin the
  on-disk format (`TDB1`, little-endian) byte for byte.
- Malformed input gets a test that asserts the specific `Error` variant, not
  just "is an error".
- Changing a module's behavior means updating its test file in the same change.
  Run `cargo test --workspace` before committing.
