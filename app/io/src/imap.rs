//! Microsoft's mailbox over IMAP, and a real maildir to land it in: [`Outlook`] and [`Folder`]
//! interpret the `Session` and [`Maildir`] traits of `suzatary_core::mail::imap`, whose `pull`
//! decides everything. Each keeps the promises the core's properties rest on: UIDs sorted, reads
//! peeked, folders quoted once.
//!
//! [`Outlook::connect`] hands `imap::Client::new` a stream from this workspace's own rustls
//! rather than enabling the crate's `rustls-tls`, which pins an older rustls: one crypto stack
//! to keep patched. The provider is `ring`, installed lazily because a caller may never reach
//! `main.rs`'s install.
//!
//! Folder names are passed raw: `imap`'s `select`/`examine` quote and escape their argument
//! themselves, so quoting here would send `SELECT ""Sent Items""`, a mistake no test without a
//! live mailbox would see.
//!
//! Delivery writes `tmp/<name>` and renames into `new/` or `cur/`, so no reader ever sees a
//! half-written message.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::present::Present as _;
use suzatary_core::capability::ImapToken;
use suzatary_core::mail::idle::Wake;
use suzatary_core::mail::imap::{ImapError, Maildir, Selected};

// --- The XOAUTH2 initial response, pure and testable without a socket ------

/// The SASL `XOAUTH2` initial response, unencoded: `Client::authenticate` base64-encodes it.
#[must_use]
pub fn xoauth2(email: &str, token: &str) -> String {
    format!("user={email}\x01auth=Bearer {token}\x01\x01")
}

/// [`imap::Authenticator`] over [`xoauth2`]. Holds the token as plain text for one handshake,
/// because the trait returns owned data; it is never logged or placed in an [`ImapError`].
struct XOAuth2 {
    email: String,
    token: String,
}

impl imap::Authenticator for XOAuth2 {
    type Response = String;

    fn process(&self, _challenge: &[u8]) -> String {
        xoauth2(&self.email, &self.token)
    }
}

// --- Outlook: an IMAP session over this workspace's own rustls -------------

/// Microsoft's IMAP endpoint.
pub const IMAP_HOST: &str = "outlook.office365.com";
/// Implicit TLS.
pub const IMAP_PORT: u16 = 993;

/// How long a read or write may block: room for a large message's fetch, and a stalled server
/// is a failure rather than a hang.
const TIMEOUT: Duration = Duration::from_mins(1);

/// The raw TLS transport underneath the IMAP session: this workspace's own
/// rustls, over a plain TCP socket.
type RawTlsStream = rustls::StreamOwned<rustls::ClientConnection, TcpStream>;

/// `RawTlsStream` in a local type, so the orphan rule lets it implement
/// [`imap::extensions::idle::SetReadTimeout`], which [`Outlook::watch`] needs.
pub struct TlsStream(RawTlsStream);

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl imap::extensions::idle::SetReadTimeout for TlsStream {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::Result<()> {
        self.0
            .sock
            .set_read_timeout(timeout)
            .map_err(imap::Error::Io)
    }
}

/// [`suzatary_core::mail::imap::Session`], against Microsoft's IMAP server; connected by hand
/// because `imap::ClientBuilder` builds only the TLS stacks this crate does not enable. Over
/// any stream, so a session another door authenticated pulls through the same commands.
pub struct Outlook<S: Read + Write = TlsStream> {
    session: imap::Session<S>,
}

impl<S: Read + Write> Outlook<S> {
    /// An authenticated session, nothing selected yet.
    #[must_use]
    pub fn over(session: imap::Session<S>) -> Self {
        Self { session }
    }

    /// `LOGOUT`, consuming the session.
    ///
    /// # Errors
    /// [`ImapError::Server`] if the server answered anything but a normal close (a `BYE` is
    /// one).
    pub fn logout(mut self) -> Result<(), ImapError> {
        self.session.logout().map_err(|error| ImapError::Server {
            what: "LOGOUT".to_owned(),
            why: error.to_string(),
        })
    }
}

impl Outlook {
    /// Connect to `host:port` over TLS and authenticate `email` with
    /// XOAUTH2, using `token`'s bearer secret.
    ///
    /// # Errors
    /// [`ImapError::Server`] for anything short of a completed handshake, and
    /// [`ImapError::Unauthorised`] when the server refuses the XOAUTH2 exchange; neither
    /// ever carries the bearer token.
    pub fn connect(
        host: &str,
        port: u16,
        email: &str,
        token: &ImapToken,
    ) -> Result<Self, ImapError> {
        Ok(Self {
            session: authenticated(host, port, email, token)?,
        })
    }

    /// `IDLE`, blocking until the mailbox changes or `renew_after` elapses; only
    /// `EXISTS`/`RECENT` wake it. The crate's own keepalive is off, so [`Wake::TimedOut`]
    /// returns and [`suzatary_core::mail::idle::after`] decides whether to re-issue IDLE.
    ///
    /// # Errors
    /// [`ImapError::Server`] for anything the connection could not recover from; the caller
    /// reconnects.
    pub fn watch(&mut self, renew_after: Duration) -> Result<Wake, ImapError> {
        let outcome = self
            .session
            .idle()
            .timeout(renew_after)
            .keepalive(false)
            .wait_while(|response| {
                !matches!(
                    response,
                    imap::types::UnsolicitedResponse::Exists(_)
                        | imap::types::UnsolicitedResponse::Recent(_)
                )
            })
            .map_err(|error| ImapError::Server {
                what: "IDLE".to_owned(),
                why: error.to_string(),
            })?;
        Ok(match outcome {
            imap::extensions::idle::WaitOutcome::TimedOut => Wake::TimedOut,
            imap::extensions::idle::WaitOutcome::MailboxChanged => Wake::Changed,
        })
    }
}

/// TCP, TLS and XOAUTH2 to `host:port` as `email`: an authenticated session
/// with no mailbox selected yet, for [`Outlook`] and `crate::junk` alike.
///
/// # Errors
/// As [`Outlook::connect`].
pub(crate) fn authenticated(
    host: &str,
    port: u16,
    email: &str,
    token: &ImapToken,
) -> Result<imap::Session<TlsStream>, ImapError> {
    let what = format!("connecting to {host}:{port}");
    let tcp = TcpStream::connect((host, port)).map_err(|error| ImapError::Server {
        what: what.clone(),
        why: error.to_string(),
    })?;
    tcp.set_read_timeout(Some(TIMEOUT))
        .and_then(|()| tcp.set_write_timeout(Some(TIMEOUT)))
        .map_err(|error| ImapError::Server {
            what: format!("setting a timeout on the connection to {host}:{port}"),
            why: error.to_string(),
        })?;

    // `webpki-roots`, not the platform store: a systemd unit's view of /etc/ssl can be wrong.
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    // The provider is passed, as SMTP's is, so no process-wide default is installed or read.
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|error| ImapError::Server {
        what: "choosing the TLS versions".to_owned(),
        why: error.to_string(),
    })?
    .with_root_certificates(roots)
    .with_no_client_auth();

    let server_name = rustls_pki_types::ServerName::try_from(host.to_owned()).map_err(|error| {
        ImapError::Server {
            what: "resolving the TLS server name".to_owned(),
            why: error.to_string(),
        }
    })?;
    let connection =
        rustls::ClientConnection::new(Arc::new(config), server_name).map_err(|error| {
            ImapError::Server {
                what: "starting the TLS handshake".to_owned(),
                why: error.to_string(),
            }
        })?;
    let stream = TlsStream(rustls::StreamOwned::new(connection, tcp));

    let mut client = imap::Client::new(stream);
    client.read_greeting().map_err(|error| ImapError::Server {
        what: "reading the server's greeting".to_owned(),
        why: error.to_string(),
    })?;

    let authenticator = XOAuth2 {
        email: email.to_owned(),
        token: token.secret().expose().to_owned(),
    };
    let session = client
        .authenticate("XOAUTH2", &authenticator)
        .map_err(|(error, _client)| {
            ImapError::Unauthorised(format!("XOAUTH2 authentication for {email}: {error}"))
        })?;

    Ok(session)
}

impl<S: Read + Write> suzatary_core::mail::imap::Session for Outlook<S> {
    fn list(&mut self) -> Result<Vec<String>, ImapError> {
        let names = self
            .session
            .list(Some(""), Some("*"))
            .map_err(|error| ImapError::Server {
                what: "LIST".to_owned(),
                why: error.to_string(),
            })?;
        Ok(names.iter().map(|name| name.name().to_owned()).collect())
    }

    fn select(&mut self, folder: &str, readonly: bool) -> Result<Selected, ImapError> {
        // Raw: the crate quotes it (module docs).
        let mailbox = if readonly {
            self.session.examine(folder)
        } else {
            self.session.select(folder)
        }
        .map_err(|error| ImapError::Server {
            what: format!("{} {folder}", if readonly { "EXAMINE" } else { "SELECT" }),
            why: error.to_string(),
        })?;
        let uidvalidity = mailbox
            .uid_validity
            .ok_or_else(|| ImapError::NoUidValidity(folder.to_owned()))?;
        Ok(Selected {
            uidvalidity: uidvalidity.to_string(),
        })
    }

    fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, ImapError> {
        let found = self
            .session
            .uid_search(criteria)
            .map_err(|error| ImapError::Server {
                what: format!("UID SEARCH {criteria}"),
                why: error.to_string(),
            })?;
        // Sorted ascending: `Pull::limit` takes the tail as the newest, and a `HashSet` has no
        // order.
        let mut uids: Vec<u32> = found.into_iter().collect();
        uids.sort_unstable();
        Ok(uids)
    }

    fn uid_fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>, ImapError> {
        // `BODY.PEEK[]`, never `BODY[]`: a fetch must not set \Seen, or the read-flag pushback
        // means nothing.
        let fetches = self
            .session
            .uid_fetch(uid.to_string(), "(BODY.PEEK[])")
            .map_err(|error| ImapError::Server {
                what: format!("UID FETCH {uid}"),
                why: error.to_string(),
            })?;
        // The answer for this UID, since a server may send FETCH responses for other messages
        // beside it. No body is a message expunged since the search: `None`, not an error.
        Ok(fetches
            .iter()
            .find(|fetch| fetch.uid == Some(uid))
            .and_then(imap::types::Fetch::body)
            .map(<[u8]>::to_vec))
    }

    fn uid_mark_seen(&mut self, uid: u32) -> Result<(), ImapError> {
        self.session
            .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Seen)")
            .map_err(|error| ImapError::Server {
                what: format!("UID STORE {uid}"),
                why: error.to_string(),
            })?;
        Ok(())
    }
}

// --- Folder: a real maildir leaf --------------------------------------------

/// One maildir leaf, `~/Mail/<subdir>/<leaf>`, in the layout `notmuch new` walks.
pub struct Folder {
    path: PathBuf,
}

impl Folder {
    /// Bind to a directory without touching it; [`Folder::ensure`] makes it.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Create `new/`, `cur/` and `tmp/` under this leaf, idempotently.
    ///
    /// # Errors
    /// [`ImapError::Maildir`] if any of the three could not be created.
    pub fn ensure(&self) -> Result<(), ImapError> {
        for leaf in ["new", "cur", "tmp"] {
            std::fs::create_dir_all(self.path.join(leaf)).map_err(|error| ImapError::Maildir {
                what: "written",
                why: format!("creating {}/{leaf}: {error}", self.path.display()),
            })?;
        }
        Ok(())
    }

    /// Where this leaf is, for the caller's log line.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Maildir for Folder {
    fn write(&self, name: &str, unread: bool, raw: &[u8]) -> Result<(), ImapError> {
        let leaf = if unread { "new" } else { "cur" };
        let landed = self.path.join(leaf).join(name);
        // Staged in `tmp/`, so no reader sees a partial file, and synced with its directory
        // before the caller records the UID as fetched: a power cut never loses a message the
        // state says it holds.
        crate::durable::deliver(
            &landed,
            &self.path.join("tmp"),
            raw,
            crate::durable::Mode::Private,
        )
        .map_err(|error| ImapError::Maildir {
            what: "written",
            why: format!("delivering {}: {error}", landed.display()),
        })
    }

    fn file_names(&self) -> Result<Vec<String>, ImapError> {
        let mut names = Vec::new();
        for leaf in ["new", "cur"] {
            let dir = self.path.join(leaf);
            // A first sync may have no `cur/` yet: an empty listing, not a failure.
            let Some(entries) =
                std::fs::read_dir(&dir)
                    .present()
                    .map_err(|error| ImapError::Maildir {
                        what: "read",
                        why: format!("listing {}: {error}", dir.display()),
                    })?
            else {
                continue;
            };
            for entry in entries {
                let entry = entry.map_err(|error| ImapError::Maildir {
                    what: "read",
                    why: format!("reading an entry under {}: {error}", dir.display()),
                })?;
                // Lossy rather than skipped: a non-UTF-8 name is another syncer's file, still
                // real mail.
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use suzatary_core::mail::imap::{INBOX, Pull, Session, SyncState, maildir_name, pull};

    use super::*;

    /// A short, process- and thread-unique scratch directory.
    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "suzatary-io-imap-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path);
    }

    // --- Folder -----------------------------------------------------------------

    /// Writing the same name twice leaves one file, so a refetch never gives notmuch two
    /// copies of one message.
    #[test]
    fn writing_the_same_name_twice_leaves_one_file() {
        let root = scratch("write-idempotent");
        let folder = Folder::new(&root);
        folder.ensure().expect("ensure");

        folder
            .write("same.ms", true, b"first")
            .expect("first write");
        folder
            .write("same.ms", true, b"second")
            .expect("second write");

        let new_entries: Vec<_> = std::fs::read_dir(root.join("new"))
            .expect("reads new/")
            .collect();
        assert_eq!(new_entries.len(), 1, "exactly one file, not two");
        assert_eq!(
            std::fs::read(root.join("new").join("same.ms")).expect("reads it back"),
            b"second",
            "the rewrite's bytes win"
        );
        cleanup(&root);
    }

    // --- pull, driven for real, against a fake Session and a real Folder ---------

    /// A minimal IMAP server as a value, enough to drive the real `pull`.
    struct FakeSession {
        messages: BTreeMap<u32, Vec<u8>>,
        uidvalidity: String,
        stored_seen: RefCell<Vec<u32>>,
    }

    impl Session for FakeSession {
        fn list(&mut self) -> Result<Vec<String>, ImapError> {
            Ok(vec![INBOX.to_owned()])
        }

        fn select(&mut self, _folder: &str, _readonly: bool) -> Result<Selected, ImapError> {
            Ok(Selected {
                uidvalidity: self.uidvalidity.clone(),
            })
        }

        fn uid_search(&mut self, _criteria: &str) -> Result<Vec<u32>, ImapError> {
            let mut uids: Vec<u32> = self.messages.keys().copied().collect();
            uids.sort_unstable();
            Ok(uids)
        }

        fn uid_fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>, ImapError> {
            Ok(self.messages.get(&uid).cloned())
        }

        fn uid_mark_seen(&mut self, uid: u32) -> Result<(), ImapError> {
            self.stored_seen.borrow_mut().push(uid);
            Ok(())
        }
    }

    /// The real `pull` against a fake `Session` and a real [`Folder`] leaves exactly the files
    /// [`maildir_name`] names: the transport and the core's naming agree.
    #[test]
    fn pull_writes_exactly_what_maildir_name_says_it_should() {
        let root = scratch("pull-payoff");
        let folder = Folder::new(&root);
        folder.ensure().expect("ensure");

        let mut session = FakeSession {
            messages: BTreeMap::from([
                (1, b"one".to_vec()),
                (2, b"two".to_vec()),
                (3, b"three".to_vec()),
            ]),
            uidvalidity: "7".to_owned(),
            stored_seen: RefCell::new(Vec::new()),
        };

        let done = pull(
            &mut session,
            &folder,
            SyncState::default(),
            &Pull::Window {
                folder: INBOX.to_owned(),
                since: chrono::NaiveDate::from_ymd_opt(2026, 9, 1).expect("a date"),
            },
        )
        .expect("a pull");
        assert_eq!(done.fetched, 3);

        let mut expected: Vec<String> = session
            .messages
            .iter()
            .map(|(uid, raw)| maildir_name(raw, *uid, true))
            .collect();
        expected.sort();

        let mut landed = folder.file_names().expect("lists what pull wrote");
        landed.sort();

        assert_eq!(landed, expected);
        cleanup(&root);
    }

    #[test]
    fn ensure_creates_the_three_subdirectories_and_is_idempotent() {
        let root = scratch("ensure");
        let folder = Folder::new(&root);
        folder.ensure().expect("creates the three directories");
        for leaf in ["new", "cur", "tmp"] {
            assert!(root.join(leaf).is_dir(), "{leaf}/ should exist");
        }
        // Idempotent: calling again does not fail.
        folder.ensure().expect("ensure is idempotent");
        cleanup(&root);
    }

    #[test]
    fn an_unread_write_lands_in_new_and_a_read_write_lands_in_cur() {
        let root = scratch("write-split");
        let folder = Folder::new(&root);
        folder.ensure().expect("ensure");

        folder
            .write("unread.ms", true, b"hello unread")
            .expect("writes unread");
        folder
            .write("read.ms:2,S", false, b"hello read")
            .expect("writes read");

        assert_eq!(
            std::fs::read(root.join("new").join("unread.ms")).expect("reads new/"),
            b"hello unread"
        );
        assert_eq!(
            std::fs::read(root.join("cur").join("read.ms:2,S")).expect("reads cur/"),
            b"hello read"
        );
        assert!(!root.join("cur").join("unread.ms").exists());
        assert!(!root.join("new").join("read.ms:2,S").exists());
        cleanup(&root);
    }

    #[test]
    fn file_names_sees_both_directories_and_is_empty_for_a_folder_that_does_not_exist() {
        let root = scratch("file-names");
        let folder = Folder::new(&root);

        // A first sync has no `cur/`.
        assert_eq!(
            folder.file_names().expect("an empty list, not an error"),
            Vec::<String>::new()
        );

        folder.ensure().expect("ensure");
        folder.write("in-new.ms", true, b"a").expect("writes new/");
        folder
            .write("in-cur.ms:2,S", false, b"b")
            .expect("writes cur/");

        let mut names = folder.file_names().expect("lists both directories");
        names.sort();
        assert_eq!(
            names,
            vec!["in-cur.ms:2,S".to_owned(), "in-new.ms".to_owned()]
        );
        cleanup(&root);
    }

    #[test]
    fn nothing_is_left_behind_in_tmp() {
        let root = scratch("tmp-clean");
        let folder = Folder::new(&root);
        folder.ensure().expect("ensure");
        folder.write("a.ms", true, b"a").expect("writes");
        folder.write("b.ms:2,S", false, b"b").expect("writes");

        let tmp_entries: Vec<_> = std::fs::read_dir(root.join("tmp"))
            .expect("reads tmp/")
            .collect();
        assert!(
            tmp_entries.is_empty(),
            "tmp/ should be empty after a write lands"
        );
        cleanup(&root);
    }
}
