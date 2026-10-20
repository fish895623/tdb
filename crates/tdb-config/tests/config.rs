use std::fs;
use std::path::{Path, PathBuf};

use tdb_config::{Config, ConfigError, LogLevel, MAX_PAGE_SIZE, MIN_PAGE_SIZE};

fn parse(text: &str) -> Result<Config, ConfigError> {
    Config::from_toml_str(text)
}

fn config_error(text: &str) -> String {
    match parse(text) {
        Err(e) => e.message().to_string(),
        Ok(config) => panic!("expected an error, got {config:?}"),
    }
}

#[test]
fn config_defaults_are_the_documented_values() {
    let c = Config::default();
    assert_eq!(c.database.path, PathBuf::from("tdatabase.db"));
    assert_eq!(c.log.level, LogLevel::Trace);
    assert_eq!(c.storage.page_size, 16384);
    assert_eq!(c.storage.buffer_pool_mb, 256);
    c.validate().unwrap();
}

#[test]
fn config_the_committed_config_toml_equals_the_defaults() {
    // config.toml documents the defaults; this fails if the two drift apart.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config.toml");
    assert_eq!(Config::load(path).unwrap(), Config::default());
}

#[test]
fn config_empty_file_is_all_defaults() {
    assert_eq!(parse("").unwrap(), Config::default());
}

#[test]
fn config_reads_every_setting() {
    let c = parse(
        r#"
        [database]
        path = "data/main.db"
        [log]
        level = "warn"
        [storage]
        page_size = 4096
        buffer_pool_mb = 1024
        "#,
    )
    .unwrap();
    assert_eq!(c.database.path, PathBuf::from("data/main.db"));
    assert_eq!(c.log.level, LogLevel::Warn);
    assert_eq!(c.storage.page_size, 4096);
    assert_eq!(c.storage.buffer_pool_mb, 1024);
}

#[test]
fn config_missing_keys_and_sections_fall_back_to_defaults() {
    let c = parse("[log]\nlevel = \"error\"\n").unwrap();
    assert_eq!(c.log.level, LogLevel::Error);
    assert_eq!(c.database, Config::default().database);
    assert_eq!(c.storage, Config::default().storage);

    let c = parse("[storage]\npage_size = 8192\n").unwrap();
    assert_eq!(c.storage.page_size, 8192);
    assert_eq!(
        c.storage.buffer_pool_mb,
        Config::default().storage.buffer_pool_mb
    );
}

#[test]
fn config_every_log_level_parses_and_maps_to_a_filter() {
    let cases = [
        ("off", LogLevel::Off, log::LevelFilter::Off),
        ("error", LogLevel::Error, log::LevelFilter::Error),
        ("warn", LogLevel::Warn, log::LevelFilter::Warn),
        ("info", LogLevel::Info, log::LevelFilter::Info),
        ("debug", LogLevel::Debug, log::LevelFilter::Debug),
        ("trace", LogLevel::Trace, log::LevelFilter::Trace),
    ];
    for (text, level, filter) in cases {
        let c = parse(&format!("[log]\nlevel = \"{text}\"\n")).unwrap();
        assert_eq!(c.log.level, level);
        assert_eq!(level.filter(), filter);
    }
}

#[test]
fn config_rejects_unknown_keys_instead_of_ignoring_typos() {
    assert!(config_error("loglevel = \"info\"\n").contains("loglevel"));
    assert!(config_error("[log]\nlvl = \"info\"\n").contains("lvl"));
    assert!(config_error("[storage]\npagesize = 4096\n").contains("pagesize"));
    assert!(config_error("[cache]\nsize = 1\n").contains("cache"));
}

#[test]
fn config_rejects_bad_values() {
    assert!(config_error("[log]\nlevel = \"loud\"\n").contains("loud"));
    assert!(config_error("[log]\nlevel = \"INFO\"\n").contains("INFO")); // lowercase only
    assert!(config_error("[storage]\npage_size = \"big\"\n").contains("page_size"));
    assert!(config_error("[storage]\npage_size = -4096\n").contains("page_size"));
    assert!(config_error("[database]\npath = 5\n").contains("path"));
}

#[test]
fn config_rejects_malformed_toml() {
    config_error("[log\nlevel = \"info\"\n");
    config_error("level = \n");
}

#[test]
fn config_page_size_must_be_a_power_of_two_in_range() {
    for ok in [MIN_PAGE_SIZE, 8192, 16384, 32768, MAX_PAGE_SIZE] {
        assert!(
            parse(&format!("[storage]\npage_size = {ok}\n")).is_ok(),
            "{ok}"
        );
    }
    for bad in [0, 1, 2048, 3000, 4097, 12288, 131072, u32::MAX] {
        let msg = config_error(&format!("[storage]\npage_size = {bad}\n"));
        assert!(msg.contains("storage.page_size"), "{bad}: {msg}");
        assert!(msg.contains(&bad.to_string()), "{bad}: {msg}");
    }
}

#[test]
fn config_buffer_pool_must_be_at_least_one_mib() {
    assert!(parse("[storage]\nbuffer_pool_mb = 1\n").is_ok());
    assert!(config_error("[storage]\nbuffer_pool_mb = 0\n").contains("buffer_pool_mb"));
}

#[test]
fn config_database_path_must_not_be_empty() {
    assert!(config_error("[database]\npath = \"\"\n").contains("database.path"));
}

#[test]
fn config_validate_checks_hand_built_settings() {
    let mut c = Config::default();
    c.storage.page_size = 5000;
    assert!(c.validate().is_err());
}

#[test]
fn config_error_display_names_the_problem() {
    let err = parse("[storage]\nbuffer_pool_mb = 0\n").unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid configuration: storage.buffer_pool_mb must be at least 1"
    );
}

#[test]
fn config_load_reads_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("my.toml");
    fs::write(&path, "[log]\nlevel = \"debug\"\n").unwrap();
    assert_eq!(Config::load(&path).unwrap().log.level, LogLevel::Debug);
}

#[test]
fn config_load_missing_file_names_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.toml");
    let Err(err) = Config::load(&path) else {
        panic!("expected an error");
    };
    let msg = err.message();
    assert!(msg.contains("cannot read"), "{msg}");
    assert!(msg.contains("nope.toml"), "{msg}");
}

#[test]
fn config_load_invalid_file_names_the_path_and_the_problem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    fs::write(&path, "[storage]\npage_size = 3000\n").unwrap();
    let Err(err) = Config::load(&path) else {
        panic!("expected an error");
    };
    let msg = err.message();
    assert!(msg.contains("bad.toml"), "{msg}");
    assert!(msg.contains("page_size"), "{msg}");
}

#[test]
fn config_load_directory_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Config::load(dir.path()).is_err());
}
