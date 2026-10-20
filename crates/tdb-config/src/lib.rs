//! Settings, as read from `config.toml`.
//!
//! Every key is optional. Unknown keys are rejected, so a typo is an error
//! instead of a silently ignored setting.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The file the programs look for in the current directory.
pub const DEFAULT_CONFIG_FILE: &str = "config.toml";

pub const MIN_PAGE_SIZE: u32 = 4096;
pub const MAX_PAGE_SIZE: u32 = 65536;

/// Why settings could not be loaded: unreadable file, malformed TOML, an
/// unknown key, or an invalid value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> ConfigError {
        ConfigError {
            message: message.into(),
        }
    }

    /// What went wrong, without the "invalid configuration" prefix.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid configuration: {}", self.message)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub database: DatabaseConfig,
    pub log: LogConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
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

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    /// Verbosity. The `RUST_LOG` environment variable overrides it.
    pub level: LogLevel,
}

/// Page-store tunables from `docs/dd-001-large-index.md`. They are validated
/// now so config files written today stay valid, and take effect once the
/// page store lands.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// Parse and validate settings from TOML text.
    pub fn from_toml_str(text: &str) -> Result<Config, ConfigError> {
        Config::parse(text).map_err(ConfigError::new)
    }

    /// Read, parse and validate the settings file at `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::new(format!("cannot read {}: {e}", path.display())))?;
        Config::parse(&text).map_err(|e| ConfigError::new(format!("{}: {e}", path.display())))
    }

    /// Check every value. [`Config::from_toml_str`] calls this; call it yourself
    /// after building a `Config` by hand.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.check().map_err(ConfigError::new)
    }

    fn parse(text: &str) -> Result<Config, String> {
        let config: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        config.check()?;
        Ok(config)
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
}
