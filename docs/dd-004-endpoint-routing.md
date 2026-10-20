# DD-004: Endpoint Routing

**Status.** Proposal, written from the plan below. The open questions at the
end need answers before any of this is built.

## The plan

A client connects to one URL, and the **path says which model** it is talking
to and **which database** inside that model:

```
<scheme>://<host>[:<port>]/keyval/<database>   Redis-style key-value
<scheme>://<host>[:<port>]/rdbms/<database>    relational (SQL, ORM)
<scheme>://<host>[:<port>]/vector/<database>   vector search
<scheme>://<host>[:<port>]/graph/<database>    graph
```

`<scheme>` is a placeholder for the wire protocol (see open question 1).

## Route parsing

A `Route` is `(Model, database)`:

- Exactly two path segments. The first is one of `keyval`, `rdbms`, `vector`,
  `graph`; anything else is an unknown-model error.
- The database name is non-empty and limited to `[A-Za-z0-9_-]`, at most 64
  bytes. No dots, slashes or `..`, so a name can never reach outside the data
  directory, whatever it is later used for.
- Extra segments, an empty segment, or a query string that is not understood
  are errors, not ignored.
- The router returns a typed error (unknown model, bad database name) and
  never panics on a hostile URL.

This is the first thing the server does with a connection, before any request
is read, and it fixes the model for the life of the connection.

## What it means for the lexer (DD-003)

DD-003 detects the model from the first word of each statement. With routing,
the **caller already knows the model**, so the lexer should be told:

- `Lexer::for_model(input, Model::Keyval | Rdbms | Vector | Graph)`, using the
  same `Model` the router produces (done: DD-003).
- Each model has its own keyword set. `MATCH` and `RETURN` are ordinary names
  in `rdbms`, and `SELECT` is an ordinary name in `graph`. That removes most of
  the reserved-word pain of one shared keyword list.
- `Model::Keyval` is Redis-style (line-oriented, bare-word arguments) for
  every statement; no detection needed.
- The detecting lexer stays as `Lexer::new` for tools that read scripts mixing
  models.

## What it means for the engine

Two ways to back `/<model>/<database>`:

1. **One file per (model, database).** Simple and isolated, but a request can
   never be atomic across models or databases.
2. **One engine, databases as catalog namespaces.** `/graph/social` and
   `/vector/docs` are entries in the same catalog and share storage, buffer pool
   and commit, so a future transaction can span them. This keeps DD-002's
   cross-model atomicity possible.

Recommendation: 2. It costs a namespace prefix in the catalog and keeps the
property that justified binding the models into one engine in the first place.

## Crates

- `tdb-server`: listener, `Route` parsing, one handler per model. It depends on
  `tdb-core`, `tdb-lexer` and `tdb-config`.
- Settings (`tdb-config`) gain a `[server]` section: listen address, data
  directory, and the allowed models.

## Open questions

1. **Wire protocol.** What is `<scheme>`? For `keyval`, true Redis clients speak
   RESP, which is length-prefixed and needs no lexer at all (inline commands are
   only for debugging). For the others a text protocol (HTTP + a body, or a
   line protocol) is simplest. One protocol for all four, or each model speaks
   its own?
2. **Vector's language.** SQL-like with distance operators
   (`ORDER BY v <-> [..] LIMIT 5`), a JSON search API, or something else? It
   decides what the vector dialect lexes.
3. **Cross-model.** Are the four endpoints strictly separate, or will there be a
   way to run one transaction over several (a `/tx` endpoint, or a header)?
4. **Auth and multi-tenancy.** Is `<database>` a security boundary? If so,
   credentials belong in the connection, not the URL path.
