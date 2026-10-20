use tdb_lexer::{LexError, Lexer, Token, TokenType};

fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(input).tokenize()
}

fn kinds(tokens: &[Token]) -> Vec<TokenType> {
    tokens.iter().map(|t| t.kind).collect()
}

fn values(tokens: &[Token]) -> Vec<&str> {
    tokens.iter().map(|t| t.value.as_str()).collect()
}

#[test]
fn lexer_tokenizes_a_set_command() {
    let tokens = lex("set name = hello\n").unwrap();
    assert_eq!(
        kinds(&tokens),
        [
            TokenType::Set,
            TokenType::Identifier,
            TokenType::Equal,
            TokenType::Identifier,
            TokenType::EndOfFile,
        ]
    );
    assert_eq!(values(&tokens), ["set", "name", "=", "hello", ""]);
}

#[test]
fn lexer_empty_input_is_just_end_of_file() {
    let tokens = lex("").unwrap();
    assert_eq!(kinds(&tokens), [TokenType::EndOfFile]);
    assert_eq!((tokens[0].line, tokens[0].column), (1, 1));
}

#[test]
fn lexer_recognizes_all_keywords() {
    let tokens = lex("set get del").unwrap();
    assert_eq!(
        kinds(&tokens),
        [
            TokenType::Set,
            TokenType::Get,
            TokenType::Del,
            TokenType::EndOfFile
        ]
    );
}

#[test]
fn lexer_keywords_are_lowercase_only() {
    let tokens = lex("SET Get").unwrap();
    assert_eq!(kinds(&tokens)[..2], [TokenType::Identifier; 2]);
}

#[test]
fn lexer_keyword_prefix_is_still_an_identifier() {
    let tokens = lex("settings get_all del2").unwrap();
    assert_eq!(kinds(&tokens)[..3], [TokenType::Identifier; 3]);
    assert_eq!(values(&tokens)[..3], ["settings", "get_all", "del2"]);
}

#[test]
fn lexer_reads_numbers() {
    let tokens = lex("42 -7 3.25 -0.5").unwrap();
    assert_eq!(kinds(&tokens)[..4], [TokenType::Number; 4]);
    assert_eq!(values(&tokens)[..4], ["42", "-7", "3.25", "-0.5"]);
}

#[test]
fn lexer_number_takes_only_one_decimal_point() {
    let err = lex("1.2.3").unwrap_err();
    // "1.2" lexes fine; the second '.' cannot start a token.
    assert_eq!(err.message, "unexpected character '.'");
    assert_eq!((err.line, err.column), (1, 4));
}

#[test]
fn lexer_reads_quoted_strings_without_the_quotes() {
    let tokens = lex("set k = \"a b = c\"").unwrap();
    assert_eq!(tokens[3].kind, TokenType::String);
    assert_eq!(tokens[3].value, "a b = c");
}

#[test]
fn lexer_empty_string_is_a_string_token() {
    let tokens = lex("\"\"").unwrap();
    assert_eq!(tokens[0].kind, TokenType::String);
    assert_eq!(tokens[0].value, "");
}

#[test]
fn lexer_tracks_line_and_column_of_each_token() {
    let tokens = lex("set a = 1\n  get b").unwrap();
    let positions: Vec<_> = tokens.iter().map(|t| (t.line, t.column)).collect();
    assert_eq!(
        positions,
        [(1, 1), (1, 5), (1, 7), (1, 9), (2, 3), (2, 7), (2, 8)]
    );
}

#[test]
fn lexer_counts_lines_inside_strings() {
    let tokens = lex("\"a\nb\" x").unwrap();
    assert_eq!(tokens[0].value, "a\nb");
    assert_eq!((tokens[1].line, tokens[1].column), (2, 4));
}

#[test]
fn lexer_rejects_unexpected_characters() {
    let err = lex("set k = $").unwrap_err();
    assert_eq!(err.message, "unexpected character '$'");
    assert_eq!((err.line, err.column), (1, 9));
    assert_eq!(err.to_string(), "unexpected character '$' at 1:9");
}

#[test]
fn lexer_rejects_a_lone_minus() {
    let err = lex("set k = -").unwrap_err();
    assert_eq!(err.message, "unexpected character '-'");
}

#[test]
fn lexer_rejects_non_ascii_without_panicking() {
    let err = lex("set k = é").unwrap_err();
    assert_eq!(err.message, "unexpected character 'é'");
    assert_eq!((err.line, err.column), (1, 9));
}

#[test]
fn lexer_rejects_unterminated_strings_at_their_opening_quote() {
    let err = lex("set k = \"abc").unwrap_err();
    assert_eq!(err.message, "unterminated string");
    assert_eq!((err.line, err.column), (1, 9));
}

#[test]
fn token_type_displays_its_name() {
    assert_eq!(TokenType::Set.to_string(), "Set");
    assert_eq!(TokenType::Identifier.to_string(), "Identifier");
    assert_eq!(TokenType::EndOfFile.to_string(), "EndOfFile");
}
