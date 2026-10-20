use std::fmt;
use std::io;

/// Everything the engine can fail with.
#[derive(Debug)]
pub enum Error {
    /// The operating system refused a read or write.
    Io(io::Error),
    /// The database file is malformed or truncated.
    Corrupt(String),
    /// The file's format version is not one this build can read.
    UnsupportedVersion(u32),
    /// A record was built with the wrong number of values for its schema.
    Arity { expected: usize, got: usize },
    /// A record's byte length does not match its table's schema.
    RecordSize { expected: usize, got: usize },
    /// The settings are missing, unreadable, or have an invalid value.
    Config(String),
    /// A table with this name already exists.
    TableExists(String),
    /// A length does not fit the on-disk field that stores it.
    TooLong {
        what: &'static str,
        len: usize,
        max: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "i/o error: {e}"),
            Error::Corrupt(msg) => write!(f, "corrupt database file: {msg}"),
            Error::UnsupportedVersion(v) => {
                write!(f, "unsupported tdb format version: {v}")
            }
            Error::Arity { expected, got } => write!(
                f,
                "value count does not match schema (want {expected}, got {got})"
            ),
            Error::RecordSize { expected, got } => {
                write!(f, "record is {got} bytes but the schema needs {expected}")
            }
            Error::Config(msg) => write!(f, "invalid configuration: {msg}"),
            Error::TableExists(name) => write!(f, "table '{name}' already exists"),
            Error::TooLong { what, len, max } => {
                write!(f, "{what} is too long ({len} > {max})")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
