//! Mail-in's laws ([`suzatary_mail::mail::puller::Puller`]), read in the maildir a pull writes.

use std::fmt;

use super::{Broken, hold};
use suzatary_mail::mail::fault::Fault;
use suzatary_mail::mail::puller::{Puller, Unpulled};

/// A law of a puller, in the order the suite asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Law {
    /// After a pull, the INBOX's leaf holds every server INBOX message byte for byte, once each.
    Lands,
    /// A pull that finds nothing new on the server leaves the maildir as it was.
    Once,
    /// An unreachable server fails the pull in the implementation's class, never a success.
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

pub trait Rig {
    type Puller: Puller;

    const UNREACHABLE: Fault;

    /// `messages` oldest first.
    fn holding(&self, messages: &[Vec<u8>]) -> Self::Puller;

    fn unreachable(&self) -> Self::Puller;

    /// Each file under the INBOX leaf's `new/` or `cur/`.
    fn landed(&self) -> Vec<Vec<u8>>;
}

/// Before each pull the server gains the next of `arrivals`; every message is dated after `since`.
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
