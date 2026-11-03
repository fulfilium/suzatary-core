//! The folders a Microsoft account declares, as its sync pulls them: each leaf of its
//! `inbox_folders` as a window of recent mail that lands unread and is triage input, then its sent
//! folder whole, read-only, landing read in the [`SENT_LEAF`]. Each is checked against the folders
//! the server lists, and the account's line names every folder with what its pull came to, so a
//! folder that does not sync fails the account by name and the rest still sync.

use crate::mail::account::MsImap;
use crate::mail::imap::{INBOX, ImapError, Pull, Pulled};
use crate::mail::puller::Unpulled;

/// The maildir leaf the sent folder lands in.
pub const SENT_LEAF: &str = "Sent";

/// One declared folder: the maildir leaf it lands in and the pull that fills it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The leaf under the account's maildir.
    pub leaf: String,
    /// The pull, which names the server folder.
    pub pull: Pull,
}

impl Declared {
    /// Whether the server's `LIST` names this folder: INBOX in any case (RFC 3501 §5.1), any
    /// other folder exactly.
    #[must_use]
    pub fn listed_in(&self, names: &[String]) -> bool {
        let folder = self.pull.folder();
        names
            .iter()
            .any(|name| name == folder || (folder == INBOX && name.eq_ignore_ascii_case(INBOX)))
    }
}

/// Every folder `ms` declares, its windows reaching back to `since`: the inbox folders in the
/// instance file's order, then the sent folder.
#[must_use]
pub fn declared(ms: &MsImap, since: chrono::NaiveDate) -> Vec<Declared> {
    ms.inbox_folders
        .as_slice()
        .iter()
        .map(|leaf| Declared {
            leaf: leaf.clone(),
            pull: Pull::Window {
                folder: server_folder(leaf),
                since,
            },
        })
        .chain(std::iter::once(Declared {
            leaf: SENT_LEAF.to_owned(),
            pull: Pull::Backfill {
                folder: ms.sent_folder.clone(),
                limit: None,
            },
        }))
        .collect()
}

/// The server folder a leaf of `inbox_folders` is pulled from: the leaf `Inbox` is the server's
/// INBOX, and any other leaf is the folder of its own name.
fn server_folder(leaf: &str) -> String {
    if leaf.eq_ignore_ascii_case(INBOX) {
        INBOX.to_owned()
    } else {
        leaf.to_owned()
    }
}

/// What one declared folder's pull came to.
#[derive(Debug)]
pub enum Landed {
    /// It was pulled.
    Pulled(Pulled),
    /// The server lists no folder of its name, so nothing was asked of it.
    Unlisted,
    /// Its pull failed.
    Failed(ImapError),
}

/// The account's line: every declared folder by its server name with what its pull came to.
/// It is a success only when every folder was pulled, and a failure takes the class of the first
/// folder that failed.
///
/// # Errors
/// [`Unpulled`] when any folder was unlisted or failed, its words the whole line.
pub fn line(landed: &[(Declared, Landed)]) -> Result<String, Unpulled> {
    let said = landed
        .iter()
        .map(|(declared, landed)| {
            let folder = declared.pull.folder();
            match landed {
                Landed::Pulled(Pulled {
                    fetched,
                    unreturned: 0,
                    ..
                }) => format!("{folder}: {fetched} new"),
                Landed::Pulled(Pulled {
                    fetched,
                    unreturned,
                    ..
                }) => format!(
                    "{folder}: {fetched} new, {unreturned} found and not returned by the server"
                ),
                Landed::Unlisted => format!("{folder}: the server lists no folder of this name"),
                Landed::Failed(failed) => format!("{folder}: {failed}"),
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    let class = landed.iter().find_map(|(_, landed)| match landed {
        Landed::Pulled(_) => None,
        Landed::Unlisted => Some(crate::connect::Failure::Unclassified),
        Landed::Failed(failed) => Some(failed.class()),
    });
    match class {
        None => Ok(said),
        Some(class) => Err(Unpulled { class, said }),
    }
}
