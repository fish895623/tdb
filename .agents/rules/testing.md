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
  - `tdb-protocol`: `route.rs`, `endpoint.rs`, and `codec.rs` (frames and
    messages). `codec.rs` keeps hand-built byte fixtures that pin
    `docs/dd-005-wire-protocol.md` byte for byte, and truncation, lying-count
    and oversize cases that assert the specific `ProtocolError`
  - `tdb-server`: `server.rs` starts `serve_with` on a thread with short
    timeouts, talks to it with a blocking client and raw hand-built frames,
    and puts a read timeout on every client socket so a bug fails a test
    instead of hanging it
  - `tdb-cli`: `smoke.rs` runs `tdbd` end to end and clears `RUST_LOG` and
    `TDB_CONFIG` so results don't depend on the developer's shell; `tdb.rs`
    runs `tdb` end to end. Both cover `--help`, `--version` and usage
    errors (exit status 2). `smoke.rs` runs `tdbd --once` so each run ends by
    itself; its unix-only `daemon` module spawns `tdbd` without `--once`,
    waits for the "is running" line, sends SIGTERM or SIGINT with `kill`,
    and kills the process on drop so a failed test leaves nothing behind. That
    handle lives in `tests/support/mod.rs`, shared with `tdb.rs`, whose
    `remote` module runs `tdb --url` against a real `tdbd`. Daemons always
    listen on port 0 (`Daemon::spawn` adds `--listen 127.0.0.1:0`), so tests
    never collide with each other or with a real `tdbd`
- Name tests `<module>_<behaviour>` in snake_case, e.g.
  `record_round_trip_int32_truncates_fraction`.
- Tests that touch disk use `tempfile` and run inside the temp directory. Never
  write into the source tree or the current directory.
- Keep the hand-built byte fixtures in `tdb-core/tests/db.rs`: they pin the
  on-disk format (`TDB1`, little-endian) byte for byte.
- Malformed input gets a test that asserts the specific `Error` variant, not
  just "is an error".
- Tests run in the debug profile on 2 MiB test threads, which is how a stack
  overflow on long input shows up. Test long inputs there, not only in
  `--release`.
- Changing a module's behavior means updating its test file in the same change.
  Run `cargo test --workspace` before committing.
