use std::fmt;
use std::str::FromStr;

use tdb_lexer::Model;

/// The longest database name, in bytes.
pub const MAX_DATABASE_LEN: usize = 64;

/// How much of a bad model name an error repeats, so a hostile peer cannot
/// make the reply as big as its request.
const ECHO_LEN: usize = 32;

/// Which model a connection talks to, and which database inside it
/// (`/<model>/<database>`, DD-004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    model: Model,
    database: String,
}

/// Why a route is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// The path does not start with `/`.
    MissingSlash,
    /// There is a model but no database segment.
    MissingDatabase,
    /// There are more than two segments.
    ExtraSegments,
    /// The first segment is not `keyval`, `rdbms`, `vector` or `graph`. Holds
    /// at most the first 32 characters of it.
    UnknownModel(String),
    /// The database name is empty.
    EmptyDatabase,
    /// The database name is longer than [`MAX_DATABASE_LEN`] bytes.
    DatabaseTooLong(usize),
    /// The database name has a character outside `[A-Za-z0-9_-]`.
    BadDatabaseChar(char),
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteError::MissingSlash => f.write_str("the path must start with `/`"),
            RouteError::MissingDatabase => {
                f.write_str("the path needs a database: /<model>/<database>")
            }
            RouteError::ExtraSegments => {
                f.write_str("the path has more than two segments: /<model>/<database>")
            }
            RouteError::UnknownModel(name) => write!(
                f,
                "unknown model {name:?} (expected keyval, rdbms, vector or graph)"
            ),
            RouteError::EmptyDatabase => f.write_str("the database name is empty"),
            RouteError::DatabaseTooLong(len) => write!(
                f,
                "the database name is {len} bytes; the limit is {MAX_DATABASE_LEN}"
            ),
            RouteError::BadDatabaseChar(c) => write!(
                f,
                "{c:?} is not allowed in a database name (use letters, digits, `_` and `-`)"
            ),
        }
    }
}

impl std::error::Error for RouteError {}

impl Route {
    /// A route to `database` in `model`. The name must be 1 to
    /// [`MAX_DATABASE_LEN`] bytes of `[A-Za-z0-9_-]`, so it can never reach
    /// outside the data directory, whatever it is later used for.
    pub fn new(model: Model, database: impl Into<String>) -> Result<Route, RouteError> {
        let database = database.into();
        if database.is_empty() {
            return Err(RouteError::EmptyDatabase);
        }
        if database.len() > MAX_DATABASE_LEN {
            return Err(RouteError::DatabaseTooLong(database.len()));
        }
        if let Some(c) = database
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-'))
        {
            return Err(RouteError::BadDatabaseChar(c));
        }
        Ok(Route { model, database })
    }

    pub fn model(&self) -> Model {
        self.model
    }

    pub fn database(&self) -> &str {
        &self.database
    }
}

/// The path form: `/graph/social`.
impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}/{}", model_name(self.model), self.database)
    }
}

/// Parses the path form, `/<model>/<database>`: exactly two non-empty
/// segments. A query string or anything else that is not a database-name
/// character is an error, not ignored.
impl FromStr for Route {
    type Err = RouteError;

    fn from_str(path: &str) -> Result<Route, RouteError> {
        let rest = path.strip_prefix('/').ok_or(RouteError::MissingSlash)?;
        // At most three pieces, so a path of a million slashes costs nothing.
        let mut segments = rest.splitn(3, '/');
        let model = segments.next().unwrap_or_default();
        let database = segments.next().ok_or(RouteError::MissingDatabase)?;
        if segments.next().is_some() {
            return Err(RouteError::ExtraSegments);
        }
        let model = parse_model(model)?;
        Route::new(model, database)
    }
}

/// The name of `model` in a route.
pub fn model_name(model: Model) -> &'static str {
    match model {
        Model::Keyval => "keyval",
        Model::Rdbms => "rdbms",
        Model::Vector => "vector",
        Model::Graph => "graph",
    }
}

fn parse_model(name: &str) -> Result<Model, RouteError> {
    match name {
        "keyval" => Ok(Model::Keyval),
        "rdbms" => Ok(Model::Rdbms),
        "vector" => Ok(Model::Vector),
        "graph" => Ok(Model::Graph),
        other => Err(RouteError::UnknownModel(
            other.chars().take(ECHO_LEN).collect(),
        )),
    }
}
