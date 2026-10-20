use std::fmt;
use std::io;

use crate::route::RouteError;

/// Why a frame could not be read, decoded or encoded.
#[derive(Debug)]
pub enum ProtocolError {
    /// The transport failed.
    Io(io::Error),
    /// The input ended before the frame or field was complete.
    Truncated,
    /// A length prefix above [`MAX_FRAME_LEN`](crate::MAX_FRAME_LEN), or a
    /// message too big to send.
    FrameTooLarge { len: u64 },
    /// The frame's first byte is not a message this side knows.
    UnknownTag(u8),
    /// A hello frame without the `TDBP` magic: not a tdb client.
    BadMagic,
    /// A string field that is not UTF-8.
    BadUtf8,
    /// Bytes left over after a complete message.
    TrailingBytes(usize),
    /// An error frame with a kind this side does not know.
    UnknownErrorKind(u8),
    /// A hello frame whose route is not valid.
    Route(RouteError),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::Io(e) => write!(f, "i/o error: {e}"),
            ProtocolError::Truncated => f.write_str("frame ended early"),
            ProtocolError::FrameTooLarge { len } => write!(
                f,
                "frame of {len} bytes is too large (the limit is {})",
                crate::MAX_FRAME_LEN
            ),
            ProtocolError::UnknownTag(tag) => write!(f, "unknown message type {tag:#04x}"),
            ProtocolError::BadMagic => f.write_str("not a tdb hello frame"),
            ProtocolError::BadUtf8 => f.write_str("text field is not valid UTF-8"),
            ProtocolError::TrailingBytes(n) => {
                write!(f, "{n} unexpected byte(s) after the message")
            }
            ProtocolError::UnknownErrorKind(code) => write!(f, "unknown error kind {code}"),
            ProtocolError::Route(e) => write!(f, "invalid route: {e}"),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProtocolError::Io(e) => Some(e),
            ProtocolError::Route(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(e: io::Error) -> Self {
        ProtocolError::Io(e)
    }
}

impl From<RouteError> for ProtocolError {
    fn from(e: RouteError) -> Self {
        ProtocolError::Route(e)
    }
}
