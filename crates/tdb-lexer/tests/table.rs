//! Relational / ORM statements, lexed in normal mode.
//!
//! Every expectation is written from `docs/dd-003-query-lexer.md`, not from the
//! lexer's behaviour. Token streams are compared as `(kind, value)` pairs (the
//! final `EndOfFile` is checked by the helpers and then dropped), and positions
//! are asserted separately where they matter.

use tdb_lexer::{Keyword as K, LexError, Token, TokenType as T, tokenize};

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
const COMMA: Pair<'static> = (T::Comma, ",");
const SEMI: Pair<'static> = (T::Semicolon, ";");
const DOT: Pair<'static> = (T::Dot, ".");
const STAR: Pair<'static> = (T::Star, "*");
const SLASH: Pair<'static> = (T::Slash, "/");
const EQ: Pair<'static> = (T::Eq, "=");

const SELECT: Pair<'static> = (T::Keyword(K::Select), "SELECT");
const FROM: Pair<'static> = (T::Keyword(K::From), "FROM");
const WHERE: Pair<'static> = (T::Keyword(K::Where), "WHERE");
const INSERT: Pair<'static> = (T::Keyword(K::Insert), "INSERT");
const INTO: Pair<'static> = (T::Keyword(K::Into), "INTO");
const VALUES: Pair<'static> = (T::Keyword(K::Values), "VALUES");
const CREATE: Pair<'static> = (T::Keyword(K::Create), "CREATE");
const TABLE: Pair<'static> = (T::Keyword(K::Table), "TABLE");
const UPDATE: Pair<'static> = (T::Keyword(K::Update), "UPDATE");
const SET: Pair<'static> = (T::Keyword(K::Set), "SET");
const DELETE: Pair<'static> = (T::Keyword(K::Delete), "DELETE");
const AND: Pair<'static> = (T::Keyword(K::And), "AND");
const OR: Pair<'static> = (T::Keyword(K::Or), "OR");
const NOT: Pair<'static> = (T::Keyword(K::Not), "NOT");
const IN: Pair<'static> = (T::Keyword(K::In), "IN");
const IS: Pair<'static> = (T::Keyword(K::Is), "IS");
const NULL: Pair<'static> = (T::Keyword(K::Null), "NULL");
const TRUE: Pair<'static> = (T::Keyword(K::True), "TRUE");
const FALSE: Pair<'static> = (T::Keyword(K::False), "FALSE");
const ORDER: Pair<'static> = (T::Keyword(K::Order), "ORDER");
const BY: Pair<'static> = (T::Keyword(K::By), "BY");
const ASC: Pair<'static> = (T::Keyword(K::Asc), "ASC");
const DESC: Pair<'static> = (T::Keyword(K::Desc), "DESC");
const LIMIT: Pair<'static> = (T::Keyword(K::Limit), "LIMIT");
const OFFSET: Pair<'static> = (T::Keyword(K::Offset), "OFFSET");
const AS: Pair<'static> = (T::Keyword(K::As), "AS");
const PRIMARY: Pair<'static> = (T::Keyword(K::Primary), "PRIMARY");
const KEY: Pair<'static> = (T::Keyword(K::Key), "KEY");

// ------------------------------------------------------------ DDL

#[test]
fn table_create_table_with_every_column_type() {
    check(
        "CREATE TABLE t (a BOOL, b INT32, c INT64, d FLOAT32, e FLOAT64, f TEXT, g BYTES);",
        &[
            CREATE,
            TABLE,
            id("t"),
            LPAREN,
            id("a"),
            k(K::Bool, "BOOL"),
            COMMA,
            id("b"),
            k(K::Int32, "INT32"),
            COMMA,
            id("c"),
            k(K::Int64, "INT64"),
            COMMA,
            id("d"),
            k(K::Float32, "FLOAT32"),
            COMMA,
            id("e"),
            k(K::Float64, "FLOAT64"),
            COMMA,
            id("f"),
            k(K::Text, "TEXT"),
            COMMA,
            id("g"),
            k(K::Bytes, "BYTES"),
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn table_create_table_primary_key_inline_and_composite() {
    check(
        "CREATE TABLE users (id INT64 PRIMARY KEY, name TEXT, active BOOL);",
        &[
            CREATE,
            TABLE,
            id("users"),
            LPAREN,
            id("id"),
            k(K::Int64, "INT64"),
            PRIMARY,
            KEY,
            COMMA,
            id("name"),
            k(K::Text, "TEXT"),
            COMMA,
            id("active"),
            k(K::Bool, "BOOL"),
            RPAREN,
            SEMI,
        ],
    );
    check(
        "CREATE TABLE pairs (src INT64, dst INT64, PRIMARY KEY (src, dst));",
        &[
            CREATE,
            TABLE,
            id("pairs"),
            LPAREN,
            id("src"),
            k(K::Int64, "INT64"),
            COMMA,
            id("dst"),
            k(K::Int64, "INT64"),
            COMMA,
            PRIMARY,
            KEY,
            LPAREN,
            id("src"),
            COMMA,
            id("dst"),
            RPAREN,
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn table_create_index_on() {
    check(
        "CREATE INDEX idx_users_email ON users (email);",
        &[
            CREATE,
            k(K::Index, "INDEX"),
            id("idx_users_email"),
            k(K::On, "ON"),
            id("users"),
            LPAREN,
            id("email"),
            RPAREN,
            SEMI,
        ],
    );
    check(
        "create index by_age on users(age)",
        &[
            k(K::Create, "create"),
            k(K::Index, "index"),
            id("by_age"),
            k(K::On, "on"),
            id("users"),
            LPAREN,
            id("age"),
            RPAREN,
        ],
    );
}

#[test]
fn table_drop_table() {
    check(
        "DROP TABLE users;",
        &[k(K::Drop, "DROP"), TABLE, id("users"), SEMI],
    );
    check(
        "drop table users",
        &[k(K::Drop, "drop"), k(K::Table, "table"), id("users")],
    );
}

// ------------------------------------------------------------ INSERT

#[test]
fn table_insert_values_with_every_literal_kind() {
    check(
        "INSERT INTO users (id, name, score, active, bio) VALUES (1, 'alice', 9.5, TRUE, NULL);",
        &[
            INSERT,
            INTO,
            id("users"),
            LPAREN,
            id("id"),
            COMMA,
            id("name"),
            COMMA,
            id("score"),
            COMMA,
            id("active"),
            COMMA,
            id("bio"),
            RPAREN,
            VALUES,
            LPAREN,
            int("1"),
            COMMA,
            string("alice"),
            COMMA,
            float("9.5"),
            COMMA,
            TRUE,
            COMMA,
            NULL,
            RPAREN,
            SEMI,
        ],
    );
    check(
        "INSERT INTO flags VALUES (1, FALSE), (2, TRUE);",
        &[
            INSERT,
            INTO,
            id("flags"),
            VALUES,
            LPAREN,
            int("1"),
            COMMA,
            FALSE,
            RPAREN,
            COMMA,
            LPAREN,
            int("2"),
            COMMA,
            TRUE,
            RPAREN,
            SEMI,
        ],
    );
}

// ------------------------------------------------------------ SELECT

#[test]
fn table_select_star_and_qualified_columns() {
    check(
        "SELECT * FROM users;",
        &[SELECT, STAR, FROM, id("users"), SEMI],
    );
    check(
        "SELECT u.id, u.name FROM users u;",
        &[
            SELECT,
            id("u"),
            DOT,
            id("id"),
            COMMA,
            id("u"),
            DOT,
            id("name"),
            FROM,
            id("users"),
            id("u"),
            SEMI,
        ],
    );
}

#[test]
fn table_select_function_call_and_alias() {
    // COUNT is not in the keyword list, so it is a plain identifier.
    check(
        "SELECT COUNT(*) AS total FROM users;",
        &[
            SELECT,
            id("COUNT"),
            LPAREN,
            STAR,
            RPAREN,
            AS,
            id("total"),
            FROM,
            id("users"),
            SEMI,
        ],
    );
}

#[test]
fn table_where_comparison_operators_with_spaces() {
    let operators = [
        ("=", T::Eq),
        ("!=", T::NotEq),
        ("<>", T::NotEq),
        ("<", T::Lt),
        ("<=", T::LtEq),
        (">", T::Gt),
        (">=", T::GtEq),
    ];
    for (op, kind) in operators {
        check(
            &format!("SELECT * FROM t WHERE a {op} 1"),
            &[
                SELECT,
                STAR,
                FROM,
                id("t"),
                WHERE,
                id("a"),
                (kind, op),
                int("1"),
            ],
        );
    }
}

#[test]
fn table_where_comparison_operators_without_spaces() {
    let operators = [
        ("=", T::Eq),
        ("!=", T::NotEq),
        ("<>", T::NotEq),
        ("<", T::Lt),
        ("<=", T::LtEq),
        (">", T::Gt),
        (">=", T::GtEq),
    ];
    for (op, kind) in operators {
        check(
            &format!("WHERE a{op}1"),
            &[WHERE, id("a"), (kind, op), int("1")],
        );
    }
}

#[test]
fn table_where_and_or_not() {
    check(
        "SELECT * FROM t WHERE a = 1 AND b = 2 OR NOT c = 3;",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            WHERE,
            id("a"),
            EQ,
            int("1"),
            AND,
            id("b"),
            EQ,
            int("2"),
            OR,
            NOT,
            id("c"),
            EQ,
            int("3"),
            SEMI,
        ],
    );
}

#[test]
fn table_where_in_list() {
    check(
        "SELECT * FROM t WHERE id IN (1, 2, 3) AND kind NOT IN ('a', 'b');",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            WHERE,
            id("id"),
            IN,
            LPAREN,
            int("1"),
            COMMA,
            int("2"),
            COMMA,
            int("3"),
            RPAREN,
            AND,
            id("kind"),
            NOT,
            IN,
            LPAREN,
            string("a"),
            COMMA,
            string("b"),
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn table_where_is_null_and_is_not_null() {
    check(
        "SELECT * FROM t WHERE deleted_at IS NULL OR email IS NOT NULL;",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            WHERE,
            id("deleted_at"),
            IS,
            NULL,
            OR,
            id("email"),
            IS,
            NOT,
            NULL,
            SEMI,
        ],
    );
}

#[test]
fn table_order_by_asc_desc() {
    check(
        "SELECT * FROM t ORDER BY name ASC, id DESC;",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            ORDER,
            BY,
            id("name"),
            ASC,
            COMMA,
            id("id"),
            DESC,
            SEMI,
        ],
    );
}

#[test]
fn table_limit_and_offset() {
    check(
        "SELECT * FROM t LIMIT 10 OFFSET 20;",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            LIMIT,
            int("10"),
            OFFSET,
            int("20"),
            SEMI,
        ],
    );
}

// ------------------------------------------------------------ UPDATE / DELETE

#[test]
fn table_update_set_where() {
    // `SET` is a normal keyword here: the statement starts with UPDATE.
    check(
        "UPDATE users SET name = 'bob', score = score + 1 WHERE id = 7;",
        &[
            UPDATE,
            id("users"),
            SET,
            id("name"),
            EQ,
            string("bob"),
            COMMA,
            id("score"),
            EQ,
            id("score"),
            (T::Plus, "+"),
            int("1"),
            WHERE,
            id("id"),
            EQ,
            int("7"),
            SEMI,
        ],
    );
    // Not lexed as a KV-style bare word: `a=1,b=2;` splits into real tokens.
    check(
        "UPDATE t SET a=1,b=2;",
        &[
            UPDATE,
            id("t"),
            SET,
            id("a"),
            EQ,
            int("1"),
            COMMA,
            id("b"),
            EQ,
            int("2"),
            SEMI,
        ],
    );
}

#[test]
fn table_delete_from() {
    // The first word is `DELETE`, not the KV command `DEL`: normal mode.
    check(
        "DELETE FROM users WHERE id = 3;",
        &[
            DELETE,
            FROM,
            id("users"),
            WHERE,
            id("id"),
            EQ,
            int("3"),
            SEMI,
        ],
    );
    check(
        "delete from t where a=1;",
        &[
            k(K::Delete, "delete"),
            k(K::From, "from"),
            id("t"),
            k(K::Where, "where"),
            id("a"),
            EQ,
            int("1"),
            SEMI,
        ],
    );
}

#[test]
fn table_kv_keywords_inside_sql_are_plain_keywords() {
    // Only a statement's first word selects KV mode; elsewhere `GET` is a keyword.
    check(
        "SELECT get, ttl FROM t;",
        &[
            SELECT,
            k(K::Get, "get"),
            COMMA,
            k(K::Ttl, "ttl"),
            FROM,
            id("t"),
            SEMI,
        ],
    );
}

// ------------------------------------------------------------ parameters

#[test]
fn table_orm_select_with_named_parameters() {
    check(
        "SELECT id, name FROM users WHERE email = $email AND age >= $min_age \
         ORDER BY created_at DESC LIMIT $limit OFFSET $offset;",
        &[
            SELECT,
            id("id"),
            COMMA,
            id("name"),
            FROM,
            id("users"),
            WHERE,
            id("email"),
            EQ,
            param("email"),
            AND,
            id("age"),
            (T::GtEq, ">="),
            param("min_age"),
            ORDER,
            BY,
            id("created_at"),
            DESC,
            LIMIT,
            param("limit"),
            OFFSET,
            param("offset"),
            SEMI,
        ],
    );
}

#[test]
fn table_positional_parameters() {
    check(
        "SELECT * FROM users WHERE id = $1 AND age > $2 AND n = $10;",
        &[
            SELECT,
            STAR,
            FROM,
            id("users"),
            WHERE,
            id("id"),
            EQ,
            param("1"),
            AND,
            id("age"),
            (T::Gt, ">"),
            param("2"),
            AND,
            id("n"),
            EQ,
            param("10"),
            SEMI,
        ],
    );
}

#[test]
fn table_insert_and_update_with_parameters() {
    check(
        "INSERT INTO users (name, age) VALUES ($1, $2);",
        &[
            INSERT,
            INTO,
            id("users"),
            LPAREN,
            id("name"),
            COMMA,
            id("age"),
            RPAREN,
            VALUES,
            LPAREN,
            param("1"),
            COMMA,
            param("2"),
            RPAREN,
            SEMI,
        ],
    );
    check(
        "UPDATE users SET name = $name, age = $age WHERE id = $id;",
        &[
            UPDATE,
            id("users"),
            SET,
            id("name"),
            EQ,
            param("name"),
            COMMA,
            id("age"),
            EQ,
            param("age"),
            WHERE,
            id("id"),
            EQ,
            param("id"),
            SEMI,
        ],
    );
}

#[test]
fn table_parameter_position_is_the_dollar_sign() {
    let tokens = lex("x = $abc, y = $2");
    assert_eq!(
        located(&tokens),
        vec![
            at(id("x"), 1, 1),
            at(EQ, 1, 3),
            at(param("abc"), 1, 5),
            at(COMMA, 1, 9),
            at(id("y"), 1, 11),
            at(EQ, 1, 13),
            at(param("2"), 1, 15),
        ]
    );
}

// ------------------------------------------------------------ operators and punctuation

#[test]
fn table_arithmetic_operators() {
    let expected = [
        SELECT,
        id("a"),
        (T::Plus, "+"),
        id("b"),
        (T::Minus, "-"),
        id("c"),
        STAR,
        id("d"),
        SLASH,
        id("e"),
        (T::Percent, "%"),
        id("f"),
        FROM,
        id("t"),
        SEMI,
    ];
    check("SELECT a + b - c * d / e % f FROM t;", &expected);
    check("SELECT a+b-c*d/e%f FROM t;", &expected);
    // `*` is both the wildcard and multiplication; the lexer does not care.
    check(
        "SELECT *, price * qty FROM t",
        &[
            SELECT,
            STAR,
            COMMA,
            id("price"),
            STAR,
            id("qty"),
            FROM,
            id("t"),
        ],
    );
}

#[test]
fn table_parentheses_and_commas_nest() {
    check(
        "SELECT * FROM t WHERE (a = 1 OR (b = 2 AND c = 3)) AND d IN (4, 5);",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            WHERE,
            LPAREN,
            id("a"),
            EQ,
            int("1"),
            OR,
            LPAREN,
            id("b"),
            EQ,
            int("2"),
            AND,
            id("c"),
            EQ,
            int("3"),
            RPAREN,
            RPAREN,
            AND,
            id("d"),
            IN,
            LPAREN,
            int("4"),
            COMMA,
            int("5"),
            RPAREN,
            SEMI,
        ],
    );
}

// ------------------------------------------------------------ words

#[test]
fn table_keywords_in_any_case_keep_their_spelling() {
    check(
        "Select * fRoM users WhErE id = 1 oRdEr bY id dEsC LiMiT 1;",
        &[
            k(K::Select, "Select"),
            STAR,
            k(K::From, "fRoM"),
            id("users"),
            k(K::Where, "WhErE"),
            id("id"),
            EQ,
            int("1"),
            k(K::Order, "oRdEr"),
            k(K::By, "bY"),
            id("id"),
            k(K::Desc, "dEsC"),
            k(K::Limit, "LiMiT"),
            int("1"),
            SEMI,
        ],
    );
    check(
        "create table t (a bool, b Int32, c int64, d float32, e Float64, f text, g bYtEs);",
        &[
            k(K::Create, "create"),
            k(K::Table, "table"),
            id("t"),
            LPAREN,
            id("a"),
            k(K::Bool, "bool"),
            COMMA,
            id("b"),
            k(K::Int32, "Int32"),
            COMMA,
            id("c"),
            k(K::Int64, "int64"),
            COMMA,
            id("d"),
            k(K::Float32, "float32"),
            COMMA,
            id("e"),
            k(K::Float64, "Float64"),
            COMMA,
            id("f"),
            k(K::Text, "text"),
            COMMA,
            id("g"),
            k(K::Bytes, "bYtEs"),
            RPAREN,
            SEMI,
        ],
    );
    check(
        "select id from users where active = true or deleted = false;",
        &[
            k(K::Select, "select"),
            id("id"),
            k(K::From, "from"),
            id("users"),
            k(K::Where, "where"),
            id("active"),
            EQ,
            k(K::True, "true"),
            k(K::Or, "or"),
            id("deleted"),
            EQ,
            k(K::False, "false"),
            SEMI,
        ],
    );
}

#[test]
fn table_backtick_quoted_names_may_be_keywords_or_contain_spaces() {
    check(
        "SELECT `order`, `Table`, `select` FROM `from`",
        &[
            SELECT,
            id("order"),
            COMMA,
            id("Table"),
            COMMA,
            id("select"),
            FROM,
            id("from"),
        ],
    );
    check(
        "SELECT `first name`, `a-b.c` FROM `my table`",
        &[
            SELECT,
            id("first name"),
            COMMA,
            id("a-b.c"),
            FROM,
            id("my table"),
        ],
    );
    check(
        "CREATE TABLE `order` (`key` INT32);",
        &[
            CREATE,
            TABLE,
            id("order"),
            LPAREN,
            id("key"),
            k(K::Int32, "INT32"),
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn table_doubled_backtick_is_one_backtick() {
    check("SELECT `a``b` FROM t", &[SELECT, id("a`b"), FROM, id("t")]);
    check("SELECT `x```", &[SELECT, id("x`")]);
    // Four backticks: opening, one escaped backtick, closing.
    check("SELECT ```` FROM t", &[SELECT, id("`"), FROM, id("t")]);
}

#[test]
fn table_identifiers_with_digits_and_underscores() {
    check(
        "SELECT user_id2, _private, t1, col_1_a, __x__ FROM t1;",
        &[
            SELECT,
            id("user_id2"),
            COMMA,
            id("_private"),
            COMMA,
            id("t1"),
            COMMA,
            id("col_1_a"),
            COMMA,
            id("__x__"),
            FROM,
            id("t1"),
            SEMI,
        ],
    );
}

#[test]
fn table_words_that_merely_start_with_a_keyword_are_identifiers() {
    let words = [
        "selection",
        "order_id",
        "tables",
        "fromage",
        "inserted",
        "ascending",
        "nullable",
        "indexes",
        "primary_key",
        "keys",
        "valuesx",
        "text2",
        "int32x",
        "float64_total",
        "deleted",
        "setting",
        "getter",
        "or_x",
        "and_y",
        "in_stock",
        "isnull",
    ];
    for word in words {
        // On its own (so it is also the first word of its statement)...
        check(word, &[id(word)]);
        // ...and in the middle of one.
        check(
            &format!("SELECT {word} FROM t"),
            &[SELECT, id(word), FROM, id("t")],
        );
    }
}

#[test]
fn table_sql_words_outside_the_keyword_list_are_identifiers() {
    let words = [
        "JOIN", "GROUP", "HAVING", "LIKE", "BETWEEN", "UNIQUE", "DEFAULT", "DISTINCT", "INTEGER",
        "VARCHAR", "COUNT", "IF",
    ];
    for word in words {
        check(word, &[id(word)]);
    }
    // `IF` is an identifier, but `EXISTS` is a (KV) keyword wherever it appears.
    check(
        "CREATE TABLE IF NOT EXISTS users (id INT32);",
        &[
            CREATE,
            TABLE,
            id("IF"),
            NOT,
            k(K::Exists, "EXISTS"),
            id("users"),
            LPAREN,
            id("id"),
            k(K::Int32, "INT32"),
            RPAREN,
            SEMI,
        ],
    );
}

// ------------------------------------------------------------ unicode

#[test]
fn table_unicode_inside_string_literals() {
    check(
        "INSERT INTO t (name) VALUES ('héllo wörld 한국어 🎉', \"日本語\");",
        &[
            INSERT,
            INTO,
            id("t"),
            LPAREN,
            id("name"),
            RPAREN,
            VALUES,
            LPAREN,
            string("héllo wörld 한국어 🎉"),
            COMMA,
            string("日本語"),
            RPAREN,
            SEMI,
        ],
    );
}

#[test]
fn table_columns_count_characters_not_bytes() {
    let tokens = lex("INSERT INTO t VALUES ('한국어', 1);");
    assert_eq!(
        located(&tokens),
        vec![
            at(INSERT, 1, 1),
            at(INTO, 1, 8),
            at(id("t"), 1, 13),
            at(VALUES, 1, 15),
            at(LPAREN, 1, 22),
            at(string("한국어"), 1, 23),
            at(COMMA, 1, 28),
            at(int("1"), 1, 30),
            at(RPAREN, 1, 31),
            at(SEMI, 1, 32),
        ]
    );
    assert_eq!(eof_position(&tokens), (1, 33));

    // A 4-byte character is still one column.
    let tokens = lex("SELECT '🎉🎉', x");
    assert_eq!(
        located(&tokens),
        vec![
            at(SELECT, 1, 1),
            at(string("🎉🎉"), 1, 8),
            at(COMMA, 1, 12),
            at(id("x"), 1, 14),
        ]
    );
    assert_eq!(eof_position(&tokens), (1, 15));
}

#[test]
fn table_unicode_inside_quoted_names() {
    let tokens = lex("SELECT `이름`, `naïve col` FROM `테이블`");
    assert_eq!(
        located(&tokens),
        vec![
            at(SELECT, 1, 1),
            at(id("이름"), 1, 8),
            at(COMMA, 1, 12),
            at(id("naïve col"), 1, 14),
            at(FROM, 1, 26),
            at(id("테이블"), 1, 31),
        ]
    );
    assert_eq!(eof_position(&tokens), (1, 36));
}

// ------------------------------------------------------------ scripts

#[test]
fn table_semicolon_separated_script() {
    check(
        "CREATE TABLE t (id INT64); INSERT INTO t VALUES (1); SELECT * FROM t;",
        &[
            CREATE,
            TABLE,
            id("t"),
            LPAREN,
            id("id"),
            k(K::Int64, "INT64"),
            RPAREN,
            SEMI,
            INSERT,
            INTO,
            id("t"),
            VALUES,
            LPAREN,
            int("1"),
            RPAREN,
            SEMI,
            SELECT,
            STAR,
            FROM,
            id("t"),
            SEMI,
        ],
    );
}

#[test]
fn table_script_with_comments_and_blank_lines_between_statements() {
    let src = [
        "// schema",
        "CREATE TABLE t (id INT64);",
        "",
        "/* seed */",
        "INSERT INTO t VALUES (1);",
        "SELECT * FROM t;",
    ]
    .join("\n");
    let tokens = lex(&src);
    assert_eq!(
        shape(&tokens),
        vec![
            CREATE,
            TABLE,
            id("t"),
            LPAREN,
            id("id"),
            k(K::Int64, "INT64"),
            RPAREN,
            SEMI,
            INSERT,
            INTO,
            id("t"),
            VALUES,
            LPAREN,
            int("1"),
            RPAREN,
            SEMI,
            SELECT,
            STAR,
            FROM,
            id("t"),
            SEMI,
        ]
    );
    let position_of = |kind: T| {
        tokens
            .iter()
            .find(|t| t.kind == kind)
            .map(|t| (t.line, t.column))
    };
    assert_eq!(position_of(T::Keyword(K::Create)), Some((2, 1)));
    assert_eq!(position_of(T::Keyword(K::Insert)), Some((5, 1)));
    assert_eq!(position_of(T::Keyword(K::Select)), Some((6, 1)));
    assert_eq!(eof_position(&tokens), (6, 17));
}

#[test]
fn table_comments_inside_a_statement() {
    check(
        "SELECT /* all columns */ * FROM t // trailing\nWHERE a = 1;",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            WHERE,
            id("a"),
            EQ,
            int("1"),
            SEMI,
        ],
    );
    check(
        "SELECT a/* c */, b FROM t",
        &[SELECT, id("a"), COMMA, id("b"), FROM, id("t")],
    );
    // `//` starts a comment, a single `/` is division.
    check("SELECT 1 // 2", &[SELECT, int("1")]);
    check("SELECT 6 / 3", &[SELECT, int("6"), SLASH, int("3")]);
    // Block comments do not nest: the first `*/` closes it.
    check(
        "SELECT 1 /* a /* b */ c */",
        &[SELECT, int("1"), id("c"), STAR, SLASH],
    );
}

#[test]
fn table_double_dash_is_two_minus_tokens_not_a_comment() {
    let minus = (T::Minus, "-");
    check(
        "SELECT a --b FROM t",
        &[SELECT, id("a"), minus, minus, id("b"), FROM, id("t")],
    );
    check("SELECT 1--1", &[SELECT, int("1"), minus, minus, int("1")]);
}

// ------------------------------------------------------------ literals

#[test]
fn table_numbers_and_signs() {
    check(
        "SELECT 42, 3.14, 1e3, 1.5e-3, 007 FROM t",
        &[
            SELECT,
            int("42"),
            COMMA,
            float("3.14"),
            COMMA,
            float("1e3"),
            COMMA,
            float("1.5e-3"),
            COMMA,
            int("007"),
            FROM,
            id("t"),
        ],
    );
    // Signs are separate tokens.
    check(
        "WHERE a > -5 AND b = -2.5 AND c < +3",
        &[
            WHERE,
            id("a"),
            (T::Gt, ">"),
            (T::Minus, "-"),
            int("5"),
            AND,
            id("b"),
            EQ,
            (T::Minus, "-"),
            float("2.5"),
            AND,
            id("c"),
            (T::Lt, "<"),
            (T::Plus, "+"),
            int("3"),
        ],
    );
}

#[test]
fn table_string_quote_styles_and_escapes() {
    let cases: [(&str, &str); 13] = [
        ("'it''s'", "it's"),
        ("\"d\"\"d\"", "d\"d"),
        (r"'tab\there'", "tab\there"),
        (r"'a\\b'", "a\\b"),
        (r"'nl\nx'", "nl\nx"),
        (r"'cr\r'", "cr\r"),
        (r"'z\0'", "z\0"),
        (r"'q\'q'", "q'q"),
        (r#""say \"hi\"""#, "say \"hi\""),
        (r#"'say \"hi\"'"#, "say \"hi\""),
        ("\"it's\"", "it's"),
        ("'say \"hi\"'", "say \"hi\""),
        ("''", ""),
    ];
    for (literal, expected) in cases {
        check(&format!("SELECT {literal}"), &[SELECT, string(expected)]);
    }
}

#[test]
fn table_double_quoted_text_is_a_string_not_an_identifier() {
    check(
        "SELECT \"name\" FROM t WHERE a = \"x\"",
        &[
            SELECT,
            string("name"),
            FROM,
            id("t"),
            WHERE,
            id("a"),
            EQ,
            string("x"),
        ],
    );
}

#[test]
fn table_string_may_span_lines_and_shifts_later_positions() {
    let tokens = lex("INSERT INTO t VALUES ('a\nb', 1);");
    assert_eq!(
        located(&tokens),
        vec![
            at(INSERT, 1, 1),
            at(INTO, 1, 8),
            at(id("t"), 1, 13),
            at(VALUES, 1, 15),
            at(LPAREN, 1, 22),
            at(string("a\nb"), 1, 23),
            at(COMMA, 2, 3),
            at(int("1"), 2, 5),
            at(RPAREN, 2, 6),
            at(SEMI, 2, 7),
        ]
    );
}

#[test]
fn table_string_contents_are_opaque_to_the_lexer() {
    check(
        "SELECT * FROM t WHERE note = 'a; DROP TABLE t; -- x /* y */ // z';",
        &[
            SELECT,
            STAR,
            FROM,
            id("t"),
            WHERE,
            id("note"),
            EQ,
            string("a; DROP TABLE t; -- x /* y */ // z"),
            SEMI,
        ],
    );
}

// ------------------------------------------------------------ positions

#[test]
fn table_multi_line_select_has_correct_positions() {
    let src = ["SELECT id,", "       name", "FROM users", "WHERE id = 1;"].join("\n");
    let tokens = lex(&src);
    assert_eq!(
        located(&tokens),
        vec![
            at(SELECT, 1, 1),
            at(id("id"), 1, 8),
            at(COMMA, 1, 10),
            at(id("name"), 2, 8),
            at(FROM, 3, 1),
            at(id("users"), 3, 6),
            at(WHERE, 4, 1),
            at(id("id"), 4, 7),
            at(EQ, 4, 10),
            at(int("1"), 4, 12),
            at(SEMI, 4, 13),
        ]
    );
    assert_eq!(eof_position(&tokens), (4, 14));

    // After a trailing newline, EndOfFile is at the start of the next line.
    assert_eq!(eof_position(&lex("SELECT 1;\n")), (2, 1));
}

#[test]
fn table_a_tab_is_one_column() {
    let tokens = lex("SELECT\t*\tFROM\tt");
    assert_eq!(
        located(&tokens),
        vec![
            at(SELECT, 1, 1),
            at(STAR, 1, 8),
            at(FROM, 1, 10),
            at(id("t"), 1, 15),
        ]
    );
    assert_eq!(eof_position(&tokens), (1, 16));
}

#[test]
fn table_crlf_line_endings_are_whitespace() {
    let tokens = lex("SELECT 1\r\nFROM t\r\nWHERE a = 1;\r\n");
    assert_eq!(
        located(&tokens),
        vec![
            at(SELECT, 1, 1),
            at(int("1"), 1, 8),
            at(FROM, 2, 1),
            at(id("t"), 2, 6),
            at(WHERE, 3, 1),
            at(id("a"), 3, 7),
            at(EQ, 3, 9),
            at(int("1"), 3, 11),
            at(SEMI, 3, 12),
        ]
    );
    assert_eq!(eof_position(&tokens), (4, 1));
}

// ------------------------------------------------------------ errors

#[test]
fn table_unterminated_string_is_reported_at_the_opening_quote() {
    assert_error("INSERT INTO t VALUES ('abc", "unterminated string", 1, 23);
    assert_error("SELECT 1;\nSELECT 'oops", "unterminated string", 2, 8);
    assert_error("SELECT \"oops", "unterminated string", 1, 8);
    // The backslash escapes the closing quote, so the string never ends.
    assert_error(r"SELECT 'abc\'", "unterminated string", 1, 8);
}

#[test]
fn table_quoted_name_errors() {
    assert_error("SELECT `abc FROM t", "unterminated quoted name", 1, 8);
    assert_error("SELECT `` FROM t", "empty quoted name", 1, 8);
}

#[test]
fn table_bare_dollar_is_an_error_at_the_dollar() {
    let message = "expected a parameter name after '$'";
    assert_error("SELECT * FROM t WHERE id = $", message, 1, 28);
    assert_error("SELECT * FROM t WHERE id = $ 1", message, 1, 28);
    assert_error("SELECT ($)", message, 1, 9);
}

#[test]
fn table_unexpected_characters_are_errors() {
    // `?` is a common ORM placeholder, but this language only has `$1`/`$name`.
    for ch in ['?', '@', '#', '!', '~', '^', '&', '\\', 'é'] {
        assert_error(
            &format!("SELECT 1 {ch} 2"),
            &format!("unexpected character '{ch}'"),
            1,
            10,
        );
    }
    // The column counts characters, not bytes.
    assert_error("SELECT '한국어', ?", "unexpected character '?'", 1, 15);
}

#[test]
fn table_invalid_escape_is_reported_at_the_backslash() {
    assert_error(
        r"INSERT INTO t VALUES ('a\qb')",
        r"invalid escape '\q'",
        1,
        25,
    );
    // `\xHH` escapes are out of scope, so `\x` is invalid.
    assert_error(r"SELECT 'a\x41'", r"invalid escape '\x'", 1, 10);
}

#[test]
fn table_unterminated_block_comment_is_reported_at_the_opener() {
    assert_error("SELECT 1 /* oops", "unterminated comment", 1, 10);
}
