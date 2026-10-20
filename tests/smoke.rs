//! Runs the `tdbd` program end to end.

use std::process::Command;

#[test]
fn tdbd_prints_hello_and_persists() {
    // Run in a scratch directory so the demo's tdatabase.db stays out of the repo.
    let dir = tempfile::tempdir().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_tdbd"))
            .current_dir(dir.path())
            .output()
            .unwrap()
    };

    let first = run();
    assert!(first.status.success());
    assert!(String::from_utf8_lossy(&first.stdout).contains("Hello, World!"));
    assert!(dir.path().join("tdatabase.db").exists());

    // Each run appends two records.
    let second = run();
    assert!(second.status.success());
    assert!(String::from_utf8_lossy(&second.stdout).contains("has 4 record(s)"));
}
