//! The tdb wire protocol, as specified in `docs/dd-005-wire-protocol.md`.
//!
//! A client connects to `tdb://host[:port]/<model>/<database>` (an
//! [`Endpoint`]), says hello with that [`Route`], then sends [`Request`]s and
//! reads [`Response`]s. Every message is one length-prefixed frame.
//!
//! This crate only describes and encodes the protocol: it does no networking
//! and has no async runtime, so the server (`tdb-server`) and the clients can
//! share it. Everything read from a peer is untrusted: decoding returns a
//! [`ProtocolError`] and never panics, and no length field is trusted for an
//! allocation.

mod endpoint;
mod error;
mod frame;
mod message;
mod route;

pub use endpoint::{DEFAULT_PORT, Endpoint, EndpointError, SCHEME};
pub use error::ProtocolError;
pub use frame::{frame_len, frame_len_within, read_payload};
pub use message::{ErrorKind, Request, Response, WireToken, hello_version};
pub use route::{MAX_DATABASE_LEN, Route, RouteError, model_name};

/// The first four bytes of a hello frame's body, so a peer that is not
/// speaking this protocol is turned away at once.
pub const MAGIC: [u8; 4] = *b"TDBP";

/// The protocol version this build speaks.
pub const VERSION: u16 = 1;

/// The largest frame body, in bytes. A longer length prefix is refused before
/// anything is read or allocated.
pub const MAX_FRAME_LEN: u32 = 4 * 1024 * 1024;

/// The largest body a hello frame may have, in bytes. A hello is a magic, a
/// version and a route of at most about a hundred bytes, so a server reads
/// the first frame of a connection with this limit instead of
/// [`MAX_FRAME_LEN`]: a peer that has not said hello costs almost nothing.
pub const MAX_HELLO_LEN: u32 = 256;

/// The longest statement a server accepts, in bytes. A token's reply is at
/// most about 27 bytes for each byte of statement, so the reply to an accepted
/// statement stays well inside [`MAX_FRAME_LEN`].
pub const MAX_STATEMENT_LEN: usize = 64 * 1024;
