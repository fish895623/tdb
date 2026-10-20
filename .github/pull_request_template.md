## What and why

<!-- What does this change, and why? Link the issue where the scope was agreed. -->

Closes #

## Checklist

- [ ] I agreed the scope in an issue first (required for a pull request that
      should count toward a commercial license).
- [ ] I have accepted the contributor agreement in `CONTRIBUTING.md`.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] I added or updated tests, including a test for malformed or hostile input
      where the change reads untrusted data.
- [ ] No `unsafe` and no new `unwrap()`/`expect()` in library code
      (`.agents/rules/cargo.md`).
- [ ] Any new dependency is permissively licensed (MIT, Apache-2.0, BSD, ISC) and
      is not a copyleft license.
- [ ] I updated the docs that describe the change (for the lexer, the spec in
      `docs/dd-003-query-lexer.md`; for a setting, `config.toml` and its test).

## Commercial license (optional)

<!-- If a company is sending this to cover a six-month period of its commercial
license, say which one here, or tell the maintainer privately. -->
