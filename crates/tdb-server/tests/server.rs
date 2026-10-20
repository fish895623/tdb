//! The server end to end, over real sockets, against
//! `docs/dd-005-wire-protocol.md`.
//!
//! Every test starts its own server (a tokio runtime on a background thread,
//! listening on an ephemeral loopback port) and talks to it with blocking std
//! sockets, the way a client does. Read timeouts on every client socket turn a
//! server that hangs into a failing test instead of a stuck one. Hand-built
//! byte frames are used wherever `Request` cannot express the message, which
//! is every hostile case.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc;
use std::sync::{Mutex, Once, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tdb_lexer::{LexError, Lexer, Model};
use tdb_protocol::{
    ErrorKind, MAGIC, MAX_FRAME_LEN, MAX_HELLO_LEN, MAX_STATEMENT_LEN, ProtocolError, Request,
    Response, Route, VERSION, WireToken,
};
use tdb_server::{Options, serve, serve_with};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// --- the server under test ---

/// Short, so tests that wait for a timeout stay fast.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(200);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);

/// Every client read and every wait on the server thread is bounded by this.
const IO_TIMEOUT: Duration = Duration::from_secs(10);

fn short_options() -> Options {
    Options {
        handshake_timeout: HANDSHAKE_TIMEOUT,
        shutdown_grace: SHUTDOWN_GRACE,
        ..Options::default()
    }
}

/// Error-level log records. The server logs at that level only when a
/// connection task fails (a panic), which no peer may cause, so every server
/// checks that none were logged when it stops.
struct ErrorLog(Mutex<Vec<String>>);

static ERRORS: ErrorLog = ErrorLog(Mutex::new(Vec::new()));

impl log::Log for ErrorLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Error
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(format!("{}: {}", record.target(), record.args()));
        }
    }

    fn flush(&self) {}
}

fn capture_server_errors() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = log::set_logger(&ERRORS);
        log::set_max_level(log::LevelFilter::Error);
    });
}

fn assert_no_server_errors() {
    let errors = std::mem::take(&mut *ERRORS.0.lock().unwrap_or_else(PoisonError::into_inner));
    assert!(
        errors.is_empty(),
        "the server logged errors (did a connection task panic?): {errors:?}"
    );
}

/// A server on its own thread. Dropping it stops it, so a failing assertion
/// cannot leak the thread.
struct Server {
    addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    done: mpsc::Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// `serve_with` and the short options.
    fn start() -> Server {
        Server::start_with(short_options())
    }

    fn start_with(options: Options) -> Server {
        Server::spawn(Some(options))
    }

    /// `serve`, with its default options.
    fn start_default() -> Server {
        Server::spawn(None)
    }

    fn spawn(options: Option<Options>) -> Server {
        capture_server_errors();
        let (addr_tx, addr_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let (stop, stop_rx) = oneshot::channel::<()>();

        let thread = thread::spawn(move || {
            {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("build the tokio runtime");
                runtime.block_on(async move {
                    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
                    addr_tx
                        .send(listener.local_addr().expect("local address"))
                        .expect("report the address");
                    let shutdown = async {
                        let _ = stop_rx.await;
                    };
                    match options {
                        Some(options) => serve_with(listener, shutdown, options).await,
                        None => serve(listener, shutdown).await,
                    }
                });
                // The runtime is dropped here: whatever `serve_with` cut off
                // is gone, and its sockets are closed.
            }
            let _ = done_tx.send(());
        });

        let addr = addr_rx
            .recv_timeout(IO_TIMEOUT)
            .expect("the server reports its address");
        Server {
            addr,
            stop: Some(stop),
            done,
            thread: Some(thread),
        }
    }

    fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Signal shutdown and wait for the server to return. Returns how long
    /// that took.
    fn stop(&mut self) -> Duration {
        let started = Instant::now();
        let result = self.shut_down();
        let elapsed = started.elapsed();
        result.expect("the server stops");
        assert_no_server_errors();
        elapsed
    }

    fn shut_down(&mut self) -> Result<(), String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        match self.done.recv_timeout(IO_TIMEOUT) {
            Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "serve_with did not return within {IO_TIMEOUT:?} of the shutdown signal"
            )),
            // Disconnected: the thread ended without reporting, so it
            // panicked, and `join` says so.
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => thread
                .join()
                .map_err(|_| "the server thread panicked".to_string()),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let result = self.shut_down();
        if !thread::panicking() {
            result.expect("the server stops");
            assert_no_server_errors();
        }
    }
}

// --- the client side ---

const HELLO: u8 = 0x01;
const QUERY: u8 = 0x02;

/// One route per model, in the order of `Model`.
const ROUTES: [(&str, Model); 4] = [
    ("/keyval/main", Model::Keyval),
    ("/rdbms/main", Model::Rdbms),
    ("/vector/main", Model::Vector),
    ("/graph/main", Model::Graph),
];

/// A connected socket with timeouts and nothing sent yet.
fn raw(addr: SocketAddr) -> TcpStream {
    let stream = TcpStream::connect(addr).expect("connect to the server");
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .expect("set the read timeout");
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .expect("set the write timeout");
    stream.set_nodelay(true).expect("set TCP_NODELAY");
    stream
}

/// Connect, say hello for `route`, and check the server accepted it.
fn connect(addr: SocketAddr, route: &str) -> TcpStream {
    let mut stream = raw(addr);
    Request::Hello {
        version: VERSION,
        route: route.parse::<Route>().expect("a valid route"),
    }
    .write_to(&mut stream)
    .expect("send the hello");
    assert_eq!(
        read_response(&mut stream),
        Response::HelloOk { version: VERSION }
    );
    stream
}

fn read_response(stream: &mut TcpStream) -> Response {
    match Response::read_from(stream) {
        Ok(Some(response)) => response,
        Ok(None) => panic!("the server closed the connection without replying"),
        Err(e) => panic!("reading the reply failed: {e}"),
    }
}

fn query(stream: &mut TcpStream, statement: &str) -> Response {
    Request::Query {
        statement: statement.to_owned(),
    }
    .write_to(stream)
    .expect("send the query");
    read_response(stream)
}

fn tokens_of(response: Response) -> Vec<WireToken> {
    match response {
        Response::Tokens(tokens) => tokens,
        other => panic!("expected tokens, got {other:?}"),
    }
}

/// What the server must answer for `statement`: the local lexer's tokens.
fn lex(model: Model, statement: &str) -> Vec<WireToken> {
    Lexer::for_model(statement, model)
        .tokenize()
        .expect("the statement lexes")
        .iter()
        .map(WireToken::from)
        .collect()
}

fn lex_error(model: Model, statement: &str) -> LexError {
    Lexer::for_model(statement, model)
        .tokenize()
        .expect_err("the statement does not lex")
}

/// The next reply is `Error{kind}`, whatever the connection does after it.
/// Returns the message.
fn expect_error(stream: &mut TcpStream, kind: ErrorKind) -> String {
    match read_response(stream) {
        Response::Error { kind: got, message } => {
            assert_eq!(got, kind, "the error message was {message:?}");
            message
        }
        other => panic!("expected Error{{{kind:?}}}, got {other:?}"),
    }
}

/// The next reply is `Error{kind}` and the server then closes the connection.
fn expect_refused(stream: &mut TcpStream, kind: ErrorKind) -> String {
    let message = expect_error(stream, kind);
    assert_closed(stream);
    message
}

fn is_disconnect(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
    )
}

/// The server has closed the connection: a clean end of stream or a reset,
/// and no more data. A read that times out means it is still open.
fn assert_closed(stream: &mut TcpStream) {
    let mut buf = [0u8; 64];
    match stream.read(&mut buf) {
        Ok(0) => {}
        Ok(n) => panic!(
            "expected the connection to be closed, but {n} more byte(s) arrived: {:?}",
            &buf[..n]
        ),
        Err(e) if is_disconnect(&e) => {}
        Err(e) => panic!(
            "expected the connection to be closed, but reading failed: {e} \
             (a timeout means it is still open)"
        ),
    }
}

/// The connection ends, with or without a last `Error{Protocol}` for the
/// peer.
fn assert_dropped_or_told_protocol(stream: &mut TcpStream) {
    match Response::read_from(stream) {
        Ok(None) => {}
        Ok(Some(Response::Error {
            kind: ErrorKind::Protocol,
            ..
        })) => assert_closed(stream),
        Ok(Some(other)) => panic!("expected the connection to end, got {other:?}"),
        Err(ProtocolError::Io(e)) if is_disconnect(&e) => {}
        Err(e) => panic!("reading failed: {e}"),
    }
}

/// A brand-new client is served: the server survived whatever came before.
fn assert_serving(addr: SocketAddr) {
    let mut client = connect(addr, "/keyval/main");
    assert_eq!(
        tokens_of(query(&mut client, "GET k")),
        lex(Model::Keyval, "GET k")
    );
}

// --- hand-built frames ---

fn le32(n: usize) -> [u8; 4] {
    u32::try_from(n).expect("fits a u32").to_le_bytes()
}

/// `u32 LE length`, then the body.
fn frame(body: &[u8]) -> Vec<u8> {
    [&le32(body.len())[..], body].concat()
}

/// `u32 LE length`, then the bytes.
fn string(bytes: &[u8]) -> Vec<u8> {
    [&le32(bytes.len())[..], bytes].concat()
}

fn hello_body(magic: &[u8], version: u16, route: &[u8]) -> Vec<u8> {
    [&[HELLO][..], magic, &version.to_le_bytes(), &string(route)].concat()
}

fn hello_frame(route: &[u8]) -> Vec<u8> {
    frame(&hello_body(&MAGIC, VERSION, route))
}

fn query_body(statement: &[u8]) -> Vec<u8> {
    [&[QUERY][..], &string(statement)].concat()
}

/// Send `bytes` as the first thing on a new connection and expect `kind` and a
/// closed connection. Returns the message.
fn refused_first(addr: SocketAddr, bytes: &[u8], kind: ErrorKind) -> String {
    let mut stream = raw(addr);
    stream.write_all(bytes).expect("send the bytes");
    expect_refused(&mut stream, kind)
}

/// Say hello, run one query to show the session is up, then send `bytes` and
/// expect `kind` and a closed connection. Returns the message.
fn refused_mid_session(addr: SocketAddr, bytes: &[u8], kind: ErrorKind) -> String {
    let mut stream = connect(addr, "/keyval/main");
    assert!(matches!(query(&mut stream, "GET k"), Response::Tokens(_)));
    stream.write_all(bytes).expect("send the bytes");
    expect_refused(&mut stream, kind)
}

// --- the handshake ---

#[test]
fn handshake_replies_hello_ok_with_the_server_version() {
    let server = Server::start();
    let mut stream = raw(server.addr());
    Request::Hello {
        version: VERSION,
        route: "/graph/social".parse().unwrap(),
    }
    .write_to(&mut stream)
    .unwrap();
    assert_eq!(read_response(&mut stream), Response::HelloOk { version: 1 });
    assert_eq!(VERSION, 1);
}

#[test]
fn handshake_accepts_a_hand_built_hello_frame() {
    // The hand-built hostile cases below start from this exact layout, so pin
    // it: it is the spec's table, and it equals what the codec encodes.
    let route = "/graph/social";
    let expected = Request::Hello {
        version: VERSION,
        route: route.parse().unwrap(),
    }
    .to_frame()
    .unwrap();
    assert_eq!(hello_frame(route.as_bytes()), expected);

    let server = Server::start();
    let mut stream = raw(server.addr());
    stream.write_all(&hello_frame(route.as_bytes())).unwrap();
    assert_eq!(
        read_response(&mut stream),
        Response::HelloOk { version: VERSION }
    );
    stream
        .write_all(&frame(&query_body(b"MATCH (n) RETURN n;")))
        .unwrap();
    assert_eq!(
        tokens_of(read_response(&mut stream)),
        lex(Model::Graph, "MATCH (n) RETURN n;")
    );
}

#[test]
fn handshake_accepts_database_names_of_one_to_sixty_four_bytes() {
    let server = Server::start();
    let longest = format!("/rdbms/{}", "a".repeat(64));
    for route in [
        "/keyval/x",
        "/vector/Docs_2024-v2",
        "/graph/0",
        longest.as_str(),
    ] {
        connect(server.addr(), route);
    }
}

#[test]
fn handshake_reassembles_a_request_sent_a_byte_at_a_time() {
    let server = Server::start_with(Options {
        handshake_timeout: Duration::from_secs(5),
        ..short_options()
    });
    let mut stream = raw(server.addr());
    let pieces = |stream: &mut TcpStream, bytes: &[u8]| {
        for byte in bytes {
            stream.write_all(&[*byte]).unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(1));
        }
    };
    pieces(&mut stream, &hello_frame(b"/keyval/main"));
    assert_eq!(
        read_response(&mut stream),
        Response::HelloOk { version: VERSION }
    );
    pieces(&mut stream, &frame(&query_body(b"SET k v")));
    assert_eq!(
        tokens_of(read_response(&mut stream)),
        lex(Model::Keyval, "SET k v")
    );
}

// --- queries ---

#[test]
fn query_keyval_returns_the_local_lexer_tokens() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");
    let statement = "SET user:1 \"hello world\"\nGET user:1\n";

    let tokens = tokens_of(query(&mut client, statement));
    assert_eq!(tokens, lex(Model::Keyval, statement));

    let token = |kind: &str, value: &str, line: u32, column: u32| WireToken {
        kind: kind.to_string(),
        value: value.to_string(),
        line,
        column,
    };
    assert_eq!(tokens[0], token("Keyword(SET)", "SET", 1, 1));
    assert_eq!(tokens[1], token("Word", "user:1", 1, 5));
    assert_eq!(tokens[2], token("String", "hello world", 1, 12));
    assert_eq!(tokens[3], token("Newline", "\n", 1, 25));
    assert_eq!(tokens[4], token("Keyword(GET)", "GET", 2, 1));
    assert_eq!(tokens.last().unwrap().kind, "EndOfFile");
}

#[test]
fn query_text_that_is_not_ascii_survives_the_round_trip() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");
    let statement = "SET k \"h\u{e9}llo w\u{f6}rld \u{2713} \u{1f600}\"";
    let tokens = tokens_of(query(&mut client, statement));
    assert_eq!(tokens, lex(Model::Keyval, statement));
    assert_eq!(tokens[2].kind, "String");
    assert_eq!(tokens[2].value, "h\u{e9}llo w\u{f6}rld \u{2713} \u{1f600}");
}

#[test]
fn query_route_model_fixes_the_dialect() {
    let server = Server::start();
    let statement = "MATCH (n) RETURN n;";
    let answer = |route: &str| {
        let mut client = connect(server.addr(), route);
        tokens_of(query(&mut client, statement))
    };

    let graph = answer("/graph/x");
    let rdbms = answer("/rdbms/x");
    let vector = answer("/vector/x");
    let keyval = answer("/keyval/x");

    // `MATCH` and `RETURN` are keywords of the graph dialect only.
    assert_eq!(graph, lex(Model::Graph, statement));
    assert_eq!(graph[0].kind, "Keyword(MATCH)");
    assert_eq!(graph[0].value, "MATCH");
    assert_eq!(graph[4].kind, "Keyword(RETURN)");

    assert_eq!(rdbms, lex(Model::Rdbms, statement));
    assert_eq!(rdbms[0].kind, "Identifier");
    assert_eq!(rdbms[0].value, "MATCH");
    assert_eq!(rdbms[4].kind, "Identifier");

    assert_eq!(vector, lex(Model::Vector, statement));
    assert_eq!(vector[0].kind, "Identifier");

    // Keyval lexes a line as a command with arguments: `MATCH` is no command.
    assert_eq!(keyval, lex(Model::Keyval, statement));
    assert_eq!(keyval[0].kind, "Word");

    assert_ne!(graph, rdbms);
}

#[test]
fn query_dialect_stays_fixed_for_the_life_of_the_connection() {
    // `SET user:1 "x"` is a Redis command to a lexer that detects the dialect
    // per statement. On a fixed rdbms or keyval route it never changes.
    let server = Server::start();
    let statement = "SET user:1 \"x\"";

    let mut rdbms = connect(server.addr(), "/rdbms/x");
    let mut keyval = connect(server.addr(), "/keyval/x");
    for _ in 0..2 {
        let sql = tokens_of(query(&mut rdbms, statement));
        assert_eq!(sql, lex(Model::Rdbms, statement));
        assert_eq!(sql[1].kind, "Identifier");
        assert_eq!(sql[2].kind, "Colon");

        let redis = tokens_of(query(&mut keyval, statement));
        assert_eq!(redis, lex(Model::Keyval, statement));
        assert_eq!(redis[1].kind, "Word");
    }
}

#[test]
fn query_every_model_route_lexes_in_its_own_dialect() {
    let server = Server::start();
    let cases = [
        (ROUTES[0], "HSET h f v", "Keyword(HSET)"),
        (
            ROUTES[1],
            "SELECT a, b FROM t WHERE a = 1;",
            "Keyword(SELECT)",
        ),
        (
            ROUTES[2],
            "SELECT id FROM docs ORDER BY embedding <-> [0.1, 2] LIMIT 3;",
            "Keyword(SELECT)",
        ),
        (
            ROUTES[3],
            "MATCH (a:Person)-[:KNOWS*1..3]->(b) RETURN b",
            "Keyword(MATCH)",
        ),
    ];
    for ((route, model), statement, first) in cases {
        let mut client = connect(server.addr(), route);
        let tokens = tokens_of(query(&mut client, statement));
        assert_eq!(tokens, lex(model, statement), "{route}: {statement}");
        assert_eq!(tokens[0].kind, first, "{route}: {statement}");
        assert_eq!(tokens.last().unwrap().kind, "EndOfFile", "{route}");
    }
}

#[test]
fn query_several_on_one_connection_are_answered_in_order() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/graph/main");
    let statements = [
        "MATCH (a) RETURN a",
        "MATCH (b) RETURN b;",
        "RETURN 1",
        "CREATE (c:Label)",
        "MATCH (d)-[:R]->(e) DELETE d",
        "MATCH (z) RETURN z",
    ];
    for statement in statements {
        assert_eq!(
            tokens_of(query(&mut client, statement)),
            lex(Model::Graph, statement),
            "{statement}"
        );
    }
    // And again, out of order: nothing is remembered between requests.
    for statement in statements.iter().rev() {
        assert_eq!(
            tokens_of(query(&mut client, statement)),
            lex(Model::Graph, statement),
            "{statement}"
        );
    }
}

#[test]
fn query_lex_error_names_the_position_and_keeps_the_connection() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");

    let statement = "SET k \"unterminated";
    let local = lex_error(Model::Keyval, statement);
    Request::Query {
        statement: statement.to_string(),
    }
    .write_to(&mut client)
    .unwrap();
    let message = expect_error(&mut client, ErrorKind::Lex);
    assert_eq!(message, local.to_string());
    assert!(
        message.contains(&format!("{}:{}", local.line, local.column)),
        "{message:?} has no position"
    );

    // The same connection answers the next statement.
    assert_eq!(
        tokens_of(query(&mut client, "GET k")),
        lex(Model::Keyval, "GET k")
    );
}

#[test]
fn query_lex_error_on_a_later_line_reports_that_line() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");

    let statement = "GET a\nSET k \"oops";
    let local = lex_error(Model::Keyval, statement);
    assert_eq!(local.line, 2);
    match query(&mut client, statement) {
        Response::Error { kind, message } => {
            assert_eq!(kind, ErrorKind::Lex);
            assert_eq!(message, local.to_string());
            assert!(
                message.contains(&format!("2:{}", local.column)),
                "{message}"
            );
        }
        other => panic!("expected a lex error, got {other:?}"),
    }
    assert!(matches!(query(&mut client, "GET a"), Response::Tokens(_)));
}

#[test]
fn query_lex_error_is_reported_in_every_model_and_the_connection_survives() {
    let server = Server::start();
    for (route, model) in ROUTES {
        let mut client = connect(server.addr(), route);
        for _ in 0..2 {
            let local = lex_error(model, "'open");
            match query(&mut client, "'open") {
                Response::Error { kind, message } => {
                    assert_eq!(kind, ErrorKind::Lex, "{route}");
                    assert_eq!(message, local.to_string(), "{route}");
                    assert!(message.contains("1:1"), "{route}: {message}");
                }
                other => panic!("{route}: expected a lex error, got {other:?}"),
            }
        }
        assert_eq!(
            tokens_of(query(&mut client, "ok")),
            lex(model, "ok"),
            "{route}"
        );
    }
}

#[test]
fn query_empty_statement_is_just_end_of_file() {
    let server = Server::start();
    for (route, model) in ROUTES {
        let mut client = connect(server.addr(), route);
        for statement in ["", "  \n "] {
            let tokens = tokens_of(query(&mut client, statement));
            assert_eq!(tokens, lex(model, statement), "{route}: {statement:?}");
            assert_eq!(tokens.len(), 1, "{route}: {statement:?}");
            assert_eq!(tokens[0].kind, "EndOfFile", "{route}: {statement:?}");
        }
    }
}

#[test]
fn query_statement_of_exactly_the_maximum_length_is_accepted() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/rdbms/main");

    let statement = "a ".repeat(MAX_STATEMENT_LEN / 2);
    assert_eq!(statement.len(), MAX_STATEMENT_LEN);
    let tokens = tokens_of(query(&mut client, &statement));
    // One identifier per `a`, then the end of file.
    assert_eq!(tokens.len(), MAX_STATEMENT_LEN / 2 + 1);
    assert!(tokens == lex(Model::Rdbms, &statement));

    // The connection is still good after the biggest reply.
    assert_eq!(
        tokens_of(query(&mut client, "SELECT 1")),
        lex(Model::Rdbms, "SELECT 1")
    );
}

#[test]
fn query_statement_over_the_maximum_length_is_too_large_and_the_connection_stays() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/rdbms/main");

    // Valid text, one byte too long; and a much longer one that still fits a
    // frame.
    for len in [MAX_STATEMENT_LEN + 1, 1024 * 1024] {
        let statement = "a".repeat(len);
        let message = match query(&mut client, &statement) {
            Response::Error { kind, message } => {
                assert_eq!(kind, ErrorKind::TooLarge, "{len} bytes");
                message
            }
            other => panic!("{len} bytes: expected TooLarge, got {other:?}"),
        };
        assert!(!message.is_empty());
        assert_eq!(
            tokens_of(query(&mut client, "SELECT 1")),
            lex(Model::Rdbms, "SELECT 1"),
            "after {len} bytes"
        );
    }
}

#[test]
fn query_statement_length_limit_counts_bytes_not_characters() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");

    // Under the limit in characters, over it in bytes.
    let statement = "\u{e9}".repeat(MAX_STATEMENT_LEN / 2 + 1);
    assert!(statement.chars().count() < MAX_STATEMENT_LEN);
    assert!(statement.len() > MAX_STATEMENT_LEN);
    match query(&mut client, &statement) {
        Response::Error { kind, .. } => assert_eq!(kind, ErrorKind::TooLarge),
        other => panic!("expected TooLarge, got {other:?}"),
    }
    assert!(matches!(query(&mut client, "GET k"), Response::Tokens(_)));
}

// --- hostile and malformed peers ---

#[test]
fn hostile_hello_with_another_version_gets_a_version_error_and_a_close() {
    let server = Server::start();
    // None of these numbers has a `1` in it, so a `1` in the message is the
    // version the server speaks.
    for version in [0u16, 2, u16::MAX] {
        let bytes = frame(&hello_body(&MAGIC, version, b"/keyval/main"));
        let message = refused_first(server.addr(), &bytes, ErrorKind::Version);
        assert!(
            message.contains(&VERSION.to_string()),
            "version {version}: {message:?} does not name the server's version"
        );
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_hello_with_an_invalid_route_gets_a_route_error_and_a_close() {
    let server = Server::start();
    let too_long = format!("/graph/{}", "a".repeat(65));
    let routes: [&[u8]; 18] = [
        b"/nosql/x",
        b"/graph",
        b"/graph/",
        b"/graph/a/b",
        b"/graph/a/",
        b"/graph/bad.name",
        b"/graph/..",
        b"/keyval/../x",
        b"/graph/x?y=1",
        b"/graph/a b",
        b"/GRAPH/x",
        b"/Graph/x",
        b"graph/x",
        b"/",
        b"//",
        b"",
        "/graph/caf\u{e9}".as_bytes(),
        too_long.as_bytes(),
    ];
    for route in routes {
        eprintln!("route {:?}", String::from_utf8_lossy(route));
        refused_first(server.addr(), &hello_frame(route), ErrorKind::Route);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_route_error_repeats_at_most_32_characters_of_the_peers_input() {
    let server = Server::start();
    // As long as a hello may be (MAX_HELLO_LEN bytes in all).
    let models = ["x".repeat(200), "\u{e9}".repeat(90)];
    for model in models {
        let route = format!("/{model}/db");
        let message = refused_first(
            server.addr(),
            &hello_frame(route.as_bytes()),
            ErrorKind::Route,
        );
        let echoed = model.chars().next().unwrap().to_string().repeat(33);
        assert!(!message.contains(&echoed), "echoes too much: {message}");
        assert!(message.len() < 400, "{} bytes", message.len());
    }
}

#[test]
fn hostile_hello_with_bad_magic_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    for body in [
        hello_body(b"XXXX", VERSION, b"/keyval/main"),
        hello_body(b"tdbp", VERSION, b"/keyval/main"),
        // Wrong in every field: the magic is what is checked first.
        hello_body(b"NOPE", 99, b"junk"),
    ] {
        let message = refused_first(server.addr(), &frame(&body), ErrorKind::Protocol);
        assert!(!message.is_empty());
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_hello_that_ends_early_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    let version_only = [&[HELLO][..], &MAGIC, &VERSION.to_le_bytes()].concat();
    let long_route = [&version_only[..], &le32(100), b"/keyval"].concat();
    for body in [
        vec![HELLO],
        vec![HELLO, b'T', b'D'],
        [&[HELLO][..], &MAGIC].concat(),
        [&[HELLO][..], &MAGIC, &[1]].concat(),
        version_only.clone(),
        long_route,
    ] {
        refused_first(server.addr(), &frame(&body), ErrorKind::Protocol);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_hello_with_trailing_bytes_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    let mut body = hello_body(&MAGIC, VERSION, b"/keyval/main");
    body.push(0xAA);
    refused_first(server.addr(), &frame(&body), ErrorKind::Protocol);
    assert_serving(server.addr());
}

#[test]
fn hostile_hello_route_that_is_not_utf8_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    refused_first(
        server.addr(),
        &hello_frame(&[b'/', 0xFF, b'/', b'x']),
        ErrorKind::Protocol,
    );
    assert_serving(server.addr());
}

#[test]
fn hostile_first_frame_that_is_a_query_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    refused_first(
        server.addr(),
        &frame(&query_body(b"GET k")),
        ErrorKind::Protocol,
    );
    assert_serving(server.addr());
}

#[test]
fn hostile_second_hello_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    // Right after the first hello, and after some traffic, with the same
    // route and another one.
    let mut client = connect(server.addr(), "/keyval/main");
    client.write_all(&hello_frame(b"/keyval/main")).unwrap();
    expect_refused(&mut client, ErrorKind::Protocol);

    refused_mid_session(
        server.addr(),
        &hello_frame(b"/graph/other"),
        ErrorKind::Protocol,
    );
    assert_serving(server.addr());
}

#[test]
fn hostile_unknown_message_type_in_the_first_frame_gets_a_protocol_error() {
    let server = Server::start();
    // 0x81, 0x82 and 0xFF are real message types, but responses: a client
    // may not send them.
    for tag in [0x00u8, 0x03, 0x7F, 0x80, 0x81, 0x82, 0xFE, 0xFF] {
        eprintln!("tag {tag:#04x}");
        refused_first(server.addr(), &frame(&[tag]), ErrorKind::Protocol);
        refused_first(
            server.addr(),
            &frame(&[tag, 1, 2, 3, 4]),
            ErrorKind::Protocol,
        );
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_unknown_message_type_mid_session_gets_a_protocol_error() {
    let server = Server::start();
    for tag in [0x00u8, 0x03, 0x7F, 0x81, 0x82, 0xFF] {
        eprintln!("tag {tag:#04x}");
        refused_mid_session(server.addr(), &frame(&[tag]), ErrorKind::Protocol);
        refused_mid_session(server.addr(), &frame(&[tag, 9, 9, 9]), ErrorKind::Protocol);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_empty_frame_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    refused_first(server.addr(), &frame(&[]), ErrorKind::Protocol);
    refused_mid_session(server.addr(), &frame(&[]), ErrorKind::Protocol);
    assert_serving(server.addr());
}

#[test]
fn hostile_query_with_trailing_bytes_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    for extra in [&[0u8][..], &[1, 2, 3], &[0xFF; 100]] {
        let body = [&query_body(b"GET k")[..], extra].concat();
        refused_mid_session(server.addr(), &frame(&body), ErrorKind::Protocol);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_query_whose_string_is_cut_short_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    let bodies: [Vec<u8>; 5] = [
        // No string at all.
        vec![QUERY],
        // Half a length.
        vec![QUERY, 3, 0],
        // The string claims 10 bytes and has 3.
        [&[QUERY][..], &le32(10), b"abc"].concat(),
        // The string claims 4 GiB.
        vec![QUERY, 0xFF, 0xFF, 0xFF, 0xFF, b'a'],
        // The string claims one byte more than there is.
        [&[QUERY][..], &le32(6), b"GET k"].concat(),
    ];
    for body in bodies {
        refused_mid_session(server.addr(), &frame(&body), ErrorKind::Protocol);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_statement_that_is_not_utf8_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    let statements: [&[u8]; 4] = [
        &[0xFF, 0xFE, 0xFD],
        // A lone continuation byte, a cut-off multi-byte character, and a
        // UTF-16 surrogate encoded as UTF-8.
        &[0x80],
        &[b'G', b'E', b'T', b' ', 0xC3],
        &[0xED, 0xA0, 0x80],
    ];
    for statement in statements {
        refused_mid_session(
            server.addr(),
            &frame(&query_body(statement)),
            ErrorKind::Protocol,
        );
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_http_request_gets_a_protocol_error_and_a_close() {
    let server = Server::start();
    let request = b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n";
    // "GET " read as a little-endian length is about 540 MB.
    assert!(u32::from_le_bytes(*b"GET ") > MAX_FRAME_LEN);

    let mut stream = raw(server.addr());
    stream.write_all(request).unwrap();
    expect_refused(&mut stream, ErrorKind::Protocol);
    assert_serving(server.addr());
}

#[test]
fn hostile_oversized_length_header_is_refused_without_waiting_for_a_body() {
    let server = Server::start();
    let lengths = [
        u32::MAX,
        MAX_FRAME_LEN + 1,
        0x8000_0000,
        u32::from_le_bytes(*b"GET "),
    ];
    for len in lengths {
        eprintln!("length {len}");
        // Only the four header bytes are ever sent, as the first frame...
        refused_first(server.addr(), &len.to_le_bytes(), ErrorKind::Protocol);
        // ...and in the middle of a session.
        refused_mid_session(server.addr(), &len.to_le_bytes(), ErrorKind::Protocol);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_length_header_at_the_limit_is_not_refused_but_the_body_is_waited_for() {
    // After the hello the limit is MAX_FRAME_LEN, and a header at it is fine.
    let server = Server::start();
    let mut stream = connect(server.addr(), "/keyval/main");
    stream.write_all(&MAX_FRAME_LEN.to_le_bytes()).unwrap();
    // No error frame and no close, well past the hello timeout (which
    // applies to the hello only): the server is waiting for the body.
    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT * 3))
        .unwrap();
    let mut byte = [0u8; 1];
    match stream.read(&mut byte) {
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) => {}
        other => panic!("expected the connection to stay quiet and open, got {other:?}"),
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_client_closing_in_the_middle_of_a_frame_is_dropped_quietly() {
    let server = Server::start();

    // A header that promises a body, then some of it, then gone: as the first
    // frame and mid-session.
    let mut stream = raw(server.addr());
    stream.write_all(&le32(100)).unwrap();
    stream.write_all(&[HELLO, 1, 2, 3]).unwrap();
    drop(stream);

    let mut stream = connect(server.addr(), "/keyval/main");
    stream.write_all(&le32(50)).unwrap();
    stream.write_all(&[QUERY, b'a', b'b', b'c']).unwrap();
    drop(stream);

    // A header cut short.
    let mut stream = raw(server.addr());
    stream.write_all(&[0x10, 0x00]).unwrap();
    drop(stream);

    assert_serving(server.addr());
}

#[test]
fn hostile_client_that_stops_sending_in_the_middle_of_a_frame_is_dropped() {
    let server = Server::start();

    // The write half is closed, so the server sees the end of the frame and
    // may say why, but the connection ends either way.
    let mut stream = raw(server.addr());
    stream.write_all(&le32(100)).unwrap();
    stream.write_all(&[HELLO, 1, 2, 3]).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    assert_dropped_or_told_protocol(&mut stream);

    let mut stream = connect(server.addr(), "/keyval/main");
    stream.write_all(&le32(50)).unwrap();
    stream.write_all(&[QUERY, b'a', b'b', b'c']).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    assert_dropped_or_told_protocol(&mut stream);

    assert_serving(server.addr());
}

#[test]
fn hostile_client_that_sends_nothing_is_dropped_after_the_handshake_timeout() {
    let server = Server::start();
    let started = Instant::now();
    let mut stream = raw(server.addr());
    assert_closed(&mut stream);
    let waited = started.elapsed();
    assert!(
        waited >= HANDSHAKE_TIMEOUT - Duration::from_millis(20),
        "dropped after {waited:?}, before the timeout"
    );
    assert!(waited < Duration::from_secs(3), "dropped after {waited:?}");
    assert_serving(server.addr());
}

#[test]
fn hostile_client_that_sends_one_header_byte_and_stalls_is_dropped() {
    let server = Server::start();
    let started = Instant::now();
    let mut stream = raw(server.addr());
    stream.write_all(&[0x0F]).unwrap();
    assert_closed(&mut stream);
    let waited = started.elapsed();
    assert!(waited >= HANDSHAKE_TIMEOUT - Duration::from_millis(20));
    assert!(waited < Duration::from_secs(3), "dropped after {waited:?}");
    assert_serving(server.addr());
}

#[test]
fn hostile_client_that_stalls_in_the_middle_of_its_hello_is_dropped() {
    let server = Server::start();
    let hello = hello_frame(b"/keyval/main");
    let started = Instant::now();
    let mut stream = raw(server.addr());
    stream.write_all(&hello[..hello.len() - 1]).unwrap();
    assert_closed(&mut stream);
    let waited = started.elapsed();
    assert!(waited >= HANDSHAKE_TIMEOUT - Duration::from_millis(20));
    assert!(waited < Duration::from_secs(3), "dropped after {waited:?}");
    assert_serving(server.addr());
}

#[test]
fn hostile_clients_that_connect_and_close_do_not_disturb_the_server() {
    let server = Server::start();
    for _ in 0..20 {
        drop(raw(server.addr()));
    }
    // Hello and leave without reading the reply.
    for _ in 0..5 {
        let mut stream = raw(server.addr());
        stream.write_all(&hello_frame(b"/graph/main")).unwrap();
        drop(stream);
    }
    // A query and leave without reading the reply.
    for _ in 0..5 {
        let mut stream = connect(server.addr(), "/graph/main");
        stream
            .write_all(&frame(&query_body(b"MATCH (n) RETURN n")))
            .unwrap();
        drop(stream);
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_established_connection_may_idle_longer_than_the_handshake_timeout() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");
    for _ in 0..2 {
        // Only the hello has a timeout.
        thread::sleep(HANDSHAKE_TIMEOUT * 3);
        assert_eq!(
            tokens_of(query(&mut client, "GET k")),
            lex(Model::Keyval, "GET k")
        );
    }
}

#[test]
fn hostile_connection_does_not_disturb_an_established_one() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/graph/main");
    assert!(matches!(
        query(&mut client, "MATCH (n) RETURN n"),
        Response::Tokens(_)
    ));

    // A peer that is not tdb at all, and one that breaks the protocol late.
    let mut http = raw(server.addr());
    http.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    expect_refused(&mut http, ErrorKind::Protocol);
    refused_mid_session(server.addr(), &frame(&[0x7F]), ErrorKind::Protocol);

    assert_eq!(
        tokens_of(query(&mut client, "MATCH (m) RETURN m")),
        lex(Model::Graph, "MATCH (m) RETURN m")
    );
}

// A statement of any shape up to `MAX_STATEMENT_LEN` must be answered, and no
// statement may take the server down. These are the shapes that once did: in a
// debug build `tdb-lexer` overflowed the stack on a keyval word of a few
// thousand non-ASCII characters, and on a `//` comment as long, and a stack
// overflow aborts the whole process, not just the connection. (The server's
// worker threads have 2 MiB of stack, a quarter of the main thread's.)

#[test]
fn hostile_long_non_ascii_keyval_word_is_answered_and_the_server_survives() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/keyval/main");
    // The largest statement the server accepts: one word of MAX_STATEMENT_LEN
    // bytes.
    let word = "\u{e9}".repeat(MAX_STATEMENT_LEN / 2);
    assert_eq!(word.len(), MAX_STATEMENT_LEN);

    let tokens = tokens_of(query(&mut client, &word));
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, "Word");
    assert!(tokens[0].value == word);
    assert_eq!(tokens[1].kind, "EndOfFile");
    assert_serving(server.addr());
}

#[test]
fn hostile_long_non_ascii_line_comment_is_answered_and_the_server_survives() {
    let server = Server::start();
    let mut client = connect(server.addr(), "/rdbms/main");
    let statement = format!("x // {}", "\u{e9}".repeat(MAX_STATEMENT_LEN / 4));
    assert!(statement.len() <= MAX_STATEMENT_LEN);

    let tokens = tokens_of(query(&mut client, &statement));
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].kind, "Identifier");
    assert_eq!(tokens[1].kind, "EndOfFile");
    assert_serving(server.addr());
}

// --- concurrency ---

#[test]
fn concurrency_sixteen_clients_each_get_their_own_answers() {
    const CLIENTS: usize = 16;
    const QUERIES: usize = 6;

    let server = Server::start();
    // All sixteen are established before any of them asks anything.
    let clients: Vec<TcpStream> = (0..CLIENTS)
        .map(|i| connect(server.addr(), ROUTES[i % ROUTES.len()].0))
        .collect();

    thread::scope(|scope| {
        for (i, mut client) in clients.into_iter().enumerate() {
            scope.spawn(move || {
                let model = ROUTES[i % ROUTES.len()].1;
                for j in 0..QUERIES {
                    let statement = format!("SELECT {i}, {j} FROM t{i}_{j}");
                    let tokens = tokens_of(query(&mut client, &statement));
                    assert!(
                        tokens == lex(model, &statement),
                        "client {i}, query {j}: {tokens:?}"
                    );
                    let table = format!("t{i}_{j}");
                    assert!(
                        tokens.iter().any(|t| t.value == table),
                        "client {i}, query {j} got someone else's answer: {tokens:?}"
                    );
                }
            });
        }
    });
}

// --- options and shutdown ---

#[test]
fn options_default_has_five_second_timeouts() {
    let options = Options::default();
    assert_eq!(options.handshake_timeout, Duration::from_secs(5));
    assert_eq!(options.shutdown_grace, Duration::from_secs(5));
}

#[test]
fn shutdown_serve_answers_with_default_options_and_stops_on_the_signal() {
    let mut server = Server::start_default();
    let mut client = connect(server.addr(), "/keyval/main");
    assert_eq!(
        tokens_of(query(&mut client, "GET k")),
        lex(Model::Keyval, "GET k")
    );
    // The idle connection ends at once; the default 5 s grace is not needed.
    let elapsed = server.stop();
    assert!(elapsed < Duration::from_secs(2), "stopped in {elapsed:?}");
    assert_closed(&mut client);
}

#[test]
fn shutdown_with_no_connections_returns_at_once() {
    let mut server = Server::start();
    let elapsed = server.stop();
    assert!(elapsed < SHUTDOWN_GRACE, "stopped in {elapsed:?}");
}

#[test]
fn shutdown_closes_idle_connections_promptly_and_they_see_end_of_stream() {
    // A long grace, so that returning quickly can only mean the connections
    // finished by themselves rather than being cut off.
    let mut server = Server::start_with(Options {
        shutdown_grace: Duration::from_secs(2),
        ..short_options()
    });
    let mut clients: Vec<TcpStream> = ROUTES
        .iter()
        .cycle()
        .take(6)
        .map(|(route, _)| connect(server.addr(), route))
        .collect();
    // One has done some work.
    assert!(matches!(
        query(&mut clients[0], "GET k"),
        Response::Tokens(_)
    ));

    let elapsed = server.stop();
    assert!(elapsed < Duration::from_secs(1), "stopped in {elapsed:?}");
    for client in &mut clients {
        assert_closed(client);
    }
}

#[test]
fn shutdown_refuses_new_connections() {
    let mut server = Server::start();
    let addr = server.addr();
    server.stop();

    match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
        Err(_) => {}
        Ok(mut stream) => {
            stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
            let _ = stream.write_all(&hello_frame(b"/keyval/main"));
            // A reset or an end of stream is fine; an answer is not.
            if let Ok(Some(response)) = Response::read_from(&mut stream) {
                panic!("served after shutdown: {response:?}");
            }
        }
    }
}

#[test]
fn shutdown_lets_a_hello_in_flight_finish_and_then_closes() {
    let mut server = Server::start();
    let mut client = raw(server.addr());
    let hello = hello_frame(b"/keyval/main");
    client.write_all(&hello[..3]).unwrap();

    // Shutdown begins while the hello is half sent.
    let elapsed = thread::scope(|scope| {
        let stopper = scope.spawn(|| server.stop());
        thread::sleep(Duration::from_millis(50));
        // Best effort: if the machine was slow enough for the handshake
        // timeout to drop the connection, this write may fail.
        let _ = client.write_all(&hello[3..]);
        stopper.join().expect("the stopping thread")
    });
    assert!(elapsed < Duration::from_secs(2), "stopped in {elapsed:?}");

    // Either the hello was answered and the connection then closed, or it was
    // dropped; never an error or a hang.
    match Response::read_from(&mut client) {
        Ok(Some(response)) => {
            assert_eq!(response, Response::HelloOk { version: VERSION });
            assert_closed(&mut client);
        }
        Ok(None) => {}
        Err(ProtocolError::Io(e)) if is_disconnect(&e) => {}
        Err(e) => panic!("reading failed: {e}"),
    }
}

#[test]
fn shutdown_ends_a_stalled_hello_at_once_without_waiting_for_the_grace() {
    // The hello may take 5 s and the grace is 5 s: only shutdown itself can
    // end these connections quickly.
    let grace = Duration::from_secs(5);
    let mut server = Server::start_with(Options {
        handshake_timeout: Duration::from_secs(5),
        shutdown_grace: grace,
        ..Options::default()
    });
    let mut idle = connect(server.addr(), "/keyval/main");
    let mut silent = raw(server.addr());
    let mut half_header = raw(server.addr());
    half_header.write_all(&[0x20, 0x00]).unwrap();
    // Let the server accept all of them before shutdown begins.
    assert_eq!(
        tokens_of(query(&mut idle, "GET k")),
        lex(Model::Keyval, "GET k")
    );
    thread::sleep(Duration::from_millis(50));

    let elapsed = server.stop();
    assert!(
        elapsed < Duration::from_secs(2),
        "stopped in {elapsed:?}; a stalled hello waited for the grace of {grace:?}"
    );
    assert_closed(&mut idle);
    assert_closed(&mut silent);
    assert_closed(&mut half_header);
}

#[test]
fn shutdown_with_no_grace_at_all_still_returns() {
    for grace in [Duration::ZERO, Duration::from_millis(1)] {
        let mut server = Server::start_with(Options {
            handshake_timeout: Duration::from_secs(5),
            shutdown_grace: grace,
            ..Options::default()
        });
        let mut idle = connect(server.addr(), "/graph/main");
        let mut silent = raw(server.addr());
        thread::sleep(Duration::from_millis(50));

        let elapsed = server.stop();
        assert!(
            elapsed < Duration::from_secs(3),
            "grace {grace:?}: stopped in {elapsed:?}"
        );
        assert_closed(&mut idle);
        assert_closed(&mut silent);
    }
}

// --- Defences added after review ---

#[test]
fn hostile_hello_of_a_newer_version_gets_a_version_error_whatever_else_it_holds() {
    // A newer client's hello may carry a route this build rejects, fields it
    // cannot read, or bytes it cannot parse; it must still learn the version
    // the server speaks, not get a decoding error.
    let server = Server::start();
    let speaks = format!("speaks {VERSION}");
    let cases: [(&str, Vec<u8>); 4] = [
        (
            "a route this build rejects",
            frame(&hello_body(&MAGIC, 2, b"/timeseries/x")),
        ),
        (
            "a route that is not UTF-8",
            frame(&hello_body(&MAGIC, 3, &[0xff, 0xfe])),
        ),
        ("trailing fields", {
            let mut body = hello_body(&MAGIC, 7, b"/graph/a");
            body.extend_from_slice(b"credentials=...");
            frame(&body)
        }),
        ("a hello cut off after the version", {
            let mut body = vec![HELLO];
            body.extend_from_slice(&MAGIC);
            body.extend_from_slice(&9u16.to_le_bytes());
            frame(&body)
        }),
    ];
    for (what, bytes) in cases {
        let message = refused_first(server.addr(), &bytes, ErrorKind::Version);
        assert!(message.contains(&speaks), "{what}: {message}");
    }
    assert_serving(server.addr());
}

#[test]
fn hostile_first_frame_over_the_hello_limit_is_refused_from_its_header_alone() {
    let server = Server::start();
    let started = Instant::now();
    let mut stream = raw(server.addr());
    // The header only: the server must not wait for a body it will not read.
    stream.write_all(&le32(MAX_HELLO_LEN as usize + 1)).unwrap();
    let message = expect_refused(&mut stream, ErrorKind::Protocol);
    assert!(message.contains("256"), "{message}");
    assert!(
        started.elapsed() < HANDSHAKE_TIMEOUT,
        "refused after {:?}, i.e. only when the hello timed out",
        started.elapsed()
    );
    assert_serving(server.addr());
}

#[test]
fn hostile_first_frame_of_exactly_the_hello_limit_is_waited_for() {
    let server = Server::start();
    let started = Instant::now();
    let mut stream = raw(server.addr());
    stream.write_all(&le32(MAX_HELLO_LEN as usize)).unwrap();
    // Not refused (the header is fine); the body never comes, so it is
    // dropped when the hello times out.
    assert_closed(&mut stream);
    assert!(started.elapsed() >= HANDSHAKE_TIMEOUT - Duration::from_millis(20));
    assert_serving(server.addr());
}

#[test]
fn hostile_an_http_request_is_refused_and_never_buffered() {
    let server = Server::start();
    let mut stream = raw(server.addr());
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        .unwrap();
    let message = expect_refused(&mut stream, ErrorKind::Protocol);
    assert!(message.contains("256"), "{message}");
    assert_serving(server.addr());
}

#[test]
fn query_worst_case_replies_fit_in_a_frame() {
    // Statements that make the most tokens for their length, at the length
    // limit. The reply must be tokens, not a "too large" error: the limits
    // are chosen so a reply to an accepted statement always fits a frame.
    let server = Server::start();
    let n = MAX_STATEMENT_LEN;
    let shapes: [(&str, Model, String); 6] = [
        ("/rdbms/x", Model::Rdbms, "(".repeat(n)),
        ("/rdbms/x", Model::Rdbms, ";".repeat(n)),
        ("/rdbms/x", Model::Rdbms, "a ".repeat(n / 2)),
        ("/vector/x", Model::Vector, "<->".repeat(n / 3)),
        ("/graph/x", Model::Graph, "->".repeat(n / 2)),
        ("/keyval/x", Model::Keyval, "x ".repeat(n / 2)),
    ];
    for (route, model, statement) in shapes {
        assert!(statement.len() <= n);
        let mut client = connect(server.addr(), route);
        let tokens = tokens_of(query(&mut client, &statement));
        assert_eq!(tokens, lex(model, &statement), "{route}");
        // The connection is fine afterwards.
        assert_eq!(
            tokens_of(query(&mut client, "x")),
            lex(model, "x"),
            "{route}"
        );
    }
}

#[test]
fn write_a_peer_that_stops_reading_is_dropped_after_the_write_timeout() {
    let write_timeout = Duration::from_millis(300);
    let server = Server::start_with(Options {
        write_timeout,
        ..short_options()
    });
    let mut client = connect(server.addr(), "/rdbms/x");

    // Every reply is over a megabyte and this client never reads one, so the
    // socket buffers fill and the server's write stalls. It must give up,
    // not hold the reply (or the connection) for ever.
    let mut writer = client.try_clone().unwrap();
    let statement = "(".repeat(MAX_STATEMENT_LEN);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let started = Instant::now();
    let sender = thread::spawn(move || {
        let request = Request::Query { statement };
        // Far more than any buffering absorbs; stops when the server hangs up.
        for _ in 0..200 {
            if request.write_to(&mut writer).is_err() {
                break;
            }
        }
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the server never dropped a peer that was not reading");
    assert!(
        started.elapsed() >= write_timeout,
        "dropped after {:?}, before the write timeout",
        started.elapsed()
    );
    sender.join().unwrap();

    // The stalled peer did not stall anyone else.
    assert_serving(server.addr());
    // What it was owed is finite: reading drains it and ends at the close.
    // (The read timeout from `connect` still applies; setting it again would
    // fail on a socket the server has reset.)
    let mut sink = [0u8; 64 * 1024];
    loop {
        match client.read(&mut sink) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if is_disconnect(&e) => break,
            Err(e) => panic!("the dropped connection should end, got {e}"),
        }
    }
}

#[test]
fn limit_connections_beyond_the_maximum_wait_until_a_slot_frees_up() {
    let server = Server::start_with(Options {
        max_connections: 2,
        handshake_timeout: Duration::from_secs(5),
        ..short_options()
    });
    let first = connect(server.addr(), "/keyval/a");
    let _second = connect(server.addr(), "/keyval/b");

    // A third connects at the TCP level and says hello, but is not accepted:
    // no reply while both slots are taken.
    let mut third = raw(server.addr());
    third.write_all(&hello_frame(b"/keyval/c")).unwrap();
    third
        .set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    match Response::read_from(&mut third) {
        Err(ProtocolError::Io(e))
            if matches!(
                e.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) => {}
        other => panic!("a third connection was served at the limit: {other:?}"),
    }

    // A slot frees up and the waiting connection is served, its hello
    // timeout having started only now.
    drop(first);
    third.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
    assert_eq!(
        read_response(&mut third),
        Response::HelloOk { version: VERSION }
    );
    assert_eq!(
        tokens_of(query(&mut third, "GET k")),
        lex(Model::Keyval, "GET k")
    );
}

#[test]
fn limit_a_maximum_of_zero_still_serves_one_connection() {
    let server = Server::start_with(Options {
        max_connections: 0,
        ..short_options()
    });
    assert_serving(server.addr());
}

#[test]
fn options_defaults_have_the_documented_write_timeout_and_connection_limit() {
    let options = Options::default();
    assert_eq!(options.write_timeout, Duration::from_secs(10));
    assert_eq!(options.max_connections, 1024);
}
