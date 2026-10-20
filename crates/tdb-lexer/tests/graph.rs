//! Cypher-style graph queries, lexed in normal mode (DD-003).
//!
//! Every expectation here is derived from the spec, not from the lexer's
//! current behaviour.

use tdb_lexer::{Keyword, LexError, Model, Token, TokenType as T, tokenize};

/// A token's kind and value.
type Pair<'a> = (T, &'a str);

const LPAREN: Pair<'static> = (T::LParen, "(");
const RPAREN: Pair<'static> = (T::RParen, ")");
const LBRACKET: Pair<'static> = (T::LBracket, "[");
const RBRACKET: Pair<'static> = (T::RBracket, "]");
const LBRACE: Pair<'static> = (T::LBrace, "{");
const RBRACE: Pair<'static> = (T::RBrace, "}");
const COLON: Pair<'static> = (T::Colon, ":");
const COMMA: Pair<'static> = (T::Comma, ",");
const DOT: Pair<'static> = (T::Dot, ".");
const DOTDOT: Pair<'static> = (T::DotDot, "..");
const PIPE: Pair<'static> = (T::Pipe, "|");
const MINUS: Pair<'static> = (T::Minus, "-");
const STAR: Pair<'static> = (T::Star, "*");
const ARROW: Pair<'static> = (T::Arrow, "->");
const LARROW: Pair<'static> = (T::LeftArrow, "<-");
const EQ: Pair<'static> = (T::Eq, "=");
const SEMI: Pair<'static> = (T::Semicolon, ";");

fn kw(keyword: Keyword, text: &str) -> Pair<'_> {
    (T::Keyword(keyword), text)
}

fn id(text: &str) -> Pair<'_> {
    (T::Identifier, text)
}

fn int(text: &str) -> Pair<'_> {
    (T::Integer, text)
}

fn string(text: &str) -> Pair<'_> {
    (T::String, text)
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

#[test]
fn graph_match_pattern_and_return() {
    expect(
        "MATCH (a:Person)-[:KNOWS]->(b) RETURN b.name;",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            COLON,
            id("Person"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("b"),
            DOT,
            id("name"),
            SEMI,
        ],
    );
}

#[test]
fn graph_node_with_properties() {
    expect(
        "(n:Person {name: 'Ada', age: 36})",
        &[
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            LBRACE,
            id("name"),
            COLON,
            string("Ada"),
            COMMA,
            id("age"),
            COLON,
            int("36"),
            RBRACE,
            RPAREN,
        ],
    );
}

#[test]
fn graph_relationship_with_variable_and_type() {
    expect(
        "MATCH (a)-[r:KNOWS]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            id("r"),
            COLON,
            id("KNOWS"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_incoming_relationship_uses_left_arrow() {
    expect(
        "MATCH (a)<-[:KNOWS]-(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            LARROW,
            LBRACKET,
            COLON,
            id("KNOWS"),
            RBRACKET,
            MINUS,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_bidirectional_relationship_in_brackets() {
    expect(
        "MATCH (a)<-[:KNOWS]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            LARROW,
            LBRACKET,
            COLON,
            id("KNOWS"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_undirected_edge_is_two_minus_tokens() {
    expect(
        "MATCH (a)--(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            MINUS,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_right_edge_without_brackets_is_minus_then_arrow() {
    expect(
        "MATCH (a)-->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_left_edge_without_brackets_is_left_arrow_then_minus() {
    expect(
        "MATCH (a)<--(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            LARROW,
            MINUS,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_double_headed_edge_without_brackets_is_left_arrow_then_arrow() {
    // `<-` first, then `->`: `<-->` has no `<--` or `<->` prefix to match.
    expect(
        "MATCH (a)<-->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            LARROW,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_adjacent_left_arrow_and_right_angle_is_l2_distance() {
    // Longest match first: `<->` is the Euclidean-distance operator even
    // between two node patterns.
    expect(
        "MATCH (a)<->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            (T::L2Distance, "<->"),
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_anonymous_nodes_and_relationship() {
    expect(
        "MATCH ()-[]->()",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            RPAREN,
            MINUS,
            LBRACKET,
            RBRACKET,
            ARROW,
            LPAREN,
            RPAREN,
        ],
    );
}

#[test]
fn graph_variable_length_with_both_bounds() {
    expect(
        "MATCH (a)-[:KNOWS*1..3]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            STAR,
            int("1"),
            DOTDOT,
            int("3"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_variable_length_with_upper_bound_only() {
    expect(
        "MATCH (a)-[:KNOWS*..5]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            STAR,
            DOTDOT,
            int("5"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_variable_length_with_lower_bound_only() {
    // `2.` is not a float: a `.` with no digit after it is not part of the
    // number, so `2..` is Integer, DotDot.
    expect(
        "MATCH (a)-[:KNOWS*2..]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            STAR,
            int("2"),
            DOTDOT,
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_variable_length_exact_and_unbounded() {
    expect(
        "MATCH (a)-[:KNOWS*3]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            STAR,
            int("3"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
    expect(
        "MATCH (a)-[*]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            STAR,
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_label_alternation_uses_pipe() {
    expect(
        "MATCH (a)-[:KNOWS|LIKES]->(b)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            PIPE,
            id("LIKES"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
        ],
    );
    expect(
        "[:A|B]",
        &[LBRACKET, COLON, id("A"), PIPE, id("B"), RBRACKET],
    );
}

#[test]
fn graph_node_with_several_labels() {
    expect(
        "MATCH (n:Person:Admin)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            COLON,
            id("Admin"),
            RPAREN,
        ],
    );
}

#[test]
fn graph_property_access_chain() {
    expect(
        "RETURN a.b.c.d",
        &[
            kw(Keyword::Return, "RETURN"),
            id("a"),
            DOT,
            id("b"),
            DOT,
            id("c"),
            DOT,
            id("d"),
        ],
    );
}

#[test]
fn graph_create_node_uses_shared_keyword() {
    expect(
        "CREATE (n:Person {name: \"x\"})",
        &[
            kw(Keyword::Create, "CREATE"),
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            LBRACE,
            id("name"),
            COLON,
            string("x"),
            RBRACE,
            RPAREN,
        ],
    );
    // CREATE is shared, so it does not say which model the statement is.
    assert_eq!(Keyword::Create.model(), None);
}

#[test]
fn graph_merge_with_parameters() {
    expect(
        "MERGE (a:Person {id: $id})-[:KNOWS]->(b:Person {id: $other})",
        &[
            kw(Keyword::Merge, "MERGE"),
            LPAREN,
            id("a"),
            COLON,
            id("Person"),
            LBRACE,
            id("id"),
            COLON,
            param("id"),
            RBRACE,
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            COLON,
            id("Person"),
            LBRACE,
            id("id"),
            COLON,
            param("other"),
            RBRACE,
            RPAREN,
        ],
    );
}

#[test]
fn graph_set_property_after_match() {
    // `SET` is only the Redis command at the start of a statement; here it is
    // just the shared keyword.
    expect(
        "MATCH (n:Person) SET n.age = 37",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            RPAREN,
            kw(Keyword::Set, "SET"),
            id("n"),
            DOT,
            id("age"),
            EQ,
            int("37"),
        ],
    );
}

#[test]
fn graph_detach_delete() {
    expect(
        "MATCH (n) DETACH DELETE n",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Detach, "DETACH"),
            kw(Keyword::Delete, "DELETE"),
            id("n"),
        ],
    );
}

#[test]
fn graph_return_with_order_by_skip_and_limit() {
    expect(
        "MATCH (n:Person) RETURN n.name AS name ORDER BY n.age DESC SKIP 10 LIMIT 5;",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("n"),
            DOT,
            id("name"),
            kw(Keyword::As, "AS"),
            id("name"),
            kw(Keyword::Order, "ORDER"),
            kw(Keyword::By, "BY"),
            id("n"),
            DOT,
            id("age"),
            kw(Keyword::Desc, "DESC"),
            kw(Keyword::Skip, "SKIP"),
            int("10"),
            kw(Keyword::Limit, "LIMIT"),
            int("5"),
            SEMI,
        ],
    );
}

#[test]
fn graph_where_with_comparison_and_not() {
    expect(
        "MATCH (a)-[:KNOWS]->(b) WHERE a.age >= 18 AND NOT b.blocked RETURN b",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            COLON,
            id("KNOWS"),
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("a"),
            DOT,
            id("age"),
            (T::GtEq, ">="),
            int("18"),
            kw(Keyword::And, "AND"),
            kw(Keyword::Not, "NOT"),
            id("b"),
            DOT,
            id("blocked"),
            kw(Keyword::Return, "RETURN"),
            id("b"),
        ],
    );
}

#[test]
fn graph_where_with_inequality_and_range_operators() {
    expect(
        "MATCH (a) WHERE a.x <> 1 AND a.y != 2 OR a.z <= 3 AND a.w < 4 AND a.v > 5 RETURN a",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("a"),
            DOT,
            id("x"),
            (T::NotEq, "<>"),
            int("1"),
            kw(Keyword::And, "AND"),
            id("a"),
            DOT,
            id("y"),
            (T::NotEq, "!="),
            int("2"),
            kw(Keyword::Or, "OR"),
            id("a"),
            DOT,
            id("z"),
            (T::LtEq, "<="),
            int("3"),
            kw(Keyword::And, "AND"),
            id("a"),
            DOT,
            id("w"),
            (T::Lt, "<"),
            int("4"),
            kw(Keyword::And, "AND"),
            id("a"),
            DOT,
            id("v"),
            (T::Gt, ">"),
            int("5"),
            kw(Keyword::Return, "RETURN"),
            id("a"),
        ],
    );
}

#[test]
fn graph_where_in_list_literal() {
    expect(
        "MATCH (a) WHERE a.name IN ['x','y'] RETURN a",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("a"),
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("a"),
            DOT,
            id("name"),
            kw(Keyword::In, "IN"),
            LBRACKET,
            string("x"),
            COMMA,
            string("y"),
            RBRACKET,
            kw(Keyword::Return, "RETURN"),
            id("a"),
        ],
    );
}

#[test]
fn graph_count_star() {
    expect(
        "MATCH (n) RETURN count(*)",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("count"),
            LPAREN,
            STAR,
            RPAREN,
        ],
    );
}

#[test]
fn graph_parameters_named_and_numbered() {
    expect(
        "MATCH (n:Person {id: $id}) WHERE n.age > $1 RETURN n",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            LBRACE,
            id("id"),
            COLON,
            param("id"),
            RBRACE,
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("n"),
            DOT,
            id("age"),
            (T::Gt, ">"),
            param("1"),
            kw(Keyword::Return, "RETURN"),
            id("n"),
        ],
    );
}

#[test]
fn graph_backtick_quoted_label_and_property() {
    expect_at(
        "MATCH (n:`my label`) RETURN n.`first name`",
        &[
            (T::Keyword(Keyword::Match), "MATCH", 1, 1),
            (T::LParen, "(", 1, 7),
            (T::Identifier, "n", 1, 8),
            (T::Colon, ":", 1, 9),
            (T::Identifier, "my label", 1, 10),
            (T::RParen, ")", 1, 20),
            (T::Keyword(Keyword::Return), "RETURN", 1, 22),
            (T::Identifier, "n", 1, 29),
            (T::Dot, ".", 1, 30),
            (T::Identifier, "first name", 1, 31),
        ],
        (1, 43),
    );
}

#[test]
fn graph_backtick_makes_keyword_and_doubled_backtick_names() {
    expect(
        "MATCH (n:`Match`) RETURN n.`a``b`",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            COLON,
            id("Match"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("n"),
            DOT,
            id("a`b"),
        ],
    );
}

#[test]
fn graph_multi_line_pattern_has_correct_positions() {
    expect_at(
        "MATCH (a:Person)\n  -[:KNOWS]->(b)\nRETURN b;",
        &[
            (T::Keyword(Keyword::Match), "MATCH", 1, 1),
            (T::LParen, "(", 1, 7),
            (T::Identifier, "a", 1, 8),
            (T::Colon, ":", 1, 9),
            (T::Identifier, "Person", 1, 10),
            (T::RParen, ")", 1, 16),
            (T::Minus, "-", 2, 3),
            (T::LBracket, "[", 2, 4),
            (T::Colon, ":", 2, 5),
            (T::Identifier, "KNOWS", 2, 6),
            (T::RBracket, "]", 2, 11),
            (T::Arrow, "->", 2, 12),
            (T::LParen, "(", 2, 14),
            (T::Identifier, "b", 2, 15),
            (T::RParen, ")", 2, 16),
            (T::Keyword(Keyword::Return), "RETURN", 3, 1),
            (T::Identifier, "b", 3, 8),
            (T::Semicolon, ";", 3, 9),
        ],
        (3, 10),
    );
}

#[test]
fn graph_line_comment_inside_query() {
    expect_at(
        "MATCH (n) // everything\nRETURN n",
        &[
            (T::Keyword(Keyword::Match), "MATCH", 1, 1),
            (T::LParen, "(", 1, 7),
            (T::Identifier, "n", 1, 8),
            (T::RParen, ")", 1, 9),
            (T::Keyword(Keyword::Return), "RETURN", 2, 1),
            (T::Identifier, "n", 2, 8),
        ],
        (2, 9),
    );
}

#[test]
fn graph_block_comment_inside_query() {
    expect_at(
        "MATCH /* all */ (n) RETURN n",
        &[
            (T::Keyword(Keyword::Match), "MATCH", 1, 1),
            (T::LParen, "(", 1, 17),
            (T::Identifier, "n", 1, 18),
            (T::RParen, ")", 1, 19),
            (T::Keyword(Keyword::Return), "RETURN", 1, 21),
            (T::Identifier, "n", 1, 28),
        ],
        (1, 29),
    );
}

#[test]
fn graph_block_comment_spanning_lines_moves_positions() {
    expect_at(
        "MATCH (n) /* a\nb */ RETURN n",
        &[
            (T::Keyword(Keyword::Match), "MATCH", 1, 1),
            (T::LParen, "(", 1, 7),
            (T::Identifier, "n", 1, 8),
            (T::RParen, ")", 1, 9),
            (T::Keyword(Keyword::Return), "RETURN", 2, 6),
            (T::Identifier, "n", 2, 13),
        ],
        (2, 14),
    );
}

#[test]
fn graph_comments_before_the_first_keyword_are_skipped() {
    let tokens = lex("// find all\n/* really */ MATCH (n) RETURN n");
    let first = &tokens[0];
    assert_eq!(first.keyword(), Some(Keyword::Match));
    assert_eq!((first.line, first.column), (2, 14));
    assert_eq!(first.value, "MATCH");
}

#[test]
fn graph_model_of_a_statement_is_discoverable_from_its_first_keyword() {
    assert_eq!(Keyword::Match.model(), Some(Model::Graph));

    let tokens = lex("MATCH (n) RETURN n");
    let model = tokens[0].keyword().and_then(Keyword::model);
    assert_eq!(model, Some(Model::Graph));
}

#[test]
fn graph_keywords_belong_to_the_graph_model_and_shared_ones_to_none() {
    for k in [
        Keyword::Match,
        Keyword::Return,
        Keyword::Detach,
        Keyword::Merge,
        Keyword::Skip,
    ] {
        assert_eq!(k.model(), Some(Model::Graph), "{k}");
    }
    for k in [
        Keyword::Set,
        Keyword::Create,
        Keyword::Delete,
        Keyword::Where,
        Keyword::Order,
        Keyword::By,
        Keyword::Limit,
        Keyword::As,
        Keyword::And,
        Keyword::Or,
        Keyword::Not,
        Keyword::In,
        Keyword::Is,
        Keyword::Null,
        Keyword::True,
        Keyword::False,
        Keyword::Asc,
        Keyword::Desc,
    ] {
        assert_eq!(k.model(), None, "{k}");
    }
}

#[test]
fn graph_keywords_are_case_insensitive_and_keep_their_spelling() {
    expect(
        "match (n) Return n",
        &[
            kw(Keyword::Match, "match"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Return, "Return"),
            id("n"),
        ],
    );
}

#[test]
fn graph_redis_command_names_inside_a_query_are_plain_keywords() {
    expect(
        "MATCH (n) WHERE n.ttl > 5 RETURN n.get",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("n"),
            DOT,
            kw(Keyword::Ttl, "ttl"),
            (T::Gt, ">"),
            int("5"),
            kw(Keyword::Return, "RETURN"),
            id("n"),
            DOT,
            kw(Keyword::Get, "get"),
        ],
    );
    // A reserved word becomes a name with backticks.
    expect(
        "RETURN n.`ttl`",
        &[kw(Keyword::Return, "RETURN"), id("n"), DOT, id("ttl")],
    );
}

#[test]
fn graph_cypher_words_outside_the_keyword_list_are_identifiers() {
    expect(
        "OPTIONAL MATCH (n) WITH n RETURN DISTINCT n",
        &[
            id("OPTIONAL"),
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            id("WITH"),
            id("n"),
            kw(Keyword::Return, "RETURN"),
            id("DISTINCT"),
            id("n"),
        ],
    );
}

#[test]
fn graph_left_arrow_swallows_a_minus_before_a_number() {
    expect(
        "MATCH (n) WHERE n.x<-1 RETURN n",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("n"),
            DOT,
            id("x"),
            LARROW,
            int("1"),
            kw(Keyword::Return, "RETURN"),
            id("n"),
        ],
    );
    // With spaces, it is a comparison against a negative number.
    expect(
        "MATCH (n) WHERE n.x < -1 RETURN n",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("n"),
            RPAREN,
            kw(Keyword::Where, "WHERE"),
            id("n"),
            DOT,
            id("x"),
            (T::Lt, "<"),
            MINUS,
            int("1"),
            kw(Keyword::Return, "RETURN"),
            id("n"),
        ],
    );
}

#[test]
fn graph_property_map_with_every_literal_kind() {
    expect(
        "CREATE (n:Person {active: TRUE, score: 2.5, nick: NULL, tags: ['a', 'b']})",
        &[
            kw(Keyword::Create, "CREATE"),
            LPAREN,
            id("n"),
            COLON,
            id("Person"),
            LBRACE,
            id("active"),
            COLON,
            kw(Keyword::True, "TRUE"),
            COMMA,
            id("score"),
            COLON,
            (T::Float, "2.5"),
            COMMA,
            id("nick"),
            COLON,
            kw(Keyword::Null, "NULL"),
            COMMA,
            id("tags"),
            COLON,
            LBRACKET,
            string("a"),
            COMMA,
            string("b"),
            RBRACKET,
            RBRACE,
            RPAREN,
        ],
    );
}

#[test]
fn graph_semicolon_inside_a_string_does_not_end_the_statement() {
    expect(
        "CREATE (n {bio: 'a;b'});",
        &[
            kw(Keyword::Create, "CREATE"),
            LPAREN,
            id("n"),
            LBRACE,
            id("bio"),
            COLON,
            string("a;b"),
            RBRACE,
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn graph_named_path_with_unbounded_relationship() {
    expect(
        "MATCH p = (a)-[*]->(b) RETURN p",
        &[
            kw(Keyword::Match, "MATCH"),
            id("p"),
            EQ,
            LPAREN,
            id("a"),
            RPAREN,
            MINUS,
            LBRACKET,
            STAR,
            RBRACKET,
            ARROW,
            LPAREN,
            id("b"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("p"),
        ],
    );
}

#[test]
fn graph_arithmetic_in_return() {
    expect(
        "RETURN n.age * 2 + n.bonus / 4 % 3 - 1",
        &[
            kw(Keyword::Return, "RETURN"),
            id("n"),
            DOT,
            id("age"),
            STAR,
            int("2"),
            (T::Plus, "+"),
            id("n"),
            DOT,
            id("bonus"),
            (T::Slash, "/"),
            int("4"),
            (T::Percent, "%"),
            int("3"),
            MINUS,
            int("1"),
        ],
    );
}

#[test]
fn graph_vector_distance_in_order_by() {
    expect(
        "MATCH (d:Doc) RETURN d ORDER BY d.emb <-> [0.1, 0.2] LIMIT 3",
        &[
            kw(Keyword::Match, "MATCH"),
            LPAREN,
            id("d"),
            COLON,
            id("Doc"),
            RPAREN,
            kw(Keyword::Return, "RETURN"),
            id("d"),
            kw(Keyword::Order, "ORDER"),
            kw(Keyword::By, "BY"),
            id("d"),
            DOT,
            id("emb"),
            (T::L2Distance, "<->"),
            LBRACKET,
            (T::Float, "0.1"),
            COMMA,
            (T::Float, "0.2"),
            RBRACKET,
            kw(Keyword::Limit, "LIMIT"),
            int("3"),
        ],
    );
}

#[test]
fn graph_unterminated_string_in_a_property_map() {
    expect_err("MATCH (n:Person {name: 'Ada)", "unterminated string", 1, 24);
}

#[test]
fn graph_unexpected_character_on_a_later_line() {
    expect_err(
        "MATCH (n)\nWHERE n.age @ 3",
        "unexpected character '@'",
        2,
        13,
    );
}
