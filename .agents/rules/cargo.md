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
| `tdb-protocol` | the wire protocol: routes, `tdb://` endpoints, frames (`docs/dd-005-wire-protocol.md`) | `tdb-lexer` |
| `tdb-server` | the listener and connection handling | `tdb-protocol`, `tdb-lexer`, `log`, `tokio` |
| `tdb-cli` | the `tdb` and `tdbd` programs | the crates above, `clap`, `tokio`, `env_logger`, `anyhow` |

- Crates live in `crates/<name>/` and are listed in the root `Cargo.toml`
  `members`. Edition, version, `rust-version` and lints are inherited
  (`version.workspace = true`, `[lints] workspace = true`).
- Dependencies point one way: `tdb-cli` depends on the others; the engine and
  lexer never depend on `tdb-config` or `tdb-cli`.
- Every `crates/tdb-cli/src/bin/*.rs` is its own program, named after the file
  (`tdb.rs` -> `tdb`), found automatically. Throwaway programs go in
  `crates/tdb-cli/examples/` (they may use dev-dependencies).
- Share code through a library crate, never between `src/bin/` files.
- Command-line arguments are parsed with `clap` (derive) in each program, as
  an `Args` struct with an explicit `#[command(name = "..")]`: the derived
  name would be the package, `tdb-cli`. Parse first in `main`, so `--help`,
  `--version` and a usage error (exit status 2) never touch the database. A
  type from a crate that must not depend on `clap` (`tdb_lexer::Model`) gets
  a local `ValueEnum` mirror in the program.
- `tdbd` is a long-running process on a `tokio` runtime, built by hand in
  `main` after the arguments, settings and logger (not `#[tokio::main]`). It
  stays up until SIGINT or SIGTERM, then flushes and exits 0; `--once` runs
  the startup step and exits. Register the signal listeners before opening
  the database, and run the engine's blocking file I/O through
  `spawn_blocking`, never on a worker thread. Only `tdb-cli` (and the future
  `tdb-server`) may depend on `tokio`; the engine and lexer stay
  runtime-free. `tdb-protocol` has no sockets and no runtime either, so the
  clients and the server share it; the blocking `tdb` client uses `std::net`.
- The wire format is `docs/dd-005-wire-protocol.md`. Everything a peer sends
  is untrusted: check a length against its limit before reading or
  allocating, return a `ProtocolError`, never panic. Change the spec and its
  tests in the same commit as the code, and bump `VERSION` for a change an
  old peer could misread.
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
- tdb is dual-licensed: open source under AGPL-3.0-only (`LICENSE`, with the
  notice in `NOTICE`) and a paid commercial license (`LICENSE-COMMERCIAL.md`).
  Each crate sets `license.workspace = true`. The commercial license costs one
  accepted pull request or a fee per six months; keep `LICENSE-COMMERCIAL.md`,
  `README.md` and `CONTRIBUTING.md` consistent when the terms change.
- Dependencies must be permissively licensed (MIT, Apache-2.0, BSD, ISC,
  Unicode, and similar). Do not add GPL, LGPL, AGPL, SSPL or other copyleft
  dependencies: the commercial license could not include them. Check with
  `cargo metadata` before adding one.
- Do not accept outside code without a signed CLA (`CONTRIBUTING.md`): the
  commercial license needs the rights to all of it.

## Safety
- The root `[workspace.lints.rust] unsafe_code = "forbid"` covers every crate.
  Do not add `#[allow(unsafe_code)]`; find a safe design (see `docs/dd-001` on
  `mmap`).
- Data read from disk is untrusted. Return `Error::Corrupt`, never panic,
  index unchecked, or trust a length field for an allocation size.
- Use `?` and `Result`; no `unwrap()`/`expect()` in library `src/` outside
  tests.
- Stack depth must not depend on input. In `tdb-lexer`'s `logos` patterns, do
  not repeat a class that matches non-ASCII text (`\S*`, `\s+`, `[^\n]*`, `.*`):
  the generated code recurses once per character, and a debug build, which has
  no tail calls, overflows the stack on a long run. That would abort a whole
  `tdbd` from one statement. Match a short head with the pattern and loop over
  the rest in a callback (see `bump_while` in `raw.rs`). ASCII-only classes are
  fine. A new pattern gets a 200,000-character test in `tests/common.rs`.

## Settings
- Settings live in `tdb-config` (`Config`) and are documented, with their
  defaults, in the root `config.toml`. `crates/tdb-config/tests/config.rs`
  fails if the two drift.
- A new setting needs: a field with a default, a check in `Config::check`, a
  line in `config.toml`, and tests for good and bad values. Unknown keys are
  rejected (`deny_unknown_fields`), so a typo is an error.
- Lookup order in `tdbd`: `--config <path>`, `$TDB_CONFIG`, `./config.toml`,
  then built-in defaults. `RUST_LOG` overrides `log.level`, and
  `--listen <addr>` overrides `server.listen`.
- `server.listen` defaults to loopback (there is no authentication yet). Its
  port, `tdb_config::DEFAULT_LISTEN_PORT`, equals `tdb_protocol::DEFAULT_PORT`;
  a `tdb-cli` test keeps them equal, so change both together.
- Log targets are crate names (`tdb_core::db`), so `RUST_LOG=tdb_core=trace`.

## Verify before committing
```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- All tests must pass and clippy must be warning-free.
- `target/` is git-ignored; never commit it. Use `/tmp` for throwaway work.
