//! `Lexer::for_model`: the model is fixed up front (as a router would, from
//! `/keyval/..`, `/rdbms/..`, `/vector/..`, `/graph/..`), so there is no
//! per-statement detection and each model reserves only its own keywords.

use tdb_lexer::{Keyword, LexError, Lexer, Model, Token, TokenType};

const ALL_MODELS: [Model; 4] = [Model::Keyval, Model::Rdbms, Model::Vector, Model::Graph];

fn lex(model: Model, input: &str) -> Result<Vec<Token>, LexError> {
    Lexer::for_model(input, model).tokenize()
}

/// (kind, value) pairs without the trailing EndOfFile.
fn pairs(model: Model, input: &str) -> Vec<(TokenType, String)> {
    let mut tokens = lex(model, input).unwrap();
    assert_eq!(tokens.pop().unwrap().kind, TokenType::EndOfFile);
    tokens.into_iter().map(|t| (t.kind, t.value)).collect()
}

fn kinds(model: Model, input: &str) -> Vec<TokenType> {
    pairs(model, input).into_iter().map(|(k, _)| k).collect()
}

fn kw(k: Keyword) -> TokenType {
    TokenType::Keyword(k)
}

fn err(model: Model, input: &str) -> LexError {
    lex(model, input).unwrap_err()
}

// --- Keyval: every statement is Redis-style ---

#[test]
fn models_keyval_lexes_every_line_as_a_command() {
    assert_eq!(
        kinds(Model::Keyval, "GET user:1\nSET a b\n"),
        [
            kw(Keyword::Get),
            TokenType::Word,
            TokenType::Newline,
            kw(Keyword::Set),
            TokenType::Word,
            TokenType::Word,
            TokenType::Newline,
        ]
    );
}

#[test]
fn models_keyval_needs_no_detection_for_a_non_command_first_word() {
    // Auto would lex this as SQL; the keyval route never does.
    assert_eq!(
        kinds(Model::Keyval, "SELECT * FROM t"),
        [TokenType::Word; 4]
    );
}

#[test]
fn models_keyval_a_command_glued_to_punctuation_is_one_word() {
    // `GET(x)` is one bare word here, not the command GET followed by `(x)`.
    assert_eq!(kinds(Model::Keyval, "GET(x)"), [TokenType::Word]);
}

#[test]
fn models_keyval_commands_are_case_insensitive_and_keep_their_spelling() {
    let tokens = pairs(Model::Keyval, "hSeT k v");
    assert_eq!(tokens[0], (kw(Keyword::HSet), "hSeT".to_string()));
}

#[test]
fn models_keyval_skips_blank_and_whitespace_only_lines() {
    assert_eq!(
        kinds(Model::Keyval, "\n  \t\nGET k\n\n\nGET j\n"),
        [
            kw(Keyword::Get),
            TokenType::Word,
            TokenType::Newline,
            kw(Keyword::Get),
            TokenType::Word,
            TokenType::Newline,
        ]
    );
}

#[test]
fn models_keyval_has_no_comments_and_semicolons_are_word_characters() {
    assert_eq!(
        pairs(Model::Keyval, "// note\nGET k;"),
        [
            (TokenType::Word, "//".to_string()),
            (TokenType::Word, "note".to_string()),
            (TokenType::Newline, "\n".to_string()),
            (kw(Keyword::Get), "GET".to_string()),
            (TokenType::Word, "k;".to_string()),
        ]
    );
}

#[test]
fn models_keyval_classifies_numbers_after_the_command() {
    assert_eq!(
        kinds(Model::Keyval, "ZADD z -1.5 m 3 1e3 -inf"),
        [
            kw(Keyword::ZAdd),
            TokenType::Word,
            TokenType::Float,
            TokenType::Word,
            TokenType::Integer,
            TokenType::Float,
            TokenType::Word,
        ]
    );
}

#[test]
fn models_keyval_quoted_arguments_and_the_whitespace_rule() {
    assert_eq!(
        pairs(Model::Keyval, "SET k \"a b\""),
        [
            (kw(Keyword::Set), "SET".to_string()),
            (TokenType::Word, "k".to_string()),
            (TokenType::String, "a b".to_string()),
        ]
    );
    let e = err(Model::Keyval, "SET k \"a\"b");
    assert_eq!(e.message, "expected whitespace after a quoted argument");
    assert_eq!((e.line, e.column), (1, 10));
}

#[test]
fn models_keyval_last_command_without_newline_has_no_newline_token() {
    assert!(!kinds(Model::Keyval, "GET k").contains(&TokenType::Newline));
}

#[test]
fn models_keyval_keywords_are_only_the_redis_commands() {
    // SELECT is not a Keyval keyword even where a word is reserved elsewhere.
    for k in Keyword::ALL {
        assert_eq!(k.in_model(Model::Keyval), k.is_kv_command(), "{k}");
    }
}

// --- Rdbms ---

#[test]
fn models_rdbms_graph_words_are_ordinary_names() {
    assert_eq!(
        kinds(Model::Rdbms, "SELECT match, return FROM skip"),
        [
            kw(Keyword::Select),
            TokenType::Identifier,
            TokenType::Comma,
            TokenType::Identifier,
            kw(Keyword::From),
            TokenType::Identifier,
        ]
    );
}

#[test]
fn models_rdbms_redis_commands_are_ordinary_names() {
    assert_eq!(
        kinds(Model::Rdbms, "SELECT get, hset, zadd FROM t"),
        [
            kw(Keyword::Select),
            TokenType::Identifier,
            TokenType::Comma,
            TokenType::Identifier,
            TokenType::Comma,
            TokenType::Identifier,
            kw(Keyword::From),
            TokenType::Identifier,
        ]
    );
}

#[test]
fn models_rdbms_keeps_relational_vector_and_shared_keywords() {
    assert_eq!(
        kinds(
            Model::Rdbms,
            "CREATE TABLE t (id INT64 PRIMARY KEY, v VECTOR(3))"
        )[..6],
        [
            kw(Keyword::Create),
            kw(Keyword::Table),
            TokenType::Identifier,
            TokenType::LParen,
            TokenType::Identifier,
            kw(Keyword::Int64),
        ]
    );
    assert!(kinds(Model::Rdbms, "v VECTOR(3)").contains(&kw(Keyword::Vector)));
}

#[test]
fn models_rdbms_does_not_switch_to_keyval_mode_for_set() {
    // Auto would lex `SET user:1 "x"` as a Redis command; here `:` is Colon.
    assert_eq!(
        kinds(Model::Rdbms, "SET user:1 \"x\""),
        [
            kw(Keyword::Set),
            TokenType::Identifier,
            TokenType::Colon,
            TokenType::Integer,
            TokenType::String,
        ]
    );
}

#[test]
fn models_rdbms_runs_through_semicolons() {
    assert_eq!(
        kinds(Model::Rdbms, "SELECT 1; SELECT 2;"),
        [
            kw(Keyword::Select),
            TokenType::Integer,
            TokenType::Semicolon,
            kw(Keyword::Select),
            TokenType::Integer,
            TokenType::Semicolon,
        ]
    );
}

#[test]
fn models_rdbms_skips_comments_before_the_first_token() {
    assert_eq!(
        kinds(Model::Rdbms, "// c\n/* d */ SELECT 1"),
        [kw(Keyword::Select), TokenType::Integer]
    );
}

#[test]
fn models_rdbms_backticks_still_quote_a_keyword() {
    assert_eq!(
        pairs(Model::Rdbms, "SELECT `select` FROM t")[1],
        (TokenType::Identifier, "select".to_string())
    );
}

// --- Vector (SQL-like for now) ---

#[test]
fn models_vector_lexes_distance_search_statements() {
    assert_eq!(
        kinds(
            Model::Vector,
            "SELECT id FROM docs ORDER BY embedding <-> [0.1, 2] LIMIT 3;"
        ),
        [
            kw(Keyword::Select),
            TokenType::Identifier,
            kw(Keyword::From),
            TokenType::Identifier,
            kw(Keyword::Order),
            kw(Keyword::By),
            TokenType::Identifier,
            TokenType::L2Distance,
            TokenType::LBracket,
            TokenType::Float,
            TokenType::Comma,
            TokenType::Integer,
            TokenType::RBracket,
            kw(Keyword::Limit),
            TokenType::Integer,
            TokenType::Semicolon,
        ]
    );
}

#[test]
fn models_vector_reserves_the_vector_type_but_not_graph_or_redis_words() {
    let k = kinds(Model::Vector, "VECTOR match get");
    assert_eq!(
        k,
        [
            kw(Keyword::Vector),
            TokenType::Identifier,
            TokenType::Identifier
        ]
    );
}

// --- Graph ---

#[test]
fn models_graph_relational_words_are_ordinary_names() {
    assert_eq!(
        kinds(Model::Graph, "MATCH (n) WHERE n.table = n.from RETURN n"),
        [
            kw(Keyword::Match),
            TokenType::LParen,
            TokenType::Identifier,
            TokenType::RParen,
            kw(Keyword::Where),
            TokenType::Identifier,
            TokenType::Dot,
            TokenType::Identifier,
            TokenType::Eq,
            TokenType::Identifier,
            TokenType::Dot,
            TokenType::Identifier,
            kw(Keyword::Return),
            TokenType::Identifier,
        ]
    );
}

#[test]
fn models_graph_select_and_vector_are_ordinary_names() {
    assert_eq!(
        kinds(Model::Graph, "select vector text"),
        [TokenType::Identifier; 3]
    );
}

#[test]
fn models_graph_keeps_graph_and_shared_keywords() {
    assert_eq!(
        kinds(Model::Graph, "MATCH (n) DETACH DELETE n"),
        [
            kw(Keyword::Match),
            TokenType::LParen,
            TokenType::Identifier,
            TokenType::RParen,
            kw(Keyword::Detach),
            kw(Keyword::Delete),
            TokenType::Identifier,
        ]
    );
    assert_eq!(kinds(Model::Graph, "SET")[0], kw(Keyword::Set));
}

#[test]
fn models_graph_lexes_patterns_like_the_other_modes_do() {
    assert_eq!(
        kinds(Model::Graph, "(a:Person)-[:KNOWS*1..3]->(b)"),
        [
            TokenType::LParen,
            TokenType::Identifier,
            TokenType::Colon,
            TokenType::Identifier,
            TokenType::RParen,
            TokenType::Minus,
            TokenType::LBracket,
            TokenType::Colon,
            TokenType::Identifier,
            TokenType::Star,
            TokenType::Integer,
            TokenType::DotDot,
            TokenType::Integer,
            TokenType::RBracket,
            TokenType::Arrow,
            TokenType::LParen,
            TokenType::Identifier,
            TokenType::RParen,
        ]
    );
}

// --- Keyword::in_model ---

#[test]
fn models_shared_keywords_belong_to_every_model_but_keyval() {
    for k in Keyword::ALL.iter().filter(|k| k.model().is_none()) {
        for model in [Model::Rdbms, Model::Vector, Model::Graph] {
            assert!(k.in_model(model), "{k} in {model}");
        }
    }
}

#[test]
fn models_home_keywords_stay_in_their_own_dialect() {
    for k in Keyword::ALL {
        match k.model() {
            Some(Model::Keyval) => {
                for model in [Model::Rdbms, Model::Vector, Model::Graph] {
                    assert!(!k.in_model(model), "{k} in {model}");
                }
            }
            Some(Model::Rdbms) => {
                assert!(k.in_model(Model::Rdbms) && k.in_model(Model::Vector), "{k}");
                assert!(
                    !k.in_model(Model::Graph) && !k.in_model(Model::Keyval),
                    "{k}"
                );
            }
            Some(Model::Vector) => {
                assert!(k.in_model(Model::Rdbms) && k.in_model(Model::Vector), "{k}");
                assert!(
                    !k.in_model(Model::Graph) && !k.in_model(Model::Keyval),
                    "{k}"
                );
            }
            Some(Model::Graph) => {
                assert!(k.in_model(Model::Graph), "{k}");
                for model in [Model::Rdbms, Model::Vector, Model::Keyval] {
                    assert!(!k.in_model(model), "{k} in {model}");
                }
            }
            None => {}
        }
    }
}

#[test]
fn models_every_keyword_belongs_to_at_least_one_model() {
    for k in Keyword::ALL {
        assert!(ALL_MODELS.iter().any(|m| k.in_model(*m)), "{k}");
    }
}

#[test]
fn models_set_is_a_keyword_in_every_model() {
    for model in ALL_MODELS {
        assert!(Keyword::Set.in_model(model), "{model}");
    }
}

// --- behaviour that does not depend on the model ---

#[test]
fn models_errors_and_positions_are_the_same_in_every_model() {
    for model in ALL_MODELS {
        let e = err(model, "'open");
        assert_eq!(e.message, "unterminated string", "{model}");
        assert_eq!((e.line, e.column), (1, 1), "{model}");
    }
}

#[test]
fn models_end_of_file_is_positioned_after_the_last_character() {
    for model in ALL_MODELS {
        let tokens = lex(model, "ab\ncd").unwrap();
        let eof = tokens.last().unwrap();
        assert_eq!(eof.kind, TokenType::EndOfFile, "{model}");
        assert_eq!((eof.line, eof.column), (2, 3), "{model}");
    }
}

#[test]
fn models_empty_input_is_just_end_of_file_in_every_model() {
    for model in ALL_MODELS {
        assert_eq!(kinds(model, ""), [], "{model}");
        assert_eq!(kinds(model, "  \n "), [], "{model}");
    }
}

#[test]
fn models_new_still_detects_the_model_per_statement() {
    // The same text that Rdbms lexes as SQL tokens is a Redis command to `new`.
    let auto: Vec<TokenType> = Lexer::new("SET user:1 \"x\"")
        .tokenize()
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(auto[1], TokenType::Word);
    assert_eq!(
        kinds(Model::Rdbms, "SET user:1 \"x\"")[1],
        TokenType::Identifier
    );
}

#[test]
fn models_no_input_panics_in_any_model() {
    let samples = [
        "",
        "'",
        "\"",
        "`",
        "$",
        "/*",
        "!",
        "é",
        "\\",
        "1e",
        "12abc",
        "SET \"a\"b",
        "MATCH (a)-[:X*..]->(b) // c",
        "\u{0}\u{b}\u{2028}",
    ];
    for model in ALL_MODELS {
        for input in samples {
            let _ = lex(model, input);
            for end in 0..=input.len() {
                if input.is_char_boundary(end) {
                    let _ = lex(model, &input[..end]);
                }
            }
        }
    }
}
