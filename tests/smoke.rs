//! Runs the `tdbd` program end to end.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Run `tdbd` in `dir` with a clean environment (no inherited RUST_LOG or
/// TDB_CONFIG) so the results don't depend on the developer's shell.
fn tdbd(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tdbd"));
    cmd.current_dir(dir)
        .args(args)
        .env_remove("RUST_LOG")
        .env_remove("TDB_CONFIG");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
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
fn tdbd_rejects_bad_arguments_and_prints_help_on_request() {
    let dir = tempfile::tempdir().unwrap();

    let out = tdbd(dir.path(), &["--bogus"], &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("usage: tdbd"));

    let out = tdbd(dir.path(), &["--config"], &[]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--config needs a path"));

    let out = tdbd(dir.path(), &["--help"], &[]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("usage: tdbd"));
    assert!(!dir.path().join("tdatabase.db").exists());
}
