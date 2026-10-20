---
paths:
  - "include/tdb/**"
  - "bin/**"
  - "tests/**"
  - ".clang-tidy"
---

# Style rules

Google C++ naming, enforced by `readability-identifier-naming` in
`.clang-tidy`. Check with `clang-tidy -p build/debug bin/*.cpp tests/*.cpp`.

| What | Style | Example |
| --- | --- | --- |
| Namespace | `lower_case` | `tdb` |
| Class, struct, enum, alias | `CamelCase` | `Database`, `FieldType` |
| Function, method (accessors too) | `CamelCase` | `GetTable()`, `Bytes()` |
| Variable, parameter | `lower_case` | `schema`, `values` |
| Struct data member | `lower_case` | `field.name` |
| Class private member | `lower_case_` | `path_`, `tables_` |
| Enumerator | `kCamelCase` | `FieldType::kInt64` |
| `constexpr` / global / static const | `kCamelCase` | `kMaxNameLen` |
| Macro, include guard | `UPPER_CASE` | `TDB_KV_LEXER_H` |
| File | `snake_case` | `kv_lexer.h` |

- Do not add a check to `.clang-tidy` to silence a naming warning; rename.
- Accessors are `CamelCase` like every other method, not `bytes()`.
