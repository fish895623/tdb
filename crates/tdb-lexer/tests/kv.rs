//! KV (Redis-style) mode of the lexer, written against
//! `docs/dd-003-query-lexer.md`. Uses only the public API.

use tdb_lexer::{Keyword, LexError, Token, TokenType, tokenize};

/// Kind plus value, for streams where positions are not the point.
type Shape = (TokenType, String);

/// Every KV command in the spec, `SET` included.
const SPEC_KV_COMMANDS: &[&str] = &[
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

// ---------------------------------------------------------------- helpers

fn lex(input: &str) -> Vec<Token> {
    tokenize(input).unwrap_or_else(|e| panic!("lexing {input:?} failed: {e}"))
}

fn lex_err(input: &str) -> LexError {
    match tokenize(input) {
        Ok(tokens) => panic!("lexing {input:?} should fail, got {tokens:?}"),
        Err(e) => e,
    }
}

/// All tokens except the trailing `EndOfFile`, which is checked to be there.
fn body(input: &str) -> Vec<Token> {
    let mut tokens = lex(input);
    let last = tokens
        .pop()
        .unwrap_or_else(|| panic!("no tokens at all for {input:?}"));
    assert_eq!(last.kind, TokenType::EndOfFile, "last token of {input:?}");
    tokens
}

fn shapes(input: &str) -> Vec<Shape> {
    body(input).into_iter().map(|t| (t.kind, t.value)).collect()
}

fn kinds(input: &str) -> Vec<TokenType> {
    body(input).into_iter().map(|t| t.kind).collect()
}

fn spans(input: &str) -> Vec<(TokenType, usize, usize)> {
    body(input)
        .into_iter()
        .map(|t| (t.kind, t.line, t.column))
        .collect()
}

fn eof_position(input: &str) -> (usize, usize) {
    let tokens = lex(input);
    let last = tokens
        .last()
        .unwrap_or_else(|| panic!("no tokens at all for {input:?}"));
    assert_eq!(last.kind, TokenType::EndOfFile, "last token of {input:?}");
    (last.line, last.column)
}

/// The `Keyword` token type for `word`, looked up through the public API.
fn kw(word: &str) -> TokenType {
    match Keyword::from_word(word) {
        Some(k) => TokenType::Keyword(k),
        None => panic!("{word:?} should be a keyword"),
    }
}

fn shape(kind: TokenType, value: &str) -> Shape {
    (kind, value.to_string())
}

/// A command token spelled exactly as given.
fn cmd(spelling: &str) -> Shape {
    shape(kw(spelling), spelling)
}

fn word(value: &str) -> Shape {
    shape(TokenType::Word, value)
}

fn int(value: &str) -> Shape {
    shape(TokenType::Integer, value)
}

fn float(value: &str) -> Shape {
    shape(TokenType::Float, value)
}

fn string(value: &str) -> Shape {
    shape(TokenType::String, value)
}

fn newline() -> Shape {
    shape(TokenType::Newline, "\n")
}

fn tok(kind: TokenType, value: &str, line: usize, column: usize) -> Token {
    Token {
        kind,
        value: value.to_string(),
        line,
        column,
    }
}

fn cmd_tok(spelling: &str, line: usize, column: usize) -> Token {
    tok(kw(spelling), spelling, line, column)
}

fn word_tok(value: &str, line: usize, column: usize) -> Token {
    tok(TokenType::Word, value, line, column)
}

fn int_tok(value: &str, line: usize, column: usize) -> Token {
    tok(TokenType::Integer, value, line, column)
}

fn float_tok(value: &str, line: usize, column: usize) -> Token {
    tok(TokenType::Float, value, line, column)
}

fn newline_tok(line: usize, column: usize) -> Token {
    tok(TokenType::Newline, "\n", line, column)
}

/// Assert the exact tokens (kind, value, line, column) and where `EndOfFile`
/// sits.
fn assert_stream(input: &str, expected: &[Token], eof: (usize, usize)) {
    assert_eq!(body(input), expected, "tokens of {input:?}");
    assert_eq!(eof_position(input), eof, "EndOfFile position of {input:?}");
}

/// Lex `GET <arg>` and return the single argument token as (kind, value).
fn arg_shape(arg: &str) -> Shape {
    let input = format!("GET {arg}");
    let mut tokens = body(&input);
    assert_eq!(tokens.len(), 2, "tokens of {input:?}: {tokens:?}");
    let token = tokens.remove(1);
    (token.kind, token.value)
}

// ------------------------------------------------------- mode detection

#[test]
fn kv_command_then_space_or_tab_starts_kv_mode() {
    assert_eq!(shapes("GET user:1"), vec![cmd("GET"), word("user:1")]);
    assert_eq!(shapes("GET\tuser:1"), vec![cmd("GET"), word("user:1")]);
}

#[test]
fn kv_command_then_newline_starts_kv_mode() {
    // A `Newline` token only exists in KV mode.
    assert_eq!(shapes("EXEC\n"), vec![cmd("EXEC"), newline()]);
}

#[test]
fn kv_command_at_end_of_input_is_keyword_without_newline() {
    assert_stream("GET", &[cmd_tok("GET", 1, 1)], (1, 4));
}

#[test]
fn kv_command_then_non_whitespace_is_not_kv_mode() {
    assert_eq!(
        kinds("GET(a-b)"),
        vec![
            kw("GET"),
            TokenType::LParen,
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
            TokenType::RParen,
        ]
    );
    assert_eq!(
        kinds("SET.x"),
        vec![kw("SET"), TokenType::Dot, TokenType::Identifier]
    );
    assert_eq!(kinds("GET;"), vec![kw("GET"), TokenType::Semicolon]);
    assert_eq!(kinds("GET\"a\""), vec![kw("GET"), TokenType::String]);
    assert_eq!(
        kinds("GET,a-b"),
        vec![
            kw("GET"),
            TokenType::Comma,
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
        ]
    );
}

#[test]
fn kv_command_as_prefix_of_longer_word_is_not_kv_mode() {
    // The first word is the whole `[A-Za-z0-9_]` run, so none of these is a
    // command, and the rest of the line is lexed in normal mode.
    for first in ["GETX", "SETS", "GET_", "GET1", "HSETS"] {
        let input = format!("{first} a:1");
        let tokens = body(&input);
        assert_eq!(tokens[0].kind, TokenType::Identifier, "first of {input:?}");
        assert_eq!(tokens[0].value, first, "first of {input:?}");
        assert_eq!(
            tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
            vec![
                TokenType::Identifier,
                TokenType::Identifier,
                TokenType::Colon,
                TokenType::Integer,
            ],
            "kinds of {input:?}"
        );
    }
}

#[test]
fn kv_non_command_first_word_uses_normal_mode() {
    assert_eq!(
        kinds("foo a-b"),
        vec![
            TokenType::Identifier,
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
        ]
    );
    // `DELETE` is a shared keyword, not a Redis command.
    assert_eq!(
        kinds("DELETE a-b"),
        vec![
            kw("DELETE"),
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
        ]
    );
    assert_eq!(
        kinds("SELECT a-b"),
        vec![
            kw("SELECT"),
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
        ]
    );
}

#[test]
fn kv_shared_set_at_statement_start_is_the_redis_command() {
    assert_eq!(shapes("SET a-b 1"), vec![cmd("SET"), word("a-b"), int("1")]);
}

#[test]
fn kv_set_inside_update_stays_a_normal_keyword() {
    for input in ["UPDATE t SET a = 1", "update t set a = 1"] {
        assert_eq!(
            kinds(input),
            vec![
                kw("UPDATE"),
                TokenType::Identifier,
                kw("SET"),
                TokenType::Identifier,
                TokenType::Eq,
                TokenType::Integer,
            ],
            "kinds of {input:?}"
        );
    }
}

#[test]
fn kv_command_after_statement_start_is_a_plain_keyword() {
    assert_eq!(
        kinds("SELECT GET FROM t"),
        vec![kw("SELECT"), kw("GET"), kw("FROM"), TokenType::Identifier]
    );
}

#[test]
fn kv_command_is_case_insensitive_and_keeps_original_spelling() {
    for spelling in ["GET", "get", "Get", "gEt", "gET"] {
        let input = format!("{spelling} k\n");
        assert_eq!(
            shapes(&input),
            vec![cmd(spelling), word("k"), newline()],
            "shapes of {input:?}"
        );
    }
    assert_eq!(
        shapes("hGetAll user:1\n"),
        vec![cmd("hGetAll"), word("user:1"), newline()]
    );
}

#[test]
fn kv_mode_is_detected_for_a_statement_after_a_semicolon() {
    let input = "SELECT 1; GET user:1";
    assert_eq!(
        kinds(input),
        vec![
            kw("SELECT"),
            TokenType::Integer,
            TokenType::Semicolon,
            kw("GET"),
            TokenType::Word,
        ]
    );
    assert_eq!(body(input)[4].value, "user:1");
}

#[test]
fn kv_newline_inside_normal_statement_does_not_start_a_statement() {
    // Only `;` ends a normal statement, so `GET` here is just a keyword and
    // `a-b` is still three tokens.
    assert_eq!(
        kinds("SELECT\nGET a-b"),
        vec![
            kw("SELECT"),
            kw("GET"),
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
        ]
    );
}

#[test]
fn kv_normal_mode_resumes_after_the_kv_newline() {
    assert_eq!(
        kinds("GET a-b\nSELECT a-b FROM t;"),
        vec![
            kw("GET"),
            TokenType::Word,
            TokenType::Newline,
            kw("SELECT"),
            TokenType::Identifier,
            TokenType::Minus,
            TokenType::Identifier,
            kw("FROM"),
            TokenType::Identifier,
            TokenType::Semicolon,
        ]
    );
}

#[test]
fn kv_blank_lines_and_indentation_before_command_are_skipped() {
    assert_eq!(
        spans("\n\n  GET a\n"),
        vec![
            (kw("GET"), 3, 3),
            (TokenType::Word, 3, 7),
            (TokenType::Newline, 3, 8),
        ]
    );
}

#[test]
fn kv_comment_before_command_is_skipped() {
    assert_eq!(
        spans("// note\nGET a-b"),
        vec![(kw("GET"), 2, 1), (TokenType::Word, 2, 5)]
    );
    assert_eq!(
        spans("/* note */ GET a-b"),
        vec![(kw("GET"), 1, 12), (TokenType::Word, 1, 16)]
    );
}

#[test]
fn kv_comments_between_kv_statements_are_skipped() {
    let expected = vec![
        cmd("GET"),
        word("a"),
        newline(),
        cmd("GET"),
        word("b"),
        newline(),
    ];
    assert_eq!(shapes("GET a\n// note\nGET b\n"), expected);
    assert_eq!(shapes("GET a\n/* note */ GET b\n"), expected);
}

// ------------------------------------------------------------ bare words

#[test]
fn kv_spec_bare_words_are_words() {
    for arg in [
        "user:1", "a-b", "10.0.0.1", "http://x", "-inf", "(5", "1.2.3", "+5", "1e",
    ] {
        assert_eq!(arg_shape(arg), word(arg), "argument {arg:?}");
    }
}

#[test]
fn kv_malformed_numbers_are_words_not_errors() {
    // None of these matches `-?digits` or a `-?` number with a fraction or
    // exponent, so they are `Word`s (in normal mode `12abc` would be an error).
    for arg in [
        "-", "--5", "5-", "1.", ".5", "12abc", "1e+", "0x10", "1_000",
    ] {
        assert_eq!(arg_shape(arg), word(arg), "argument {arg:?}");
    }
}

#[test]
fn kv_slashes_and_hash_are_word_characters_not_comments() {
    assert_eq!(
        shapes("SET #tag 1 // not a comment"),
        vec![
            cmd("SET"),
            word("#tag"),
            int("1"),
            word("//"),
            word("not"),
            word("a"),
            word("comment"),
        ]
    );
    assert_eq!(arg_shape("//x"), word("//x"));
    assert_eq!(arg_shape("a#b"), word("a#b"));
    // Block comment markers are word characters too.
    assert_eq!(
        shapes("SET k /* v */"),
        vec![cmd("SET"), word("k"), word("/*"), word("v"), word("*/")]
    );
}

#[test]
fn kv_semicolon_is_a_word_character_and_does_not_end_the_statement() {
    assert_eq!(
        shapes("GET a; GET b"),
        vec![cmd("GET"), word("a;"), word("GET"), word("b")]
    );
    assert_eq!(shapes("SET k ;"), vec![cmd("SET"), word("k"), word(";")]);
    // Only the end of the line ends the statement.
    assert_eq!(
        shapes("SET k v;\nGET k"),
        vec![
            cmd("SET"),
            word("k"),
            word("v;"),
            newline(),
            cmd("GET"),
            word("k"),
        ]
    );
}

#[test]
fn kv_arguments_that_spell_keywords_are_words() {
    assert_eq!(
        shapes("SET SELECT FROM"),
        vec![cmd("SET"), word("SELECT"), word("FROM")]
    );
    assert_eq!(shapes("GET GET"), vec![cmd("GET"), word("GET")]);
    assert_eq!(
        shapes("SET k NULL"),
        vec![cmd("SET"), word("k"), word("NULL")]
    );
}

#[test]
fn kv_quote_inside_a_bare_word_is_a_word_character() {
    assert_eq!(
        shapes("GET ab\"c d\""),
        vec![cmd("GET"), word("ab\"c"), word("d\"")]
    );
    assert_eq!(arg_shape("it's"), word("it's"));
}

// --------------------------------------------------------------- numbers

#[test]
fn kv_integers_and_floats_are_classified_with_optional_leading_minus() {
    for arg in ["42", "0", "-5", "-0", "99999999999999999999999999"] {
        assert_eq!(arg_shape(arg), int(arg), "argument {arg:?}");
    }
    for arg in [
        "3.5", "-0.25", "1e3", "0.0", "1E3", "-1e3", "2.5E+10", "1e-3", "-1.5e-3",
    ] {
        assert_eq!(arg_shape(arg), float(arg), "argument {arg:?}");
    }
}

#[test]
fn kv_quoted_digits_stay_a_string() {
    assert_eq!(
        shapes("SET k \"42\" '3.5'"),
        vec![cmd("SET"), word("k"), string("42"), string("3.5")]
    );
}

// ----------------------------------------------------------- quoted args

#[test]
fn kv_quoted_argument_keeps_spaces_and_has_no_quotes_in_value() {
    // The position of a string is its opening quote.
    assert_eq!(
        spans("SET k \"a b\" c"),
        vec![
            (kw("SET"), 1, 1),
            (TokenType::Word, 1, 5),
            (TokenType::String, 1, 7),
            (TokenType::Word, 1, 13),
        ]
    );
    assert_eq!(
        shapes("SET k \"hello world\""),
        vec![cmd("SET"), word("k"), string("hello world")]
    );
    assert_eq!(
        shapes("SET k 'hello world'"),
        vec![cmd("SET"), word("k"), string("hello world")]
    );
}

#[test]
fn kv_quoted_argument_escapes() {
    // \n \r \t \0 \\ \" \' in one string.
    let input = r#"SET k "a\nb\rc\td\0e\\f\"g\'h""#;
    assert_eq!(
        shapes(input),
        vec![cmd("SET"), word("k"), string("a\nb\rc\td\0e\\f\"g'h")]
    );
    assert_eq!(
        shapes(r"SET k 'it\'s'"),
        vec![cmd("SET"), word("k"), string("it's")]
    );
}

#[test]
fn kv_doubled_quote_inside_quoted_argument_is_one_quote() {
    assert_eq!(
        shapes("SET k 'it''s'"),
        vec![cmd("SET"), word("k"), string("it's")]
    );
    assert_eq!(
        shapes("SET k \"say \"\"hi\"\"\""),
        vec![cmd("SET"), word("k"), string("say \"hi\"")]
    );
}

#[test]
fn kv_other_quote_character_needs_no_escape_inside_quotes() {
    assert_eq!(
        shapes("SET k \"it's\" 'say \"hi\"'"),
        vec![cmd("SET"), word("k"), string("it's"), string("say \"hi\"")]
    );
}

#[test]
fn kv_empty_quoted_argument() {
    assert_eq!(
        shapes("SET k \"\" ''"),
        vec![cmd("SET"), word("k"), string(""), string("")]
    );
}

#[test]
fn kv_quoted_argument_may_be_followed_by_whitespace_or_end_of_input() {
    let cases: Vec<(&str, Vec<Shape>)> = vec![
        ("GET \"a b\"", vec![cmd("GET"), string("a b")]),
        ("GET \"a\"\n", vec![cmd("GET"), string("a"), newline()]),
        ("GET \"a\"\r\n", vec![cmd("GET"), string("a"), newline()]),
        ("GET \"a\"\tb", vec![cmd("GET"), string("a"), word("b")]),
        ("GET 'a' b", vec![cmd("GET"), string("a"), word("b")]),
    ];
    for (input, expected) in cases {
        assert_eq!(shapes(input), expected, "shapes of {input:?}");
    }
}

#[test]
fn kv_quoted_argument_followed_by_text_is_an_error_at_that_text() {
    let err = lex_err("GET \"a\"b");
    assert_eq!(
        err.to_string(),
        "expected whitespace after a quoted argument at 1:8"
    );

    let cases = [
        // single quotes
        ("GET 'a'b", (1, 8)),
        // a semicolon is not whitespace
        ("GET \"a\";", (1, 8)),
        // two quoted arguments with no space between them
        ("GET \"a\"'b'", (1, 8)),
        // after other arguments
        ("SET k \"v\"x", (1, 10)),
        // on a later line
        ("GET a\nSET k \"v\"x", (2, 10)),
    ];
    for (input, (line, column)) in cases {
        let err = lex_err(input);
        assert_eq!(
            err.message, "expected whitespace after a quoted argument",
            "message for {input:?}"
        );
        assert_eq!(
            (err.line, err.column),
            (line, column),
            "position for {input:?}"
        );
    }
}

#[test]
fn kv_unterminated_quoted_argument_points_at_the_opening_quote() {
    let cases = [
        ("GET \"abc", (1, 5)),
        ("GET 'abc", (1, 5)),
        ("GET a\nSET k \"abc", (2, 7)),
    ];
    for (input, (line, column)) in cases {
        let err = lex_err(input);
        assert_eq!(err.message, "unterminated string", "message for {input:?}");
        assert_eq!(
            (err.line, err.column),
            (line, column),
            "position for {input:?}"
        );
    }
}

#[test]
fn kv_invalid_escape_in_quoted_argument_points_at_the_backslash() {
    let err = lex_err(r#"GET "a\q""#);
    assert_eq!(err.message, r"invalid escape '\q'");
    assert_eq!((err.line, err.column), (1, 7));
}

// -------------------------------------------------------------- newlines

#[test]
fn kv_newline_token_has_value_and_sits_at_the_newline_character() {
    let tokens = body("GET a\nGET b\n");
    let newlines: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.kind == TokenType::Newline)
        .collect();
    assert_eq!(
        newlines,
        vec![&newline_tok(1, 6), &newline_tok(2, 6)],
        "newline tokens of {tokens:?}"
    );
}

#[test]
fn kv_crlf_ends_the_statement_and_cr_is_not_part_of_the_last_word() {
    assert_stream(
        "GET a\r\nGET b\r\n",
        &[
            cmd_tok("GET", 1, 1),
            word_tok("a", 1, 5),
            newline_tok(1, 7),
            cmd_tok("GET", 2, 1),
            word_tok("b", 2, 5),
            newline_tok(2, 7),
        ],
        (3, 1),
    );
}

#[test]
fn kv_blank_lines_between_statements_produce_no_tokens() {
    // Line 2 is empty, line 3 holds spaces, a tab and a CR, line 4 is empty.
    assert_stream(
        "GET a\n\n  \t\r\n\nGET b\n",
        &[
            cmd_tok("GET", 1, 1),
            word_tok("a", 1, 5),
            newline_tok(1, 6),
            cmd_tok("GET", 5, 1),
            word_tok("b", 5, 5),
            newline_tok(5, 6),
        ],
        (6, 1),
    );
}

#[test]
fn kv_final_statement_without_newline_has_no_newline_token() {
    assert_stream(
        "GET a\nGET b",
        &[
            cmd_tok("GET", 1, 1),
            word_tok("a", 1, 5),
            newline_tok(1, 6),
            cmd_tok("GET", 2, 1),
            word_tok("b", 2, 5),
        ],
        (2, 6),
    );
}

#[test]
fn kv_arguments_are_separated_by_spaces_and_tabs_and_trailing_blanks_are_ignored() {
    // A tab is one column.
    assert_stream(
        "SET  k \t v  \n",
        &[
            cmd_tok("SET", 1, 1),
            word_tok("k", 1, 6),
            word_tok("v", 1, 10),
            newline_tok(1, 13),
        ],
        (2, 1),
    );
}

#[test]
fn kv_columns_count_characters_not_bytes() {
    assert_stream(
        "SET ключ значение\nSET k \"日本\" x",
        &[
            cmd_tok("SET", 1, 1),
            word_tok("ключ", 1, 5),
            word_tok("значение", 1, 10),
            newline_tok(1, 18),
            cmd_tok("SET", 2, 1),
            word_tok("k", 2, 5),
            tok(TokenType::String, "日本", 2, 7),
            word_tok("x", 2, 12),
        ],
        (2, 13),
    );
}

#[test]
fn kv_token_keyword_accessor_reports_the_command() {
    let tokens = body("hset k f v");
    assert_eq!(tokens[0].keyword(), Keyword::from_word("HSET"));
    assert!(tokens[0].keyword().is_some());
    assert_eq!(tokens[1].keyword(), None);
}

// ------------------------------------------------- statements and scripts

#[test]
fn kv_consecutive_statements_exact_stream() {
    assert_stream(
        "SET a 1\nGET a\nDEL a\n",
        &[
            cmd_tok("SET", 1, 1),
            word_tok("a", 1, 5),
            int_tok("1", 1, 7),
            newline_tok(1, 8),
            cmd_tok("GET", 2, 1),
            word_tok("a", 2, 5),
            newline_tok(2, 6),
            cmd_tok("DEL", 3, 1),
            word_tok("a", 3, 5),
            newline_tok(3, 6),
        ],
        (4, 1),
    );
}

#[test]
fn kv_script_mixing_kv_and_non_kv_statements() {
    let input = "SELECT a FROM t;\nSET user:1 alice\nMATCH (n) RETURN n;\nGET user:1";
    assert_eq!(
        spans(input),
        vec![
            (kw("SELECT"), 1, 1),
            (TokenType::Identifier, 1, 8),
            (kw("FROM"), 1, 10),
            (TokenType::Identifier, 1, 15),
            (TokenType::Semicolon, 1, 16),
            (kw("SET"), 2, 1),
            (TokenType::Word, 2, 5),
            (TokenType::Word, 2, 12),
            (TokenType::Newline, 2, 17),
            (kw("MATCH"), 3, 1),
            (TokenType::LParen, 3, 7),
            (TokenType::Identifier, 3, 8),
            (TokenType::RParen, 3, 9),
            (kw("RETURN"), 3, 11),
            (TokenType::Identifier, 3, 18),
            (TokenType::Semicolon, 3, 19),
            (kw("GET"), 4, 1),
            (TokenType::Word, 4, 5),
        ]
    );
    let tokens = body(input);
    assert_eq!(tokens[6].value, "user:1");
    assert_eq!(tokens[7].value, "alice");
    assert_eq!(tokens[17].value, "user:1");
    assert_eq!(eof_position(input), (4, 11));
}

#[test]
fn kv_every_spec_command_is_a_keyword_at_statement_start() {
    for &command in SPEC_KV_COMMANDS {
        let input = format!("{command} a:1");
        assert_eq!(
            shapes(&input),
            vec![cmd(command), word("a:1")],
            "shapes of {input:?}"
        );
        // Alone at the end of the input there is still no `Newline`.
        assert_eq!(shapes(command), vec![cmd(command)], "shapes of {command:?}");
    }
}

#[test]
fn kv_every_spec_command_in_lower_case_keeps_spelling_and_enters_kv_mode() {
    for &command in SPEC_KV_COMMANDS {
        let lower = command.to_ascii_lowercase();
        let input = format!("{lower} x-y\n");
        assert_eq!(
            shapes(&input),
            vec![cmd(&lower), word("x-y"), newline()],
            "shapes of {input:?}"
        );
    }
}

// ------------------------------------------------------- exact streams

#[test]
fn kv_exact_stream_set_with_expiry() {
    assert_stream(
        "SET session:42 token EX 60\n",
        &[
            cmd_tok("SET", 1, 1),
            word_tok("session:42", 1, 5),
            word_tok("token", 1, 16),
            word_tok("EX", 1, 22),
            int_tok("60", 1, 25),
            newline_tok(1, 27),
        ],
        (2, 1),
    );
}

#[test]
fn kv_exact_stream_hset_with_several_pairs() {
    assert_stream(
        "HSET user:1 name alice age 30 score 4.5\n",
        &[
            cmd_tok("HSET", 1, 1),
            word_tok("user:1", 1, 6),
            word_tok("name", 1, 13),
            word_tok("alice", 1, 18),
            word_tok("age", 1, 24),
            int_tok("30", 1, 28),
            word_tok("score", 1, 31),
            float_tok("4.5", 1, 37),
            newline_tok(1, 40),
        ],
        (2, 1),
    );
}

#[test]
fn kv_exact_stream_zadd_with_floats() {
    assert_stream(
        "ZADD board 1.5 alice -2.25 bob 1e3 carol",
        &[
            cmd_tok("ZADD", 1, 1),
            word_tok("board", 1, 6),
            float_tok("1.5", 1, 12),
            word_tok("alice", 1, 16),
            float_tok("-2.25", 1, 22),
            word_tok("bob", 1, 28),
            float_tok("1e3", 1, 32),
            word_tok("carol", 1, 36),
        ],
        (1, 41),
    );
}

#[test]
fn kv_exact_stream_lrange_with_negative_indexes() {
    assert_stream(
        "LRANGE queue -100 -1\n",
        &[
            cmd_tok("LRANGE", 1, 1),
            word_tok("queue", 1, 8),
            int_tok("-100", 1, 14),
            int_tok("-1", 1, 19),
            newline_tok(1, 21),
        ],
        (2, 1),
    );
}

#[test]
fn kv_exact_stream_zrangebyscore_with_infinities() {
    assert_stream(
        "ZRANGEBYSCORE board -inf +inf\n",
        &[
            cmd_tok("ZRANGEBYSCORE", 1, 1),
            word_tok("board", 1, 15),
            word_tok("-inf", 1, 21),
            word_tok("+inf", 1, 26),
            newline_tok(1, 30),
        ],
        (2, 1),
    );
}

#[test]
fn kv_exact_stream_zrangebyscore_with_exclusive_bounds_and_limit() {
    // `(1` and `(5.5` are words, and `LIMIT` is an argument here, not a keyword.
    assert_stream(
        "ZRANGEBYSCORE board (1 (5.5 LIMIT 0 10",
        &[
            cmd_tok("ZRANGEBYSCORE", 1, 1),
            word_tok("board", 1, 15),
            word_tok("(1", 1, 21),
            word_tok("(5.5", 1, 24),
            word_tok("LIMIT", 1, 29),
            int_tok("0", 1, 35),
            int_tok("10", 1, 37),
        ],
        (1, 39),
    );
}

#[test]
fn kv_exact_stream_multi_exec_transaction() {
    assert_stream(
        "MULTI\nSET a 1\nINCR a\nEXEC\n",
        &[
            cmd_tok("MULTI", 1, 1),
            newline_tok(1, 6),
            cmd_tok("SET", 2, 1),
            word_tok("a", 2, 5),
            int_tok("1", 2, 7),
            newline_tok(2, 8),
            cmd_tok("INCR", 3, 1),
            word_tok("a", 3, 6),
            newline_tok(3, 7),
            cmd_tok("EXEC", 4, 1),
            newline_tok(4, 5),
        ],
        (5, 1),
    );
}

#[test]
fn kv_exact_stream_multi_exec_transaction_with_crlf() {
    assert_stream(
        "MULTI\r\nDISCARD\r\n",
        &[
            cmd_tok("MULTI", 1, 1),
            newline_tok(1, 7),
            cmd_tok("DISCARD", 2, 1),
            newline_tok(2, 9),
        ],
        (3, 1),
    );
}
