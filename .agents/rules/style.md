---
paths:
  - "src/**"
  - "tests/**"
  - "examples/**"
  - "Cargo.toml"
---

# Style rules

Standard Rust style and naming (the Rust API Guidelines). `cargo fmt` formats,
`cargo clippy` lints, and rustc's own `non_snake_case` / `non_camel_case_types`
lints enforce naming.

| What | Style | Example |
| --- | --- | --- |
| Crate, module, file | `snake_case` | `kv_lexer` |
| Type, trait, enum, variant | `CamelCase` | `Database`, `FieldType::Int64` |
| Function, method, variable, field | `snake_case` | `get_table()`, `path` |
| Constant, static | `SCREAMING_SNAKE_CASE` | `MAX_NAME_LEN` |
| Getter | the bare name, no `get_` | `name()`, `bytes()` |

- Getters drop the `get_` prefix (`Table::name()`). Lookups that can fail keep
  it where it reads naturally (`Database::get_table`, `Record::get`).
- Fallible functions return `Result<_, Error>`; "not found" is `Option`.
- Run `cargo fmt` before every commit; do not hand-format.
- Do not silence a lint with `#[allow(...)]` to dodge a rename or a fix.
