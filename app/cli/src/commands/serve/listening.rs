//! Where `serve` listens and what it speaks there: https anywhere, plain http only on loopback.
//!
//! Plain http to a tailnet address carries mail in the clear and lets a page another name
//! resolves here speak to the app, so it does not construct.

use std::net::SocketAddr;
use std::path::PathBuf;

/// A bound address and its protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listening {
    /// Plain http, on a loopback address alone.
    Loopback(SocketAddr),
    /// https, with a certificate and its key (PEM).
    Tls {
        at: SocketAddr,
        cert: PathBuf,
        key: PathBuf,
    },
}

/// Plain http asked for on an address other than loopback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plain(pub SocketAddr);

impl std::fmt::Display for Plain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plain http on {} is refused: serve https there (a certificate and its key), or bind \
             loopback",
            self.0
        )
    }
}

impl std::error::Error for Plain {}

impl Listening {
    /// # Errors
    /// [`Plain`] for no pair on an address that is not loopback.
    pub fn new(at: SocketAddr, tls: Option<(PathBuf, PathBuf)>) -> Result<Self, Plain> {
        match tls {
            Some((cert, key)) => Ok(Listening::Tls { at, cert, key }),
            None if at.ip().is_loopback() => Ok(Listening::Loopback(at)),
            None => Err(Plain(at)),
        }
    }

    #[must_use]
    pub fn at(&self) -> SocketAddr {
        match self {
            Listening::Loopback(at) | Listening::Tls { at, .. } => *at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> SocketAddr {
        text.parse().expect("an address")
    }

    fn pair() -> (PathBuf, PathBuf) {
        (PathBuf::from("cert.pem"), PathBuf::from("key.pem"))
    }

    #[test]
    fn plain_http_listens_on_loopback_alone() {
        for loopback in ["127.0.0.1:8787", "[::1]:8787", "127.0.0.2:80"] {
            assert_eq!(
                Listening::new(at(loopback), None),
                Ok(Listening::Loopback(at(loopback)))
            );
        }
        for elsewhere in [
            "100.64.0.9:443",
            "0.0.0.0:8787",
            "[::]:8787",
            "192.0.2.1:80",
        ] {
            assert_eq!(
                Listening::new(at(elsewhere), None),
                Err(Plain(at(elsewhere))),
                "{elsewhere}"
            );
        }
    }

    #[test]
    fn https_listens_anywhere() {
        for anywhere in ["100.64.0.9:443", "127.0.0.1:8787"] {
            let listening = Listening::new(at(anywhere), Some(pair())).expect("https");
            assert!(matches!(listening, Listening::Tls { .. }));
            assert_eq!(listening.at(), at(anywhere));
        }
    }
}
