//! Mail-out's laws ([`suzatary_mail::mail::transport::Transport`]), read at the far end of the wire.

use std::fmt;

use super::{Broken, hold};
use suzatary_mail::mail::send::{Outgoing, angle, build_mime};
use suzatary_mail::mail::transport::{Delivered, Transport};

/// A law of a transport, in the order the suite asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Law {
    /// A message sent reaches the wire once, from its sender to exactly `to + cc` (no Bcc), and
    /// the answer names its id and those recipients.
    Delivered,
    /// What arrives is the message as `build_mime` prints it, every line, blank ones included, up
    /// to how its lines end.
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

/// One message to send. It has no attachments: a multipart boundary is drawn at random, so only
/// a message without one prints the same twice.
#[derive(Debug, Clone)]
pub struct Case {
    /// The message.
    pub out: Outgoing,
    /// Its bare `Message-Id`.
    pub message_id: String,
    /// Its date, epoch seconds.
    pub at: i64,
}

/// Every law over every case, on one transport whose far end `delivered` reads.
///
/// # Errors
/// The first [`Broken`] law.
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

/// The message's lines, every one kept, empty lines included, whatever ended them: SMTP's DATA
/// writes each line with CRLF and ends the last, so one final empty line is the wire's alone.
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
