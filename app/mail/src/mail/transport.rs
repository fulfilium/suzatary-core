//! Mail-out as a port: one message onto one account's wire.

use super::send::{Outgoing, SendError, Sent};

/// What puts a message on a wire for one account: Microsoft's SMTP, the Proton Bridge, the
/// Gmail API, or [`super::outbox::Outbox`] in memory. The message was checked when it was
/// built ([`Outgoing::new`]); an implementation sends the envelope `to + cc` and nothing else,
/// and says of a failure whether the message may have gone; `conformance::transport` holds any
/// implementation to that.
///
/// Reached only by the human's `send` and the owner's tap, and held in no effect record, so no
/// verb can send.
pub trait Transport {
    /// Send `out` under `message_id` (bare), dated `at` (epoch seconds).
    ///
    /// # Errors
    /// [`SendError::Refused`] or [`SendError::WrongIdentity`] for an envelope this transport
    /// cannot carry, before anything is opened, else the transport's own failure.
    fn send(&self, out: &Outgoing, message_id: &str, at: i64) -> Result<Sent, SendError>;
}

/// What one send put on the far end's wire: the envelope and the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivered {
    /// The envelope's sender.
    pub from: String,
    /// The envelope's recipients, in the order they were given.
    pub to: Vec<String>,
    /// The message, as the far end read it.
    pub message: Vec<u8>,
}
