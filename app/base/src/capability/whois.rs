//! Who a peer is: the one door a [`Login`](super::Login) comes through, absent from the agent.

use super::Login;

/// The whois interpreter could not answer; an unresolved peer is a refusal, not an anonymous one.
#[derive(Debug, thiserror::Error)]
pub enum WhoisError {
    /// The local daemon is not answering, unlike [`Peer::Stranger`], which it answered.
    #[error("the tailnet daemon could not be asked who owns {peer}: {reason}")]
    Unreachable {
        peer: std::net::IpAddr,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Peer {
    /// No login owns it, so it can neither read nor write.
    Stranger,
    /// This box's own node at any address: it reads like loopback, never writes, and its login
    /// is not kept.
    ThisBox,
    Known(Login),
}

pub trait Whois: Sync {
    fn peer(&self, address: std::net::IpAddr) -> Result<Peer, WhoisError>;
}
