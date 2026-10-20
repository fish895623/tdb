//! Lexer for the key-value command language: `set name = hello`.

use std::fmt;

// Token spellings, shared by the lexer and anything that builds commands.
pub const KEYWORD_SET: &str = "set";
pub const KEYWORD_GET: &str = "get";
pub const KEYWORD_DEL: &str = "del";
pub const EQUAL: char = '=';
pub const QUOTE: char = '"';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    // Keywords
    Set,
    Get,
    Del,

    // Literals & identifiers
    Identifier,
    Number,
    String,

    // Operators & symbols
    Equal,
    EndOfFile,
}

impl fmt::Display for TokenType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TokenType::Set => "Set",
            TokenType::Get => "Get",
            TokenType::Del => "Del",
            TokenType::Identifier => "Identifier",
            TokenType::Number => "Number",
            TokenType::String => "String",
            TokenType::Equal => "Equal",
            TokenType::EndOfFile => "EndOfFile",
        };
        f.write_str(name)
    }
}

/// One token. `line` and `column` are 1-based and count characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenType,
    pub value: String,
    pub line: usize,
    pub column: usize,
}

/// A lexing failure and where it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.message, self.line, self.column)
    }
}

impl std::error::Error for LexError {}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Lexer {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Splits the input into tokens, ending with an `EndOfFile` token. Fails
    /// on an unexpected character or an unterminated string.
    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let Some(c) = self.peek() else { break };
            tokens.push(self.next_token(c)?);
        }
        tokens.push(Token {
            kind: TokenType::EndOfFile,
            value: String::new(),
            line: self.line,
            column: self.column,
        });
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_second(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.advance();
        }
    }

    fn next_token(&mut self, c: char) -> Result<Token, LexError> {
        let (line, column) = (self.line, self.column);
        let starts_number = c.is_ascii_digit()
            || (c == '-' && self.peek_second().is_some_and(|d| d.is_ascii_digit()));

        if c == EQUAL {
            self.advance();
            return Ok(Token {
                kind: TokenType::Equal,
                value: EQUAL.to_string(),
                line,
                column,
            });
        }
        if c == QUOTE {
            return self.read_string(line, column);
        }
        if starts_number {
            return Ok(self.read_number(line, column));
        }
        if c.is_ascii_alphabetic() || c == '_' {
            return Ok(self.read_word(line, column));
        }
        Err(LexError {
            message: format!("unexpected character '{c}'"),
            line,
            column,
        })
    }

    fn read_string(&mut self, line: usize, column: usize) -> Result<Token, LexError> {
        self.advance(); // opening quote
        let mut value = String::new();
        loop {
            match self.advance() {
                Some(QUOTE) => break, // closing quote
                Some(c) => value.push(c),
                None => {
                    return Err(LexError {
                        message: "unterminated string".into(),
                        line,
                        column,
                    });
                }
            }
        }
        Ok(Token {
            kind: TokenType::String,
            value,
            line,
            column,
        })
    }

    fn read_number(&mut self, line: usize, column: usize) -> Token {
        let mut value = String::new();
        value.extend(self.advance()); // first digit or '-'
        let mut seen_dot = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                value.push(c);
            } else if c == '.' && !seen_dot {
                seen_dot = true;
                value.push(c);
            } else {
                break;
            }
            self.advance();
        }
        Token {
            kind: TokenType::Number,
            value,
            line,
            column,
        }
    }

    fn read_word(&mut self, line: usize, column: usize) -> Token {
        let mut value = String::new();
        while let Some(c) = self.peek() {
            if !c.is_ascii_alphanumeric() && c != '_' {
                break;
            }
            value.push(c);
            self.advance();
        }
        let kind = match value.as_str() {
            KEYWORD_SET => TokenType::Set,
            KEYWORD_GET => TokenType::Get,
            KEYWORD_DEL => TokenType::Del,
            _ => TokenType::Identifier,
        };
        Token {
            kind,
            value,
            line,
            column,
        }
    }
}
