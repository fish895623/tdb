# tdb

An embedded, file-based **multi-model database engine** in Rust: key-value
(Redis-style), relational with an ORM, vector, and graph, built toward one
storage core and one transaction domain. Written from scratch, in safe Rust only
(`unsafe` is forbidden), with no SQLite or other storage dependencies.

> **Status: early development. Not production-ready.** What exists today is a
> v1 storage engine (fixed-width typed tables, a single-file format), a lexer
> for the command language, and a server and client that speak the tdb wire
> protocol (statements are tokenized, not yet executed). The pages/B+tree
> storage core, the four model layers and the parser are planned, not built. See
> the roadmap in [docs/goal.md](docs/goal.md).

## Layout

A Cargo workspace:

| Crate | What it is |
|---|---|
| `tdb-core` | the storage engine: types, records, tables, the database file |
| `tdb-lexer` | lexer for the four-model command language |
| `tdb-config` | settings, read from `config.toml` |
| `tdb-protocol` | the wire protocol: `tdb://` URLs, routes, frames |
| `tdb-server` | the listener `tdbd` runs: accepts `tdb://` connections |
| `tdb-cli` | the `tdb` client and the `tdbd` daemon |

## Build and test

Requires Rust 1.85 or newer.

```sh
cargo build --workspace
cargo test --workspace
cargo run --bin tdbd     # appends two records to ./tdatabase.db, then keeps
                         # running on 127.0.0.1:7878 until Ctrl-C or SIGTERM
cargo run --bin tdbd -- --once   # the same, but exit right after
cargo run --bin tdb      # tokenizes one built-in statement per model, here
# with tdbd running, send it a statement over the tdb protocol:
cargo run --bin tdb -- "MATCH (n) RETURN n;"
cargo run --bin tdb -- --url tdb://127.0.0.1:7878/graph/social "MATCH (n) RETURN n;"
cargo run --bin tdb -- --local "MATCH (n) RETURN n;"   # tokenize here, no server
cargo run --bin tdbd -- --help
```

Settings live in [`config.toml`](config.toml); every key is optional.

## Design

- [Project goal and roadmap](docs/goal.md)
- [DD-001: indexing beyond RAM](docs/dd-001-large-index.md)
- [DD-002: one engine, four models](docs/dd-002-multi-model.md)
- [DD-003: the query lexer](docs/dd-003-query-lexer.md)
- [DD-004: endpoint routing](docs/dd-004-endpoint-routing.md)
- [DD-005: the wire protocol](docs/dd-005-wire-protocol.md)

## License

Dual-licensed:

- **Open source under the [AGPL-3.0](LICENSE).** Free to use, including
  commercially, if you follow the AGPL.
- **A commercial license** if you cannot or do not want to follow the AGPL. It
  costs one accepted pull request or a fee, every six months. See
  [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). A contributor agreement is required
before pull requests can be accepted, and it is not set up yet, so please open an
issue first.
