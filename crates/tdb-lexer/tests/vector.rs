//! Vector search: distance operators, vector literals, `VECTOR(n)` columns and
//! number edge cases, lexed in normal mode.
//!
//! Every expectation is written from `docs/dd-003-query-lexer.md`, not from the
//! lexer's behaviour. Token streams are compared as `(kind, value)` pairs (the
//! final `EndOfFile` is checked by the helpers and then dropped), and positions
//! are asserted separately where they matter.

use tdb_lexer::{Keyword as K, LexError, Model, Token, TokenType as T, tokenize};

type Pair<'a> = (T, &'a str);
type Located<'a> = (T, &'a str, usize, usize);

// ---------------------------------------------------------------- helpers

fn lex(src: &str) -> Vec<Token> {
    match tokenize(src) {
        Ok(tokens) => tokens,
        Err(e) => panic!("lexing {src:?} failed: {e}"),
    }
}

fn lex_err(src: &str) -> LexError {
    match tokenize(src) {
        Ok(tokens) => panic!("expected {src:?} to fail, got {tokens:?}"),
        Err(e) => e,
    }
}

/// Kind and value of every token, without the trailing `EndOfFile`.
fn shape(tokens: &[Token]) -> Vec<Pair<'_>> {
    let (last, rest) = tokens.split_last().expect("a token stream is never empty");
    assert_eq!(last.kind, T::EndOfFile, "stream must end with EndOfFile");
    rest.iter().map(|t| (t.kind, t.value.as_str())).collect()
}

/// Kind, value, line and column of every token, without the trailing `EndOfFile`.
fn located(tokens: &[Token]) -> Vec<Located<'_>> {
    let (last, rest) = tokens.split_last().expect("a token stream is never empty");
    assert_eq!(last.kind, T::EndOfFile, "stream must end with EndOfFile");
    rest.iter()
        .map(|t| (t.kind, t.value.as_str(), t.line, t.column))
        .collect()
}

fn eof_position(tokens: &[Token]) -> (usize, usize) {
    let last = tokens.last().expect("a token stream is never empty");
    assert_eq!(last.kind, T::EndOfFile, "stream must end with EndOfFile");
    (last.line, last.column)
}

/// Lex `src` and compare the stream (minus `EndOfFile`) with `expected`.
fn check(src: &str, expected: &[Pair<'_>]) {
    let tokens = lex(src);
    assert_eq!(shape(&tokens), expected, "source: {src:?}");
}

fn assert_error(src: &str, message: &str, line: usize, column: usize) {
    let err = lex_err(src);
    assert_eq!(
        (err.message.as_str(), err.line, err.column),
        (message, line, column),
        "source: {src:?}"
    );
    assert_eq!(err.to_string(), format!("{message} at {line}:{column}"));
}

fn at(pair: Pair<'_>, line: usize, column: usize) -> Located<'_> {
    (pair.0, pair.1, line, column)
}

fn k(keyword: K, text: &str) -> Pair<'_> {
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

fn param(text: &str) -> Pair<'_> {
    (T::Parameter, text)
}

const LPAREN: Pair<'static> = (T::LParen, "(");
const RPAREN: Pair<'static> = (T::RParen, ")");
const LBRACKET: Pair<'static> = (T::LBracket, "[");
const RBRACKET: Pair<'static> = (T::RBracket, "]");
const COMMA: Pair<'static> = (T::Comma, ",");
const SEMI: Pair<'static> = (T::Semicolon, ";");
const DOT: Pair<'static> = (T::Dot, ".");
const DOTDOT: Pair<'static> = (T::DotDot, "..");
const MINUS: Pair<'static> = (T::Minus, "-");
const PLUS: Pair<'static> = (T::Plus, "+");
const LT: Pair<'static> = (T::Lt, "<");
const GT: Pair<'static> = (T::Gt, ">");
const EQ: Pair<'static> = (T::Eq, "=");

const L2: Pair<'static> = (T::L2Distance, "<->");
const COSINE: Pair<'static> = (T::CosineDistance, "<=>");
const INNER: Pair<'static> = (T::InnerProduct, "<#>");
const LEFT_ARROW: Pair<'static> = (T::LeftArrow, "<-");
const ARROW: Pair<'static> = (T::Arrow, "->");

const SELECT: Pair<'static> = (T::Keyword(K::Select), "SELECT");
const FROM: Pair<'static> = (T::Keyword(K::From), "FROM");
const WHERE: Pair<'static> = (T::Keyword(K::Where), "WHERE");
const ORDER: Pair<'static> = (T::Keyword(K::Order), "ORDER");
const BY: Pair<'static> = (T::Keyword(K::By), "BY");
const LIMIT: Pair<'static> = (T::Keyword(K::Limit), "LIMIT");
const AS: Pair<'static> = (T::Keyword(K::As), "AS");
const INSERT: Pair<'static> = (T::Keyword(K::Insert), "INSERT");
const INTO: Pair<'static> = (T::Keyword(K::Into), "INTO");
const VALUES: Pair<'static> = (T::Keyword(K::Values), "VALUES");

// ------------------------------------------------------------ distance operators

#[test]
fn vector_l2_distance_operator() {
    check("a <-> b", &[id("a"), L2, id("b")]);
}

#[test]
fn vector_cosine_distance_operator() {
    check("a <=> b", &[id("a"), COSINE, id("b")]);
}

#[test]
fn vector_inner_product_operator() {
    check("a <#> b", &[id("a"), INNER, id("b")]);
}

#[test]
fn vector_distance_operators_without_spaces() {
    check("a<->b", &[id("a"), L2, id("b")]);
    check("a<=>b", &[id("a"), COSINE, id("b")]);
    check("a<#>b", &[id("a"), INNER, id("b")]);
}

#[test]
fn vector_distance_operator_positions() {
    let tokens = lex("a<->b");
    assert_eq!(
        located(&tokens),
        vec![at(id("a"), 1, 1), at(L2, 1, 2), at(id("b"), 1, 5)]
    );
    assert_eq!(eof_position(&tokens), (1, 6));

    let tokens = lex("a <=> b");
    assert_eq!(
        located(&tokens),
        vec![at(id("a"), 1, 1), at(COSINE, 1, 3), at(id("b"), 1, 7)]
    );

    let tokens = lex("a <#> b");
    assert_eq!(
        located(&tokens),
        vec![at(id("a"), 1, 1), at(INNER, 1, 3), at(id("b"), 1, 7)]
    );
}

#[test]
fn vector_distance_operator_next_to_parameters_and_brackets() {
    check("$1<=>$2", &[param("1"), COSINE, param("2")]);
    check(
        "embedding<->[0.1]",
        &[id("embedding"), L2, LBRACKET, float("0.1"), RBRACKET],
    );
    check(
        "[1,2]<#>[3,4]",
        &[
            LBRACKET,
            int("1"),
            COMMA,
            int("2"),
            RBRACKET,
            INNER,
            LBRACKET,
            int("3"),
            COMMA,
            int("4"),
            RBRACKET,
        ],
    );
}

// ------------------------------------------------------------ longest match

#[test]
fn vector_left_arrow_needs_spaces_for_a_negative_comparison() {
    // Per the spec: `a<-1` is `a` `LeftArrow` `1`; write `a < -1` instead.
    check("a<-1", &[id("a"), LEFT_ARROW, int("1")]);
    check("a < -1", &[id("a"), LT, MINUS, int("1")]);
    // `<--` is `LeftArrow` `Minus`.
    check("x <-- y", &[id("x"), LEFT_ARROW, MINUS, id("y")]);
}

#[test]
fn vector_comparisons_are_not_mistaken_for_distance_operators() {
    let operators = [
        ("<=", T::LtEq),
        ("<>", T::NotEq),
        (">=", T::GtEq),
        ("<", T::Lt),
        (">", T::Gt),
    ];
    for (op, kind) in operators {
        check(&format!("a {op} b"), &[id("a"), (kind, op), id("b")]);
        check(&format!("a{op}b"), &[id("a"), (kind, op), id("b")]);
    }
}

#[test]
fn vector_split_cosine_operator_is_lteq_then_gt() {
    check("a <= > b", &[id("a"), (T::LtEq, "<="), GT, id("b")]);
    check("a <=  > b", &[id("a"), (T::LtEq, "<="), GT, id("b")]);
    // `=>` is not a token: it is `Eq` then `Gt`.
    check("a < => b", &[id("a"), LT, EQ, GT, id("b")]);
}

#[test]
fn vector_split_l2_operator_is_not_a_distance_operator() {
    check("a <- > b", &[id("a"), LEFT_ARROW, GT, id("b")]);
    check("a < -> b", &[id("a"), LT, ARROW, id("b")]);
    check("a < - > b", &[id("a"), LT, MINUS, GT, id("b")]);
}

#[test]
fn vector_split_inner_product_operator_is_an_error() {
    // There is no `<#` token: `<` lexes, then `#` is an unexpected character.
    let message = "unexpected character '#'";
    assert_error("a <# > b", message, 1, 4);
    assert_error("a < #> b", message, 1, 5);
    assert_error("a<#b", message, 1, 3);
}

#[test]
fn vector_distance_operator_followed_by_another_operator_character() {
    // Longest match first: the three-character operator wins, the rest lexes on.
    check("a<=>=b", &[id("a"), COSINE, EQ, id("b")]);
    check("a<=>>b", &[id("a"), COSINE, GT, id("b")]);
    check("a<->>b", &[id("a"), L2, GT, id("b")]);
    check("a<#>>b", &[id("a"), INNER, GT, id("b")]);
    check("a<=><=>b", &[id("a"), COSINE, COSINE, id("b")]);
}

#[test]
fn vector_distance_compared_with_each_operator() {
    let distances = [
        ("<->", T::L2Distance),
        ("<=>", T::CosineDistance),
        ("<#>", T::InnerProduct),
    ];
    let comparisons = [
        ("<", T::Lt),
        ("<=", T::LtEq),
        (">", T::Gt),
        (">=", T::GtEq),
        ("=", T::Eq),
        ("<>", T::NotEq),
        ("!=", T::NotEq),
    ];
    for (dist, dist_kind) in distances {
        for (cmp, cmp_kind) in comparisons {
            let expected = [
                id("embedding"),
                (dist_kind, dist),
                param("1"),
                (cmp_kind, cmp),
                float("0.5"),
            ];
            check(&format!("embedding {dist} $1 {cmp} 0.5"), &expected);
            check(&format!("embedding{dist}$1{cmp}0.5"), &expected);
        }
    }
}

// ------------------------------------------------------------ vector literals

#[test]
fn vector_literal_of_floats() {
    check(
        "[0.1, 0.2, 0.3]",
        &[
            LBRACKET,
            float("0.1"),
            COMMA,
            float("0.2"),
            COMMA,
            float("0.3"),
            RBRACKET,
        ],
    );
    // Values are kept exactly as written.
    check(
        "[0.10, 1.50]",
        &[LBRACKET, float("0.10"), COMMA, float("1.50"), RBRACKET],
    );
}

#[test]
fn vector_literal_without_spaces() {
    check(
        "[0.1,0.2,0.3]",
        &[
            LBRACKET,
            float("0.1"),
            COMMA,
            float("0.2"),
            COMMA,
            float("0.3"),
            RBRACKET,
        ],
    );
}

#[test]
fn vector_literal_with_negative_components() {
    check(
        "[-0.5, 0.25, -1]",
        &[
            LBRACKET,
            MINUS,
            float("0.5"),
            COMMA,
            float("0.25"),
            COMMA,
            MINUS,
            int("1"),
            RBRACKET,
        ],
    );
    check(
        "[-1,-2.5]",
        &[
            LBRACKET,
            MINUS,
            int("1"),
            COMMA,
            MINUS,
            float("2.5"),
            RBRACKET,
        ],
    );
}

#[test]
fn vector_literal_of_integers() {
    check(
        "[1, 2, 3]",
        &[
            LBRACKET,
            int("1"),
            COMMA,
            int("2"),
            COMMA,
            int("3"),
            RBRACKET,
        ],
    );
    check("[0]", &[LBRACKET, int("0"), RBRACKET]);
}

#[test]
fn vector_literal_with_scientific_notation() {
    check(
        "[1e-3, 2.5E+10, 1e3]",
        &[
            LBRACKET,
            float("1e-3"),
            COMMA,
            float("2.5E+10"),
            COMMA,
            float("1e3"),
            RBRACKET,
        ],
    );
    check(
        "[1E5, 1.5e2, 3e+2]",
        &[
            LBRACKET,
            float("1E5"),
            COMMA,
            float("1.5e2"),
            COMMA,
            float("3e+2"),
            RBRACKET,
        ],
    );
}

#[test]
fn vector_exponent_sign_belongs_to_the_number() {
    // `1e-3` is one Float; without the `e` the `-` is a separate Minus.
    check("1e-3", &[float("1e-3")]);
    check("1-3", &[int("1"), MINUS, int("3")]);
    check("1e3-3", &[float("1e3"), MINUS, int("3")]);
}

#[test]
fn vector_empty_literal() {
    check("[]", &[LBRACKET, RBRACKET]);
    check("[ ]", &[LBRACKET, RBRACKET]);
}

#[test]
fn vector_literal_with_128_components() {
    let components: Vec<String> = (0..128).map(|i| format!("{i}.{i}")).collect();
    let literal = format!("[{}]", components.join(", "));

    let tokens = lex(&literal);
    // `[` + 128 floats + 127 commas + `]` + EndOfFile.
    assert_eq!(tokens.len(), 1 + 128 + 127 + 1 + 1);
    assert_eq!(tokens[0].kind, T::LBracket);
    assert_eq!(tokens[tokens.len() - 2].kind, T::RBracket);
    let floats: Vec<&str> = tokens
        .iter()
        .filter(|t| t.kind == T::Float)
        .map(|t| t.value.as_str())
        .collect();
    assert_eq!(floats, components);
    let commas = tokens.iter().filter(|t| t.kind == T::Comma).count();
    assert_eq!(commas, 127);
    // The closing bracket is the last character of the single line.
    let close = &tokens[tokens.len() - 2];
    assert_eq!((close.line, close.column), (1, literal.chars().count()));

    // The same literal inside a statement: 8 tokens before the vector,
    // 257 for the vector, then `LIMIT 10 ;` and EndOfFile.
    let query = format!("SELECT id FROM docs ORDER BY embedding <-> {literal} LIMIT 10;");
    let tokens = lex(&query);
    assert_eq!(tokens.len(), 8 + 257 + 3 + 1);
    assert_eq!(tokens[7].kind, T::L2Distance);
    assert_eq!(tokens[8].kind, T::LBracket);
    assert_eq!(tokens[8 + 256].kind, T::RBracket);
    assert_eq!(tokens[8 + 257].kind, T::Keyword(K::Limit));
}

#[test]
fn vector_brackets_are_not_balanced_by_the_lexer() {
    // The lexer never decides what a statement means, so an unclosed or
    // unopened bracket is not its problem.
    check("[0.1, 0.2", &[LBRACKET, float("0.1"), COMMA, float("0.2")]);
    check("]]", &[RBRACKET, RBRACKET]);
    check(
        "[[1, 2], [3, 4]]",
        &[
            LBRACKET,
            LBRACKET,
            int("1"),
            COMMA,
            int("2"),
            RBRACKET,
            COMMA,
            LBRACKET,
            int("3"),
            COMMA,
            int("4"),
            RBRACKET,
            RBRACKET,
        ],
    );
}

// ------------------------------------------------------------ VECTOR(n) columns

#[test]
fn vector_column_type_with_a_small_dimension() {
    assert_eq!(K::Vector.model(), Some(Model::Vector));
    check(
        "CREATE TABLE docs (id INT64 PRIMARY KEY, embedding VECTOR(3));",
        &[
            k(K::Create, "CREATE"),
            k(K::Table, "TABLE"),
            id("docs"),
            LPAREN,
            id("id"),
            k(K::Int64, "INT64"),
            k(K::Primary, "PRIMARY"),
            k(K::Key, "KEY"),
            COMMA,
            id("embedding"),
            k(K::Vector, "VECTOR"),
            LPAREN,
            int("3"),
            RPAREN,
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn vector_column_type_with_768_dimensions_in_any_case() {
    check(
        "embedding VECTOR(768)",
        &[
            id("embedding"),
            k(K::Vector, "VECTOR"),
            LPAREN,
            int("768"),
            RPAREN,
        ],
    );
    check(
        "embedding vector(768)",
        &[
            id("embedding"),
            k(K::Vector, "vector"),
            LPAREN,
            int("768"),
            RPAREN,
        ],
    );
    check(
        "embedding Vector( 1536 )",
        &[
            id("embedding"),
            k(K::Vector, "Vector"),
            LPAREN,
            int("1536"),
            RPAREN,
        ],
    );
}

// ------------------------------------------------------------ parameters

#[test]
fn vector_parameter_as_the_query_vector() {
    check("embedding <-> $1", &[id("embedding"), L2, param("1")]);
    check(
        "embedding <-> $query_vec",
        &[id("embedding"), L2, param("query_vec")],
    );
}

#[test]
fn vector_parameter_with_each_distance_operator() {
    let distances = [
        ("<->", T::L2Distance),
        ("<=>", T::CosineDistance),
        ("<#>", T::InnerProduct),
    ];
    for (op, kind) in distances {
        check(
            &format!("embedding {op} $q"),
            &[id("embedding"), (kind, op), param("q")],
        );
        check(
            &format!("$q {op} embedding"),
            &[param("q"), (kind, op), id("embedding")],
        );
    }
}

// ------------------------------------------------------------ full statements

#[test]
fn vector_nearest_neighbour_select() {
    check(
        "SELECT id FROM docs ORDER BY embedding <-> [0.1, 0.2] LIMIT 5;",
        &[
            SELECT,
            id("id"),
            FROM,
            id("docs"),
            ORDER,
            BY,
            id("embedding"),
            L2,
            LBRACKET,
            float("0.1"),
            COMMA,
            float("0.2"),
            RBRACKET,
            LIMIT,
            int("5"),
            SEMI,
        ],
    );
}

#[test]
fn vector_cosine_distance_filter_in_where() {
    check(
        "SELECT id FROM docs WHERE embedding <=> $q < 0.3;",
        &[
            SELECT,
            id("id"),
            FROM,
            id("docs"),
            WHERE,
            id("embedding"),
            COSINE,
            param("q"),
            LT,
            float("0.3"),
            SEMI,
        ],
    );
    check(
        "SELECT id FROM docs WHERE embedding <=> $1 < 0.3 AND lang = 'en' \
         ORDER BY embedding <=> $1 LIMIT 10;",
        &[
            SELECT,
            id("id"),
            FROM,
            id("docs"),
            WHERE,
            id("embedding"),
            COSINE,
            param("1"),
            LT,
            float("0.3"),
            k(K::And, "AND"),
            id("lang"),
            EQ,
            string("en"),
            ORDER,
            BY,
            id("embedding"),
            COSINE,
            param("1"),
            LIMIT,
            int("10"),
            SEMI,
        ],
    );
}

#[test]
fn vector_distance_in_the_select_list() {
    check(
        "SELECT id, embedding <-> $1 AS dist FROM docs ORDER BY dist LIMIT 3;",
        &[
            SELECT,
            id("id"),
            COMMA,
            id("embedding"),
            L2,
            param("1"),
            AS,
            id("dist"),
            FROM,
            id("docs"),
            ORDER,
            BY,
            id("dist"),
            LIMIT,
            int("3"),
            SEMI,
        ],
    );
    check(
        "SELECT 1 - (embedding <=> $1) AS similarity FROM docs;",
        &[
            SELECT,
            int("1"),
            MINUS,
            LPAREN,
            id("embedding"),
            COSINE,
            param("1"),
            RPAREN,
            AS,
            id("similarity"),
            FROM,
            id("docs"),
            SEMI,
        ],
    );
}

#[test]
fn vector_insert_with_a_vector_literal() {
    check(
        "INSERT INTO docs (id, embedding) VALUES (1, [0.1, 0.2, 0.3]);",
        &[
            INSERT,
            INTO,
            id("docs"),
            LPAREN,
            id("id"),
            COMMA,
            id("embedding"),
            RPAREN,
            VALUES,
            LPAREN,
            int("1"),
            COMMA,
            LBRACKET,
            float("0.1"),
            COMMA,
            float("0.2"),
            COMMA,
            float("0.3"),
            RBRACKET,
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn vector_text_form_literal_is_a_single_string() {
    check(
        "INSERT INTO docs VALUES (1, '[0.1,0.2,0.3]');",
        &[
            INSERT,
            INTO,
            id("docs"),
            VALUES,
            LPAREN,
            int("1"),
            COMMA,
            string("[0.1,0.2,0.3]"),
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn vector_multi_line_query_has_correct_positions() {
    let src = [
        "SELECT id",
        "FROM docs",
        "ORDER BY embedding <-> [0.1, 0.2]",
        "LIMIT 5;",
    ]
    .join("\n");
    let tokens = lex(&src);
    assert_eq!(
        located(&tokens),
        vec![
            at(SELECT, 1, 1),
            at(id("id"), 1, 8),
            at(FROM, 2, 1),
            at(id("docs"), 2, 6),
            at(ORDER, 3, 1),
            at(BY, 3, 7),
            at(id("embedding"), 3, 10),
            at(L2, 3, 20),
            at(LBRACKET, 3, 24),
            at(float("0.1"), 3, 25),
            at(COMMA, 3, 28),
            at(float("0.2"), 3, 30),
            at(RBRACKET, 3, 33),
            at(LIMIT, 4, 1),
            at(int("5"), 4, 7),
            at(SEMI, 4, 8),
        ]
    );
    assert_eq!(eof_position(&tokens), (4, 9));
}

// ------------------------------------------------------------ number edge cases

#[test]
fn vector_trailing_dot_is_an_integer_then_dot() {
    check("1.", &[int("1"), DOT]);
    check(
        "[1., 2.]",
        &[LBRACKET, int("1"), DOT, COMMA, int("2"), DOT, RBRACKET],
    );
}

#[test]
fn vector_double_dot_between_integers_is_a_range() {
    check("1..3", &[int("1"), DOTDOT, int("3")]);
    check("1.5..3", &[float("1.5"), DOTDOT, int("3")]);
}

#[test]
fn vector_leading_dot_is_a_dot_then_an_integer() {
    check(".5", &[DOT, int("5")]);
    check(
        "[.5, .25]",
        &[LBRACKET, DOT, int("5"), COMMA, DOT, int("25"), RBRACKET],
    );
}

#[test]
fn vector_sign_is_a_separate_token() {
    check("+1.5", &[PLUS, float("1.5")]);
    check("-1e-3", &[MINUS, float("1e-3")]);
    check("- 1", &[MINUS, int("1")]);
}

#[test]
fn vector_exponent_without_digits_is_an_error() {
    // The `e` is the offending character, so the message shows the number
    // text before it plus the `e`.
    assert_error("1e", "invalid number '1e'", 1, 1);
    assert_error("1E", "invalid number '1E'", 1, 1);
    assert_error("1.5e", "invalid number '1.5e'", 1, 1);
    assert_error("[1e, 2]", "invalid number '1e'", 1, 2);
}

#[test]
fn vector_number_followed_by_a_letter_or_underscore_is_an_error() {
    assert_error("12abc", "invalid number '12a'", 1, 1);
    assert_error("x = 12abc", "invalid number '12a'", 1, 5);
    assert_error("1.5abc", "invalid number '1.5a'", 1, 1);
    assert_error("1e3x", "invalid number '1e3x'", 1, 1);
    assert_error("3x", "invalid number '3x'", 1, 1);
    assert_error("1_000", "invalid number '1_'", 1, 1);
    // Hex literals are out of scope, so they are just a number followed by a letter.
    assert_error("0x1F", "invalid number '0x'", 1, 1);
    assert_error("[1, 2x]", "invalid number '2x'", 1, 5);
}
