# tdb — Project Goal

`tdb` is an embedded, file-based **multi-model** database engine written in
Rust: **key-value (Redis-style), relational with an ORM, vector, and graph**,
bound into one engine with one storage core and one transaction domain.
It is built from scratch: no SQLite, no external storage dependencies.

The point of binding them is atomicity and one query surface across models: a
single write can insert a row, its embedding, a graph edge, and a cache key
all-or-nothing. See `dd-002-multi-model.md`.

## Purpose

- Provide a small, readable, correctness-first storage engine
- Support tables with fixed-width typed columns (v1: Bool, Int32, Int64, Float32, Float64)
- Persist data in a single, self-describing file (`tdatabase.db`)
- Be embeddable: the engine crate (`tdb-core`) has no dependencies beyond the
  `log` facade; everything heavier lives in its own crate
- Be open source and sustainable: AGPL-3.0 for everyone, plus a commercial
  license (one accepted pull request or a fee, every six months) for those who
  cannot follow the AGPL (see `LICENSE-COMMERCIAL.md`)
- Be configurable: settings (database path, log level, and the DD-001 storage
  tunables) come from `config.toml` via the `tdb-config` crate; see
  `config.toml` for every key

## One Engine, Four Models

All four reduce to ordered byte-keyed maps in B+trees on pages (DD-001); each
model is a key encoding over that, not a new storage engine.

| Model             | Maps to                                                                                                  |
|-------------------|----------------------------------------------------------------------------------------------------------|
| Key-value (Redis) | `(key, field/member/seq) -> value`; strings, hashes, sets, lists, sorted sets, TTL; optional RESP server |
| Relational + ORM  | `pk -> row` plus secondary-index keyspaces; `#[derive(Model)]`                                           |
| Vector            | `Vector(dim)` column; exact search, then HNSW                                                            |
| Graph             | nodes are rows; edges are rows plus out/in index keyspaces                                               |

## Core Design Principles

1. **Own the bytes.** The on-disk format is documented, versioned (magic `TDB1`, little-endian), and never trusts
   in-memory state.
2. **RAM is a cache, not the storage.** The engine must run on small
   machines (e.g. 16GB RAM) while managing data sets far larger than RAM (see `dd-001-large-index.md`).
3. **Safe by construction.** The engine is safe Rust only (`unsafe_code = "forbid"`). Bytes from disk are untrusted:
   truncated or
   malformed files, lying length fields and bad UTF-8 come back as errors,
   never as panics, undefined behavior, or silently zeroed data.
4. **Grow in stages.** v1 ships fixed-width records, append-only tables,
   linear scans. Indexes, variable-length types, delete/update, WAL, and a
   SQL layer are incremental milestones.

## Current State (v1)

| Component                                                   | Status              |
|-------------------------------------------------------------|---------------------|
| Storage types (`FieldType`)                                 | ✅ done             |
| Fixed-width records, little-endian (de)serialization        | ✅ done             |
| Tables: named schema, append-only records, linear scans     | ✅ done             |
| Single-file persistence, format v1 (`TDB1`), `Open`/`Flush` | ✅ done             |
| Page-based storage + buffer pool                            | ⏳ planned (dd-001) |
| B+tree indexes                                              | ⏳ planned (dd-001) |
| Variable-length types (`kText`)                             | ⏳ planned          |
| Delete / update / WAL                                       | ⏳ planned          |
| SQL query layer                                             | ⏳ planned          |

## IDE Integration (schema-aware tooling)

goal: IDE users get automatic schema-based code completion for `tdb`
tables/columns, even though no existing IDE supports `tdb` out of the box (same situation as Neo4j in IntelliJ — support
is what the plugin builds).

contract:

- the engine exports its schema as **`tdb.schema.json`** (tables, columns,
  `FieldType`). This JSON is the stable interface for all IDE tooling —
  no tool ever parses the binary `tdatabase.db` format.
- optional `tdb` CLI binary for query execution from IDE tools.

front-ends (one contract, three consumers):

1. **IntelliJ plugin** (Kotlin): custom tool window + completion.
    - schema completion: table/column names in queries and in
      Rust API calls like `db.get_table("…")`
    - read-only data browser (from schema JSON)
    - query console (shells out to the `tdb` CLI)
    - deliberately the *lightweight* tier: own tool window, NOT the
      JDBC-based `com.intellij.database` tier-2 API (that path serves
      JDBC databases and belongs to DataGrip; Neo4j/Redis/MongoDB
      plugins all use the lightweight tier).
2. **LSP server**: completion for both VS Code and IntelliJ (JetBrains
   LSP client, 2023.3+) from one implementation — candidate for
   replacing/duplicating the plugin later.
3. **VS Code**: today covered by rust-analyzer + generated schema types;
   later the LSP server.

## Roadmap

Dependency order; details and rationale in `dd-002-multi-model.md`.

1. **Storage core** (dd-001): pages, `BufferPool` (clock eviction), B+tree
   sized for "index > RAM" (100GB index on a 16GB host), file format v2,
   copy-on-write commit, catalog.
2. **Typed values**: `Value` enum, `Text`/`Bytes`, variable-length records.
   Replaces the v1 `f64` record API.
3. **Keyspaces**: order-preserving key encoding, range/prefix scans; tables
   gain primary keys and secondary indexes.
4. **Key-value layer**: strings, hashes, sets, lists, sorted sets, TTL; then a
   RESP server so Redis clients work.
5. **ORM**: `#[derive(Model)]`, query builder, schema JSON export (`tdb.schema.json`, the IDE contract).
6. **Vector**: `Vector(dim)` column, exact search, then HNSW.
7. **Graph**: edge tables with out/in indexes, traversal API.
8. **Cross-model**: unified transaction handle and mixed queries, then the
   IntelliJ plugin and LSP server, and a query language if the API needs one.
