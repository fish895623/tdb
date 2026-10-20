//! The tdb server: accepts connections that speak the wire protocol of
//! `docs/dd-005-wire-protocol.md` and answers their statements.
//!
//! [`serve`] runs on an existing tokio runtime and stops when the future it
//! is given completes, so the caller decides what "stop" means (a signal, in
//! `tdbd`). It does not yet execute statements: it lexes each one in the
//! dialect of the connection's model and replies with the tokens.

mod connection;

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;
use tokio::time;

/// Timing knobs. The defaults suit production; tests shorten them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How long a new connection has to send its hello frame before it is
    /// dropped, so a socket that never speaks cannot sit unnoticed. Only the
    /// hello has a deadline: a connection that has said hello may idle.
    pub handshake_timeout: Duration,
    /// How long the peer has to take one reply before its connection is
    /// dropped, so a peer that stops reading cannot hold a reply for ever.
    pub write_timeout: Duration,
    /// How many connections may be open at once (at least 1). At the limit the
    /// server stops accepting, and new connections wait in the operating
    /// system's queue until a slot frees up.
    pub max_connections: usize,
    /// How long open connections get to finish after shutdown begins before
    /// they are cut off.
    pub shutdown_grace: Duration,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            handshake_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(10),
            max_connections: 1024,
            shutdown_grace: Duration::from_secs(5),
        }
    }
}

/// How long to wait after a failed `accept` (typically out of file
/// descriptors) before trying again, so the failure cannot spin the CPU.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// Accept connections on `listener` until `shutdown` completes.
///
/// Shutdown stops accepting, tells every connection to finish its current
/// request and close, and waits up to [`Options::shutdown_grace`] for them.
pub async fn serve(listener: TcpListener, shutdown: impl Future<Output = ()>) {
    serve_with(listener, shutdown, Options::default()).await;
}

/// [`serve`] with explicit [`Options`].
pub async fn serve_with(
    listener: TcpListener,
    shutdown: impl Future<Output = ()>,
    options: Options,
) {
    tokio::pin!(shutdown);
    let (stop_tx, stop_rx) = watch::channel(false);
    let limit = Arc::new(Semaphore::new(options.max_connections.max(1)));
    let mut connections = JoinSet::new();

    loop {
        // A free slot first, then a connection to put in it. At the limit the
        // server does not accept, so the excess waits in the operating
        // system's queue instead of costing memory here.
        let permit = tokio::select! {
            () = &mut shutdown => break,
            permit = limit.clone().acquire_owned() => match permit {
                Ok(permit) => permit,
                // The semaphore is never closed.
                Err(_) => break,
            },
            Some(finished) = connections.join_next() => {
                reaped(finished);
                continue;
            }
        };
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    connections.spawn(connection::handle(
                        stream,
                        peer,
                        options,
                        stop_rx.clone(),
                        permit,
                    ));
                }
                Err(e) => {
                    log::warn!("accept failed: {e}");
                    time::sleep(ACCEPT_BACKOFF).await;
                }
            },
            // Reap finished connections as they end. With none open this
            // branch is skipped, not busy.
            Some(finished) = connections.join_next() => reaped(finished),
        }
    }

    // Stop accepting, then let the open connections wind down.
    drop(listener);
    let _ = stop_tx.send(true);
    let drained = time::timeout(options.shutdown_grace, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        log::warn!(
            "{} connection(s) still open after {:?}; closing them",
            connections.len(),
            options.shutdown_grace
        );
        connections.abort_all();
    }
}

/// A connection task ended; only a panic or a cancellation is worth a line.
fn reaped(finished: Result<(), tokio::task::JoinError>) {
    if let Err(e) = finished {
        log::error!("connection task failed: {e}");
    }
}
