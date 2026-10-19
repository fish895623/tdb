# DD-001: Indexing Beyond RAM

**Problem.** The index must support sizes far larger than available RAM
(reference case: **100GB index on a 16GB host**). Loading the index into
memory is therefore off the table from the start.

## Key Insight

The B+tree is a *lookup structure*, not a working set. A point lookup only
traverses the tree depth — a handful of pages — no matter how large the
index is. RAM is used as a **page cache**, not as index storage.

## Design

### 1. Page-based storage

- All engine structures (index and data) live in the file as
  fixed-size **pages** (16KB by default: large enough to cut syscall
  overhead and pack keys densely, small enough to keep cache thrash low).
- Page layout (v2 format):

  ```
  u32 next_page | u32 level | u32 num_entries | u8 is_leaf
  entries...                                        (key + child_page_id or key + offset)
  ```

- No structure larger than one page is ever required in RAM.

### 2. B+tree with wide fan-out

- Leaf entry: `key (8B) + page_id (4B)` → **no record data in the index**;
  a lookup returns a pointer, then one data-page read materializes the row.
- 16KB page, 12B entry → fan-out ≈ **1300+**
- Depth for a 100GB index: `log_1300(100GB / 16KB) ≈ 3`
- **Cost of a random point lookup: 3–4 pages ≈ 48–64KB**, independent of
  total index size.
- Node split on overflow; right-leaning split to reduce latchwork
  (single-threaded v1, so splits are transactional, not concurrent).

### 3. Buffer pool (RAM budget, e.g. 8–10GB of 16GB)

- Fixed pool of pages ≈ 500K at 16KB.
- **Clock (second-chance) eviction** — simpler than LRU lists,
  cache-friendly, good enough at this scale.
- Clean pages are evicted immediately; dirty pages are written back
  before eviction (or during a background flush phase).
- Root + top internal levels are effectively permanent: the entire
  tree-top for a 100GB index is **< 1MB**.

### 4. OS page cache as second layer

- Use buffered `read()`/`write()` (or `mmap` + `MADV_WILLNEED`) so the
  kernel caches recently touched pages even if our pool evicts them.
- Sequential scans: `posix_fadvise(POSIX_FADV_SEQUENTIAL)` so the kernel
  pre-reads and discards ahead of us.
- Preallocate index/data files with `fallocate()` to avoid fragmentation.

### 5. Scans

- Full-table / index-ordered scans stream page by page through the pool
  (`leaf → leaf.next_page`); memory stays flat at pool size.
- 100GB scan = NVMe bandwidth bound (minutes), never RAM bound.

## RAM Budget (16GB host, 100GB index)

| Component | RAM |
|---|---|
| Buffer pool | ~8–10GB (tunable at open time) |
| Cached tree top (levels 0–1) | < 1MB |
| In-flight random-lookup working set | ~50KB |
| Engine bookkeeping (free lists, pin tables) | < 100MB |
| **Total engine** | **≈ 10–11GB** (OS + process get the rest) |

## File Layout (format v2)

```
tdatabase.db
├── superblock        (magic TDB2, version, page_size, pool hints)
├── page directory    (page_id → offset map; appended in extents)
├── data pages        (table records, fixed-width)
└── index pages       (B+tree levels; root page id in superblock)
```

One file remains (project goal: single self-describing file); the page
directory makes pages relocatable without repacking.

## Failure Semantics (v1)

- Power loss before flush → last `Flush()`/close wins; at most one
  transaction's worth of inserts is lost (same as v1).
- WAL (crash-consistent B+tree splits) is deliberately deferred;
  split/replace pages are written as new pages (copy-on-write), which
  already limits corruption blast radius to one page.

## Milestones

1. `Page` format + `BufferPool` (clock eviction, dirty tracking)
2. `BPlusTree` (insert/lookup/split) on top of the pool
3. File format v2: superblock + page directory; `Table::Lookup(key)`
4. Tests: synthetic 100GB index (compressed/scaled-down variant in CI),
   eviction stress, corruption detection

## Rejected Alternatives

- **Tries / LSM trees**: more moving parts for v1; B+tree is the
  best-known algorithm for this exact problem and matches the
  "readable, correctness-first" principle.
- **In-memory index + spill**: a 100GB index still does not fit;
  partial in-memory indexes complicate correctness. Rejected.
- **4KB pages**: too-small fan-out → deeper tree, more syscalls.
  16KB chosen as default, configurable.
