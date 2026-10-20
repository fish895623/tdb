use std::fmt;

/// The data model a keyword belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    /// Redis-style commands: `SET`, `HGET`, `ZADD`, ... (route `/keyval/..`)
    Keyval,
    /// Relational statements: `SELECT`, `INSERT`, `CREATE TABLE`, ... (route
    /// `/rdbms/..`)
    Rdbms,
    /// Vector search: the `VECTOR` type; distance operators are punctuation
    /// (route `/vector/..`)
    Vector,
    /// Cypher-style graph queries: `MATCH`, `RETURN`, ... (route `/graph/..`)
    Graph,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Model::Keyval => "Keyval",
            Model::Rdbms => "Rdbms",
            Model::Vector => "Vector",
            Model::Graph => "Graph",
        })
    }
}

// One row per keyword: variant, canonical spelling, home model. `None` means
// the word is shared by more than one model (`CREATE`, `WHERE`, `AND`, ...).
// This table is the only place a spelling is written down.
macro_rules! keywords {
    ($($variant:ident = $text:literal, $home:expr;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Keyword {
            $($variant,)*
        }

        impl Keyword {
            /// Every keyword, in declaration order.
            pub const ALL: &'static [Keyword] = &[$(Keyword::$variant,)*];

            /// The canonical (upper-case) spelling.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Keyword::$variant => $text,)*
                }
            }

            /// The model this keyword belongs to, or `None` if it is shared.
            pub const fn model(self) -> Option<Model> {
                match self {
                    $(Keyword::$variant => $home,)*
                }
            }
        }
    };
}

keywords! {
    // --- key-value (Redis-style) commands ---
    // strings
    Get = "GET", Some(Model::Keyval);
    Del = "DEL", Some(Model::Keyval);
    Exists = "EXISTS", Some(Model::Keyval);
    Incr = "INCR", Some(Model::Keyval);
    Decr = "DECR", Some(Model::Keyval);
    IncrBy = "INCRBY", Some(Model::Keyval);
    DecrBy = "DECRBY", Some(Model::Keyval);
    Append = "APPEND", Some(Model::Keyval);
    StrLen = "STRLEN", Some(Model::Keyval);
    MGet = "MGET", Some(Model::Keyval);
    MSet = "MSET", Some(Model::Keyval);
    // hashes
    HSet = "HSET", Some(Model::Keyval);
    HGet = "HGET", Some(Model::Keyval);
    HDel = "HDEL", Some(Model::Keyval);
    HExists = "HEXISTS", Some(Model::Keyval);
    HGetAll = "HGETALL", Some(Model::Keyval);
    HKeys = "HKEYS", Some(Model::Keyval);
    HVals = "HVALS", Some(Model::Keyval);
    HLen = "HLEN", Some(Model::Keyval);
    // sets
    SAdd = "SADD", Some(Model::Keyval);
    SRem = "SREM", Some(Model::Keyval);
    SIsMember = "SISMEMBER", Some(Model::Keyval);
    SMembers = "SMEMBERS", Some(Model::Keyval);
    SCard = "SCARD", Some(Model::Keyval);
    // lists
    LPush = "LPUSH", Some(Model::Keyval);
    RPush = "RPUSH", Some(Model::Keyval);
    LPop = "LPOP", Some(Model::Keyval);
    RPop = "RPOP", Some(Model::Keyval);
    LLen = "LLEN", Some(Model::Keyval);
    LRange = "LRANGE", Some(Model::Keyval);
    // sorted sets
    ZAdd = "ZADD", Some(Model::Keyval);
    ZRem = "ZREM", Some(Model::Keyval);
    ZScore = "ZSCORE", Some(Model::Keyval);
    ZRange = "ZRANGE", Some(Model::Keyval);
    ZRangeByScore = "ZRANGEBYSCORE", Some(Model::Keyval);
    ZCard = "ZCARD", Some(Model::Keyval);
    // expiry and transactions
    Expire = "EXPIRE", Some(Model::Keyval);
    Ttl = "TTL", Some(Model::Keyval);
    Persist = "PERSIST", Some(Model::Keyval);
    Multi = "MULTI", Some(Model::Keyval);
    Exec = "EXEC", Some(Model::Keyval);
    Discard = "DISCARD", Some(Model::Keyval);

    // --- shared across models ---
    // `SET` is a Redis command and also appears in `UPDATE .. SET` and
    // Cypher's `SET`, so it has no single home.
    Set = "SET", None;
    Create = "CREATE", None;
    Delete = "DELETE", None;
    Where = "WHERE", None;
    Order = "ORDER", None;
    By = "BY", None;
    Limit = "LIMIT", None;
    As = "AS", None;
    And = "AND", None;
    Or = "OR", None;
    Not = "NOT", None;
    In = "IN", None;
    Is = "IS", None;
    Null = "NULL", None;
    True = "TRUE", None;
    False = "FALSE", None;
    Asc = "ASC", None;
    Desc = "DESC", None;

    // --- relational / ORM ---
    Table = "TABLE", Some(Model::Rdbms);
    Drop = "DROP", Some(Model::Rdbms);
    Insert = "INSERT", Some(Model::Rdbms);
    Into = "INTO", Some(Model::Rdbms);
    Values = "VALUES", Some(Model::Rdbms);
    Select = "SELECT", Some(Model::Rdbms);
    From = "FROM", Some(Model::Rdbms);
    Update = "UPDATE", Some(Model::Rdbms);
    Offset = "OFFSET", Some(Model::Rdbms);
    Primary = "PRIMARY", Some(Model::Rdbms);
    Key = "KEY", Some(Model::Rdbms);
    Index = "INDEX", Some(Model::Rdbms);
    On = "ON", Some(Model::Rdbms);
    // column types, matching `FieldType` plus the variable-length types
    Bool = "BOOL", Some(Model::Rdbms);
    Int32 = "INT32", Some(Model::Rdbms);
    Int64 = "INT64", Some(Model::Rdbms);
    Float32 = "FLOAT32", Some(Model::Rdbms);
    Float64 = "FLOAT64", Some(Model::Rdbms);
    Text = "TEXT", Some(Model::Rdbms);
    Bytes = "BYTES", Some(Model::Rdbms);

    // --- vector ---
    Vector = "VECTOR", Some(Model::Vector);

    // --- graph (Cypher-style) ---
    Match = "MATCH", Some(Model::Graph);
    Return = "RETURN", Some(Model::Graph);
    Detach = "DETACH", Some(Model::Graph);
    Merge = "MERGE", Some(Model::Graph);
    Skip = "SKIP", Some(Model::Graph);
}

impl Keyword {
    /// The keyword spelled `word`, ignoring ASCII case.
    pub fn from_word(word: &str) -> Option<Keyword> {
        Keyword::ALL
            .iter()
            .copied()
            .find(|k| k.as_str().eq_ignore_ascii_case(word))
    }

    /// Whether `model`'s dialect treats this word as a keyword. Shared words
    /// belong to every dialect but `Keyval`; Redis commands belong only to
    /// `Keyval`. The vector dialect is SQL-like for now (relational words plus
    /// `VECTOR`), pending DD-004's open question on its language.
    pub fn in_model(self, model: Model) -> bool {
        match model {
            Model::Keyval => self.is_kv_command(),
            Model::Rdbms | Model::Vector => {
                matches!(self.model(), None | Some(Model::Rdbms | Model::Vector))
            }
            Model::Graph => matches!(self.model(), None | Some(Model::Graph)),
        }
    }

    /// Whether a statement that starts with this keyword is a Redis-style
    /// command, whose arguments are lexed as bare words up to the end of the
    /// line. `SET` counts: at the start of a statement it is the Redis command.
    pub fn is_kv_command(self) -> bool {
        self == Keyword::Set || self.model() == Some(Model::Keyval)
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
