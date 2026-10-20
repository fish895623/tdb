use std::fmt;

use crate::keyword::Keyword;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    // Words and literals
    /// A reserved word, in any letter case.
    Keyword(Keyword),
    /// A name, or a backtick-quoted name (value is the text inside).
    Identifier,
    /// A bare argument of a Redis-style command (`user:1`, `10.0.0.1`).
    Word,
    /// Digits only, e.g. `42`. Redis-style arguments may have a leading `-`.
    Integer,
    /// `3.14`, `1e-3`, `2.5E+10`.
    Float,
    /// A quoted string; the value is the unescaped text.
    String,
    /// `$1` or `$name`; the value is what follows the `$`.
    Parameter,

    // Punctuation
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semicolon,
    Dot,
    /// `..`, as in the Cypher range `*1..3`.
    DotDot,
    Pipe,

    // Comparison
    Eq,
    /// `!=` or `<>`.
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,

    // Arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // Graph edges
    /// `->`
    Arrow,
    /// `<-`
    LeftArrow,

    // Vector distances
    /// `<->` Euclidean distance.
    L2Distance,
    /// `<=>` cosine distance.
    CosineDistance,
    /// `<#>` negative inner product.
    InnerProduct,

    // Structure
    /// The end of a Redis-style command (they end at the line, not at `;`).
    Newline,
    EndOfFile,
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TokenType::Keyword(k) => return write!(f, "Keyword({k})"),
            TokenType::Identifier => "Identifier",
            TokenType::Word => "Word",
            TokenType::Integer => "Integer",
            TokenType::Float => "Float",
            TokenType::String => "String",
            TokenType::Parameter => "Parameter",
            TokenType::LParen => "LParen",
            TokenType::RParen => "RParen",
            TokenType::LBracket => "LBracket",
            TokenType::RBracket => "RBracket",
            TokenType::LBrace => "LBrace",
            TokenType::RBrace => "RBrace",
            TokenType::Comma => "Comma",
            TokenType::Colon => "Colon",
            TokenType::Semicolon => "Semicolon",
            TokenType::Dot => "Dot",
            TokenType::DotDot => "DotDot",
            TokenType::Pipe => "Pipe",
            TokenType::Eq => "Eq",
            TokenType::NotEq => "NotEq",
            TokenType::Lt => "Lt",
            TokenType::LtEq => "LtEq",
            TokenType::Gt => "Gt",
            TokenType::GtEq => "GtEq",
            TokenType::Plus => "Plus",
            TokenType::Minus => "Minus",
            TokenType::Star => "Star",
            TokenType::Slash => "Slash",
            TokenType::Percent => "Percent",
            TokenType::Arrow => "Arrow",
            TokenType::LeftArrow => "LeftArrow",
            TokenType::L2Distance => "L2Distance",
            TokenType::CosineDistance => "CosineDistance",
            TokenType::InnerProduct => "InnerProduct",
            TokenType::Newline => "Newline",
            TokenType::EndOfFile => "EndOfFile",
        };
        f.write_str(name)
    }
}

/// One token. `line` and `column` are 1-based, count characters, and point at
/// the token's first character. `value` is the source text, except for
/// strings (unescaped), quoted identifiers (without the backticks) and
/// parameters (without the `$`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenType,
    pub value: String,
    pub line: usize,
    pub column: usize,
}

impl Token {
    /// The keyword, if this token is one.
    pub fn keyword(&self) -> Option<Keyword> {
        match self.kind {
            TokenType::Keyword(k) => Some(k),
            _ => None,
        }
    }
}
