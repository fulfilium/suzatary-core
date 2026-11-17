//! A transport held in memory: the reference implementation mail-out's suite is read against.

use std::sync::{Mutex, PoisonError};

use super::send::{Outgoing, SendError, Sent, angle, build_mime};
use super::transport::{Delivered, Transport};

/// A wire that keeps every message put on it, in order. Its meaning is that list: each send
/// appends exactly one [`Delivered`].
#[derive(Debug, Default)]
pub struct Outbox(Mutex<Vec<Delivered>>);

impl Outbox {
    /// Everything delivered so far, oldest first.
    #[must_use]
    pub fn delivered(&self) -> Vec<Delivered> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Transport for Outbox {
    fn send(&self, out: &Outgoing, message_id: &str, at: i64) -> Result<Sent, SendError> {
        let message = build_mime(out, message_id, at)?;
        let sent = Sent {
            message_id: angle(message_id),
            recipients: out.recipients(),
            bytes_sent: message.len(),
        };
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Delivered {
                from: out.sender().to_string(),
                to: out.recipients(),
                message,
            });
        Ok(sent)
    }
}
