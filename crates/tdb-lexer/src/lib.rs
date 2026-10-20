//! Lexer for tdb's command language.
//!
//! One lexer serves all four data models, because a statement's first word
//! says which model it is:
//!
//! - **Key-value (Redis-style)**: a statement starting with a Redis command
//!   (`SET`, `GET`, `HSET`, `ZADD`, ...) is lexed as that command followed by
//!   bare-word arguments up to the end of the line, so keys like `user:1` need
//!   no quotes.
//! - **Relational, vector and graph**: everything else is lexed as free-form
//!   SQL/Cypher-style tokens, up to `;`. Vector search uses the `<->`, `<=>`
//!   and `<#>` distance operators and `[..]` literals; graph patterns use
//!   `( ) [ ] { } : -> <-`.
//!
//! The full token set and the rules behind it are in
//! `docs/dd-003-query-lexer.md`.

mod error;
mod keyword;
mod lexer;
mod raw;
mod token;

pub use error::LexError;
pub use keyword::{Keyword, Model};
pub use lexer::Lexer;
pub use token::{Token, TokenType};

/// Lex `input` into tokens, ending with an `EndOfFile` token.
pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(input).tokenize()
}
