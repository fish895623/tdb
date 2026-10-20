//! One client connection: the hello, then one request at a time.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tdb_lexer::{Lexer, Model};
use tdb_protocol::{
    ErrorKind, MAX_FRAME_LEN, MAX_HELLO_LEN, MAX_STATEMENT_LEN, ProtocolError, Request, Response,
    Route, VERSION, WireToken, frame_len_within, hello_version,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, watch};
use tokio::time;

use crate::Options;

/// Run a connection to its end. Nothing a peer sends can make this panic or
/// fail the server: problems end the one connection and are logged. The
/// `permit` is the connection's place under [`Options::max_connections`], held
/// until it ends.
pub(crate) async fn handle(
    stream: TcpStream,
    peer: SocketAddr,
    options: Options,
    stop: watch::Receiver<bool>,
    _permit: OwnedSemaphorePermit,
) {
    log::debug!("{peer}: connected");
    let mut connection = Connection {
        stream,
        peer,
        write_timeout: options.write_timeout,
        stop,
    };
    match connection.run(options.handshake_timeout).await {
        Ok(()) => log::debug!("{peer}: closed"),
        Err(e) => log::debug!("{peer}: closed: {e}"),
    }
}

struct Connection {
    stream: TcpStream,
    peer: SocketAddr,
    write_timeout: Duration,
    stop: watch::Receiver<bool>,
}

impl Connection {
    async fn run(&mut self, handshake_timeout: Duration) -> io::Result<()> {
        let Some(route) = self.handshake(handshake_timeout).await? else {
            return Ok(());
        };
        log::debug!("{}: route {route}", self.peer);

        loop {
            let payload = match self.next_frame(MAX_FRAME_LEN).await {
                Ok(Some(payload)) => payload,
                // The peer left between frames, or the server is stopping.
                Ok(None) => return Ok(()),
                Err(e) => return self.reject(e).await,
            };
            match Request::from_payload(&payload) {
                Ok(Request::Query { statement }) => {
                    // Built whole, and the tokens dropped, before anything is
                    // written: a peer that stops reading holds one frame, not
                    // the lexer's output as well.
                    let frame = answer(route.model(), &statement);
                    self.send_frame(&frame).await?;
                }
                Ok(Request::Hello { .. }) => {
                    return self.fail(ErrorKind::Protocol, "already connected").await;
                }
                Err(e) => return self.reject(e).await,
            }
        }
    }

    /// Read the hello and answer it. `None` means the connection is over: the
    /// peer left, was too slow, the server is stopping, or the peer was turned
    /// away with an error frame.
    async fn handshake(&mut self, timeout: Duration) -> io::Result<Option<Route>> {
        // A hello is small, and nothing has been said yet that earns a big
        // buffer: read the first frame with a small limit.
        let first = match time::timeout(timeout, self.next_frame(MAX_HELLO_LEN)).await {
            Err(_elapsed) => return Ok(None),
            Ok(Ok(Some(payload))) => payload,
            Ok(Ok(None)) => return Ok(None),
            Ok(Err(ProtocolError::FrameTooLarge { len })) => {
                let message = format!(
                    "the first frame may be at most {MAX_HELLO_LEN} bytes; this one announces {len}"
                );
                self.fail(ErrorKind::Protocol, message).await?;
                return Ok(None);
            }
            Ok(Err(e)) => {
                self.reject(e).await?;
                return Ok(None);
            }
        };

        // The version before anything else in the hello: a client from a newer
        // protocol may have a route or fields this build cannot read, and
        // still has to be told which version this server speaks.
        match hello_version(&first) {
            Some(version) if version != VERSION => {
                let message =
                    format!("unsupported protocol version {version}; this server speaks {VERSION}");
                self.fail(ErrorKind::Version, message).await?;
                return Ok(None);
            }
            _ => {}
        }

        match Request::from_payload(&first) {
            Ok(Request::Hello { route, .. }) => {
                self.send(&Response::HelloOk { version: VERSION }).await?;
                Ok(Some(route))
            }
            Ok(Request::Query { .. }) => {
                self.fail(ErrorKind::Protocol, "expected a hello frame first")
                    .await?;
                Ok(None)
            }
            Err(e) => {
                self.reject(e).await?;
                Ok(None)
            }
        }
    }

    /// The next frame body, up to `max` bytes. `None` is a clean end: the peer
    /// closed between frames, or the server began shutting down.
    async fn next_frame(&mut self, max: u32) -> Result<Option<Vec<u8>>, ProtocolError> {
        tokio::select! {
            () = stopping(&mut self.stop) => Ok(None),
            read = read_payload(&mut self.stream, max) => read,
        }
    }

    async fn send(&mut self, response: &Response) -> io::Result<()> {
        self.send_frame(&frame_of(response)).await
    }

    /// Write a frame, giving the peer [`Options::write_timeout`] to take it.
    async fn send_frame(&mut self, frame: &[u8]) -> io::Result<()> {
        match time::timeout(self.write_timeout, self.stream.write_all(frame)).await {
            Ok(written) => written,
            Err(_elapsed) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the peer is not reading its replies",
            )),
        }
    }

    /// Tell the peer why its frame was refused, unless the transport itself
    /// failed, in which case there is no one to tell. The caller then closes.
    async fn reject(&mut self, error: ProtocolError) -> io::Result<()> {
        match error {
            ProtocolError::Io(e) => Err(e),
            other => self.fail(ErrorKind::of(&other), other.to_string()).await,
        }
    }

    async fn fail(&mut self, kind: ErrorKind, message: impl Into<String>) -> io::Result<()> {
        let message = message.into();
        log::debug!("{}: refused: {message}", self.peer);
        self.send(&Response::error(kind, message)).await
    }
}

/// Completes when shutdown begins, or at once if it already has.
async fn stopping(stop: &mut watch::Receiver<bool>) {
    // An error means the sender is gone, which is shutdown too. The guard is
    // dropped here, not held across the caller's awaits.
    let _ = stop.wait_for(|stopping| *stopping).await;
}

/// The reply frame to one statement. The server does not execute statements
/// yet, so this lexes it in the connection's model and returns the tokens.
fn answer(model: Model, statement: &str) -> Vec<u8> {
    if statement.len() > MAX_STATEMENT_LEN {
        return frame_of(&Response::error(
            ErrorKind::TooLarge,
            format!(
                "the statement is {} bytes; the limit is {MAX_STATEMENT_LEN}",
                statement.len()
            ),
        ));
    }
    let response = match Lexer::for_model(statement, model).tokenize() {
        Ok(tokens) => Response::Tokens(tokens.iter().map(WireToken::from).collect()),
        Err(e) => Response::error(ErrorKind::Lex, e.to_string()),
    };
    frame_of(&response)
}

/// `response` as a frame. A reply too big for one cannot happen while
/// [`MAX_STATEMENT_LEN`] holds, but if it did, the peer would get an error
/// instead of silence.
fn frame_of(response: &Response) -> Vec<u8> {
    response.to_frame().unwrap_or_else(|e| {
        log::error!("a reply could not be framed: {e}");
        // A short message always fits.
        Response::error(ErrorKind::TooLarge, "the reply is too large")
            .to_frame()
            .unwrap_or_default()
    })
}

/// Read one frame body of at most `max` bytes, or `None` if the peer closed
/// between frames.
async fn read_payload<R: AsyncRead + Unpin>(
    reader: &mut R,
    max: u32,
) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut header = [0u8; 4];
    if reader.read(&mut header[..1]).await? == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut header[1..])
        .await
        .map_err(truncated_if_eof)?;
    let len = frame_len_within(header, max)?;

    // Grows as bytes arrive: a peer that promises a big frame and sends
    // nothing costs nothing.
    let mut payload = Vec::new();
    (&mut *reader)
        .take(len as u64)
        .read_to_end(&mut payload)
        .await?;
    if payload.len() < len {
        return Err(ProtocolError::Truncated);
    }
    Ok(Some(payload))
}

fn truncated_if_eof(e: io::Error) -> ProtocolError {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        ProtocolError::Truncated
    } else {
        e.into()
    }
}
