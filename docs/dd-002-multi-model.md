# DD-002: One Engine, Four Models

**Goal.** Bind four data models into one embedded engine: **key-value
(Redis-style)**, **relational with an ORM**, **vector**, and **graph**. One
file, one storage core, one transaction domain.

**Problem.** Four separate databases glued together cannot be atomic across
models, duplicate every storage concern (paging, caching, recovery), and
force the application to keep them consistent. The point of binding them is
that one write can touch a row, its embedding, a graph edge, and a cache key
**atomically**, and one query can mix them.

## Key Insight

All four models reduce to the same primitive: **ordered maps from byte-string
keys to byte-string values, stored in B+trees on pages** (the structure
DD-001 already designs). Each model is a thin *key encoding* over that
primitive, not a new storage engine.

| Model | What it stores | Where it lives |
|---|---|---|
| Table / ORM | rows by primary key, secondary indexes | `pk -> row`, `(index, cols.., pk) -> ∅` |
| Key-value | Redis strings, hashes, sets, lists, sorted sets, TTLs | `(key, field/member/seq..) -> value` |
| Vector | embeddings as a column, ANN index | `Vector(dim)` column + HNSW adjacency in a keyspace |
| Graph | nodes (rows) and edges (rows + two indexes) | `(src, label, dst, id) -> ∅` and the reverse |

Build the substrate once; the four layers stay small.

## Layers

```
L3  Access        Rust API (one Tx over all models) · #[derive(Model)] ORM · RESP server
L2  Models        tables · kv · vectors · graph
L1  Keyspaces     named B+trees, order-preserving key encoding, range/prefix scans
L0  Storage core  pages · buffer pool · COW commit · catalog   (DD-001)
```

### L0 — Storage core (DD-001, plus transactions)

- Pages, buffer pool and B+tree exactly as DD-001. File format v2.
- **Transactions via copy-on-write commit, not a WAL.** DD-001 already writes
  split/replaced pages as new pages. Extend that LMDB-style: a writer builds
  new pages, then commits by atomically switching one of two alternating
  superblocks to the new root. Readers pin the root they started from, so they
  see a stable snapshot without locking. Crash safety falls out: an
  interrupted commit leaves the old superblock intact.
- Single writer, many readers in v1. A cross-model atomic write is just one
  root swap. (Write amplification is the cost; revisit a WAL only if it hurts.)
- A **catalog** (itself a B+tree) maps names to `{kind, root page, schema}`.
  Superblock → catalog root → everything else.

### L1 — Keyspaces

- A `Keyspace` is a named B+tree with `&[u8]` keys and values. Large values go
  to overflow pages (this needs variable-length storage; see milestones).
- **Order-preserving key encoding** so composite keys sort correctly:
  big-endian integers with the sign bit flipped, `f64` via the total-order bit
  trick, strings terminated or escaped so no key is a prefix of another.
  Everything above is built from `encode(a) ++ encode(b) ++ ..`.
- Operations: `get`, `put`, `delete`, `range(lo..hi)`, `prefix(p)`, both
  directions. Prefix scans are what make hashes, sets and graph adjacency cheap.

### L2 — Models

**Tables (relational).** The v1 engine, grown up.
- Primary key → row; each secondary index is a keyspace
  `(index_id, col values.., pk) -> ∅`.
- Types grow beyond fixed-width: `Text`, `Bytes`, `Vector(dim)`, nullable.

**Key-value (Redis).** One logical `kv` namespace, typed values:
- string: `key -> bytes`
- hash: `(key, field) -> value` (so `HGET`/`HSET` never rewrite the hash)
- set: `(key, member) -> ∅`
- list: `(key, seq) -> element`, with head/tail counters in the key's metadata
- sorted set: `(key, member) -> score` **and** `(key, score, member) -> ∅`
  (point lookup by member, range by score)
- TTL: an `expiry` keyspace `(deadline, key) -> ∅`; expire lazily on read and
  by scanning the head of that keyspace.
- Subset first: strings, hashes, sets, lists, sorted sets, `EXPIRE`, `MULTI`.

**Vector.**
- `FieldType::Vector(dim)` is `dim` little-endian `f32`s: fixed width, so it
  fits the existing record layout.
- v1 search is exact (flat scan with top-k heap): correct, simple, and fine to
  a few hundred thousand vectors. Metrics: cosine, L2, dot.
- v2 is **HNSW**, whose layers are adjacency lists in a keyspace
  `(layer, node) -> [neighbor ids]`. That is graph storage, so it reuses the
  graph layer instead of adding a new structure.
- Filtered search (`WHERE` + nearest): use a secondary index when the filter is
  selective, otherwise filter while traversing.
- Scalar/binary quantization later, to keep the index within the DD-001 RAM
  budget.

**Graph (property graph).**
- Nodes are rows (the label is the table). Edges are rows
  `(id, src, dst, label, props..)` with two index keyspaces:
  out `(src, label, dst, id)` and in `(dst, label, src, id)`.
- A graph is therefore "a table with two special indexes". Adjacency is a
  prefix scan; `neighbors(node, label, direction)`, k-hop and shortest path are
  loops over it.
- Query surface is the API first. A Cypher-like language is a later decision.

### L3 — Access

One transaction handle spans every model (illustrative API):

```rust
let mut tx = db.begin()?;
let doc = tx.table::<Document>().insert(Document { title, body, embedding })?;
tx.graph().add_edge(author_id, "WROTE", doc.id)?;
tx.kv().incr(format!("doc:{}:views", doc.id))?;
tx.commit()?;                                   // all or nothing

let hits = db.table::<Document>().nearest(Document::EMBEDDING, &query, 10)?;
```

- **ORM**: a `#[derive(Model)]` proc macro (its own `tdb-derive` crate; proc
  macros must be separate) generates the schema, row <-> struct conversion and
  typed column constants for the query builder. The same metadata feeds the
  `tdb.schema.json` export from `goal.md`, so the IDE contract and the ORM share
  one source of truth.
- **Redis protocol**: an optional `tdb-server` crate speaks RESP so existing
  Redis clients work. The engine itself stays an embedded library.

## What Has To Change In The Current Code

- **`Record` takes `&[f64]`.** That was a v1 shortcut: `f64` cannot hold every
  `i64`, and it cannot express text, bytes or vectors. Replace it with a typed
  `Value` enum (`Bool, Int32, Int64, Float32, Float64, Text, Bytes,
  Vector(Vec<f32>)`) and encode records from the schema. This is a breaking API
  change and should land before anything is built on top of it.
- **Fixed-width only.** Redis values and ORM strings need variable-length
  storage (in-page, with overflow pages).
- **`Database` is one flat file written whole on `flush()`.** It becomes the L0
  superblock/catalog/pages structure; `Table` becomes one model among four.
- **Safety stays.** All of the above is safe Rust (`unsafe_code = "forbid"`);
  untrusted bytes are errors, not panics. Page checksums are added so a torn or
  corrupt page is detected, not trusted.

## Milestones (dependency order)

1. **Storage core**: page format, buffer pool, B+tree, file format v2,
   COW commit with two superblocks, catalog. (DD-001 + this DD.)
2. **Typed values**: `Value` enum, `Text`/`Bytes`, variable-length records and
   overflow pages. Retire the `f64` API.
3. **Keyspaces**: key encoding, range/prefix scans; tables gain primary keys
   and secondary indexes.
4. **Key-value layer**: strings, hashes, sets, lists, sorted sets, TTL;
   embedded API. Then the RESP server.
5. **ORM**: `#[derive(Model)]`, query builder, schema JSON export.
6. **Vector**: `Vector(dim)` column, exact search, then HNSW.
7. **Graph**: edge tables and the two indexes, traversal API.
8. **Cross-model**: the unified `Tx`, mixed queries, then the IDE tooling and a
   query language if the API proves too verbose.

Key-value goes first among the models because it is the thinnest layer over
keyspaces and exercises the substrate hardest; tables, vectors and graph then
reuse what it proved.

## Non-Goals (for now)

- Full Redis command coverage or cluster/replication. A useful subset only.
- Distributed anything: this is an embedded engine.
- Full SQL or Cypher in the first cut.
- GPU or approximate-everything vector search; exact first.

## Open Questions

1. **Redis compatibility**: wire-compatible RESP server, an embedded Redis-style
   API, or both? Recommendation: both, embedded first.
2. **Process model**: embedded library only, or also a standalone server
   binary? Recommendation: library first; the server is an optional crate.
3. **Vector scale target**: millions of vectors in RAM, or beyond RAM like the
   DD-001 reference case? This decides how early quantization and disk-based
   ANN are needed.
4. **Durability**: COW-commit as proposed, or a WAL? Recommendation: COW; it
   matches the existing page design and the safe-Rust constraint.
5. **Graph queries**: traversal API only, or a Cypher-like language early?
   Recommendation: API first.

## Rejected Alternatives

- **Four separate engines behind one facade**: no cross-model atomicity, four
  caches competing for RAM, four recovery stories.
- **Graph or KV as the only primitive** (e.g. everything as triples): forces
  awkward encodings on tables and vectors and loses fixed-width fast paths.
- **mmap-backed pages**: needs `unsafe` (see DD-001).
