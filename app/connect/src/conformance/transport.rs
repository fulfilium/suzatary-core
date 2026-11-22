//! Mail-out's laws ([`suzatary_mail::mail::transport::Transport`]), read at the far end of the
//! wire.

use std::fmt;

use super::{Broken, hold};
use suzatary_mail::mail::send::{Outgoing, angle, build_mime};
use suzatary_mail::mail::transport::{Delivered, Transport};

/// A law of a transport, in the order the suite asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Law {
    /// A message reaches the wire once, from its sender to exactly `to + cc` (no Bcc).
    Delivered,
    /// What arrives is `build_mime`'s message line for line, whatever ends the lines.
    Intact,
}

impl fmt::Display for Law {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Law::Delivered => "a sent message reaches exactly its recipients, once",
            Law::Intact => "what arrives is the message built",
        })
    }
}

/// One message to send, with no attachments, because a multipart boundary is random.
#[derive(Debug, Clone)]
pub struct Case {
    pub out: Outgoing,
    /// Bare, without angle brackets.
    pub message_id: String,
    /// Epoch seconds.
    pub at: i64,
}

/// `delivered` reads the transport's far end.
pub fn check<T: Transport + ?Sized>(
    transport: &T,
    delivered: impl Fn() -> Vec<Delivered>,
    cases: &[Case],
) -> Result<(), Broken<Law>> {
    for case in cases {
        let Case {
            out,
            message_id,
            at,
        } = case;

        let before = delivered().len();
        let sent = transport
            .send(out, message_id, *at)
            .map_err(|failed| Broken::of(Law::Delivered, failed.to_string()))?;
        hold(
            Law::Delivered,
            sent.message_id == angle(message_id) && sent.recipients == out.recipients(),
            || format!("the answer was {sent:?}"),
        )?;
        let now = delivered();
        let [arrived] = now.get(before..).unwrap_or_default() else {
            return Err(Broken::of(
                Law::Delivered,
                format!("{} messages arrived for one send", now.len() - before),
            ));
        };
        let mut to = arrived.to.clone();
        let mut recipients = out.recipients();
        to.sort();
        recipients.sort();
        hold(
            Law::Delivered,
            arrived.from.eq_ignore_ascii_case(out.sender().as_str()) && to == recipients,
            || {
                format!(
                    "from {} to {:?}, for {recipients:?}",
                    arrived.from, arrived.to
                )
            },
        )?;

        let built = build_mime(out, message_id, *at)
            .map_err(|failed| Broken::of(Law::Intact, failed.to_string()))?;
        hold(
            Law::Intact,
            lines(&arrived.message) == lines(&built),
            || {
                format!(
                    "{} bytes arrived for a message of {}",
                    arrived.message.len(),
                    built.len()
                )
            },
        )?;
    }
    Ok(())
}

/// SMTP's DATA ends every line with CRLF, so one final empty line belongs to the wire alone.
fn lines(message: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = message
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .collect();
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}
