//! Mail-in's laws ([`crate::mail::puller::Puller`]), read in the maildir a pull writes.

use std::fmt;

use super::{Broken, hold};
use crate::mail::fault::Fault;
use crate::mail::puller::{Puller, Unpulled};

/// A law of a puller, in the order the suite asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Law {
    /// After a pull, the INBOX's leaf holds every message the server's INBOX holds, byte for
    /// byte and each once, in its `new/` or `cur/`, where the index reads it.
    Lands,
    /// A pull that finds nothing new on the server leaves the maildir as it was.
    Once,
    /// A server that cannot be reached fails the pull, in the class the implementation gives
    /// that failure, which names its fix card; it never succeeds having landed nothing.
    Fails,
}

impl fmt::Display for Law {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Law::Lands => "a pull lands every message the server holds, once",
            Law::Once => "a second pull lands nothing new",
            Law::Fails => "an unreachable server fails the pull",
        })
    }
}

/// An implementation for one account, with its server scripted and its maildir read back.
pub trait Rig {
    /// The puller under test.
    type Puller: Puller;

    /// The class of the failure a pull from an unreachable server gives.
    const UNREACHABLE: Fault;

    /// The puller, its server holding `messages` in its INBOX, oldest first.
    fn holding(&self, messages: &[Vec<u8>]) -> Self::Puller;

    /// The puller, its server unreachable.
    fn unreachable(&self) -> Self::Puller;

    /// Every message in the leaf the server's INBOX lands in: each file under its `new/` or
    /// `cur/`.
    fn landed(&self) -> Vec<Vec<u8>>;
}

/// Every law through `rig`, on one maildir: before each pull the server gains the next of
/// `arrivals`, keeping what it held, and each pull reads from `since`, which every message is
/// dated after.
///
/// # Errors
/// The first [`Broken`] law.
pub fn check<R: Rig>(
    rig: &R,
    since: chrono::NaiveDate,
    arrivals: &[Vec<Vec<u8>>],
) -> Result<(), Broken<Law>> {
    let mut held: Vec<Vec<u8>> = Vec::new();
    for arrived in arrivals {
        held.extend(arrived.iter().cloned());
        let puller = rig.holding(&held);
        puller
            .pull(since)
            .map_err(|failed| Broken::of(Law::Lands, failed.to_string()))?;
        let landed = sorted(rig.landed());
        let server = sorted(held.clone());
        hold(Law::Lands, landed == server, || {
            format!(
                "the server holds {} messages, and the maildir {} that differ",
                server.len(),
                landed.len()
            )
        })?;

        puller
            .pull(since)
            .map_err(|failed| Broken::of(Law::Once, failed.to_string()))?;
        let again = sorted(rig.landed());
        hold(Law::Once, again == landed, || {
            format!(
                "the maildir held {} messages, and {} after a pull that found nothing new",
                landed.len(),
                again.len()
            )
        })?;
    }
    let failed = rig.unreachable().pull(since);
    hold(
        Law::Fails,
        matches!(&failed, Err(Unpulled { class, .. }) if *class == R::UNREACHABLE),
        || {
            format!(
                "a pull from an unreachable server answered {failed:?}, not a failure of class {:?}",
                R::UNREACHABLE
            )
        },
    )
}

fn sorted(mut messages: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    messages.sort();
    messages
}
