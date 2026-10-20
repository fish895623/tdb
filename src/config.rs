//! Settings, as read from `config.toml`.
//!
//! The types and defaults are always available. Reading TOML needs the
//! `config` feature. Every key is optional; unknown keys are rejected so a
//! typo is an error instead of a silently ignored setting.

use std::path::PathBuf;

use crate::error::Error;

/// The file the programs look for in the current directory.
pub const DEFAULT_CONFIG_FILE: &str = "config.toml";

pub const MIN_PAGE_SIZE: u32 = 4096;
pub const MAX_PAGE_SIZE: u32 = 65536;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "config",
    derive(serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct Config {
    pub database: DatabaseConfig,
    pub log: LogConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "config",
    derive(serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct DatabaseConfig {
    /// The database file.
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        DatabaseConfig {
            path: PathBuf::from("tdatabase.db"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "config",
    derive(serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct LogConfig {
    /// Verbosity. The `RUST_LOG` environment variable overrides it.
    pub level: LogLevel,
}

/// Page-store tunables from `docs/dd-001-large-index.md`. They are validated
/// now so config files written today stay valid, and take effect once the
/// page store lands.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "config",
    derive(serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct StorageConfig {
    /// Page size in bytes: a power of two from 4096 to 65536.
    pub page_size: u32,
    /// RAM for the buffer pool, in MiB.
    pub buffer_pool_mb: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig {
            page_size: 16384,
            buffer_pool_mb: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "config",
    derive(serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    #[default]
    Trace,
}

impl LogLevel {
    pub fn filter(self) -> log::LevelFilter {
        match self {
            LogLevel::Off => log::LevelFilter::Off,
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

impl Config {
    /// Check every value. [`Config::from_toml_str`] calls this; call it yourself
    /// after building a `Config` by hand.
    pub fn validate(&self) -> Result<(), Error> {
        self.check().map_err(Error::Config)
    }

    fn check(&self) -> Result<(), String> {
        let page_size = self.storage.page_size;
        if !page_size.is_power_of_two() || !(MIN_PAGE_SIZE..=MAX_PAGE_SIZE).contains(&page_size) {
            return Err(format!(
                "storage.page_size must be a power of two from {MIN_PAGE_SIZE} to \
                 {MAX_PAGE_SIZE}, got {page_size}"
            ));
        }
        if self.storage.buffer_pool_mb == 0 {
            return Err("storage.buffer_pool_mb must be at least 1".into());
        }
        if self.database.path.as_os_str().is_empty() {
            return Err("database.path must not be empty".into());
        }
        Ok(())
    }

    /// Parse and validate settings from TOML text.
    #[cfg(feature = "config")]
    pub fn from_toml_str(text: &str) -> Result<Config, Error> {
        Config::parse(text).map_err(Error::Config)
    }

    /// Read, parse and validate the settings file at `path`.
    #[cfg(feature = "config")]
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Config, Error> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("cannot read {}: {e}", path.display())))?;
        Config::parse(&text).map_err(|e| Error::Config(format!("{}: {e}", path.display())))
    }

    #[cfg(feature = "config")]
    fn parse(text: &str) -> Result<Config, String> {
        let config: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        config.check()?;
        Ok(config)
    }
}
