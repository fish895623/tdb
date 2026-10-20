//! The `Keyword` / `Model` API, written against `docs/dd-003-query-lexer.md`.
//! Uses only the public API.

use std::collections::{BTreeSet, HashSet};

use tdb_lexer::{Keyword, Model, TokenType};

// The spec's keyword lists, transcribed from the "Keywords" and "KV mode"
// sections.

const SHARED: &[&str] = &[
    "SET", "CREATE", "DELETE", "WHERE", "ORDER", "BY", "LIMIT", "AS", "AND", "OR", "NOT", "IN",
    "IS", "NULL", "TRUE", "FALSE", "ASC", "DESC",
];

const TABLE: &[&str] = &[
    "TABLE", "DROP", "INSERT", "INTO", "VALUES", "SELECT", "FROM", "UPDATE", "OFFSET", "PRIMARY",
    "KEY", "INDEX", "ON", // column types
    "BOOL", "INT32", "INT64", "FLOAT32", "FLOAT64", "TEXT", "BYTES",
];

const VECTOR: &[&str] = &["VECTOR"];

const GRAPH: &[&str] = &["MATCH", "RETURN", "DETACH", "MERGE", "SKIP"];

/// The KV command list as the spec prints it, `SET` included.
const KV_COMMANDS: &[&str] = &[
    "SET",
    "GET",
    "DEL",
    "EXISTS",
    "INCR",
    "DECR",
    "INCRBY",
    "DECRBY",
    "APPEND",
    "STRLEN",
    "MGET",
    "MSET",
    "HSET",
    "HGET",
    "HDEL",
    "HEXISTS",
    "HGETALL",
    "HKEYS",
    "HVALS",
    "HLEN",
    "SADD",
    "SREM",
    "SISMEMBER",
    "SMEMBERS",
    "SCARD",
    "LPUSH",
    "RPUSH",
    "LPOP",
    "RPOP",
    "LLEN",
    "LRANGE",
    "ZADD",
    "ZREM",
    "ZSCORE",
    "ZRANGE",
    "ZRANGEBYSCORE",
    "ZCARD",
    "EXPIRE",
    "TTL",
    "PERSIST",
    "MULTI",
    "EXEC",
    "DISCARD",
];

/// The `Kv` group: the KV commands "(except `SET`)".
fn kv_model_words() -> Vec<&'static str> {
    KV_COMMANDS
        .iter()
        .copied()
        .filter(|w| *w != "SET")
        .collect()
}

/// Every keyword the spec lists, with the home model the spec gives it.
fn spec_keywords() -> Vec<(&'static str, Option<Model>)> {
    let mut all = Vec::new();
    all.extend(SHARED.iter().map(|w| (*w, None)));
    all.extend(TABLE.iter().map(|w| (*w, Some(Model::Rdbms))));
    all.extend(VECTOR.iter().map(|w| (*w, Some(Model::Vector))));
    all.extend(GRAPH.iter().map(|w| (*w, Some(Model::Graph))));
    all.extend(
        kv_model_words()
            .into_iter()
            .map(|w| (w, Some(Model::Keyval))),
    );
    all
}

fn kw(word: &str) -> Keyword {
    Keyword::from_word(word).unwrap_or_else(|| panic!("{word:?} should be a keyword"))
}

#[test]
fn keyword_all_has_unique_canonical_spellings() {
    let spellings: HashSet<&str> = Keyword::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        spellings.len(),
        Keyword::ALL.len(),
        "two keywords share a canonical spelling"
    );
    let variants: HashSet<Keyword> = Keyword::ALL.iter().copied().collect();
    assert_eq!(
        variants.len(),
        Keyword::ALL.len(),
        "a keyword appears twice in Keyword::ALL"
    );
}

#[test]
fn keyword_canonical_spellings_are_upper_case_ascii() {
    for k in Keyword::ALL {
        let s = k.as_str();
        assert!(!s.is_empty(), "empty spelling for {k:?}");
        assert!(
            s.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{s:?} is not upper-case ASCII letters and digits"
        );
        assert!(
            s.starts_with(|c: char| c.is_ascii_uppercase()),
            "{s:?} should start with a letter"
        );
    }
}

#[test]
fn keyword_from_word_round_trips_every_canonical_spelling() {
    for &k in Keyword::ALL {
        assert_eq!(Keyword::from_word(k.as_str()), Some(k), "{}", k.as_str());
    }
}

#[test]
fn keyword_from_word_is_case_insensitive() {
    let select = kw("SELECT");
    assert_eq!(Keyword::from_word("select"), Some(select));
    assert_eq!(Keyword::from_word("SeLeCt"), Some(select));
    assert_eq!(Keyword::from_word("Select"), Some(select));
    for &k in Keyword::ALL {
        let lower = k.as_str().to_ascii_lowercase();
        assert_eq!(Keyword::from_word(&lower), Some(k), "{lower:?}");
    }
}

#[test]
fn keyword_from_word_rejects_non_keywords_and_near_misses() {
    for word in [
        "", "selects", "sel", "selec", "xselect", "foo", "user", "SELECT1", "SELECT_", " SELECT",
        "SELECT ", "SELECT;", "SEL ECT", "GE", "GETT", "INT", "INT16", "FLOAT", "STRING",
    ] {
        assert_eq!(Keyword::from_word(word), None, "{word:?}");
    }
}

#[test]
fn keyword_model_of_representative_keywords() {
    let cases = [
        ("SELECT", Model::Rdbms),
        ("INSERT", Model::Rdbms),
        ("TABLE", Model::Rdbms),
        ("PRIMARY", Model::Rdbms),
        ("INT32", Model::Rdbms),
        ("TEXT", Model::Rdbms),
        ("BYTES", Model::Rdbms),
        ("VECTOR", Model::Vector),
        ("MATCH", Model::Graph),
        ("RETURN", Model::Graph),
        ("MERGE", Model::Graph),
        ("GET", Model::Keyval),
        ("HSET", Model::Keyval),
        ("ZADD", Model::Keyval),
        ("MULTI", Model::Keyval),
    ];
    for (word, model) in cases {
        assert_eq!(kw(word).model(), Some(model), "{word}");
    }
}

#[test]
fn keyword_model_is_none_for_every_shared_keyword() {
    for word in SHARED {
        assert_eq!(kw(word).model(), None, "{word} is shared");
    }
}

#[test]
fn keyword_model_matches_the_spec_group_for_every_keyword() {
    for (word, model) in spec_keywords() {
        assert_eq!(kw(word).model(), model, "home model of {word}");
    }
}

#[test]
fn keyword_all_matches_spec_lists_in_count_and_membership() {
    let spec = spec_keywords();
    let spec_words: BTreeSet<&str> = spec.iter().map(|(w, _)| *w).collect();
    assert_eq!(
        spec_words.len(),
        spec.len(),
        "the spec's lists overlap or repeat a word"
    );

    let actual: BTreeSet<&str> = Keyword::ALL.iter().map(|k| k.as_str()).collect();
    let missing: Vec<&&str> = spec_words.difference(&actual).collect();
    let extra: Vec<&&str> = actual.difference(&spec_words).collect();
    assert!(
        missing.is_empty(),
        "in the spec but not in ALL: {missing:?}"
    );
    assert!(extra.is_empty(), "in ALL but not in the spec: {extra:?}");

    // Count: 18 shared (SET included) + 20 table + 1 vector + 5 graph + 42 KV
    // commands other than SET.
    assert_eq!(kv_model_words().len(), 42);
    assert_eq!(Keyword::ALL.len(), spec.len());
    assert_eq!(Keyword::ALL.len(), 86);
}

#[test]
fn keyword_is_kv_command_is_true_for_exactly_the_spec_commands_plus_set() {
    for &k in Keyword::ALL {
        let expected = KV_COMMANDS.contains(&k.as_str());
        assert_eq!(
            k.is_kv_command(),
            expected,
            "is_kv_command of {}",
            k.as_str()
        );
    }
    let count = Keyword::ALL.iter().filter(|k| k.is_kv_command()).count();
    assert_eq!(count, KV_COMMANDS.len());
}

#[test]
fn keyword_display_is_the_canonical_spelling() {
    assert_eq!(kw("select").to_string(), "SELECT");
    assert_eq!(kw("HGetAll").to_string(), "HGETALL");
    for &k in Keyword::ALL {
        assert_eq!(k.to_string(), k.as_str());
    }
}

#[test]
fn keyword_model_display_is_the_variant_name() {
    assert_eq!(Model::Keyval.to_string(), "Keyval");
    assert_eq!(Model::Rdbms.to_string(), "Rdbms");
    assert_eq!(Model::Vector.to_string(), "Vector");
    assert_eq!(Model::Graph.to_string(), "Graph");
}

#[test]
fn keyword_token_type_display_is_the_variant_name_with_the_keyword_inside() {
    assert_eq!(
        TokenType::Keyword(kw("select")).to_string(),
        "Keyword(SELECT)"
    );
    for &k in Keyword::ALL {
        assert_eq!(
            TokenType::Keyword(k).to_string(),
            format!("Keyword({})", k.as_str())
        );
    }
    let cases = [
        (TokenType::Identifier, "Identifier"),
        (TokenType::Word, "Word"),
        (TokenType::Integer, "Integer"),
        (TokenType::Float, "Float"),
        (TokenType::String, "String"),
        (TokenType::Parameter, "Parameter"),
        (TokenType::Arrow, "Arrow"),
        (TokenType::L2Distance, "L2Distance"),
        (TokenType::Newline, "Newline"),
        (TokenType::EndOfFile, "EndOfFile"),
    ];
    for (kind, text) in cases {
        assert_eq!(kind.to_string(), text);
    }
}
