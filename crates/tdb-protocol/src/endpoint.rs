use std::fmt;
use std::str::FromStr;

use crate::route::{Route, RouteError};

/// The URL scheme: `tdb://host[:port]/<model>/<database>`.
pub const SCHEME: &str = "tdb";

/// The port used when a URL has none.
pub const DEFAULT_PORT: u16 = 7878;

/// Where to connect and what to ask for: a `tdb://` URL.
///
/// ```text
/// tdb://127.0.0.1:7878/keyval/main
/// tdb://localhost/graph/social        (port 7878)
/// tdb://[::1]:7878/vector/docs
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
    route: Route,
}

/// Why a `tdb://` URL is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    /// The URL does not start with `tdb://`.
    Scheme,
    /// There is no host.
    MissingHost,
    /// The host is not a name, an IPv4 address or a bracketed IPv6 address.
    BadHost(String),
    /// The URL has `user@` or `user:password@` before the host. Nothing of it
    /// is repeated in the error.
    UserInfo,
    /// The port is not a number from 0 to 65535.
    BadPort(String),
    /// The path is not a valid [`Route`].
    Route(RouteError),
}

impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointError::Scheme => write!(f, "the URL must start with {SCHEME}://"),
            EndpointError::MissingHost => f.write_str("the URL has no host"),
            EndpointError::BadHost(host) => write!(
                f,
                "bad host {host:?} (write an IPv6 address in brackets: [::1])"
            ),
            EndpointError::UserInfo => f.write_str("credentials in the URL are not supported"),
            EndpointError::BadPort(port) => write!(f, "bad port {port:?}"),
            EndpointError::Route(e) => write!(f, "bad route: {e}"),
        }
    }
}

impl std::error::Error for EndpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EndpointError::Route(e) => Some(e),
            _ => None,
        }
    }
}

impl From<RouteError> for EndpointError {
    fn from(e: RouteError) -> Self {
        EndpointError::Route(e)
    }
}

impl Endpoint {
    pub fn new(host: impl Into<String>, port: u16, route: Route) -> Endpoint {
        Endpoint {
            host: host.into(),
            port,
            route,
        }
    }

    /// The host name or address, without the brackets of an IPv6 address.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn route(&self) -> &Route {
        &self.route
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "{SCHEME}://[{}]:{}{}", self.host, self.port, self.route)
        } else {
            write!(f, "{SCHEME}://{}:{}{}", self.host, self.port, self.route)
        }
    }
}

impl FromStr for Endpoint {
    type Err = EndpointError;

    fn from_str(url: &str) -> Result<Endpoint, EndpointError> {
        let rest = url
            .strip_prefix(SCHEME)
            .and_then(|r| r.strip_prefix("://"))
            .ok_or(EndpointError::Scheme)?;
        let (authority, path) = match rest.find('/') {
            Some(i) => rest.split_at(i),
            None => (rest, ""),
        };
        let (host, port) = split_authority(authority)?;
        Ok(Endpoint {
            host: host.to_owned(),
            port,
            route: path.parse()?,
        })
    }
}

/// Split `host`, `host:port`, `[v6]` or `[v6]:port`.
fn split_authority(authority: &str) -> Result<(&str, u16), EndpointError> {
    // Before anything is split or echoed: `user:password@host` must not be
    // taken for a host and a port, or have its password repeated.
    if authority.contains('@') {
        return Err(EndpointError::UserInfo);
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner
            .split_once(']')
            .ok_or_else(|| EndpointError::BadHost(authority.to_owned()))?;
        match after {
            "" => (host, None),
            _ => (
                host,
                Some(
                    after
                        .strip_prefix(':')
                        .ok_or_else(|| EndpointError::BadHost(authority.to_owned()))?,
                ),
            ),
        }
    } else {
        // More than one colon outside brackets is a bare IPv6 address, which
        // cannot be told from a host and a port.
        if authority.matches(':').count() > 1 {
            return Err(EndpointError::BadHost(authority.to_owned()));
        }
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };

    if host.is_empty() {
        return Err(EndpointError::MissingHost);
    }
    // Brackets are for IPv6 addresses, and only for them.
    let bracketed = authority.starts_with('[');
    if bracketed != host.contains(':') || host.contains(['/', '?', '#', ' ']) {
        return Err(EndpointError::BadHost(authority.to_owned()));
    }
    let port = match port {
        None => DEFAULT_PORT,
        // Digits only: `u16::from_str` would also take `+80`.
        Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => p
            .parse()
            .map_err(|_| EndpointError::BadPort(p.to_owned()))?,
        Some(p) => return Err(EndpointError::BadPort(p.to_owned())),
    };
    Ok((host, port))
}
