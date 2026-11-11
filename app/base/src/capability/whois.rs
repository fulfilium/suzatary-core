//! Who a peer is: the one door a [`Login`](super::Login) comes through, absent from the agent.

use super::Login;

/// The whois interpreter could not answer; an unresolved peer is a refusal, not an anonymous one.
#[derive(Debug, thiserror::Error)]
pub enum WhoisError {
    /// The local daemon is not answering, unlike [`Peer::Stranger`], which it answered.
    #[error("the tailnet daemon could not be asked who owns {peer}: {reason}")]
    Unreachable {
        /// The peer address the question was about.
        peer: std::net::IpAddr,
        /// What the interpreter saw, verbatim.
        reason: String,
    },
}

/// What the tailnet says of a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Peer {
    /// No login owns it, so it can neither read nor write.
    Stranger,
    /// This box's own node, whatever address it dialled: whois names the box's owner for it,
    /// so it reads like loopback and never writes, and that login is not kept.
    ThisBox,
    /// Another node, owned by this login.
    Known(Login),
}

/// Who a peer is.
pub trait Whois: Sync {
    /// What the tailnet says of the peer at this address.
    fn peer(&self, address: std::net::IpAddr) -> Result<Peer, WhoisError>;
}
