//! The `logos` token definitions. Each enum is a DFA over one kind of text;
//! `lexer.rs` turns their output into public `Token`s.

use logos::{Lexer, Logos};

use crate::token::TokenType;

/// A failure inside a callback, or an unmatched character. `at` is the byte
/// offset, from the start of the token, of the character to blame.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum RawError {
    #[default]
    Unexpected,
    Message {
        message: &'static str,
        at: usize,
    },
}

fn fail<T>(message: &'static str, at: usize) -> Result<T, RawError> {
    Err(RawError::Message { message, at })
}

/// The next char of `text` and its byte index, or `None` at the end.
fn next_char(text: &str, from: usize) -> Option<(usize, char)> {
    text.get(from..)?.chars().next().map(|c| (from, c))
}

/// Read a quote-delimited string from the text after the opening quote, which
/// the token already covers. A backslash escapes `n r t 0 \ " '` and a doubled
/// quote is one quote. Bumps the lexer past the closing quote.
fn read_string<'s, T: Logos<'s, Source = str>>(
    lex: &mut Lexer<'s, T>,
    quote: char,
) -> Result<String, RawError> {
    let rest = lex.remainder();
    let mut value = String::new();
    let mut at = 0;
    while let Some((i, c)) = next_char(rest, at) {
        at = i + c.len_utf8();
        if c == quote {
            if next_char(rest, at).is_some_and(|(_, n)| n == quote) {
                at += quote.len_utf8();
                value.push(quote);
            } else {
                lex.bump(at);
                return Ok(value);
            }
        } else if c == '\\' {
            let Some((_, escaped)) = next_char(rest, at) else {
                break;
            };
            at += escaped.len_utf8();
            match escaped {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                '0' => value.push('\0'),
                '\\' | '"' | '\'' => value.push(escaped),
                // `at` counts from after the opening quote, which is 1 byte.
                _ => return fail("invalid escape", 1 + i),
            }
        } else {
            value.push(c);
        }
    }
    lex.bump(rest.len());
    fail("unterminated string", 0)
}

/// Normal mode: relational, vector and graph statements.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(error = RawError)]
#[logos(skip r"\s+")]
#[logos(skip(r"//[^\n]*", allow_greedy = true))]
pub(crate) enum Normal {
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Word,

    #[regex(r"[0-9]+")]
    Integer,
    #[regex(r"[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?|[0-9]+[eE][+-]?[0-9]+")]
    Float,
    /// A number directly followed by a letter or `_`: the number's text plus
    /// that one character is the error.
    #[regex(r"[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?[A-Za-z_]", bad_number)]
    BadNumber,

    #[token("'", |lex| read_string(lex, '\''))]
    #[token("\"", |lex| read_string(lex, '"'))]
    String(String),

    #[token("`", read_quoted_name)]
    QuotedName(String),

    #[regex(r"\$[A-Za-z0-9_]+")]
    Parameter,
    #[token("$", bad_dollar)]
    BadDollar,

    #[token("/*", read_block_comment)]
    Comment,

    #[token("<=>", |_| TokenType::CosineDistance)]
    #[token("<->", |_| TokenType::L2Distance)]
    #[token("<#>", |_| TokenType::InnerProduct)]
    #[token("<=", |_| TokenType::LtEq)]
    #[token("<>", |_| TokenType::NotEq)]
    #[token("<-", |_| TokenType::LeftArrow)]
    #[token(">=", |_| TokenType::GtEq)]
    #[token("->", |_| TokenType::Arrow)]
    #[token("!=", |_| TokenType::NotEq)]
    #[token("..", |_| TokenType::DotDot)]
    #[token("<", |_| TokenType::Lt)]
    #[token(">", |_| TokenType::Gt)]
    #[token("=", |_| TokenType::Eq)]
    #[token("+", |_| TokenType::Plus)]
    #[token("-", |_| TokenType::Minus)]
    #[token("*", |_| TokenType::Star)]
    #[token("/", |_| TokenType::Slash)]
    #[token("%", |_| TokenType::Percent)]
    #[token("(", |_| TokenType::LParen)]
    #[token(")", |_| TokenType::RParen)]
    #[token("[", |_| TokenType::LBracket)]
    #[token("]", |_| TokenType::RBracket)]
    #[token("{", |_| TokenType::LBrace)]
    #[token("}", |_| TokenType::RBrace)]
    #[token(",", |_| TokenType::Comma)]
    #[token(":", |_| TokenType::Colon)]
    #[token(";", |_| TokenType::Semicolon)]
    #[token(".", |_| TokenType::Dot)]
    #[token("|", |_| TokenType::Pipe)]
    Punct(TokenType),
}

fn bad_number(lex: &mut Lexer<Normal>) -> Result<(), RawError> {
    // The error names the whole token (the number plus the offending letter)
    // and points at the number's start.
    let _ = lex;
    fail("invalid number", 0)
}

fn bad_dollar(_: &mut Lexer<Normal>) -> Result<(), RawError> {
    fail("expected a parameter name after '$'", 0)
}

/// A backtick-quoted name; a doubled backtick is one backtick.
fn read_quoted_name(lex: &mut Lexer<Normal>) -> Result<String, RawError> {
    let rest = lex.remainder();
    let mut value = String::new();
    let mut at = 0;
    while let Some((i, c)) = next_char(rest, at) {
        at = i + c.len_utf8();
        if c == '`' {
            if next_char(rest, at).is_some_and(|(_, n)| n == '`') {
                at += 1;
                value.push('`');
            } else {
                lex.bump(at);
                return if value.is_empty() {
                    fail("empty quoted name", 0)
                } else {
                    Ok(value)
                };
            }
        } else {
            value.push(c);
        }
    }
    lex.bump(rest.len());
    fail("unterminated quoted name", 0)
}

/// A `/* */` comment; the opening `/*` is already consumed.
fn read_block_comment(lex: &mut Lexer<Normal>) -> Result<(), RawError> {
    match lex.remainder().find("*/") {
        Some(end) => {
            lex.bump(end + 2);
            Ok(())
        }
        None => {
            lex.bump(lex.remainder().len());
            fail("unterminated comment", 0)
        }
    }
}

/// KV mode: one Redis-style command per line, arguments are bare words.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(error = RawError)]
#[logos(skip r"[\s&&[^\n]]+")]
pub(crate) enum Kv {
    #[token("\n")]
    Newline,

    /// A run of non-whitespace not starting with a quote.
    #[regex(r#"[^\s"']\S*"#)]
    Word,

    #[token("'", |lex| read_kv_string(lex, '\''))]
    #[token("\"", |lex| read_kv_string(lex, '"'))]
    String(String),
}

fn read_kv_string(lex: &mut Lexer<Kv>, quote: char) -> Result<String, RawError> {
    let value = read_string(lex, quote)?;
    if lex
        .remainder()
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace())
    {
        return fail(
            "expected whitespace after a quoted argument",
            lex.slice().len(),
        );
    }
    Ok(value)
}

/// Classifies a whole KV word as a number.
#[derive(Logos, Debug, Clone, Copy, PartialEq)]
pub(crate) enum KvNumber {
    #[regex(r"-?[0-9]+")]
    Integer,
    #[regex(r"-?([0-9]+\.[0-9]+([eE][+-]?[0-9]+)?|[0-9]+[eE][+-]?[0-9]+)")]
    Float,
}
