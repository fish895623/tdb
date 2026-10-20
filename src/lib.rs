//! tdb: an embedded, file-based database engine.
//!
//! Safe Rust only (`unsafe` is forbidden). Malformed files and bad input come
//! back as [`Error`] values; nothing in the engine panics on untrusted bytes.

pub mod config;
pub mod db;
pub mod error;
pub mod kv_lexer;
pub mod record;
pub mod table;
pub mod types;

pub use config::Config;
pub use db::Database;
pub use error::Error;
pub use record::Record;
pub use table::Table;
pub use types::{Field, FieldType};
