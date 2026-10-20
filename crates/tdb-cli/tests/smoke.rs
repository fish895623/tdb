//! Runs the `tdbd` program end to end.

mod support;

use std::fs;
use std::path::Path;
use std::process::Output;

use support::tdbd_command;

/// Run `tdbd --once` in `dir` and collect its output. Without `--once` the
/// program stays up until it is signalled; see the daemon tests below.
fn tdbd(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut all = vec!["--once"];
    all.extend_from_slice(args);
    tdbd_command(dir, &all, envs).output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn tdbd_persists_records_across_runs() {
    // Run in a scratch directory so the demo's tdatabase.db stays out of the repo.
    let dir = tempfile::tempdir().unwrap();

    let first = tdbd(dir.path(), &[], &[]);
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("Table 'users' has 2 record(s)"));
    assert!(dir.path().join("tdatabase.db").exists());

    // Each run appends two records.
    let second = tdbd(dir.path(), &[], &[]);
    assert!(second.status.success());
    assert!(stdout(&second).contains("has 4 record(s)"));
}

#[test]
fn tdbd_without_a_config_uses_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let out = tdbd(dir.path(), &[], &[]);
    // Default level is trace, so Database::open's trace line shows up.
    assert!(stdout(&out).contains("opening database at tdatabase.db"));
}

#[test]
fn tdbd_config_flag_sets_the_database_path_and_log_level() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("my.toml");
    fs::write(
        &cfg,
        "[database]\npath = \"custom.db\"\n[log]\nlevel = \"warn\"\n",
    )
    .unwrap();

    let out = tdbd(dir.path(), &["--config", cfg.to_str().unwrap()], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(dir.path().join("custom.db").exists());
    assert!(!dir.path().join("tdatabase.db").exists());
    // warn level hides the trace line, but the program's own output remains.
    assert!(!stdout(&out).contains("opening database"));
    assert!(stdout(&out).contains("flushed"));
}

#[test]
fn tdbd_reads_config_toml_from_the_current_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "[database]\npath = \"from_cwd.db\"\n",
    )
    .unwrap();

    let out = tdbd(dir.path(), &[], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(dir.path().join("from_cwd.db").exists());
}

#[test]
fn tdbd_reads_the_tdb_config_environment_variable() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("env.toml");
    fs::write(&cfg, "[database]\npath = \"from_env.db\"\n").unwrap();

    let out = tdbd(dir.path(), &[], &[("TDB_CONFIG", cfg.to_str().unwrap())]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(dir.path().join("from_env.db").exists());
}

#[test]
fn tdbd_config_precedence_is_flag_then_env_then_cwd() {
    let dir = tempfile::tempdir().unwrap();
    for (file, db) in [
        ("flag.toml", "flag.db"),
        ("env.toml", "env.db"),
        ("config.toml", "cwd.db"),
    ] {
        fs::write(
            dir.path().join(file),
            format!("[database]\npath = \"{db}\"\n"),
        )
        .unwrap();
    }
    let flag = dir.path().join("flag.toml");
    let env = dir.path().join("env.toml");

    tdbd(
        dir.path(),
        &["-c", flag.to_str().unwrap()],
        &[("TDB_CONFIG", env.to_str().unwrap())],
    );
    assert!(dir.path().join("flag.db").exists());
    assert!(!dir.path().join("env.db").exists());

    tdbd(dir.path(), &[], &[("TDB_CONFIG", env.to_str().unwrap())]);
    assert!(dir.path().join("env.db").exists());
    assert!(!dir.path().join("cwd.db").exists());
}

#[test]
fn tdbd_rust_log_overrides_the_configured_level() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "[log]\nlevel = \"trace\"\n").unwrap();

    let out = tdbd(dir.path(), &[], &[("RUST_LOG", "off")]);
    assert!(out.status.success());
    assert!(!stdout(&out).contains("opening database"));
}

#[test]
fn tdbd_rejects_an_invalid_config_without_touching_the_database() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "[storage]\npage_size = 3000\n",
    )
    .unwrap();

    let out = tdbd(dir.path(), &[], &[]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("storage.page_size"), "{err}");
    assert!(err.contains("config.toml"), "{err}");
    assert!(!dir.path().join("tdatabase.db").exists());
}

#[test]
fn tdbd_rejects_a_config_with_an_unknown_key() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("config.toml"), "[log]\nlvl = \"info\"\n").unwrap();

    let out = tdbd(dir.path(), &[], &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("lvl"));
}

#[test]
fn tdbd_fails_when_an_explicit_config_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let out = tdbd(dir.path(), &["--config", "nope.toml"], &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("nope.toml"));
    assert!(!dir.path().join("tdatabase.db").exists());
}

#[test]
fn tdbd_rejects_bad_arguments_with_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();

    let out = tdbd(dir.path(), &["--bogus"], &[]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(err.contains("--bogus"), "{err}");
    assert!(err.contains("Usage: tdbd"), "{err}");

    let out = tdbd(dir.path(), &["extra"], &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("extra"));

    let out = tdbd(dir.path(), &["--config"], &[]);
    assert_eq!(out.status.code(), Some(2));
    let err = stderr(&out);
    assert!(err.contains("--config <PATH>"), "{err}");
    assert!(err.contains("a value is required"), "{err}");

    // A bad argument stops before the database is opened.
    assert!(!dir.path().join("tdatabase.db").exists());
}

#[test]
fn tdbd_prints_help_and_version_without_touching_the_database() {
    let dir = tempfile::tempdir().unwrap();

    for flag in ["--help", "-h"] {
        let out = tdbd(dir.path(), &[flag], &[]);
        assert!(out.status.success());
        let text = stdout(&out);
        assert!(text.contains("Usage: tdbd"), "{text}");
        assert!(text.contains("--config <PATH>"), "{text}");
        assert!(text.contains("$TDB_CONFIG"), "{text}");
    }

    for flag in ["--version", "-V"] {
        let out = tdbd(dir.path(), &[flag], &[]);
        assert!(out.status.success());
        assert_eq!(
            stdout(&out).trim(),
            concat!("tdbd ", env!("CARGO_PKG_VERSION"))
        );
    }

    assert!(!dir.path().join("tdatabase.db").exists());
}

#[test]
fn tdbd_once_exits_by_itself_without_announcing_a_running_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let out = tdbd(dir.path(), &[], &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stdout(&out).contains("is running"));
}

#[test]
fn tdbd_help_describes_once_and_the_shutdown_signals() {
    let dir = tempfile::tempdir().unwrap();
    let text = stdout(&tdbd(dir.path(), &["--help"], &[]));
    assert!(text.contains("--once"), "{text}");
    assert!(text.contains("SIGTERM"), "{text}");
}

#[cfg(unix)]
mod daemon {
    //! `tdbd` without `--once`: it stays up on its async runtime, serves
    //! `tdb://` clients, and shuts down cleanly on SIGINT or SIGTERM.

    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    use tdb_protocol::{Endpoint, Request, Response, VERSION};

    use super::support::{Daemon, tdbd_command};
    use super::{fs, stderr, stdout, tdbd};

    #[test]
    fn tdbd_stays_up_until_a_stop_signal_then_exits_cleanly() {
        for (signal, name) in [("TERM", "SIGTERM"), ("INT", "SIGINT")] {
            let dir = tempfile::tempdir().unwrap();
            let mut daemon = Daemon::spawn(dir.path(), &[]);
            daemon.wait_for("is running");

            // It has not exited on its own.
            thread::sleep(Duration::from_millis(300));
            assert!(daemon.child.try_wait().unwrap().is_none(), "{name}");

            daemon.signal(signal);
            let status = daemon.wait_exit();
            assert!(status.success(), "{name}: {status}");

            let text = daemon.output();
            assert!(text.contains(&format!("Received {name}")), "{name}: {text}");
            assert!(text.contains("tdbd stopped."), "{name}: {text}");
            assert!(dir.path().join("tdatabase.db").exists(), "{name}");
        }
    }

    #[test]
    fn tdbd_has_flushed_the_startup_records_before_it_reports_running() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::spawn(dir.path(), &[]);
        daemon.wait_for("is running");

        // Killed outright: nothing gets a chance to flush on the way out.
        daemon.child.kill().unwrap();
        daemon.wait_exit();

        let out = tdbd(dir.path(), &[], &[]);
        assert!(out.status.success(), "{}", stderr(&out));
        assert!(stdout(&out).contains("has 4 record(s)"));
    }

    #[test]
    fn tdbd_uses_the_configured_database_when_running_as_a_daemon() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("config.toml"),
            "[database]\npath = \"daemon.db\"\n[log]\nlevel = \"off\"\n",
        )
        .unwrap();

        let mut daemon = Daemon::spawn(dir.path(), &[]);
        daemon.wait_for("is running");
        daemon.signal("TERM");
        assert!(daemon.wait_exit().success());
        assert!(dir.path().join("daemon.db").exists());
        assert!(!dir.path().join("tdatabase.db").exists());
    }

    #[test]
    fn tdbd_fails_fast_on_a_bad_config_instead_of_staying_up() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config.toml"), "[log]\nlvl = \"info\"\n").unwrap();

        let mut daemon = Daemon::spawn(dir.path(), &[]);
        let status = daemon.wait_exit();
        assert_eq!(status.code(), Some(1));
        assert!(!daemon.output().contains("is running"));
        assert!(!dir.path().join("tdatabase.db").exists());
    }

    #[test]
    fn tdbd_announces_the_address_it_listens_on_and_accepts_connections() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::spawn(dir.path(), &[]);
        let addr = daemon.addr();
        assert!(addr.ip().is_loopback(), "{addr}");
        assert_ne!(addr.port(), 0, "{addr}");

        // It speaks the protocol on that address.
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let url = format!("tdb://{addr}/keyval/main");
        let route = url.parse::<Endpoint>().unwrap().route().clone();
        Request::Hello {
            version: VERSION,
            route,
        }
        .write_to(&mut stream)
        .unwrap();
        let reply = Response::read_from(&mut stream).unwrap();
        assert_eq!(reply, Some(Response::HelloOk { version: VERSION }));
    }

    #[test]
    fn tdbd_listen_flag_overrides_the_configured_address() {
        // The configured address is taken, so only the flag can work.
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("config.toml"),
            format!("[server]\nlisten = \"{}\"\n", taken.local_addr().unwrap()),
        )
        .unwrap();

        // `spawn` passes `--listen 127.0.0.1:0`.
        let mut daemon = Daemon::spawn(dir.path(), &[]);
        let addr = daemon.addr();
        assert_ne!(addr, taken.local_addr().unwrap());
        daemon.stop();
    }

    #[test]
    fn tdbd_listens_on_the_configured_address_without_the_flag() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("config.toml"),
            "[server]\nlisten = \"127.0.0.1:0\"\n",
        )
        .unwrap();

        let mut daemon = Daemon::spawn_raw(dir.path(), &[]);
        assert!(daemon.addr().ip().is_loopback());
        daemon.stop();
    }

    #[test]
    fn tdbd_fails_before_touching_the_database_when_the_address_is_in_use() {
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = taken.local_addr().unwrap().to_string();
        let dir = tempfile::tempdir().unwrap();

        let out = tdbd_command(dir.path(), &["--listen", &addr], &[])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        let err = stderr(&out);
        assert!(err.contains("listening on"), "{err}");
        assert!(err.contains(&addr), "{err}");
        assert!(!dir.path().join("tdatabase.db").exists());
    }

    #[test]
    fn tdbd_once_opens_no_listener() {
        // An address that is taken would fail a bind, so success shows there
        // was none.
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = taken.local_addr().unwrap().to_string();
        let dir = tempfile::tempdir().unwrap();

        let out = tdbd(dir.path(), &["--listen", &addr], &[]);
        assert!(out.status.success(), "{}", stderr(&out));
    }

    #[test]
    fn tdbd_rejects_a_listen_address_that_is_not_an_ip_and_port() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["nonsense", "localhost:80", "127.0.0.1", "127.0.0.1:70000"] {
            let out = tdbd(dir.path(), &["--listen", bad], &[]);
            assert_eq!(out.status.code(), Some(2), "{bad}");
            assert!(stderr(&out).contains("--listen"), "{bad}: {}", stderr(&out));
        }

        fs::write(
            dir.path().join("config.toml"),
            "[server]\nlisten = \"localhost:80\"\n",
        )
        .unwrap();
        let out = tdbd(dir.path(), &[], &[]);
        assert_eq!(out.status.code(), Some(1));
        assert!(stderr(&out).contains("listen"), "{}", stderr(&out));
        assert!(!dir.path().join("tdatabase.db").exists());
    }

    #[test]
    fn tdbd_closes_connected_clients_when_it_shuts_down() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::spawn(dir.path(), &[]);
        let addr = daemon.addr();

        let mut client = TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let route = format!("tdb://{addr}/graph/social")
            .parse::<Endpoint>()
            .unwrap()
            .route()
            .clone();
        Request::Hello {
            version: VERSION,
            route,
        }
        .write_to(&mut client)
        .unwrap();
        assert!(Response::read_from(&mut client).unwrap().is_some());

        // The connection is idle and open; shutdown must not wait for it.
        let text = daemon.stop();
        assert!(text.contains("tdbd stopped."), "{text}");
        assert_eq!(Response::read_from(&mut client).unwrap(), None);
    }
}
