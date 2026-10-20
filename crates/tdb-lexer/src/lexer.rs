use logos::Logos;

use crate::error::LexError;
use crate::keyword::{Keyword, Model};
use crate::raw::{Kv, KvNumber, Normal, RawError};
use crate::token::{Token, TokenType};

/// Turns text into tokens. [`Lexer::new`] detects each statement's model from
/// its first word; [`Lexer::for_model`] fixes the model up front, which is
/// what a router that already knows it (from `/keyval/..`, `/graph/..`, ...)
/// should use.
pub struct Lexer {
    input: String,
    /// `None` detects the model of every statement.
    model: Option<Model>,
}

impl Lexer {
    /// Detect the model of each statement from its first word, so one input
    /// may mix models. Every keyword is recognized.
    pub fn new(input: &str) -> Lexer {
        Lexer {
            input: input.to_owned(),
            model: None,
        }
    }

    /// Lex `input` as `model`'s dialect only: a Redis-style command per line
    /// for `Keyval`, and free-form statements up to `;` otherwise. Only that
    /// model's keywords (and the shared ones) are reserved, so `MATCH` is an
    /// ordinary name in `Rdbms` and `SELECT` is an ordinary name in `Graph`.
    pub fn for_model(input: &str, model: Model) -> Lexer {
        Lexer {
            input: input.to_owned(),
            model: Some(model),
        }
    }

    /// Splits the input into tokens, ending with an `EndOfFile` token. Fails
    /// at the first unexpected character, bad escape, bad number, or
    /// unterminated string, quoted name, or comment.
    pub fn tokenize(self) -> Result<Vec<Token>, LexError> {
        let mut run = Run {
            src: &self.input,
            model: self.model,
            positions: Positions::new(&self.input),
            out: Vec::new(),
        };
        match self.model {
            None => {
                let mut offset = 0;
                while offset < run.src.len() {
                    offset = match run.kv_command_at(offset) {
                        Some(start) => run.kv(start)?,
                        None => run.normal(offset, true)?,
                    };
                }
            }
            Some(Model::Keyval) => {
                run.kv(0)?;
            }
            Some(_) => {
                run.normal(0, false)?;
            }
        }
        let (line, column) = run.positions.at(run.src.len());
        run.out.push(Token {
            kind: TokenType::EndOfFile,
            value: String::new(),
            line,
            column,
        });
        Ok(run.out)
    }
}

struct Run<'a> {
    src: &'a str,
    model: Option<Model>,
    positions: Positions<'a>,
    out: Vec<Token>,
}

impl Run<'_> {
    /// If the statement at `offset` is Redis-style, the byte offset of its
    /// command word: a KV command followed by whitespace or the end of input.
    fn kv_command_at(&self, offset: usize) -> Option<usize> {
        let mut probe = Normal::lexer(&self.src[offset..]);
        // Whitespace and `//` comments are skipped by the lexer itself; block
        // comments come back as tokens, so step over those too.
        let first = loop {
            match probe.next()? {
                Ok(Normal::Comment) => continue,
                other => break other,
            }
        };
        let Ok(Normal::Word) = first else {
            return None;
        };
        let is_command = Keyword::from_word(probe.slice())?.is_kv_command();
        let ends_word = probe
            .remainder()
            .chars()
            .next()
            .is_none_or(char::is_whitespace);
        (is_command && ends_word).then(|| offset + probe.span().start)
    }

    /// Lex relational, vector and graph tokens from `offset`. With
    /// `stop_at_semicolon`, returns the offset just past the first `;`;
    /// otherwise runs to the end of the input.
    fn normal(&mut self, offset: usize, stop_at_semicolon: bool) -> Result<usize, LexError> {
        let mut lexer = Normal::lexer(&self.src[offset..]);
        while let Some(result) = lexer.next() {
            let span = lexer.span();
            let start = offset + span.start;
            let end = offset + span.end;
            let slice = lexer.slice();
            let (kind, value) = match result {
                Ok(Normal::Comment) => continue,
                Ok(Normal::Word) => match self.keyword(slice) {
                    Some(keyword) => (TokenType::Keyword(keyword), slice.to_owned()),
                    None => (TokenType::Identifier, slice.to_owned()),
                },
                Ok(Normal::Integer) => (TokenType::Integer, slice.to_owned()),
                Ok(Normal::Float) => (TokenType::Float, slice.to_owned()),
                Ok(Normal::String(text)) => (TokenType::String, text),
                Ok(Normal::QuotedName(name)) => (TokenType::Identifier, name),
                Ok(Normal::Parameter) => (TokenType::Parameter, slice[1..].to_owned()),
                Ok(Normal::Punct(kind)) => (kind, slice.to_owned()),
                // Both only ever fail; see raw.rs.
                Ok(Normal::BadNumber | Normal::BadDollar) => {
                    return Err(self.error(RawError::Unexpected, start, end));
                }
                Err(error) => return Err(self.error(error, start, end)),
            };
            self.push(kind, value, start);
            if stop_at_semicolon && kind == TokenType::Semicolon {
                return Ok(end);
            }
        }
        Ok(self.src.len())
    }

    /// Lex Redis-style commands from `start`. Each ends at a newline; in
    /// auto-detect mode the first newline ends the call (so the next statement
    /// can pick its own model), otherwise lexing continues line by line.
    fn kv(&mut self, start: usize) -> Result<usize, LexError> {
        let mut lexer = Kv::lexer(&self.src[start..]);
        let mut command_next = true;
        while let Some(result) = lexer.next() {
            let span = lexer.span();
            let at = start + span.start;
            let end = start + span.end;
            let slice = lexer.slice();
            let (kind, value) = match result {
                Ok(Kv::Newline) => {
                    if command_next {
                        continue; // a blank line is not a statement
                    }
                    self.push(TokenType::Newline, "\n".into(), at);
                    command_next = true;
                    if self.model.is_none() {
                        return Ok(end);
                    }
                    continue;
                }
                Ok(Kv::Word) if command_next => {
                    match Keyword::from_word(slice).filter(|k| k.is_kv_command()) {
                        Some(keyword) => (TokenType::Keyword(keyword), slice.to_owned()),
                        None => (TokenType::Word, slice.to_owned()),
                    }
                }
                Ok(Kv::Word) => (kv_number(slice), slice.to_owned()),
                Ok(Kv::String(text)) => (TokenType::String, text),
                Err(error) => return Err(self.error(error, at, end)),
            };
            command_next = false;
            self.push(kind, value, at);
        }
        Ok(self.src.len())
    }

    /// `word` as a keyword of this lexer's model (any model when detecting).
    fn keyword(&self, word: &str) -> Option<Keyword> {
        Keyword::from_word(word).filter(|k| self.model.is_none_or(|m| k.in_model(m)))
    }

    fn push(&mut self, kind: TokenType, value: String, at: usize) {
        let (line, column) = self.positions.at(at);
        self.out.push(Token {
            kind,
            value,
            line,
            column,
        });
    }

    /// Build the `LexError` for a failure in the token spanning `start..end`.
    fn error(&mut self, error: RawError, start: usize, end: usize) -> LexError {
        let token = self.src.get(start..end).unwrap_or_default();
        let (message, at) = match error {
            RawError::Unexpected => {
                let c = self.src.get(start..).and_then(|s| s.chars().next());
                (
                    format!("unexpected character '{}'", c.unwrap_or('?')),
                    start,
                )
            }
            RawError::Message { message, at } => {
                let message = match message {
                    "invalid number" => format!("invalid number '{token}'"),
                    "invalid escape" => {
                        // `at` is the backslash; the escaped character follows.
                        let escaped = self
                            .src
                            .get(start + at + 1..)
                            .and_then(|s| s.chars().next())
                            .unwrap_or('?');
                        format!("invalid escape '\\{escaped}'")
                    }
                    other => other.to_owned(),
                };
                (message, start + at)
            }
        };
        let (line, column) = self.positions.at(at);
        LexError {
            message,
            line,
            column,
        }
    }
}

/// `Integer` or `Float` if the whole of `word` is a number (a Redis number may
/// have a leading `-`), otherwise `Word`.
fn kv_number(word: &str) -> TokenType {
    let mut lexer = KvNumber::lexer(word);
    match lexer.next() {
        Some(Ok(kind)) if lexer.span().end == word.len() => match kind {
            KvNumber::Integer => TokenType::Integer,
            KvNumber::Float => TokenType::Float,
        },
        _ => TokenType::Word,
    }
}

/// Converts byte offsets to 1-based (line, column) with columns counted in
/// characters. Offsets are visited in increasing order, so it advances from
/// where it last stopped instead of rescanning.
struct Positions<'a> {
    src: &'a str,
    byte: usize,
    line: usize,
    column: usize,
}

impl<'a> Positions<'a> {
    fn new(src: &'a str) -> Positions<'a> {
        Positions {
            src,
            byte: 0,
            line: 1,
            column: 1,
        }
    }

    fn at(&mut self, target: usize) -> (usize, usize) {
        // Never split a character, and tolerate an offset behind the cursor.
        let mut target = target.min(self.src.len());
        while !self.src.is_char_boundary(target) {
            target -= 1;
        }
        if target < self.byte {
            *self = Positions::new(self.src);
        }
        for c in self.src[self.byte..target].chars() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.byte = target;
        (self.line, self.column)
    }
}
