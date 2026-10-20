//! Behaviour shared by all four models (DD-003): strings, quoted names,
//! comments, operators, positions, errors, mode selection in mixed scripts,
//! and the "never panics" guarantee.
//!
//! Every expectation here is derived from the spec, not from the lexer's
//! current behaviour.

use std::error::Error;
use std::iter;

use tdb_lexer::{Keyword, LexError, Lexer, Model, Token, TokenType as T, tokenize};

/// A token's kind and value.
type Pair<'a> = (T, &'a str);

const LPAREN: Pair<'static> = (T::LParen, "(");
const RPAREN: Pair<'static> = (T::RParen, ")");
const COMMA: Pair<'static> = (T::Comma, ",");
const MINUS: Pair<'static> = (T::Minus, "-");
const EQ: Pair<'static> = (T::Eq, "=");
const SEMI: Pair<'static> = (T::Semicolon, ";");
const NEWLINE: Pair<'static> = (T::Newline, "\n");

fn kw(keyword: Keyword, text: &str) -> Pair<'_> {
    (T::Keyword(keyword), text)
}

fn id(text: &str) -> Pair<'_> {
    (T::Identifier, text)
}

fn int(text: &str) -> Pair<'_> {
    (T::Integer, text)
}

fn float(text: &str) -> Pair<'_> {
    (T::Float, text)
}

fn string(text: &str) -> Pair<'_> {
    (T::String, text)
}

fn word(text: &str) -> Pair<'_> {
    (T::Word, text)
}

fn param(text: &str) -> Pair<'_> {
    (T::Parameter, text)
}

fn lex(input: &str) -> Vec<Token> {
    match tokenize(input) {
        Ok(tokens) => tokens,
        Err(e) => panic!("lexing {input:?} failed: {e}"),
    }
}

fn lex_err(input: &str) -> LexError {
    match tokenize(input) {
        Ok(tokens) => panic!("lexing {input:?} should fail, got {tokens:?}"),
        Err(e) => e,
    }
}

/// Lex `input` and compare the kinds and values of its tokens, which must end
/// with exactly one `EndOfFile` (not part of `want`).
fn expect(input: &str, want: &[Pair<'_>]) {
    let tokens = lex(input);
    let (last, body) = tokens.split_last().expect("at least an EndOfFile token");
    assert_eq!(last.kind, T::EndOfFile, "last token of {input:?}");
    assert!(
        body.iter().all(|t| t.kind != T::EndOfFile),
        "EndOfFile must appear only once in {input:?}"
    );
    let got: Vec<Pair<'_>> = body.iter().map(|t| (t.kind, t.value.as_str())).collect();
    assert_eq!(got, want, "token stream of {input:?}");
}

/// Like `expect`, but also compares each token's line and column, and the
/// position of the `EndOfFile` token.
fn expect_at(input: &str, want: &[(T, &str, usize, usize)], eof: (usize, usize)) {
    let tokens = lex(input);
    let (last, body) = tokens.split_last().expect("at least an EndOfFile token");
    assert_eq!(last.kind, T::EndOfFile, "last token of {input:?}");
    assert_eq!(
        (last.line, last.column),
        eof,
        "EndOfFile position of {input:?}"
    );
    let got: Vec<(T, &str, usize, usize)> = body
        .iter()
        .map(|t| (t.kind, t.value.as_str(), t.line, t.column))
        .collect();
    assert_eq!(got, want, "tokens with positions of {input:?}");
}

/// The kinds of the tokens of `input`, without the final `EndOfFile`.
fn kinds(input: &str) -> Vec<T> {
    let mut tokens = lex(input);
    let last = tokens.pop().expect("at least an EndOfFile token");
    assert_eq!(last.kind, T::EndOfFile, "last token of {input:?}");
    tokens.into_iter().map(|t| t.kind).collect()
}

fn eof_position(input: &str) -> (usize, usize) {
    let tokens = lex(input);
    let last = tokens.last().expect("at least an EndOfFile token");
    assert_eq!(last.kind, T::EndOfFile, "last token of {input:?}");
    (last.line, last.column)
}

fn expect_err(input: &str, message: &str, line: usize, column: usize) {
    assert_eq!(
        lex_err(input),
        LexError {
            message: message.to_string(),
            line,
            column,
        },
        "error for {input:?}"
    );
}

/// The first token of every statement: the first token of the input and the
/// first token after each `Semicolon` (normal mode) or `Newline` (KV mode).
fn statement_starts(tokens: &[Token]) -> Vec<&Token> {
    let mut starts = Vec::new();
    let mut at_start = true;
    for token in tokens {
        match token.kind {
            T::EndOfFile => break,
            T::Semicolon | T::Newline => at_start = true,
            _ if at_start => {
                starts.push(token);
                at_start = false;
            }
            _ => {}
        }
    }
    starts
}

// ---------------------------------------------------------------------------
// Strings
// ---------------------------------------------------------------------------

#[test]
fn common_string_single_and_double_quotes() {
    expect_at("'hello'", &[(T::String, "hello", 1, 1)], (1, 8));
    expect_at("\"hello\"", &[(T::String, "hello", 1, 1)], (1, 8));
    // The other quote kind is an ordinary character inside.
    expect("'say \"hi\"'", &[string("say \"hi\"")]);
    expect("\"it's\"", &[string("it's")]);
}

#[test]
fn common_string_every_escape_in_both_quote_kinds() {
    let escapes = [
        ('n', "\n"),
        ('r', "\r"),
        ('t', "\t"),
        ('0', "\0"),
        ('\\', "\\"),
        ('"', "\""),
        ('\'', "'"),
    ];
    for quote in ['\'', '"'] {
        for (escape, value) in escapes {
            let input = format!("{quote}a\\{escape}b{quote}");
            let want = format!("a{value}b");
            expect(&input, &[string(&want)]);
        }
    }
}

#[test]
fn common_string_escaped_backslash_does_not_escape_what_follows() {
    // `\\` then `n` is a backslash and a letter, not a newline.
    expect(r"'\\n'", &[string("\\n")]);
    // `\\` before the closing quote leaves the quote as the terminator.
    expect(r"'\\' x", &[string("\\"), id("x")]);
    expect(r"'a\\\\b'", &[string("a\\\\b")]);
    // `\0` is one escape; the next `0` is a plain character.
    expect(r"'\00'", &[string("\u{0}0")]);
}

#[test]
fn common_string_doubled_single_quote_is_one_quote() {
    expect("'it''s'", &[string("it's")]);
    expect("''''", &[string("'")]);
    expect("'a''''b'", &[string("a''b")]);
    expect("'a'''", &[string("a'")]);
    // A doubled double-quote means nothing inside single quotes.
    expect("'a\"\"b'", &[string("a\"\"b")]);
}

#[test]
fn common_string_doubled_double_quote_is_one_quote() {
    expect("\"say \"\"hi\"\"\"", &[string("say \"hi\"")]);
    expect("\"\"\"\"", &[string("\"")]);
    expect("\"a\"\"\"\"b\"", &[string("a\"\"b")]);
    // A doubled single-quote means nothing inside double quotes.
    expect("\"a''b\"", &[string("a''b")]);
}

#[test]
fn common_string_empty_and_empty_followed_by_another_string() {
    expect_at("''", &[(T::String, "", 1, 1)], (1, 3));
    expect_at("\"\"", &[(T::String, "", 1, 1)], (1, 3));
    expect_at(
        "'' 'x'",
        &[(T::String, "", 1, 1), (T::String, "x", 1, 4)],
        (1, 7),
    );
    expect_at(
        "'' ''",
        &[(T::String, "", 1, 1), (T::String, "", 1, 4)],
        (1, 6),
    );
    expect_at(
        "\"\",\"y\"",
        &[
            (T::String, "", 1, 1),
            (T::Comma, ",", 1, 3),
            (T::String, "y", 1, 4),
        ],
        (1, 7),
    );
}

#[test]
fn common_string_may_contain_newlines() {
    expect_at(
        "'a\nb' x",
        &[(T::String, "a\nb", 1, 1), (T::Identifier, "x", 2, 4)],
        (2, 5),
    );
    expect_at(
        "'a\nb\nc' x",
        &[(T::String, "a\nb\nc", 1, 1), (T::Identifier, "x", 3, 4)],
        (3, 5),
    );
    // The value is the text as written, line ending included.
    expect("'a\r\nb'", &[string("a\r\nb")]);
}

#[test]
fn common_string_position_is_the_opening_quote_even_after_earlier_lines() {
    expect_at(
        "x\n  'a\nb' y",
        &[
            (T::Identifier, "x", 1, 1),
            (T::String, "a\nb", 2, 3),
            (T::Identifier, "y", 3, 4),
        ],
        (3, 5),
    );
}

#[test]
fn common_string_unicode_is_kept_verbatim() {
    expect(
        "'héllo wörld ✓ 日本語 🦀'",
        &[string("héllo wörld ✓ 日本語 🦀")],
    );
    expect("\"日本語\"", &[string("日本語")]);
    expect("'é\\n日'", &[string("é\n日")]);
}

#[test]
fn common_string_keeps_comment_markers_and_semicolons() {
    expect(
        "'-- not // a /* comment */; x'",
        &[string("-- not // a /* comment */; x")],
    );
    // ... while a real comment after the string is still a comment.
    expect("'a' // real comment", &[string("a")]);
}

// ---------------------------------------------------------------------------
// Backtick-quoted names
// ---------------------------------------------------------------------------

#[test]
fn common_backtick_name_is_an_identifier_without_the_backticks() {
    expect("`my table`", &[id("my table")]);
    expect("`a-b:c.d`", &[id("a-b:c.d")]);
    expect("`x` `y`", &[id("x"), id("y")]);
}

#[test]
fn common_backtick_makes_a_keyword_usable_as_a_name() {
    expect("`select`", &[id("select")]);
    expect("`SELECT`", &[id("SELECT")]);
    expect(
        "SELECT `from` FROM t",
        &[
            kw(Keyword::Select, "SELECT"),
            id("from"),
            kw(Keyword::From, "FROM"),
            id("t"),
        ],
    );
}

#[test]
fn common_backtick_doubled_backtick_is_one_backtick() {
    expect("`a``b`", &[id("a`b")]);
    expect("````", &[id("`")]);
    expect("`a```", &[id("a`")]);
}

#[test]
fn common_backtick_name_position_counts_characters() {
    expect_at(
        "`a b` x",
        &[(T::Identifier, "a b", 1, 1), (T::Identifier, "x", 1, 7)],
        (1, 8),
    );
    expect_at(
        "`naïve` x",
        &[(T::Identifier, "naïve", 1, 1), (T::Identifier, "x", 1, 9)],
        (1, 10),
    );
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[test]
fn common_line_comment_runs_to_the_end_of_the_line() {
    expect_at(
        "a // comment\nb",
        &[(T::Identifier, "a", 1, 1), (T::Identifier, "b", 2, 1)],
        (2, 2),
    );
    expect_at(
        "a // c1\n// c2\nb",
        &[(T::Identifier, "a", 1, 1), (T::Identifier, "b", 3, 1)],
        (3, 2),
    );
    // A block-comment opener inside a line comment is just text.
    expect("// /* still a line comment\nx", &[id("x")]);
}

#[test]
fn common_block_comment_is_skipped_and_may_be_empty() {
    expect_at(
        "a /* c */ b",
        &[(T::Identifier, "a", 1, 1), (T::Identifier, "b", 1, 11)],
        (1, 12),
    );
    expect_at(
        "a/**/b",
        &[(T::Identifier, "a", 1, 1), (T::Identifier, "b", 1, 6)],
        (1, 7),
    );
}

#[test]
fn common_block_comment_spanning_lines_moves_positions() {
    expect_at(
        "a /* x\ny\nz */ b",
        &[(T::Identifier, "a", 1, 1), (T::Identifier, "b", 3, 6)],
        (3, 7),
    );
}

#[test]
fn common_block_comment_does_not_nest() {
    // The comment ends at the first `*/`; what follows is `c`, `*`, `/`.
    expect_at(
        "/* a /* b */ c */",
        &[
            (T::Identifier, "c", 1, 14),
            (T::Star, "*", 1, 16),
            (T::Slash, "/", 1, 17),
        ],
        (1, 18),
    );
}

#[test]
fn common_double_dash_is_not_a_comment() {
    expect("a -- b", &[id("a"), MINUS, MINUS, id("b")]);
    expect("a--b", &[id("a"), MINUS, MINUS, id("b")]);
    expect("-- x", &[MINUS, MINUS, id("x")]);
    expect(
        "SELECT 1 -- hi\nFROM t",
        &[
            kw(Keyword::Select, "SELECT"),
            int("1"),
            MINUS,
            MINUS,
            id("hi"),
            kw(Keyword::From, "FROM"),
            id("t"),
        ],
    );
}

#[test]
fn common_comment_at_end_of_input() {
    expect_at("a // trailing", &[(T::Identifier, "a", 1, 1)], (1, 14));
    expect_at("a // trailing\n", &[(T::Identifier, "a", 1, 1)], (2, 1));
    expect_at("a /* done */", &[(T::Identifier, "a", 1, 1)], (1, 13));
}

#[test]
fn common_input_of_only_comments_gives_just_end_of_file() {
    expect_at("// nothing", &[], (1, 11));
    expect_at("/* nothing */", &[], (1, 14));
    expect_at("/* a\nb */", &[], (2, 5));
    expect_at("// one\n// two\n", &[], (3, 1));
}

#[test]
fn common_comment_between_statements() {
    expect_at(
        "SELECT 1; // one\n/* two */ SELECT 2;",
        &[
            (T::Keyword(Keyword::Select), "SELECT", 1, 1),
            (T::Integer, "1", 1, 8),
            (T::Semicolon, ";", 1, 9),
            (T::Keyword(Keyword::Select), "SELECT", 2, 11),
            (T::Integer, "2", 2, 18),
            (T::Semicolon, ";", 2, 19),
        ],
        (2, 20),
    );
}

#[test]
fn common_single_slash_is_division_not_a_comment() {
    expect("a / b", &[id("a"), (T::Slash, "/"), id("b")]);
    expect("8/2", &[int("8"), (T::Slash, "/"), int("2")]);
    expect("a /", &[id("a"), (T::Slash, "/")]);
}

#[test]
fn common_unterminated_block_comment_is_an_error_at_its_opener() {
    expect_err("a /* never closed", "unterminated comment", 1, 3);
    expect_err("a\n  /* x\ny", "unterminated comment", 2, 3);
    expect_err("/*", "unterminated comment", 1, 1);
    // A lone `*` at the end is not a terminator.
    expect_err("/* a *", "unterminated comment", 1, 1);
}

// ---------------------------------------------------------------------------
// Operators and punctuation
// ---------------------------------------------------------------------------

const OPERATORS: &[(&str, T)] = &[
    ("<=>", T::CosineDistance),
    ("<->", T::L2Distance),
    ("<#>", T::InnerProduct),
    ("<=", T::LtEq),
    ("<>", T::NotEq),
    ("!=", T::NotEq),
    ("<-", T::LeftArrow),
    ("->", T::Arrow),
    (">=", T::GtEq),
    ("<", T::Lt),
    (">", T::Gt),
    ("=", T::Eq),
    ("+", T::Plus),
    ("-", T::Minus),
    ("*", T::Star),
    ("/", T::Slash),
    ("%", T::Percent),
    ("(", T::LParen),
    (")", T::RParen),
    ("[", T::LBracket),
    ("]", T::RBracket),
    ("{", T::LBrace),
    ("}", T::RBrace),
    (",", T::Comma),
    (":", T::Colon),
    (";", T::Semicolon),
    (".", T::Dot),
    ("..", T::DotDot),
    ("|", T::Pipe),
];

#[test]
fn common_every_operator_and_punctuation_token_has_its_kind_and_value() {
    for &(text, kind) in OPERATORS {
        let tokens = lex(text);
        assert_eq!(
            tokens.len(),
            2,
            "{text:?} should be one token plus EndOfFile"
        );
        assert_eq!(tokens[0].kind, kind, "kind of {text:?}");
        assert_eq!(tokens[0].value, text, "value of {text:?}");
        assert_eq!(
            (tokens[0].line, tokens[0].column),
            (1, 1),
            "position of {text:?}"
        );
        assert_eq!(tokens[1].kind, T::EndOfFile, "end of {text:?}");
    }
}

#[test]
fn common_operators_separated_by_spaces_keep_their_kinds_in_order() {
    let input = OPERATORS
        .iter()
        .map(|&(text, _)| text)
        .collect::<Vec<_>>()
        .join(" ");
    let got = lex(&input);
    let (last, body) = got.split_last().expect("tokens");
    assert_eq!(last.kind, T::EndOfFile);
    let got: Vec<(T, &str)> = body.iter().map(|t| (t.kind, t.value.as_str())).collect();
    let want: Vec<(T, &str)> = OPERATORS.iter().map(|&(text, kind)| (kind, text)).collect();
    assert_eq!(got, want);
}

#[test]
fn common_operators_match_longest_first() {
    let cases: &[(&str, &[T])] = &[
        ("<=>>", &[T::CosineDistance, T::Gt]),
        ("<=>=", &[T::CosineDistance, T::Eq]),
        ("<==", &[T::LtEq, T::Eq]),
        (">==", &[T::GtEq, T::Eq]),
        (">=>", &[T::GtEq, T::Gt]),
        ("<>=", &[T::NotEq, T::Eq]),
        ("<><", &[T::NotEq, T::Lt]),
        ("!==", &[T::NotEq, T::Eq]),
        ("<<", &[T::Lt, T::Lt]),
        (">>", &[T::Gt, T::Gt]),
        ("==", &[T::Eq, T::Eq]),
        ("=>", &[T::Eq, T::Gt]),
        ("<->", &[T::L2Distance]),
        ("<-->", &[T::LeftArrow, T::Arrow]),
        ("-->", &[T::Minus, T::Arrow]),
        ("<--", &[T::LeftArrow, T::Minus]),
        ("--", &[T::Minus, T::Minus]),
        ("---", &[T::Minus, T::Minus, T::Minus]),
        ("->>", &[T::Arrow, T::Gt]),
        ("<-<", &[T::LeftArrow, T::Lt]),
        ("...", &[T::DotDot, T::Dot]),
        ("....", &[T::DotDot, T::DotDot]),
        ("a<-1", &[T::Identifier, T::LeftArrow, T::Integer]),
        ("a < -1", &[T::Identifier, T::Lt, T::Minus, T::Integer]),
    ];
    for &(input, want) in cases {
        assert_eq!(kinds(input), want, "kinds of {input:?}");
    }
}

#[test]
fn common_adjacent_punctuation_without_whitespace() {
    expect_at(
        "({[,:;.]})",
        &[
            (T::LParen, "(", 1, 1),
            (T::LBrace, "{", 1, 2),
            (T::LBracket, "[", 1, 3),
            (T::Comma, ",", 1, 4),
            (T::Colon, ":", 1, 5),
            (T::Semicolon, ";", 1, 6),
            (T::Dot, ".", 1, 7),
            (T::RBracket, "]", 1, 8),
            (T::RBrace, "}", 1, 9),
            (T::RParen, ")", 1, 10),
        ],
        (1, 11),
    );
}

#[test]
fn common_compound_operators_outside_the_spec_split_into_single_tokens() {
    // `::` casts and `||` are not in scope: there is no token for them.
    assert_eq!(kinds("::"), [T::Colon, T::Colon]);
    assert_eq!(kinds("||"), [T::Pipe, T::Pipe]);
    assert_eq!(kinds("**"), [T::Star, T::Star]);
    assert_eq!(kinds("++"), [T::Plus, T::Plus]);
    assert_eq!(kinds(";;"), [T::Semicolon, T::Semicolon]);
}

// ---------------------------------------------------------------------------
// Words, numbers and parameters
// ---------------------------------------------------------------------------

#[test]
fn common_identifiers_use_ascii_letters_digits_and_underscore() {
    expect(
        "foo _bar baz_1 A9 _",
        &[id("foo"), id("_bar"), id("baz_1"), id("A9"), id("_")],
    );
    // A keyword is only a whole word: longer words are identifiers.
    expect(
        "selected select1 _select",
        &[id("selected"), id("select1"), id("_select")],
    );
    // A string right after a word is a separate token.
    expect("a'b'", &[id("a"), string("b")]);
}

#[test]
fn common_keywords_are_case_insensitive_and_keep_their_spelling() {
    expect(
        "select SELECT SeLeCt",
        &[
            kw(Keyword::Select, "select"),
            kw(Keyword::Select, "SELECT"),
            kw(Keyword::Select, "SeLeCt"),
        ],
    );
}

#[test]
fn common_integer_and_float_forms() {
    expect(
        "42 3.14 1e3 2.5E+10 1e-3 0 007 1E5 12.50",
        &[
            int("42"),
            float("3.14"),
            float("1e3"),
            float("2.5E+10"),
            float("1e-3"),
            int("0"),
            int("007"),
            float("1E5"),
            float("12.50"),
        ],
    );
}

#[test]
fn common_number_followed_by_dot_stops_at_the_dot() {
    expect("1.", &[int("1"), (T::Dot, ".")]);
    expect("1..3", &[int("1"), (T::DotDot, ".."), int("3")]);
    expect(".5", &[(T::Dot, "."), int("5")]);
    expect("a.1", &[id("a"), (T::Dot, "."), int("1")]);
    expect("1.a", &[int("1"), (T::Dot, "."), id("a")]);
    expect("1.5.2", &[float("1.5"), (T::Dot, "."), int("2")]);
}

#[test]
fn common_sign_is_a_separate_token() {
    expect("-5", &[MINUS, int("5")]);
    expect("+5", &[(T::Plus, "+"), int("5")]);
    expect("- 5", &[MINUS, int("5")]);
    expect("a-5", &[id("a"), MINUS, int("5")]);
    expect("1-2", &[int("1"), MINUS, int("2")]);
    expect("-1.5e-3", &[MINUS, float("1.5e-3")]);
}

#[test]
fn common_parameters_keep_what_follows_the_dollar() {
    expect(
        "$1 $name $a_b2 $1abc",
        &[param("1"), param("name"), param("a_b2"), param("1abc")],
    );
    expect_at(
        "x = $id)",
        &[
            (T::Identifier, "x", 1, 1),
            (T::Eq, "=", 1, 3),
            (T::Parameter, "id", 1, 5),
            (T::RParen, ")", 1, 8),
        ],
        (1, 9),
    );
    expect("$x$y", &[param("x"), param("y")]);
}

// ---------------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------------

#[test]
fn common_columns_count_characters_not_bytes() {
    expect_at(
        "\"日本語\" x",
        &[(T::String, "日本語", 1, 1), (T::Identifier, "x", 1, 7)],
        (1, 8),
    );
    expect_at(
        "'🦀🦀' x",
        &[(T::String, "🦀🦀", 1, 1), (T::Identifier, "x", 1, 6)],
        (1, 7),
    );
    expect_at("/* ñ */ x", &[(T::Identifier, "x", 1, 9)], (1, 10));
    expect_at(
        "'é' 'é' x",
        &[
            (T::String, "é", 1, 1),
            (T::String, "é", 1, 5),
            (T::Identifier, "x", 1, 9),
        ],
        (1, 10),
    );
}

#[test]
fn common_tab_counts_as_one_column() {
    expect_at(
        "\tx\t\ty",
        &[(T::Identifier, "x", 1, 2), (T::Identifier, "y", 1, 5)],
        (1, 6),
    );
    // A tab inside a string is one column too.
    expect_at(
        "'\t' z",
        &[(T::String, "\t", 1, 1), (T::Identifier, "z", 1, 5)],
        (1, 6),
    );
}

#[test]
fn common_new_line_resets_the_column() {
    expect_at(
        "a\nb\n\n  c",
        &[
            (T::Identifier, "a", 1, 1),
            (T::Identifier, "b", 2, 1),
            (T::Identifier, "c", 4, 3),
        ],
        (4, 4),
    );
    expect_at("\n\nx", &[(T::Identifier, "x", 3, 1)], (3, 2));
}

#[test]
fn common_crlf_line_endings_count_as_one_line_break() {
    expect_at(
        "a\r\nb\r\n  c",
        &[
            (T::Identifier, "a", 1, 1),
            (T::Identifier, "b", 2, 1),
            (T::Identifier, "c", 3, 3),
        ],
        (3, 4),
    );
}

#[test]
fn common_end_of_file_is_positioned_after_the_last_character() {
    let cases = [
        ("", (1, 1)),
        ("abc", (1, 4)),
        ("abc\n", (2, 1)),
        ("a\n\n", (3, 1)),
        ("a\r\n", (2, 1)),
        ("  ", (1, 3)),
        ("\t", (1, 2)),
        ("a\n  b", (2, 4)),
        ("'é'", (1, 4)),
        ("// 日本", (1, 6)),
        ("/* x */", (1, 8)),
    ];
    for (input, want) in cases {
        assert_eq!(eof_position(input), want, "EndOfFile of {input:?}");
    }
}

#[test]
fn common_newline_token_sits_at_the_newline_character() {
    expect_at(
        "GET a\nGET b\n",
        &[
            (T::Keyword(Keyword::Get), "GET", 1, 1),
            (T::Word, "a", 1, 5),
            (T::Newline, "\n", 1, 6),
            (T::Keyword(Keyword::Get), "GET", 2, 1),
            (T::Word, "b", 2, 5),
            (T::Newline, "\n", 2, 6),
        ],
        (3, 1),
    );
}

#[test]
fn common_newline_token_after_carriage_return_is_at_the_line_feed() {
    expect_at(
        "GET a\r\nGET b",
        &[
            (T::Keyword(Keyword::Get), "GET", 1, 1),
            (T::Word, "a", 1, 5),
            (T::Newline, "\n", 1, 7),
            (T::Keyword(Keyword::Get), "GET", 2, 1),
            (T::Word, "b", 2, 5),
        ],
        (2, 6),
    );
}

#[test]
fn common_blank_lines_after_a_kv_statement_produce_no_tokens() {
    expect_at(
        "GET a\n\n\nGET b",
        &[
            (T::Keyword(Keyword::Get), "GET", 1, 1),
            (T::Word, "a", 1, 5),
            (T::Newline, "\n", 1, 6),
            (T::Keyword(Keyword::Get), "GET", 4, 1),
            (T::Word, "b", 4, 5),
        ],
        (4, 6),
    );
}

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------

#[test]
fn common_empty_input_gives_just_end_of_file() {
    let tokens = lex("");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, T::EndOfFile);
    assert_eq!((tokens[0].line, tokens[0].column), (1, 1));
}

#[test]
fn common_whitespace_only_input_gives_just_end_of_file() {
    for input in [" ", "   ", "\t", "\n", "\n\n", "\r\n", " \r\n\t \n"] {
        let tokens = lex(input);
        assert_eq!(tokens.len(), 1, "tokens of {input:?}: {tokens:?}");
        assert_eq!(tokens[0].kind, T::EndOfFile, "token of {input:?}");
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn common_unexpected_ascii_characters_are_errors_at_the_character() {
    for c in ['@', '#', '~', '!', '^', '&', '?', '\\'] {
        let message = format!("unexpected character '{c}'");
        expect_err(&format!("ab {c}"), &message, 1, 4);
        expect_err(&format!("{c}"), &message, 1, 1);
        expect_err(&format!("a\n  {c}"), &message, 2, 3);
    }
}

#[test]
fn common_unexpected_non_ascii_characters_are_errors_counted_in_characters() {
    for c in ['é', 'ñ', 'ß', 'Ω', '日', '→', '€', '🦀'] {
        let message = format!("unexpected character '{c}'");
        // The string before it is 3 characters (more bytes); the error is at 5.
        expect_err(&format!("'é' {c}"), &message, 1, 5);
        expect_err(&format!("{c}"), &message, 1, 1);
    }
    // Words are ASCII-only: a letter outside `[A-Za-z]` ends the word and is
    // itself the error.
    expect_err("abé", "unexpected character 'é'", 1, 3);
}

#[test]
fn common_lone_bang_is_an_error_but_bang_equals_is_not() {
    expect_err("a ! b", "unexpected character '!'", 1, 3);
    expect_err("a !", "unexpected character '!'", 1, 3);
    expect_err("!", "unexpected character '!'", 1, 1);
    assert_eq!(kinds("a != b"), [T::Identifier, T::NotEq, T::Identifier]);
}

#[test]
fn common_unterminated_string_is_an_error_at_the_opening_quote() {
    expect_err("'abc", "unterminated string", 1, 1);
    expect_err("x 'abc", "unterminated string", 1, 3);
    expect_err("x \"abc", "unterminated string", 1, 3);
    expect_err("x\n  \"abc\ndef", "unterminated string", 2, 3);
    expect_err("'", "unterminated string", 1, 1);
    expect_err("\"", "unterminated string", 1, 1);
    // Strings may span lines, so a trailing newline does not end one.
    expect_err("'abc\n", "unterminated string", 1, 1);
    // The closing quote must match the opening one.
    expect_err("'abc\"", "unterminated string", 1, 1);
}

#[test]
fn common_unterminated_string_with_trailing_backslash() {
    expect_err("'abc\\", "unterminated string", 1, 1);
    expect_err("\"abc\\", "unterminated string", 1, 1);
    expect_err("x 'a\\", "unterminated string", 1, 3);
    expect_err("'\\", "unterminated string", 1, 1);
}

#[test]
fn common_doubled_quote_at_the_end_does_not_close_the_string() {
    expect_err("'it''s", "unterminated string", 1, 1);
    expect_err("'a''", "unterminated string", 1, 1);
    expect_err("\"say \"\"hi\"\"", "unterminated string", 1, 1);
    // ... but an odd number of quotes after the text closes it.
    expect("'a'''", &[string("a'")]);
}

#[test]
fn common_invalid_escape_is_an_error_at_the_backslash() {
    expect_err("'a\\qb'", "invalid escape '\\q'", 1, 3);
    expect_err("\"\\q\"", "invalid escape '\\q'", 1, 2);
    // `\xHH` and `\u` escapes are not in scope.
    expect_err("'\\x41'", "invalid escape '\\x'", 1, 2);
    expect_err("'\\u0041'", "invalid escape '\\u'", 1, 2);
    // Escapes are case-sensitive and `0` is the only digit.
    expect_err("'\\N'", "invalid escape '\\N'", 1, 2);
    expect_err("'\\1'", "invalid escape '\\1'", 1, 2);
    // The position counts characters, on the right line.
    expect_err("'éé\\q'", "invalid escape '\\q'", 1, 4);
    expect_err("x\n 'ab\\z'", "invalid escape '\\z'", 2, 5);
    expect_err("'\\é'", "invalid escape '\\é'", 1, 2);
}

#[test]
fn common_unterminated_quoted_name_is_an_error_at_the_opening_backtick() {
    expect_err("`abc", "unterminated quoted name", 1, 1);
    expect_err("x `abc", "unterminated quoted name", 1, 3);
    expect_err("`", "unterminated quoted name", 1, 1);
    expect_err("x\n  `abc def", "unterminated quoted name", 2, 3);
    // A doubled backtick is an escape, not a terminator.
    expect_err("`a``b", "unterminated quoted name", 1, 1);
    expect_err("```", "unterminated quoted name", 1, 1);
}

#[test]
fn common_empty_quoted_name_is_an_error_at_the_opening_backtick() {
    expect_err("``", "empty quoted name", 1, 1);
    expect_err("x ``", "empty quoted name", 1, 3);
    expect_err("SELECT `` FROM t", "empty quoted name", 1, 8);
}

#[test]
fn common_bare_dollar_is_an_error_at_the_dollar() {
    let message = "expected a parameter name after '$'";
    expect_err("$", message, 1, 1);
    expect_err("$ x", message, 1, 1);
    expect_err("a + $)", message, 1, 5);
    expect_err("x\n  $", message, 2, 3);
    expect_err("$-1", message, 1, 1);
    expect_err("$é", message, 1, 1);
    expect_err("$$", message, 1, 1);
}

#[test]
fn common_letter_after_a_number_is_an_invalid_number() {
    // The message is the number text plus the offending character.
    expect_err("12a", "invalid number '12a'", 1, 1);
    expect_err("12abc", "invalid number '12a'", 1, 1);
    expect_err("1_000", "invalid number '1_'", 1, 1);
    expect_err("3.14abc", "invalid number '3.14a'", 1, 1);
    expect_err("1e3x", "invalid number '1e3x'", 1, 1);
    expect_err("2.5E+10z", "invalid number '2.5E+10z'", 1, 1);
    // The position is the number's start.
    expect_err("x 12a", "invalid number '12a'", 1, 3);
    expect_err("a\n  12a", "invalid number '12a'", 2, 3);
    expect_err("'é' 7x", "invalid number '7x'", 1, 5);
}

#[test]
fn common_exponent_without_digits_is_an_invalid_number() {
    // An `e` with no digits after it is not part of the number, so it is a
    // letter directly after the number.
    expect_err("1e", "invalid number '1e'", 1, 1);
    expect_err("1e+", "invalid number '1e'", 1, 1);
    expect_err("1E-x", "invalid number '1E'", 1, 1);
    expect_err("1.5e", "invalid number '1.5e'", 1, 1);
    expect_err("x 1e ", "invalid number '1e'", 1, 3);
}

#[test]
fn common_kv_quoted_argument_must_be_followed_by_whitespace() {
    let message = "expected whitespace after a quoted argument";
    // The error is at the offending character, not at the quote.
    expect_err("GET 'a'b", message, 1, 8);
    expect_err("GET \"a\"b", message, 1, 8);
    expect_err("GET 'a';", message, 1, 8);
    expect_err("SET k 'a'\nGET 'b'x", message, 2, 8);
    // Whitespace, a newline, a carriage return or the end are all fine.
    expect(
        "GET 'a' 'b'\n",
        &[kw(Keyword::Get, "GET"), string("a"), string("b"), NEWLINE],
    );
    expect(
        "GET 'a'\tb",
        &[kw(Keyword::Get, "GET"), string("a"), word("b")],
    );
    expect("GET 'a'", &[kw(Keyword::Get, "GET"), string("a")]);
    expect(
        "GET 'a'\r\n",
        &[kw(Keyword::Get, "GET"), string("a"), NEWLINE],
    );
}

#[test]
fn common_string_errors_in_kv_mode_use_the_same_messages_and_positions() {
    expect_err("GET 'abc", "unterminated string", 1, 5);
    expect_err("GET 'a\\q'", "invalid escape '\\q'", 1, 7);
}

#[test]
fn common_error_displays_as_message_at_line_and_column() {
    let error = LexError {
        message: "bad thing".to_string(),
        line: 3,
        column: 7,
    };
    assert_eq!(error.to_string(), "bad thing at 3:7");

    assert_eq!(
        lex_err("a @").to_string(),
        "unexpected character '@' at 1:3"
    );
    assert_eq!(
        lex_err("x\n 'abc").to_string(),
        "unterminated string at 2:2"
    );
}

#[test]
fn common_lex_error_implements_std_error() {
    fn assert_std_error<E: Error + 'static>(_: &E) {}

    let error = lex_err("@");
    assert_std_error(&error);

    let boxed: Box<dyn Error> = Box::new(error);
    assert_eq!(boxed.to_string(), "unexpected character '@' at 1:1");

    fn run() -> Result<(), Box<dyn Error>> {
        tokenize("SELECT @")?;
        Ok(())
    }
    let failure = run().expect_err("lexing should fail");
    assert_eq!(failure.to_string(), "unexpected character '@' at 1:8");
}

#[test]
fn common_first_error_stops_lexing() {
    // Only the first problem is reported, even when later ones exist.
    expect_err("'ok' @ ~ #", "unexpected character '@'", 1, 6);
    expect_err("12a @", "invalid number '12a'", 1, 1);
    expect_err(
        "SELECT 1;\nSELECT @;\nSELECT #",
        "unexpected character '@'",
        2,
        8,
    );
    expect_err("GET a\n@\nGET b", "unexpected character '@'", 2, 1);
    // A later bad character inside an unterminated string is just string text.
    expect_err("a 'unterminated @", "unterminated string", 1, 3);
    // Nothing before the error is handed back: the result is an `Err`.
    assert!(tokenize("a b c @").is_err());
}

#[test]
fn common_lexer_new_then_tokenize_matches_the_tokenize_function() {
    let inputs = [
        "",
        "SELECT 1;",
        "GET user:1\nDEL k\n",
        "MATCH (a)-[:KNOWS]->(b) RETURN b",
        "a @ b",
        "'unterminated",
        "12a",
    ];
    for input in inputs {
        assert_eq!(Lexer::new(input).tokenize(), tokenize(input), "{input:?}");
    }
}

#[test]
fn common_every_successful_stream_ends_with_exactly_one_end_of_file() {
    let inputs = [
        "",
        " ",
        "// c",
        "a",
        "GET a",
        "GET a\n",
        "SELECT 1;",
        "MATCH (n) RETURN n;\n",
        "'x'",
        "`x`",
        "$x",
        "1.5",
    ];
    for input in inputs {
        let tokens = lex(input);
        let eofs = tokens.iter().filter(|t| t.kind == T::EndOfFile).count();
        assert_eq!(eofs, 1, "EndOfFile count of {input:?}");
        assert_eq!(
            tokens.last().map(|t| t.kind),
            Some(T::EndOfFile),
            "last token of {input:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Mixed-model scripts and mode selection
// ---------------------------------------------------------------------------

#[test]
fn common_mixed_script_exact_token_stream() {
    expect(
        "GET user:1\nSELECT a FROM t;\nMATCH (n) RETURN n;\nDEL k",
        &[
            kw(Keyword::Get, "GET"),
            word("user:1"),
            NEWLINE,
            kw(Keyword::Select, "SELECT"),
            id("a"),
            kw(Keyword::From, "FROM"),
            id("t"),
            SEMI,
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("n"),
            SEMI,
            kw(Keyword::Del, "DEL"),
            word("k"),
        ],
    );
}

const MIXED_SCRIPT: &str = concat!(
    "GET user:1\n",
    "HSET profile:7 name 'Ada Lovelace' age 36\n",
    "\n",
    "// comments and blank lines may separate statements\n",
    "SELECT name, age FROM people WHERE age >= 18 ORDER BY age DESC LIMIT 10;\n",
    "INSERT INTO docs VALUES (1, [0.1, 0.2, 0.3]);\n",
    // The lexer does not parse, so any VECTOR-led statement will do.
    "VECTOR docs <=> [0.1, 0.2, 0.3] LIMIT 5; /* nearest */\n",
    "MATCH (a:Person)-[:KNOWS*1..3]->(b) WHERE b.age > 30 RETURN b.name;\n",
    "SET answer 42\n",
    "DEL user:1\n",
);

#[test]
fn common_mixed_script_statement_model_comes_from_its_first_keyword() {
    let tokens = lex(MIXED_SCRIPT);
    let starts = statement_starts(&tokens);

    let first_keywords: Vec<Option<Keyword>> = starts.iter().map(|t| t.keyword()).collect();
    assert_eq!(
        first_keywords,
        [
            Some(Keyword::Get),
            Some(Keyword::HSet),
            Some(Keyword::Select),
            Some(Keyword::Insert),
            Some(Keyword::Vector),
            Some(Keyword::Match),
            Some(Keyword::Set),
            Some(Keyword::Del),
        ]
    );

    let models: Vec<Option<Model>> = starts
        .iter()
        .map(|t| t.keyword().and_then(Keyword::model))
        .collect();
    assert_eq!(
        models,
        [
            Some(Model::Keyval),
            Some(Model::Keyval),
            Some(Model::Rdbms),
            Some(Model::Rdbms),
            Some(Model::Vector),
            Some(Model::Graph),
            // SET has no single home model, even as a Redis command.
            None,
            Some(Model::Keyval),
        ]
    );

    let lines: Vec<(usize, usize)> = starts.iter().map(|t| (t.line, t.column)).collect();
    assert_eq!(
        lines,
        [
            (1, 1),
            (2, 1),
            (5, 1),
            (6, 1),
            (7, 1),
            (8, 1),
            (9, 1),
            (10, 1)
        ]
    );
}

#[test]
fn common_mixed_script_ends_kv_statements_at_newline_and_others_at_semicolon() {
    let tokens = lex(MIXED_SCRIPT);
    let count = |kind: T| tokens.iter().filter(|t| t.kind == kind).count();
    // GET, HSET, SET, DEL end at their newline.
    assert_eq!(count(T::Newline), 4);
    // SELECT, INSERT, VECTOR and MATCH end at their semicolon.
    assert_eq!(count(T::Semicolon), 4);
    // KV arguments are bare words and numbers; no `;` leaks out of them.
    let kv_args: Vec<(T, &str)> = tokens
        .iter()
        .skip(4)
        .take(6)
        .map(|t| (t.kind, t.value.as_str()))
        .collect();
    assert_eq!(
        kv_args,
        [
            word("profile:7"),
            word("name"),
            string("Ada Lovelace"),
            word("age"),
            int("36"),
            NEWLINE,
        ]
    );
}

#[test]
fn common_kv_statement_then_normal_statement_switches_mode() {
    expect(
        "GET a-b\nSELECT a-b FROM t",
        &[
            kw(Keyword::Get, "GET"),
            word("a-b"),
            NEWLINE,
            kw(Keyword::Select, "SELECT"),
            id("a"),
            MINUS,
            id("b"),
            kw(Keyword::From, "FROM"),
            id("t"),
        ],
    );
}

#[test]
fn common_normal_statement_then_kv_statement_switches_mode() {
    expect(
        "SELECT a-b FROM t;GET a-b\n",
        &[
            kw(Keyword::Select, "SELECT"),
            id("a"),
            MINUS,
            id("b"),
            kw(Keyword::From, "FROM"),
            id("t"),
            SEMI,
            kw(Keyword::Get, "GET"),
            word("a-b"),
            NEWLINE,
        ],
    );
    // Whitespace and newlines after the `;` are skipped before the first word.
    expect(
        "SELECT 1;\n\n  GET a:b\n",
        &[
            kw(Keyword::Select, "SELECT"),
            int("1"),
            SEMI,
            kw(Keyword::Get, "GET"),
            word("a:b"),
            NEWLINE,
        ],
    );
}

#[test]
fn common_kv_command_is_recognised_in_any_letter_case() {
    expect("get a:b", &[kw(Keyword::Get, "get"), word("a:b")]);
    expect(
        "HGetAll h:1",
        &[kw(Keyword::HGetAll, "HGetAll"), word("h:1")],
    );
}

#[test]
fn common_kv_command_needs_whitespace_or_end_after_it() {
    // Whitespace or the end of the input after the word means KV mode ...
    expect("GET\ta:b", &[kw(Keyword::Get, "GET"), word("a:b")]);
    expect(
        "GET\nSELECT 1",
        &[
            kw(Keyword::Get, "GET"),
            NEWLINE,
            kw(Keyword::Select, "SELECT"),
            int("1"),
        ],
    );
    expect("GET", &[kw(Keyword::Get, "GET")]);
    // ... anything else means normal mode.
    expect(
        "GET(a)",
        &[kw(Keyword::Get, "GET"), LPAREN, id("a"), RPAREN],
    );
    expect("SET=1", &[kw(Keyword::Set, "SET"), EQ, int("1")]);
    expect("GET-1", &[kw(Keyword::Get, "GET"), MINUS, int("1")]);
    // A longer word is not the command at all.
    expect("GETX a", &[id("GETX"), id("a")]);
}

#[test]
fn common_kv_command_names_in_the_middle_of_a_normal_statement_are_keywords() {
    expect(
        "SELECT get, del FROM t",
        &[
            kw(Keyword::Select, "SELECT"),
            kw(Keyword::Get, "get"),
            COMMA,
            kw(Keyword::Del, "del"),
            kw(Keyword::From, "FROM"),
            id("t"),
        ],
    );
    // `SET` mid-statement is the shared keyword, not a Redis command.
    expect(
        "UPDATE t SET x = 1;",
        &[
            kw(Keyword::Update, "UPDATE"),
            id("t"),
            kw(Keyword::Set, "SET"),
            id("x"),
            EQ,
            int("1"),
            SEMI,
        ],
    );
}

// ---------------------------------------------------------------------------
// The lexer never panics
// ---------------------------------------------------------------------------

/// Lex `input` and check what any outcome must satisfy. Merely returning
/// (instead of panicking) is the main assertion.
fn check_no_panic(input: &str) {
    match tokenize(input) {
        Ok(tokens) => {
            assert_eq!(
                tokens.last().map(|t| t.kind),
                Some(T::EndOfFile),
                "last token of {input:?}"
            );
            let mut previous = (1, 1);
            for token in &tokens {
                let position = (token.line, token.column);
                assert!(
                    position >= previous,
                    "{token:?} comes before an earlier token in {input:?}"
                );
                previous = position;
            }
        }
        Err(error) => {
            assert!(
                error.line >= 1 && error.column >= 1,
                "error position of {input:?} is not 1-based: {error:?}"
            );
            assert!(!error.message.is_empty(), "empty message for {input:?}");
        }
    }
}

/// A valid script touching every kind of token, in all four models.
const LONG_SCRIPT: &str = concat!(
    "GET user:1\r\n",
    "SET k 'v w' \"x\"\n",
    "HSET h f 1.5 -3 1e3 -inf\n",
    "\n",
    "// separator\n",
    "SELECT a, b FROM `my table` WHERE x >= 1.5e-3 AND y <> 'it''s' OR z != \"say \"\"hi\"\"\" LIMIT $1; // trailing\n",
    "/* block\ncomment */ MATCH (a:Person {name: 'Ada\\n', age: 36})-[r:KNOWS*1..3|LIKES]->(b)<-[:X]-(c)--(d)-->(e)<--(f)\n",
    "  WHERE a.age <= 18 AND NOT b.x RETURN count(*), a.`first name`, $name ORDER BY a.age DESC SKIP 5 LIMIT 10;\n",
    "SELECT id FROM docs ORDER BY emb <-> [0.1, 0.2], emb <#> [1, 2], emb <=> 'é日🦀' LIMIT 3;\n",
    "CREATE TABLE t (id INT64 PRIMARY KEY, v VECTOR, w TEXT);\n",
    "x % 2 + 3 * 4 / 5 - 6 | 7 :: 8 .. 9 < 1 > 2 = 3;\n",
    "DEL k\n",
    "// done",
);

/// The same, with a problem in nearly every statement.
const BROKEN_SCRIPT: &str = concat!(
    "GET 'a'b\n",
    "SELECT 12a, 1e, $, `` , `unterminated\n",
    "MATCH (n) WHERE n.x @ 3 # ~ ! ^ & ? \\ é 🦀\n",
    "'bad \\q escape' \"unterminated \\",
    "/* unterminated",
);

#[test]
fn common_long_script_is_valid() {
    // It lexes, and it uses every operator and every kind of literal, so the
    // prefix and suffix tests below exercise all of them.
    let tokens = lex(LONG_SCRIPT);
    let has = |kind: T| tokens.iter().any(|t| t.kind == kind);
    for &(text, kind) in OPERATORS {
        assert!(has(kind), "the script has no {text:?} ({kind})");
    }
    for kind in [
        T::Identifier,
        T::Word,
        T::Integer,
        T::Float,
        T::String,
        T::Parameter,
        T::Newline,
    ] {
        assert!(has(kind), "the script has no {kind}");
    }
    assert!(tokens.iter().any(|t| t.keyword().is_some()));
}

#[test]
fn common_no_panic_on_every_prefix_of_a_long_script() {
    let boundaries = LONG_SCRIPT
        .char_indices()
        .map(|(i, _)| i)
        .chain(iter::once(LONG_SCRIPT.len()));
    for end in boundaries {
        check_no_panic(&LONG_SCRIPT[..end]);
    }
}

#[test]
fn common_no_panic_on_every_suffix_of_a_long_script() {
    for (start, _) in LONG_SCRIPT.char_indices() {
        check_no_panic(&LONG_SCRIPT[start..]);
    }
}

#[test]
fn common_no_panic_on_every_prefix_and_suffix_of_a_broken_script() {
    let boundaries: Vec<usize> = BROKEN_SCRIPT
        .char_indices()
        .map(|(i, _)| i)
        .chain(iter::once(BROKEN_SCRIPT.len()))
        .collect();
    for &i in &boundaries {
        check_no_panic(&BROKEN_SCRIPT[..i]);
        check_no_panic(&BROKEN_SCRIPT[i..]);
    }
    assert!(tokenize(BROKEN_SCRIPT).is_err());
}

/// Inputs built around one character, to hit each place a lexer might look at
/// the next character and find the wrong thing.
fn around(c: char) -> Vec<String> {
    vec![
        format!("{c}"),
        format!("{c}{c}"),
        format!("a{c}"),
        format!("{c}a"),
        format!("1{c}"),
        format!("1.{c}"),
        format!("1e{c}"),
        format!("<{c}"),
        format!("-{c}"),
        format!("${c}"),
        format!("'{c}"),
        format!("'{c}'"),
        format!("\"{c}\""),
        format!("'\\{c}"),
        format!("'\\{c}'"),
        format!("`{c}"),
        format!("`{c}`"),
        format!("/*{c}"),
        format!("/*{c}*/"),
        format!("//{c}"),
        format!("GET {c}"),
        format!("GET '{c}'"),
        format!("GET '{c}' x"),
        format!("GET a{c}\nSELECT 1"),
    ]
}

#[test]
fn common_no_panic_on_every_ascii_character() {
    for byte in 0u8..=127 {
        for input in around(char::from(byte)) {
            check_no_panic(&input);
        }
    }
}

#[test]
fn common_single_ascii_characters_have_the_specified_outcome() {
    for byte in 0u8..=127 {
        let c = char::from(byte);
        let input = c.to_string();
        let result = tokenize(&input);
        if c.is_ascii_alphabetic() || c == '_' {
            let tokens = result.unwrap_or_else(|e| panic!("{input:?}: {e}"));
            assert_eq!(tokens[0].kind, T::Identifier, "{input:?}");
            assert_eq!(tokens[0].value, input, "{input:?}");
        } else if c.is_ascii_digit() {
            let tokens = result.unwrap_or_else(|e| panic!("{input:?}: {e}"));
            assert_eq!(tokens[0].kind, T::Integer, "{input:?}");
            assert_eq!(tokens[0].value, input, "{input:?}");
        } else if matches!(c, ' ' | '\t' | '\n' | '\r') {
            let tokens = result.unwrap_or_else(|e| panic!("{input:?}: {e}"));
            assert_eq!(tokens.len(), 1, "{input:?}");
        } else if let Some(&(_, kind)) = OPERATORS.iter().find(|(text, _)| *text == input) {
            let tokens = result.unwrap_or_else(|e| panic!("{input:?}: {e}"));
            assert_eq!(tokens[0].kind, kind, "{input:?}");
            assert_eq!(tokens.len(), 2, "{input:?}");
        } else if "!@#~^&?\\'\"`$".contains(c) {
            assert!(result.is_err(), "{input:?} should be an error");
        }
        // Remaining ASCII control characters: only "does not panic" is
        // required, which the call above already showed.
    }
}

#[test]
fn common_no_panic_on_non_ascii_characters() {
    let chars = [
        'é',
        'ñ',
        'ß',
        'Ω',
        '日',
        '→',
        '€',
        '🦀',
        '\u{0}',
        '\u{7f}',
        '\u{85}',
        '\u{a0}',
        '\u{200b}',
        '\u{2028}',
        '\u{301}',
        '\u{feff}',
        '\u{fffd}',
        '\u{10ffff}',
    ];
    for c in chars {
        for input in around(c) {
            check_no_panic(&input);
        }
    }
}

/// A small deterministic generator (Knuth's MMIX LCG), so the tests need no
/// external crates and every run sees the same inputs.
struct Lcg(u64);

impl Lcg {
    fn step(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("bound fits in u64");
        usize::try_from(self.step() % bound).expect("remainder fits in usize")
    }
}

#[test]
fn common_no_panic_on_pseudo_random_bytes() {
    let mut rng = Lcg(0x5eed);
    for _ in 0..3000 {
        let len = rng.below(96);
        let bytes: Vec<u8> = (0..len)
            .map(|_| u8::try_from(rng.step() & 0xff).expect("masked to a byte"))
            .collect();
        let input = String::from_utf8_lossy(&bytes);
        check_no_panic(&input);
    }
}

#[test]
fn common_no_panic_on_pseudo_random_token_soup() {
    const FRAGMENTS: &[&str] = &[
        "GET ", "SET ", "SELECT ", "MATCH ", " ", "\n", "\r\n", "\t", "'", "\"", "`", "$", "\\",
        "/*", "*/", "//", "--", "<", ">", "=", "-", "!", "(", ")", "[", "]", "{", "}", ",", ":",
        ";", ".", "..", "|", "+", "*", "/", "%", "1", "42", "3.14", "1e", "e", "E+", "a", "foo",
        "RETURN", "é", "日", "🦀", "#", "@", "?", "~", "^", "&", "<->", "<=>", "<#>", "->", "<-",
    ];
    let mut rng = Lcg(0xfeed_beef);
    for _ in 0..5000 {
        let count = rng.below(40);
        let input: String = (0..count)
            .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
            .collect();
        check_no_panic(&input);
    }
}

#[test]
fn common_no_panic_on_very_long_inputs() {
    let n = 50_000;
    let inputs = [
        "(".repeat(n),
        "'".repeat(n + 1),
        "'".repeat(n),
        "`".repeat(n + 1),
        "/*".repeat(n),
        "*/".repeat(n),
        "\\".repeat(n),
        "$".repeat(n),
        "<".repeat(n),
        "-".repeat(n),
        ".".repeat(n),
        "\n".repeat(n),
        "a ".repeat(n),
        "GET x\n".repeat(n),
        format!("'{}", "a".repeat(n)),
        format!("GET {}", "x".repeat(n)),
        format!("1{}", "e".repeat(n)),
    ];
    for input in &inputs {
        check_no_panic(input);
    }
}

#[test]
fn common_very_long_words_numbers_and_strings_are_single_tokens() {
    let n = 100_000;

    let word_text = "a".repeat(n);
    expect(&word_text, &[id(&word_text)]);

    // Numbers are not parsed, so their text can be any length.
    let number_text = "9".repeat(n);
    expect(&number_text, &[int(&number_text)]);

    let body = "x".repeat(n);
    expect(&format!("'{body}'"), &[string(&body)]);

    let blank_lines = "\n".repeat(n);
    assert_eq!(eof_position(&blank_lines), (n + 1, 1));
}

// ---------------------------------------------------------------------------
// A long run of characters never exhausts the stack (DD-003 "Details")
// ---------------------------------------------------------------------------

/// Long enough to overflow a test thread's 2 MiB stack in a debug build if
/// the lexer recursed once per character, as logos's code for `\S*`, `\s+` and
/// `[^\n]*` over multi-byte text does. Statements arrive from the network, so
/// this is a remote crash, not a curiosity.
const LONG_RUN: usize = 200_000;

/// Lex `input` in `model`, and return the tokens without their `EndOfFile`.
fn lex_run(input: &str, model: Model) -> Vec<Token> {
    let mut tokens = Lexer::for_model(input, model)
        .tokenize()
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(tokens.pop().map(|t| t.kind), Some(T::EndOfFile));
    tokens
}

#[test]
fn common_long_non_ascii_kv_word_is_one_word() {
    let word = "é".repeat(LONG_RUN);
    let tokens = lex_run(&format!("SET k {word}"), Model::Keyval);
    assert_eq!(tokens.len(), 3);
    assert_eq!(
        (tokens[2].kind, tokens[2].value.as_str()),
        (T::Word, word.as_str())
    );
}

#[test]
fn common_long_run_of_non_ascii_whitespace_separates_two_tokens() {
    // U+3000 is White_Space, so it is skipped like a space.
    let blanks = "\u{3000}".repeat(LONG_RUN);

    let tokens = lex_run(&format!("SELECT{blanks}1;"), Model::Rdbms);
    let shape: Vec<_> = tokens.iter().map(|t| t.kind).collect();
    assert_eq!(
        shape,
        [T::Keyword(Keyword::Select), T::Integer, T::Semicolon]
    );

    let tokens = lex_run(&format!("SET{blanks}k{blanks}v"), Model::Keyval);
    let words: Vec<_> = tokens.iter().map(|t| t.value.as_str()).collect();
    assert_eq!(words, ["SET", "k", "v"]);
}

#[test]
fn common_long_non_ascii_line_comment_is_skipped_up_to_the_newline() {
    let comment = "é".repeat(LONG_RUN);
    for model in [Model::Rdbms, Model::Vector, Model::Graph] {
        let tokens = lex_run(&format!("SELECT 1 // {comment}\n, 2;"), model);
        let values: Vec<_> = tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(values, ["SELECT", "1", ",", "2", ";"], "{model}");
        // The comment's line is counted: `, 2;` is on line 2.
        assert_eq!(tokens[2].line, 2, "{model}");
    }

    // A comment that ends the input needs no newline.
    let tokens = lex_run(&format!("SELECT 1; // {comment}"), Model::Rdbms);
    assert_eq!(tokens.len(), 3);
}

#[test]
fn common_long_runs_of_every_kind_lex_in_detecting_mode_too() {
    let word = "é".repeat(LONG_RUN);
    let blanks = "\u{2003}".repeat(LONG_RUN);
    // One script mixing models, as `Lexer::new` accepts.
    let script = format!("GET {word}\nSELECT{blanks}1; // {word}\nSET k v\n");
    let tokens = tokenize(&script).unwrap_or_else(|e| panic!("{e}"));
    let words: Vec<_> = tokens.iter().map(|t| t.value.as_str()).collect();
    assert_eq!(words.first().copied(), Some("GET"));
    assert_eq!(words.get(1).copied(), Some(word.as_str()));
    assert!(words.contains(&"SELECT"));
    assert_eq!(words.last().copied(), Some(""));
}
