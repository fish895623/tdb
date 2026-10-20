//! Runs the `tdb` program end to end.

#[cfg(unix)]
mod support;

use std::process::{Command, Output};

fn tdb(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tdb"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn tdb_without_arguments_tokenizes_one_statement_per_model() {
    let out = tdb(&[]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    for model in ["Keyval", "Rdbms", "Vector", "Graph"] {
        assert!(text.contains(&format!("[{model}] ")), "{model}: {text}");
    }
}

#[test]
fn tdb_model_flag_narrows_the_demo_to_that_model() {
    let out = tdb(&["--model", "graph"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("[Graph] "), "{text}");
    for other in ["Keyval", "Rdbms", "Vector"] {
        assert!(!text.contains(&format!("[{other}] ")), "{other}: {text}");
    }

    let short = tdb(&["-m", "graph"]);
    assert_eq!(stdout(&short), text);
}

#[test]
fn tdb_tokenizes_a_statement_given_as_an_argument() {
    // No model: it is detected from the first word.
    let out = tdb(&["--local", "GET user:1"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Keyword(GET)"), "{text}");
    assert!(text.contains("EndOfFile"), "{text}");
    assert!(!text.contains('['), "no demo header expected: {text}");
}

#[test]
fn tdb_model_flag_fixes_the_dialect_of_a_statement() {
    // MATCH is a keyword in graph and an ordinary name in rdbms (DD-003).
    let graph = stdout(&tdb(&["--local", "-m", "graph", "MATCH (n) RETURN n;"]));
    assert!(graph.contains("Keyword(MATCH)"), "{graph}");

    let rdbms = stdout(&tdb(&["--local", "-m", "rdbms", "SELECT match FROM t;"]));
    assert!(!rdbms.contains("Keyword(MATCH)"), "{rdbms}");
    assert!(rdbms.contains("Keyword(SELECT)"), "{rdbms}");
}

#[test]
fn tdb_reports_a_lex_error_with_its_position_and_a_failing_status() {
    let out = tdb(&["--local", "-m", "keyval", "SET k \"unterminated"]);
    assert_eq!(out.status.code(), Some(1));
    let err = stderr(&out);
    assert!(err.contains("unterminated string"), "{err}");
    assert!(err.contains("1:7"), "{err}");
}

#[test]
fn tdb_rejects_an_unknown_model_and_extra_arguments() {
    let out = tdb(&["--model", "nosql"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(err.contains("nosql"), "{err}");
    for model in ["keyval", "rdbms", "vector", "graph"] {
        assert!(err.contains(model), "{model}: {err}");
    }

    let out = tdb(&["SELECT 1;", "SELECT 2;"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("SELECT 2;"));

    let out = tdb(&["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("Usage: tdb"));
}

#[test]
fn tdb_prints_help_and_version() {
    for flag in ["--help", "-h"] {
        let out = tdb(&[flag]);
        assert!(out.status.success());
        let text = stdout(&out);
        assert!(text.contains("Usage: tdb"), "{text}");
        assert!(text.contains("--model <MODEL>"), "{text}");
        assert!(text.contains("[STATEMENT]"), "{text}");
    }

    for flag in ["--version", "-V"] {
        let out = tdb(&[flag]);
        assert!(out.status.success());
        assert_eq!(
            stdout(&out).trim(),
            concat!("tdb ", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn tdb_help_describes_the_url_option() {
    let text = stdout(&tdb(&["--help"]));
    assert!(text.contains("--url <URL>"), "{text}");
    assert!(text.contains("tdb://HOST[:PORT]"), "{text}");
    assert!(text.contains("/MODEL/DATABASE"), "{text}");
    assert!(text.contains("--local"), "{text}");
    assert!(text.contains("127.0.0.1:7878"), "{text}");
}

#[test]
fn tdb_the_default_port_is_the_one_tdbd_listens_on() {
    // tdb-protocol and tdb-config each define it; they must not drift.
    let configured = tdb_config::Config::default().server.listen.port();
    assert_eq!(tdb_protocol::DEFAULT_PORT, configured);
    assert_eq!(tdb_config::DEFAULT_LISTEN_PORT, configured);
}

#[test]
fn tdb_url_must_be_a_valid_tdb_url_and_is_checked_before_connecting() {
    let cases = [
        ("http://127.0.0.1:7878/keyval/main", "tdb://"),
        ("tdb://", "host"),
        ("tdb://127.0.0.1:99999", "port"),
        ("tdb://user:pw@127.0.0.1:7878", "credentials"),
        ("tdb://127.0.0.1:7878/nosql/main", "nosql"),
        ("tdb://127.0.0.1:7878/keyval", "database"),
        ("tdb://127.0.0.1:7878/keyval/bad.name", "database name"),
        ("tdb://127.0.0.1:99999/keyval/main", "port"),
        ("tdb:///keyval/main", "host"),
    ];
    for (url, expect) in cases {
        // Nothing listens on these, so a connection attempt would be a
        // different error: the status is clap's usage error.
        let out = tdb(&["--url", url, "GET x"]);
        assert_eq!(out.status.code(), Some(2), "{url}");
        let err = stderr(&out);
        assert!(err.contains("--url"), "{url}: {err}");
        assert!(err.contains(expect), "{url}: {err}");
    }
}

#[test]
fn tdb_url_conflicts_with_model_and_needs_a_statement() {
    let url = "tdb://127.0.0.1:7878/keyval/main";

    // --model and a route in the URL would say the model twice.
    let out = tdb(&["--url", url, "--model", "keyval", "GET x"]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(err.contains("--model"), "{err}");
    assert!(err.contains("route"), "{err}");

    // Lexing here and sending to a server are opposites.
    let out = tdb(&["--url", url, "--local", "GET x"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--local"), "{}", stderr(&out));

    let out = tdb(&["--url", url]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("STATEMENT"), "{}", stderr(&out));
}

#[cfg(unix)]
mod remote {
    //! `tdb --url`: the statement goes to a real `tdbd` and the tokens come
    //! back.

    use std::io::Read;
    use std::net::TcpListener;
    use std::thread;

    use super::support::Daemon;
    use super::{stderr, stdout, tdb};

    const STATEMENTS: &[(&str, &str)] = &[
        ("keyval", "SET user:1 \"Ada\"\n"),
        ("rdbms", "SELECT name FROM users WHERE id = 1 LIMIT 5;"),
        (
            "vector",
            "SELECT id FROM docs ORDER BY embedding <-> [0.1, 0.2] LIMIT 3;",
        ),
        ("graph", "MATCH (a:Person)-[:KNOWS]->(b) RETURN b.name;"),
    ];

    /// A `tdbd` on a free port in a scratch directory, and its URL root. Keep
    /// the directory alive for as long as the daemon runs.
    fn start() -> (tempfile::TempDir, Daemon, String) {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::spawn(dir.path(), &[]);
        let root = format!("tdb://{}", daemon.addr());
        (dir, daemon, root)
    }

    #[test]
    fn tdb_url_prints_the_same_tokens_as_lexing_locally() {
        let (_dir, daemon, root) = start();
        for (model, statement) in STATEMENTS {
            let remote = tdb(&["--url", &format!("{root}/{model}/main"), statement]);
            assert!(remote.status.success(), "{model}: {}", stderr(&remote));
            let local = tdb(&["--local", "--model", model, statement]);
            assert!(local.status.success(), "{model}");
            assert_eq!(stdout(&remote), stdout(&local), "{model}");
            assert!(stdout(&remote).contains("EndOfFile"), "{model}");
        }
        daemon.stop();
    }

    #[test]
    fn tdb_url_route_picks_the_dialect_the_server_lexes_in() {
        let (_dir, daemon, root) = start();
        let statement = "MATCH (n) RETURN n;";
        let graph = stdout(&tdb(&["--url", &format!("{root}/graph/g"), statement]));
        let rdbms = stdout(&tdb(&["--url", &format!("{root}/rdbms/g"), statement]));
        assert!(graph.contains("Keyword(MATCH)"), "{graph}");
        // MATCH is an ordinary name outside the graph dialect (DD-003).
        assert!(!rdbms.contains("Keyword(MATCH)"), "{rdbms}");
        assert_ne!(graph, rdbms);
        daemon.stop();
    }

    #[test]
    fn tdb_without_a_route_takes_the_model_from_the_statements_first_word() {
        let (_dir, daemon, root) = start();
        // (statement, the model it should be sent as)
        let cases = [
            ("GET user:1", "keyval"),
            ("HSET h f v", "keyval"),
            ("MATCH (n) RETURN n;", "graph"),
            ("SELECT name FROM users;", "rdbms"),
            ("INSERT INTO t VALUES (1);", "rdbms"),
            // A name says nothing, and neither does a word every model has:
            // the default is rdbms.
            ("x VECTOR", "rdbms"),
            ("CREATE TABLE t (id INT);", "rdbms"),
            // Not even a statement that fails to lex locally in any model.
            ("'unterminated", "rdbms"),
        ];
        for (statement, model) in cases {
            let remote = tdb(&["--url", &root, statement]);
            let local = tdb(&["--local", "--model", model, statement]);
            assert_eq!(stdout(&remote), stdout(&local), "{statement}");
            assert_eq!(remote.status.code(), local.status.code(), "{statement}");
        }
        // The server's own log says which route each connection had.
        let log = daemon.stop();
        for expected in [
            "route /keyval/main",
            "route /graph/main",
            "route /rdbms/main",
        ] {
            assert!(
                log.contains(expected),
                "no {expected:?} in the server log:\n{log}"
            );
        }
        assert!(!log.contains("route /vector/"), "{log}");
    }

    #[test]
    fn tdb_model_flag_picks_the_route_when_the_url_has_none() {
        let (_dir, daemon, root) = start();
        // The statement says rdbms; the flag says vector, and wins.
        let statement = "SELECT id FROM docs ORDER BY e <-> [0.1, 0.2] LIMIT 1;";
        let remote = tdb(&["--url", &root, "--model", "vector", statement]);
        assert!(remote.status.success(), "{}", stderr(&remote));
        let local = tdb(&["--local", "--model", "vector", statement]);
        assert_eq!(stdout(&remote), stdout(&local));

        // And in a dialect that does not reserve SELECT, the word is a name.
        let graph = stdout(&tdb(&["--url", &root, "-m", "graph", "SELECT 1;"]));
        assert!(!graph.contains("Keyword(SELECT)"), "{graph}");

        let log = daemon.stop();
        assert!(log.contains("route /vector/main"), "{log}");
        assert!(log.contains("route /graph/main"), "{log}");
    }

    #[test]
    fn tdb_a_route_in_the_url_is_used_as_written() {
        let (_dir, daemon, root) = start();
        // The statement says keyval, the URL says graph and a database.
        let out = tdb(&["--url", &format!("{root}/graph/social"), "GET user:1"]);
        let local = tdb(&["--local", "--model", "graph", "GET user:1"]);
        assert_eq!(stdout(&out), stdout(&local));
        assert!(!stdout(&out).contains("Keyword(GET)"));
        let log = daemon.stop();
        assert!(log.contains("route /graph/social"), "{log}");
    }

    #[test]
    fn tdb_sends_to_the_default_address_when_given_no_url() {
        // The default is 127.0.0.1:7878, which a developer may have a tdbd
        // on and may not, so either outcome is checked.
        let out = tdb(&["GET user:1"]);
        if out.status.success() {
            assert!(stdout(&out).contains("Keyword(GET)"), "{}", stdout(&out));
        } else {
            let err = stderr(&out);
            assert!(err.contains("connecting to 127.0.0.1:7878"), "{err}");
            assert!(err.contains("is tdbd running"), "{err}");
        }
    }

    #[test]
    fn tdb_url_reports_a_lex_error_from_the_server() {
        let (_dir, daemon, root) = start();
        let out = tdb(&[
            "--url",
            &format!("{root}/keyval/main"),
            "SET k \"unterminated",
        ]);
        assert_eq!(out.status.code(), Some(1));
        let err = stderr(&out);
        assert!(err.contains("unterminated string"), "{err}");
        assert!(err.contains("1:7"), "{err}");
        assert_eq!(stdout(&out), "");
        // One bad statement does not hurt the server.
        let ok = tdb(&["--url", &format!("{root}/keyval/main"), "GET k"]);
        assert!(ok.status.success(), "{}", stderr(&ok));
        daemon.stop();
    }

    #[test]
    fn tdb_url_reports_a_connection_that_is_refused() {
        // A port that was free a moment ago and has nothing listening.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let out = tdb(&[
            "--url",
            &format!("tdb://127.0.0.1:{port}/keyval/main"),
            "GET x",
        ]);
        assert_eq!(out.status.code(), Some(1));
        let err = stderr(&out);
        assert!(
            err.contains(&format!("connecting to 127.0.0.1:{port}")),
            "{err}"
        );
    }

    #[test]
    fn tdb_url_reports_a_server_that_hangs_up_without_replying() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            // Read what the client sent, then close without a word.
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf);
        });

        let out = tdb(&[
            "--url",
            &format!("tdb://127.0.0.1:{port}/keyval/main"),
            "GET x",
        ]);
        server.join().unwrap();
        assert_eq!(out.status.code(), Some(1));
        let err = stderr(&out);
        assert!(err.contains("closed the connection"), "{err}");
    }

    #[test]
    fn tdb_url_reports_a_server_that_does_not_speak_the_protocol() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            use std::io::Write;
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        });

        let out = tdb(&[
            "--url",
            &format!("tdb://127.0.0.1:{port}/keyval/main"),
            "GET x",
        ]);
        server.join().unwrap();
        assert_eq!(out.status.code(), Some(1));
        // The first four bytes of "HTTP" read as a frame length that is far
        // over the limit.
        let err = stderr(&out);
        assert!(err.contains("too large"), "{err}");
    }
}
