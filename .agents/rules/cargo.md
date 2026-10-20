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
- The library alone depends only on the `log` facade. Everything else is
  optional and feature-gated:
  - `config` adds `serde` + `toml` for reading `config.toml`.
  - `cli` (default) adds `config`, `env_logger` and `anyhow` for the programs.
- Programs and tests that need a feature list `required-features` for it.
- `cargo build --no-default-features` must still build the library, and
  `cargo tree --no-default-features -e normal` must show only `log`.

## Settings
- Settings live in `src/config.rs` (`Config`) and are documented, with their
  defaults, in the root `config.toml`. `tests/config.rs` fails if the two drift.
- A new setting needs: a field with a default, a check in `Config::check`, a
  line in `config.toml`, and tests for good and bad values. Unknown keys are
  rejected (`deny_unknown_fields`), so a typo is an error.
- Lookup order in the programs: `--config <path>`, `$TDB_CONFIG`,
  `./config.toml`, then built-in defaults. `RUST_LOG` overrides `log.level`.

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
cargo test --no-default-features --features config
```
- All tests must pass and clippy must be warning-free.
- `target/` is git-ignored; never commit it. Use `/tmp` for throwaway work.
