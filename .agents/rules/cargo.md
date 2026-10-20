---
paths:
  - "Cargo.toml"
  - "crates/**"
---

# Cargo rules

## Layout (workspace)
| Crate | Role | Depends on |
| --- | --- | --- |
| `tdb-core` | the engine: `types`, `record`, `table`, `db`, `error` | `log` |
| `tdb-config` | settings read from `config.toml` | `log`, `serde`, `toml` |
| `tdb-lexer` | lexer for the four-model command language (`docs/dd-003-query-lexer.md`) | `logos` |
| `tdb-cli` | the `tdb` and `tdbd` programs | the three above, `env_logger`, `anyhow` |

- Crates live in `crates/<name>/` and are listed in the root `Cargo.toml`
  `members`. Edition, version, `rust-version` and lints are inherited
  (`version.workspace = true`, `[lints] workspace = true`).
- Dependencies point one way: `tdb-cli` depends on the others; the engine and
  lexer never depend on `tdb-config` or `tdb-cli`.
- Every `crates/tdb-cli/src/bin/*.rs` is its own program, named after the file
  (`tdb.rs` -> `tdb`), found automatically. Throwaway programs go in
  `crates/tdb-cli/examples/` (they may use dev-dependencies).
- Share code through a library crate, never between `src/bin/` files.
- A new crate: add `crates/<name>/`, list it in `members`, add a `path` entry
  to `[workspace.dependencies]`, and set `[lints] workspace = true`. The planned
  crates (`tdb-derive`, `tdb-server`, one per model) are in
  `docs/dd-002-multi-model.md`.

## Dependencies
- Third-party versions live once, in the root `[workspace.dependencies]`;
  crates use `name.workspace = true`.
- `cargo tree -p tdb-core -e normal` must show only `log`, and `tdb-lexer` only
  `logos`. Anything heavier belongs in its own crate, not behind a feature of
  the engine.

## License
- tdb is licensed under PolyForm Noncommercial 1.0.0 (`LICENSE`) and sold
  under a separate commercial license (`LICENSE-COMMERCIAL.md`). Each crate sets
  `license.workspace = true`.
- Dependencies must be permissively licensed (MIT, Apache-2.0, BSD, ISC,
  Unicode, and similar). Do not add GPL, LGPL, AGPL, SSPL or other copyleft
  dependencies: they would conflict with selling commercial licenses. Check
  with `cargo metadata` before adding one.
- Keep the `Required Notice:` line at the top of `LICENSE`.

## Safety
- The root `[workspace.lints.rust] unsafe_code = "forbid"` covers every crate.
  Do not add `#[allow(unsafe_code)]`; find a safe design (see `docs/dd-001` on
  `mmap`).
- Data read from disk is untrusted. Return `Error::Corrupt`, never panic,
  index unchecked, or trust a length field for an allocation size.
- Use `?` and `Result`; no `unwrap()`/`expect()` in library `src/` outside
  tests.

## Settings
- Settings live in `tdb-config` (`Config`) and are documented, with their
  defaults, in the root `config.toml`. `crates/tdb-config/tests/config.rs`
  fails if the two drift.
- A new setting needs: a field with a default, a check in `Config::check`, a
  line in `config.toml`, and tests for good and bad values. Unknown keys are
  rejected (`deny_unknown_fields`), so a typo is an error.
- Lookup order in `tdbd`: `--config <path>`, `$TDB_CONFIG`, `./config.toml`,
  then built-in defaults. `RUST_LOG` overrides `log.level`.
- Log targets are crate names (`tdb_core::db`), so `RUST_LOG=tdb_core=trace`.

## Verify before committing
```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- All tests must pass and clippy must be warning-free.
- `target/` is git-ignored; never commit it. Use `/tmp` for throwaway work.
