use std::io::{Read, Write};

use tdb_lexer::Token;

use crate::error::ProtocolError;
use crate::frame::read_payload;
use crate::route::Route;
use crate::{MAGIC, MAX_FRAME_LEN};

// Message types: the first byte of a frame body. Requests are below 0x80,
// responses at or above it.
const HELLO: u8 = 0x01;
const QUERY: u8 = 0x02;
const HELLO_OK: u8 = 0x81;
const TOKENS: u8 = 0x82;
const ERROR: u8 = 0xFF;

/// The protocol version in a hello frame body, if `payload` is one: the right
/// type byte and the `TDBP` magic, then a `u16`. Nothing after the version is
/// looked at.
///
/// A server asks this before decoding the rest, so a client from a newer
/// protocol, whose hello may have a route or fields this build cannot read,
/// still gets [`ErrorKind::Version`] and not a decoding error.
pub fn hello_version(payload: &[u8]) -> Option<u16> {
    let mut r = Reader(payload);
    if r.u8().ok()? != HELLO || r.take(MAGIC.len()).ok()? != MAGIC {
        return None;
    }
    r.u16().ok()
}

/// What a client sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The first frame of a connection: the protocol version the client
    /// speaks and the route it wants.
    Hello { version: u16, route: Route },
    /// One statement, in the dialect of the connection's model.
    Query { statement: String },
}

/// What a server sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// The hello was accepted; this is the version the server speaks.
    HelloOk { version: u16 },
    /// The statement's tokens. The server does not execute statements yet, so
    /// lexing is all it does with one.
    Tokens(Vec<WireToken>),
    /// The request failed.
    Error { kind: ErrorKind, message: String },
}

/// One token as sent over the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireToken {
    /// The token's type as `tdb_lexer::TokenType` prints it: `Keyword(GET)`.
    pub kind: String,
    pub value: String,
    /// 1-based position in the statement.
    pub line: u32,
    pub column: u32,
}

impl From<&Token> for WireToken {
    fn from(token: &Token) -> WireToken {
        WireToken {
            kind: token.kind.to_string(),
            value: token.value.clone(),
            line: u32::try_from(token.line).unwrap_or(u32::MAX),
            column: u32::try_from(token.column).unwrap_or(u32::MAX),
        }
    }
}

/// What went wrong, in an error response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A malformed or unexpected frame. The server closes the connection.
    Protocol,
    /// The hello's version is not one the server speaks. The server closes the
    /// connection.
    Version,
    /// The hello's route is not valid. The server closes the connection.
    Route,
    /// The statement could not be lexed. The connection stays open.
    Lex,
    /// The statement is longer than [`MAX_STATEMENT_LEN`](crate::MAX_STATEMENT_LEN).
    /// The connection stays open.
    TooLarge,
}

impl ErrorKind {
    /// The byte this kind has on the wire.
    pub fn code(self) -> u8 {
        match self {
            ErrorKind::Protocol => 1,
            ErrorKind::Version => 2,
            ErrorKind::Route => 3,
            ErrorKind::Lex => 4,
            ErrorKind::TooLarge => 5,
        }
    }

    pub fn from_code(code: u8) -> Option<ErrorKind> {
        match code {
            1 => Some(ErrorKind::Protocol),
            2 => Some(ErrorKind::Version),
            3 => Some(ErrorKind::Route),
            4 => Some(ErrorKind::Lex),
            5 => Some(ErrorKind::TooLarge),
            _ => None,
        }
    }

    /// The error kind to report for a frame that failed to decode.
    pub fn of(error: &ProtocolError) -> ErrorKind {
        match error {
            ProtocolError::Route(_) => ErrorKind::Route,
            _ => ErrorKind::Protocol,
        }
    }
}

impl Request {
    /// The whole frame, header included, ready to send.
    pub fn to_frame(&self) -> Result<Vec<u8>, ProtocolError> {
        frame(|w| {
            match self {
                Request::Hello { version, route } => {
                    w.u8(HELLO);
                    w.bytes(&MAGIC);
                    w.u16(*version);
                    w.str(&route.to_string())?;
                }
                Request::Query { statement } => {
                    w.u8(QUERY);
                    w.str(statement)?;
                }
            }
            Ok(())
        })
    }

    /// Decode a frame body, as returned by [`read_payload`].
    pub fn from_payload(payload: &[u8]) -> Result<Request, ProtocolError> {
        let mut r = Reader(payload);
        let request = match r.u8()? {
            HELLO => {
                if r.take(MAGIC.len())? != MAGIC {
                    return Err(ProtocolError::BadMagic);
                }
                let version = r.u16()?;
                let route = r.str()?.parse()?;
                Request::Hello { version, route }
            }
            QUERY => Request::Query {
                statement: r.str()?.to_owned(),
            },
            tag => return Err(ProtocolError::UnknownTag(tag)),
        };
        r.finish()?;
        Ok(request)
    }

    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        writer.write_all(&self.to_frame()?)?;
        Ok(writer.flush()?)
    }

    /// Read the next request; `Ok(None)` at a clean end of stream.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Option<Request>, ProtocolError> {
        read_payload(reader)?
            .map(|payload| Request::from_payload(&payload))
            .transpose()
    }
}

impl Response {
    pub fn error(kind: ErrorKind, message: impl Into<String>) -> Response {
        Response::Error {
            kind,
            message: message.into(),
        }
    }

    /// The whole frame, header included, ready to send.
    pub fn to_frame(&self) -> Result<Vec<u8>, ProtocolError> {
        frame(|w| {
            match self {
                Response::HelloOk { version } => {
                    w.u8(HELLO_OK);
                    w.u16(*version);
                }
                Response::Tokens(tokens) => {
                    w.u8(TOKENS);
                    w.len(tokens.len())?;
                    for token in tokens {
                        w.str(&token.kind)?;
                        w.str(&token.value)?;
                        w.u32(token.line);
                        w.u32(token.column);
                    }
                }
                Response::Error { kind, message } => {
                    w.u8(ERROR);
                    w.u8(kind.code());
                    w.str(message)?;
                }
            }
            Ok(())
        })
    }

    /// Decode a frame body, as returned by [`read_payload`].
    pub fn from_payload(payload: &[u8]) -> Result<Response, ProtocolError> {
        let mut r = Reader(payload);
        let response = match r.u8()? {
            HELLO_OK => Response::HelloOk { version: r.u16()? },
            TOKENS => {
                let count = r.u32()? as usize;
                // The count is the peer's claim, so nothing is reserved for it
                // up front: the list grows as tokens are really read, and a
                // lie runs out of input.
                let mut tokens = Vec::new();
                for _ in 0..count {
                    tokens.push(WireToken {
                        kind: r.str()?.to_owned(),
                        value: r.str()?.to_owned(),
                        line: r.u32()?,
                        column: r.u32()?,
                    });
                }
                Response::Tokens(tokens)
            }
            ERROR => {
                let code = r.u8()?;
                let kind =
                    ErrorKind::from_code(code).ok_or(ProtocolError::UnknownErrorKind(code))?;
                Response::Error {
                    kind,
                    message: r.str()?.to_owned(),
                }
            }
            tag => return Err(ProtocolError::UnknownTag(tag)),
        };
        r.finish()?;
        Ok(response)
    }

    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        writer.write_all(&self.to_frame()?)?;
        Ok(writer.flush()?)
    }

    /// Read the next response; `Ok(None)` at a clean end of stream.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Option<Response>, ProtocolError> {
        read_payload(reader)?
            .map(|payload| Response::from_payload(&payload))
            .transpose()
    }
}

/// Build a frame: reserve the header, let `body` write the body, then fill in
/// the length.
fn frame(
    body: impl FnOnce(&mut Writer) -> Result<(), ProtocolError>,
) -> Result<Vec<u8>, ProtocolError> {
    let mut w = Writer(vec![0; 4]);
    body(&mut w)?;
    let len = w.0.len() - 4;
    match u32::try_from(len) {
        Ok(len) if len <= MAX_FRAME_LEN => {
            w.0[..4].copy_from_slice(&len.to_le_bytes());
            Ok(w.0)
        }
        _ => Err(ProtocolError::FrameTooLarge {
            len: u64::try_from(len).unwrap_or(u64::MAX),
        }),
    }
}

/// Little-endian fields appended to a buffer.
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn bytes(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }

    /// A length or count as a `u32`.
    fn len(&mut self, len: usize) -> Result<(), ProtocolError> {
        let len = u32::try_from(len).map_err(|_| ProtocolError::FrameTooLarge {
            len: u64::try_from(len).unwrap_or(u64::MAX),
        })?;
        self.u32(len);
        Ok(())
    }

    /// A string: its byte length as a `u32`, then the UTF-8 bytes.
    fn str(&mut self, s: &str) -> Result<(), ProtocolError> {
        self.len(s.len())?;
        self.bytes(s.as_bytes());
        Ok(())
    }
}

/// Little-endian fields read off the front of a slice. Every read checks what
/// is left.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ProtocolError> {
        if self.0.len() < n {
            return Err(ProtocolError::Truncated);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn str(&mut self) -> Result<&'a str, ProtocolError> {
        let len = self.u32()? as usize;
        std::str::from_utf8(self.take(len)?).map_err(|_| ProtocolError::BadUtf8)
    }

    /// The message must end here.
    fn finish(self) -> Result<(), ProtocolError> {
        match self.0.len() {
            0 => Ok(()),
            n => Err(ProtocolError::TrailingBytes(n)),
        }
    }
}
