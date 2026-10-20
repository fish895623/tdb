# Contributing

tdb is dual-licensed (AGPL-3.0 and a commercial license that costs a
contribution or a fee every six months, see
[LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md)). Offering the commercial license
requires the project to hold the rights to every line of code.

So **outside contributions can only be accepted after a Contributor License
Agreement (CLA) is signed.** The CLA is not set up yet. Please open an issue to
discuss a change first; do not send a pull request until it is.

Before sending anything, read `.agents/rules/` (Cargo layout, testing, style)
and run:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
