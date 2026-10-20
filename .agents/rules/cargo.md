---
paths:
  - "Cargo.toml"
  - "src/**"
  - "examples/**"
---

# Cargo rules

## Layout
- The engine is the library crate `tdb` (`src/lib.rs`, one module per file:
  `types`, `record`, `table`, `db`, `kv_lexer`, plus `error`).
- Every `src/bin/*.rs` is its own program, named after the file
  (`src/bin/tdb.rs` -> `tdb`), found automatically. To add one, drop in a file
  with a `main()`; no manifest edit needed.
- Share code through the library, never between `src/bin/` files. Throwaway
  programs go in `examples/` (they may use dev-dependencies).

## Dependencies
- The library depends only on the `log` facade. Anything that is just for the
  programs (e.g. `env_logger`) is optional and behind the `cli` feature; the
  programs and tests that need it list `required-features = ["cli"]`.
- `cargo build --no-default-features` must still build the library.

## Safety
- `[lints.rust] unsafe_code = "forbid"` is set in `Cargo.toml`. Do not add
  `#[allow(unsafe_code)]`; find a safe design (see `docs/dd-001` on `mmap`).
- Data read from disk is untrusted. Return `Error::Corrupt`, never panic,
  index unchecked, or trust a length field for an allocation size.
- Use `?` and `Result`; no `unwrap()`/`expect()` in `src/` outside tests.

## Verify before committing
```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --no-default-features
```
- All tests must pass and clippy must be warning-free.
- `target/` is git-ignored; never commit it. Use `/tmp` for throwaway work.
