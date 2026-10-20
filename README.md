# tdb

An embedded, file-based **multi-model database engine** in Rust: key-value
(Redis-style), relational with an ORM, vector, and graph, built toward one
storage core and one transaction domain. Written from scratch, in safe Rust only
(`unsafe` is forbidden), with no SQLite or other storage dependencies.

It runs two ways, over one engine: **embedded** in your own process (link
`tdb-core` and open the file, the way you use SQLite), or as a **server**:
`tdbd` is the database server and `tdb` is its command-line client, the way
MariaDB has `mariadbd` and `mariadb`. See [Server and client](#server-and-client).

> **Status: early development. Not production-ready.** What exists today is a
> v1 storage engine (fixed-width typed tables, a single-file format), a lexer
> for the command language, and a server and client that speak the tdb wire
> protocol (statements are tokenized, not yet executed). The pages/B+tree
> storage core, the four model layers and the parser are planned, not built. See
> the roadmap in [docs/goal.md](docs/goal.md).

## Server and client

`tdbd` opens the database and answers connections; `tdb` connects to it and
sends statements. If you know MariaDB, you know the arrangement:

| | MariaDB | tdb |
|---|---|---|
| Server | `mariadbd` | `tdbd` |
| Client | `mariadb` | `tdb` |
| Wire protocol | the MySQL protocol | the tdb protocol, [DD-005](docs/dd-005-wire-protocol.md) |
| Default address | port 3306 | `127.0.0.1:7878`, loopback only |
| Server settings | `my.cnf` | [`config.toml`](config.toml), and `--listen` |
| Connect | `mariadb -h HOST -P PORT db` | `tdb --url tdb://HOST:PORT/MODEL/DB` |
| Run one statement | `mariadb -e "SELECT 1"` | `tdb "SELECT 1;"` |
| Interactive shell | `mariadb` | `tdb` |
| Accounts and TLS | yes | not yet |

One difference follows from the engine: tdb has four models in one server, so a
connection picks a *model* as well as a database (`/keyval/main`, `/graph/social`,
...). `tdb` works the model out from the statement's first word, or you give it
with `--model` or in the URL.

```sh
cargo run --bin tdbd                  # terminal 1: the server, on 127.0.0.1:7878
cargo run --bin tdb -- "GET user:1"   # terminal 2: the client sends it a statement
cargo run --bin tdb                   # or no statement: a shell, many statements on one connection
```

With no statement, `tdb` is a shell. It reads statements from standard input, so
it also takes a script: `tdb < queries.tdb`. A statement ends with `;` at the end
of a line (a keyval statement is a line, as in Redis). `help` lists the shell's
commands:
`\use /MODEL/DATABASE` to change model or database, `\c` to drop a statement you
are typing, `quit` to leave. On a terminal a failed statement is reported and
you carry on; from a pipe or a file the first failure ends the run with status 1,
so a script does not go on past a statement that did not work. There is no line
editing or history yet.

`tdbd` is the process that opens the database file, and clients talk to it over
the protocol instead of opening the file themselves. The engine does not lock the
file yet, so do not point two engines (two `tdbd`s, or one plus an embedded
program) at the same file.

**Today the server tokenizes statements and does not execute them**: the
parser and the model layers are not built, so a reply is the list of tokens.
Lexing here, with no server, is `tdb --local "..."`.

## Layout

A Cargo workspace:

| Crate | What it is |
|---|---|
| `tdb-core` | the storage engine: types, records, tables, the database file |
| `tdb-lexer` | lexer for the four-model command language |
| `tdb-config` | settings, read from `config.toml` |
| `tdb-protocol` | the wire protocol: `tdb://` URLs, routes, frames |
| `tdb-server` | the server library `tdbd` runs: accepts `tdb://` connections |
| `tdb-cli` | the `tdbd` server and the `tdb` client |

## Build and test

Requires Rust 1.85 or newer.

```sh
cargo build --workspace
cargo test --workspace

# the server: appends two records to ./tdatabase.db, then keeps running on
# 127.0.0.1:7878 until Ctrl-C (SIGINT) or SIGTERM
cargo run --bin tdbd
cargo run --bin tdbd -- --once        # the same, but exit right after
cargo run --bin tdbd -- --help

# the client, with a tdbd running
cargo run --bin tdb -- "MATCH (n) RETURN n;"
cargo run --bin tdb -- --url tdb://127.0.0.1:7878/graph/social "MATCH (n) RETURN n;"
cargo run --bin tdb -- --url tdb://127.0.0.1:7878/graph/social   # a shell on that route

# the client without a server: tokenizes here
cargo run --bin tdb -- --local "MATCH (n) RETURN n;"
```

The server's settings live in [`config.toml`](config.toml); every key is
optional. The client takes the server's address on its command line.

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
