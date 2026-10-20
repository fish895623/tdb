# DD-005: The Wire Protocol

**Status.** Implemented as version 1 in `tdb-protocol` (the format) and
`tdb-server` (the listener). The server only lexes statements and replies with
their tokens; it executes nothing yet. This answers open question 1 of DD-004.

## Decision

`tdb` and `tdbd` speak a **custom binary protocol**, not RESP, HTTP or a line
protocol. A client connects to one URL, as DD-004 planned:

```
tdb://<host>[:<port>]/<model>/<database>

tdb://127.0.0.1:7878/keyval/main
tdb://localhost/graph/social            port defaults to 7878
tdb://[::1]:7878/vector/docs            IPv6 in brackets
```

Why custom:

- **One protocol for all four models.** The route says which model a connection
  speaks, and that is fixed for its life (DD-004). RESP can only carry
  `keyval`, and HTTP would put a request/response envelope around a connection
  that wants to stay open.
- **The format is ours to bound.** Length-prefixed frames make every limit
  checkable before anything is read or allocated.
- **Room to grow.** Message types and a version number are in the first frame,
  so transactions (DD-004 question 3) and credentials (question 4) can be added
  without breaking a v1 client.

This does not rule out a Redis-compatible front end. A RESP listener for
`keyval` (DD-002) would be a second listener over the same engine, not a change
to this protocol.

## Framing

Every message is one frame:

```
+----------------+----------------------+
| length: u32 LE | body: <length> bytes |
+----------------+----------------------+
```

- All integers are **little-endian**, like the file format (`TDB1`).
- `length` counts the body only, and is at most `MAX_FRAME_LEN` (4 MiB). A
  larger value is refused when the header is read: the body is not read, and
  nothing is allocated for it.
- The **first** frame of a connection, the hello, is limited to
  `MAX_HELLO_LEN` (256 bytes) instead: a hello is about a hundred bytes at
  most, and a peer that has not said hello yet gets almost nothing from the
  server.
- The body starts with a one-byte **message type**. An empty body is truncated.
- A **string** is a `u32` byte length, then that many bytes of UTF-8.
- A message ends exactly where its fields end. Leftover bytes are an error.

## Messages

Requests (client to server) have types below `0x80`; responses are `0x80` and
above.

| Type | Name | Body after the type byte |
|---|---|---|
| `0x01` | `Hello` | magic `TDBP` (4 bytes), version `u16`, route path (string) |
| `0x02` | `Query` | statement (string) |
| `0x81` | `HelloOk` | version `u16` |
| `0x82` | `Tokens` | count `u32`, then per token: kind (string), value (string), line `u32`, column `u32` |
| `0xFF` | `Error` | kind `u8`, message (string) |

The route path in `Hello` is the path form of a route, `/<model>/<database>`,
with the rules of DD-004:

- exactly two segments, so `/graph`, `/graph/a/b` and `/graph/` are errors;
- the model is `keyval`, `rdbms`, `vector` or `graph`, in lower case;
- the database is 1 to 64 bytes of `[A-Za-z0-9_-]`. No dots, slashes or `..`,
  so a name can never reach outside the data directory, and a query string
  (`?x=1`) is an error, not ignored.

`Tokens` carries each token's type as `tdb_lexer::TokenType` prints it
(`Keyword(GET)`, `Identifier`, ...), its text, and its 1-based position. It is
the placeholder reply: when statements are parsed and executed it is joined by
real result messages, and old clients keep working because the version is
negotiated in the hello.

### Error kinds

| Code | Kind | Sent when | Connection |
|---|---|---|---|
| 1 | `Protocol` | a malformed or unexpected frame | closed |
| 2 | `Version` | the hello's version is not one the server speaks | closed |
| 3 | `Route` | the hello's route is not valid | closed |
| 4 | `Lex` | the statement could not be lexed (the message has the position) | stays open |
| 5 | `TooLarge` | the statement is over `MAX_STATEMENT_LEN` (64 KiB) | stays open |

An unknown kind code in a received error frame is itself a protocol error.

## A connection

```
client                                   server
  | -- Hello{version=1, /graph/social} -> |   fixes the model for the connection
  | <- HelloOk{version=1} --------------- |
  | -- Query{"MATCH (n) RETURN n;"} ----> |   lexed as `graph`, one at a time
  | <- Tokens[..] ----------------------- |
  | -- Query{"MATCH (n"} ---------------> |
  | <- Error{Lex, "... at 1:9"} --------- |   connection stays open
  | (close)                               |
```

- The first frame must be a `Hello`. Anything else gets `Error{Protocol}` and
  the connection is closed. A second `Hello` is a protocol error too.
- The server speaks exactly one version, and **reads the version first**: from
  the first seven bytes of the hello (type, magic, version), before the route
  or anything after it is decoded. A different version gets `Error{Version}`
  naming the version the server speaks, whatever else the hello holds, so a
  client from a newer protocol, whose hello this build cannot parse, still
  learns why it was refused.
- Requests on a connection are answered one at a time, in order. A client may
  send the next request as soon as it has read the reply; pipelining is not
  part of v1.
- The server lexes with `Lexer::for_model` for the route's model (DD-003, and
  DD-004's "the caller already knows the model"), so `MATCH` is a keyword on a
  `graph` connection and an ordinary name on an `rdbms` one.
- A client that closes the connection between frames ends it cleanly; closing
  in the middle of a frame is a truncated frame, logged and dropped.

## Hostile peers

The listener is reachable by anything that can open a socket, so nothing a peer
sends may panic the server or cost more than the peer paid for:

- **Frame size.** Checked from the header, before reading: 256 bytes for the
  hello, 4 MiB after it. The body buffer grows as bytes arrive, so announcing a
  large frame and sending nothing allocates nothing.
- **Counts.** A token count is a claim: decoding reserves nothing for it up
  front, the list grows as tokens are really read, and a count that lies runs
  out of input as a truncated frame.
- **Reply size.** A token's reply is at most about 27 bytes for each byte of
  statement, and a statement has at most 64 KiB, so the reply to an accepted
  statement stays under 2 MB, inside the 4 MiB frame limit. (A test lexes the
  worst shapes at the limit.) The reply is built into one frame and the
  lexer's output dropped before the write starts, so a peer that stops reading
  holds a single frame.
- **Deep input.** The lexer's stack use does not depend on the statement: a
  long run of non-ASCII text once overflowed it in debug builds, which would
  have aborted the whole server (DD-003, "Details the tests pin").
- **Error replies.** An error that echoes the peer's input (an unknown model
  name) repeats at most 32 characters of it.
- **Slow peers.** A connection has 5 seconds to send its hello, then is
  dropped. The peer has 10 seconds to take each reply, and a peer that stops
  reading is dropped, not waited for. (An established connection that is
  idle between requests is left alone; there is no idle timeout yet.)
- **Many peers.** At most 1024 connections are open at once. At the limit the
  server stops accepting, and the rest wait in the operating system's queue
  until a slot frees up, so they cost the server nothing. (An idle timeout or
  a limit per address would stop a few peers holding every slot; neither
  exists yet.)
- **Not tdb.** The magic bytes and the hello limit turn away other protocols:
  an HTTP request sent to the port reads as a first frame of about 540 MB, is
  refused from its header and gets `Error{Protocol}`.
- **Failure is local.** A problem ends that one connection and is logged at
  debug level. A failed `accept` is logged and retried after a pause, so
  running out of file descriptors cannot spin the CPU.

## Shutdown

When `tdbd` is told to stop (SIGINT or SIGTERM), the server:

1. stops accepting connections;
2. lets each connection finish the request it is on and then close (a
   connection that has not said hello yet, or is idle, ends at once);
3. waits up to 5 seconds, then cuts off whatever is left (a reply being
   written to a peer that is not reading);
4. returns, and `tdbd` flushes the database and exits.

## Configuration and tools

- `tdbd` listens on `[server] listen` in `config.toml` (default
  `127.0.0.1:7878`), or on `--listen <ADDR>`. Port 0 picks a free port, and the
  address it got is printed on the "is running" line. **Loopback only by
  default**, because there is no authentication yet.
- `tdb --url <URL> <STATEMENT>` sends one statement and prints the tokens, in
  the same format as lexing locally. Without `--url` it lexes locally, as
  before. A URL takes no credentials (`user:password@` is refused, and not
  repeated in the error), and a port is decimal digits.
- The default port, 7878, is in both `tdb-config` (`DEFAULT_LISTEN_PORT`) and
  `tdb-protocol` (`DEFAULT_PORT`); a test in `tdb-cli` keeps them equal.

## Crates

- `tdb-protocol`: `Route`, `Endpoint`, `Request`, `Response`, the frame codec.
  Depends only on `tdb-lexer` (for `Model` and `Token`). No sockets, no async
  runtime: the clients and the server share it.
- `tdb-server`: `serve`, the connection state machine, on tokio. Depends on
  `tdb-protocol`, `tdb-lexer`, `log` and `tokio`.

## Not in v1

- **Authentication and encryption.** The hello is where credentials would go,
  and a TLS layer would wrap the whole connection. Until then, listen on
  loopback (DD-004 question 4).
- **Executing statements.** Waiting for the parser and the engine layers.
- **Pipelining and streaming results.** One request, one reply.
- **Transactions across models** (DD-004 question 3): new message types, so a
  version bump.
- **Idle timeouts, and limits per address or per user.**

## Versioning

`VERSION` is 1. A change that an old peer could misread (a changed field, a
message type that must be understood) bumps it; a new message type that is only
sent after the peer asked for it does not. The server answers a hello with a
version it does not speak with `Error{Version}`, so a newer client can read the
reason and fall back or give up.
