# tdb — Project Goal

`tdb` is an embedded, file-based database engine written in C++23.
It is built from scratch: no SQLite, no external storage dependencies.

## Purpose

- Provide a small, readable, correctness-first storage engine
- Support tables with fixed-width typed columns (v1: Bool, Int32, Int64, Float32, Float64)
- Persist data in a single, self-describing file (`tdatabase.db`)
- Be embeddable: header-only API, one library to link

## Core Design Principles

1. **Own the bytes.** The on-disk format is documented, versioned
   (magic `TDB1`, little-endian), and never trusts in-memory state.
2. **RAM is a cache, not the storage.** The engine must run on small
   machines (e.g. 16GB RAM) while managing data sets far larger than RAM
   (see `dd-001-large-index.md`).
3. **Grow in stages.** v1 ships fixed-width records, append-only tables,
   linear scans. Indexes, variable-length types, delete/update, WAL, and a
   SQL layer are incremental milestones.

## Current State (v1)

| Component | Status |
|---|---|
| Storage types (`FieldType`) | ✅ done |
| Fixed-width records, little-endian (de)serialization | ✅ done |
| Tables: named schema, append-only records, linear scans | ✅ done |
| Single-file persistence, format v1 (`TDB1`), `Open`/`Flush` | ✅ done |
| Page-based storage + buffer pool | ⏳ planned (dd-001) |
| B+tree indexes | ⏳ planned (dd-001) |
| Variable-length types (`kText`) | ⏳ planned |
| Delete / update / WAL | ⏳ planned |
| SQL query layer | ⏳ planned |

## IDE Integration (schema-aware tooling)

goal: IDE users get automatic schema-based code completion for `tdb`
tables/columns, even though no existing IDE supports `tdb` out of the box
(same situation as Neo4j in IntelliJ — support is what the plugin builds).

contract:
- the engine exports its schema as **`tdb.schema.json`** (tables, columns,
  `FieldType`). This JSON is the stable interface for all IDE tooling —
  no tool ever parses the binary `tdatabase.db` format.
- optional `tdb` CLI binary for query execution from IDE tools.

front-ends (one contract, three consumers):
1. **IntelliJ plugin** (Kotlin): custom tool window + completion.
   - schema completion: table/column names in queries and in
     C++ API calls like `db.GetTable("…")`
   - read-only data browser (from schema JSON)
   - query console (shells out to the `tdb` CLI)
   - deliberately the *lightweight* tier: own tool window, NOT the
     JDBC-based `com.intellij.database` tier-2 API (that path serves
     JDBC databases and belongs to DataGrip; Neo4j/Redis/MongoDB
     plugins all use the lightweight tier).
2. **LSP server**: completion for both VS Code and IntelliJ (JetBrains
   LSP client, 2023.3+) from one implementation — candidate for
   replacing/duplicating the plugin later.
3. **VS Code**: today covered by clangd + generated schema header;
   later the LSP server.

## Roadmap

1. **dd-001**: page format, `BufferPool` (clock eviction), B+tree index
   sized for "index > RAM" workloads (100GB index on 16GB host).
2. Variable-length records (text) with in-page / overflow-page layout.
3. Delete/update semantics + write-ahead log for crash durability.
4. Simple SQL parser over the C++ API.
5. Schema JSON export (`tdb.schema.json`) + `tdb` CLI — the IDE contract.
6. IntelliJ plugin (lightweight tier): schema completion, data browser,
   query console. (dd-002, pending)
7. LSP server for cross-editor completion (VS Code + IntelliJ).
