//! A server's INBOX and the maildir it is pulled into, both in memory: the reference
//! implementation mail-in's suite is read against.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::fault::Fault;
use super::imap::{INBOX, ImapError, Maildir, Pull, Selected, Session, SyncState, pull};
use super::puller::{Puller, Unpulled};

/// One account's server and maildir. Its pull is the core's IMAP [`pull`], so its meaning is
/// that pull's; clones share the maildir, so a mailroom whose server changed pulls into the
/// same files.
#[derive(Debug, Clone, Default)]
pub struct Mailroom {
    server: Server,
    maildir: Rc<Held>,
}

/// What the server answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Server {
    /// It holds these messages in its INBOX, oldest first.
    Holding(Vec<Vec<u8>>),
    /// It cannot be reached.
    Unreachable,
}

impl Default for Server {
    fn default() -> Self {
        Server::Holding(Vec::new())
    }
}

/// The maildir's files by name, a read flag in the name as on disk, and the sync state its
/// pulls keep.
#[derive(Debug, Default)]
struct Held {
    files: RefCell<BTreeMap<String, Vec<u8>>>,
    state: RefCell<SyncState>,
}

impl Mailroom {
    /// The same maildir, its server now `server`.
    #[must_use]
    pub fn serving(&self, server: Server) -> Self {
        Self {
            server,
            maildir: Rc::clone(&self.maildir),
        }
    }

    /// Every message in the maildir, by file name.
    #[must_use]
    pub fn landed(&self) -> Vec<Vec<u8>> {
        self.maildir.files.borrow().values().cloned().collect()
    }
}

impl Puller for Mailroom {
    fn pull(&self, since: chrono::NaiveDate) -> Result<String, Unpulled> {
        let messages = match &self.server {
            Server::Holding(messages) => messages,
            Server::Unreachable => {
                return Err(Unpulled {
                    class: Fault::Unreachable,
                    said: "the server cannot be reached".to_owned(),
                });
            }
        };
        let state = self.maildir.state.borrow().clone();
        let pulled = pull(
            &mut Inbox(messages),
            self.maildir.as_ref(),
            state,
            &Pull::Window {
                folder: INBOX.to_owned(),
                since,
            },
        )?;
        *self.maildir.state.borrow_mut() = pulled.state;
        Ok(format!("{} new", pulled.fetched))
    }
}

/// The server's INBOX as a session: UID `n` is the `n`th message, in one generation.
struct Inbox<'a>(&'a [Vec<u8>]);

impl Inbox<'_> {
    fn numbered(&self) -> impl Iterator<Item = (u32, &Vec<u8>)> {
        (1..).zip(self.0)
    }
}

impl Session for Inbox<'_> {
    fn list(&mut self) -> Result<Vec<String>, ImapError> {
        Ok(vec![INBOX.to_owned()])
    }

    fn select(&mut self, _folder: &str, _readonly: bool) -> Result<Selected, ImapError> {
        Ok(Selected {
            uidvalidity: "1".to_owned(),
        })
    }

    fn uid_search(&mut self, _criteria: &str) -> Result<Vec<u32>, ImapError> {
        Ok(self.numbered().map(|(uid, _)| uid).collect())
    }

    fn uid_fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>, ImapError> {
        Ok(self
            .numbered()
            .find(|(at, _)| *at == uid)
            .map(|(_, message)| message.clone()))
    }

    fn uid_mark_seen(&mut self, _uid: u32) -> Result<(), ImapError> {
        Ok(())
    }
}

impl Maildir for Held {
    fn write(&self, name: &str, _unread: bool, raw: &[u8]) -> Result<(), ImapError> {
        self.files
            .borrow_mut()
            .insert(name.to_owned(), raw.to_vec());
        Ok(())
    }

    fn file_names(&self) -> Result<Vec<String>, ImapError> {
        Ok(self.files.borrow().keys().cloned().collect())
    }
}
