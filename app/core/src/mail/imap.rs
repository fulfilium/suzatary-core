//! IMAP sync with no I/O: [`pull`] is both `sync` and `backfill`, over a
//! [`Session`] and a [`Maildir`], so the tests drive a real sync against fakes.
//!
//! - A UID means something only within one UIDVALIDITY (RFC 3501); see
//!   [`reconcile`].
//! - The file name is a content digest, `<sha256(raw)[..16]>.u<uid>.ms` plus
//!   `:2,S` when read: notmuch keys its index on the path, so the spelling is
//!   fixed and a refetch rewrites one file.
//! - Read-flag pushback covers only UIDs this state fetched, and each once.
//!
//! UID commands throughout, never sequence numbers: they coincide only in a
//! mailbox that never expunged, so mixing them flags the wrong message read.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

/// A sync could not be carried out.
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    /// The server refused, or the connection failed.
    #[error("the mail server refused {what}: {why}")]
    Server {
        /// Which command — "SELECT INBOX", "UID FETCH 42".
        what: String,
        /// What it said.
        why: String,
    },
    /// The mailbox reported no UIDVALIDITY, which every server must.
    #[error("{0} reported no UIDVALIDITY")]
    NoUidValidity(String),
    /// The maildir could not be read or written; it is a rebuildable cache.
    #[error("the maildir could not be {what}: {why}")]
    Maildir {
        /// "read" or "written".
        what: &'static str,
        /// What went wrong.
        why: String,
    },
    /// The state file could not be read or written.
    #[error("the sync state could not be {what}: {why}")]
    State {
        /// "read" or "written".
        what: &'static str,
        /// What went wrong.
        why: String,
    },
    /// No usable token for this account.
    #[error("{0}")]
    Unauthorised(String),
    /// The account holds no grant, or Microsoft refused it in words only a sign-in answers.
    #[error(transparent)]
    NeedsLogin(crate::mail::oauth::NeedsLogin),
}

impl ImapError {
    /// Its class: a sign-in only a person gives, or anything else.
    #[must_use]
    pub const fn class(&self) -> crate::connect::Failure {
        match self {
            ImapError::NeedsLogin(_) => crate::connect::Failure::NeedsLogin,
            ImapError::Server { .. }
            | ImapError::NoUidValidity(_)
            | ImapError::Maildir { .. }
            | ImapError::State { .. }
            | ImapError::Unauthorised(_) => crate::connect::Failure::Unclassified,
        }
    }
}

/// What a `SELECT`/`EXAMINE` answered that this machine needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The generation every UID below is relative to.
    pub uidvalidity: String,
}

/// The server name of the inbox, which RFC 3501 makes case-insensitive.
pub const INBOX: &str = "INBOX";

/// An IMAP conversation as a capability, UID-based throughout.
pub trait Session {
    /// `LIST "" "*"`: the name of every folder the server holds.
    fn list(&mut self) -> Result<Vec<String>, ImapError>;

    /// Select a mailbox. `readonly` is `EXAMINE`, through which the server
    /// cannot be mutated, so a backfill can never mark a mailbox all-read.
    fn select(&mut self, folder: &str, readonly: bool) -> Result<Selected, ImapError>;

    /// `UID SEARCH <criteria>`, in the server's order.
    fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, ImapError>;

    /// `UID FETCH <uid> (BODY.PEEK[])`, so reading does not mark it read
    /// upstream. `None` when the message was expunged since the search.
    fn uid_fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>, ImapError>;

    /// `UID STORE <uid> +FLAGS.SILENT (\Seen)`.
    fn uid_mark_seen(&mut self, uid: u32) -> Result<(), ImapError>;
}

/// The maildir this account's folder lands in, bound to its directory when
/// built so the core handles no path.
pub trait Maildir {
    /// Write `raw` under `name`, into `new/` when unread and `cur/` otherwise;
    /// idempotent by name, never a second copy.
    fn write(&self, name: &str, unread: bool, raw: &[u8]) -> Result<(), ImapError>;

    /// Every file name under `new/` and `cur/`, in any order; filtering is
    /// [`locally_seen_uids`]'s, so "read" is one rule.
    fn file_names(&self) -> Result<Vec<String>, ImapError>;
}

/// Which UIDs this folder has fetched and which read-flags were pushed. UIDs
/// are numbers so the persisted sets sort `9` before `10`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncState {
    /// The generation the two sets below are relative to. `None` means never
    /// recorded, which [`reconcile`] adopts rather than treats as a mismatch.
    #[serde(default)]
    pub uidvalidity: Option<String>,
    /// The UIDs already written into the maildir.
    #[serde(default)]
    pub synced: BTreeSet<u32>,
    /// The UIDs already flagged `\Seen` on the server.
    #[serde(default)]
    pub seen_pushed: BTreeSet<u32>,
}

/// Fit a loaded state to the mailbox's current UIDVALIDITY.
///
/// A recorded mismatch drops both UID sets, since every stored UID may now name
/// another message; an unrecorded generation is adopted with nothing dropped.
#[must_use]
pub fn reconcile(state: SyncState, current: &str) -> SyncState {
    match &state.uidvalidity {
        Some(recorded) if recorded != current => SyncState {
            uidvalidity: Some(current.to_owned()),
            synced: BTreeSet::new(),
            seen_pushed: BTreeSet::new(),
        },
        _ => SyncState {
            uidvalidity: Some(current.to_owned()),
            ..state
        },
    }
}

// --- Dates -------------------------------------------------------------------

/// How far back the routine INBOX pull reaches, in days: the corpus limit.
pub const SYNC_WINDOW_DAYS: u64 = 7;

/// Spelled out, not taken from a locale: an IMAP server parses English only.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// An IMAP `SINCE` date — `dd-Mon-yyyy`.
#[must_use]
pub fn imap_date(date: chrono::NaiveDate) -> String {
    use chrono::Datelike as _;
    let month = MONTHS[(date.month() as usize) - 1];
    format!("{:02}-{month}-{}", date.day(), date.year())
}

/// The first day the routine INBOX pull reaches on `today`.
#[must_use]
pub fn window_start(today: chrono::NaiveDate) -> chrono::NaiveDate {
    today
        .checked_sub_days(chrono::Days::new(SYNC_WINDOW_DAYS))
        .unwrap_or(chrono::NaiveDate::MIN)
}

// --- Maildir names ----------------------------------------------------------

/// The file name a fetched message lands under.
///
/// A content digest, so a refetch rewrites the same file, carrying the UID so a
/// locally-read file can be flagged `\Seen` upstream.
#[must_use]
pub fn maildir_name(raw: &[u8], uid: u32, unread: bool) -> String {
    let digest = hex::encode(Sha256::digest(raw));
    let name = format!("{}.u{uid}.ms", &digest[..16]);
    if unread { name } else { format!("{name}:2,S") }
}

/// The server UID a file name carries, if it has one.
///
/// Files written by another syncer (lieer, mbsync) have none, which is not a
/// failure.
#[must_use]
pub fn uid_of_maildir_name(name: &str) -> Option<u32> {
    // `.u<digits>.ms`, anywhere in the name — the flags suffix may follow it.
    let after = name.split(".u").nth(1)?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !after[digits.len()..].starts_with(".ms") {
        return None;
    }
    digits.parse().ok()
}

/// Is this file marked read?
///
/// The maildir `S` flag, which notmuch (`synchronize_flags=true`) writes when a
/// message's `unread` tag is removed.
#[must_use]
pub fn is_seen_name(name: &str) -> bool {
    match name.split_once(":2,") {
        Some((_, flags)) => flags.contains('S'),
        None => false,
    }
}

/// The UIDs of messages read locally, out of a maildir listing.
#[must_use]
pub fn locally_seen_uids<S: AsRef<str>>(names: &[S]) -> BTreeSet<u32> {
    names
        .iter()
        .filter(|name| is_seen_name(name.as_ref()))
        .filter_map(|name| uid_of_maildir_name(name.as_ref()))
        .collect()
}

// --- The pull ---------------------------------------------------------------

/// One pull, of one of the two kinds the syncer runs; every setting a pull has is a function
/// of its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pull {
    /// The routine pull of a folder that is triage input: mail since `since`, seen or unseen,
    /// landing unread, with local read-flags pushed back.
    ///
    /// Seen or unseen on purpose: the queue is hash-gated, so mail read on the phone before a
    /// sync must still arrive to be triaged exactly once.
    Window {
        /// The server-side folder name, unquoted. The transport quotes it.
        folder: String,
        /// The first day searched, on the owner's calendar.
        since: chrono::NaiveDate,
    },
    /// A read-only pull of a whole server folder into its own maildir leaf: `EXAMINE`d, so a
    /// full-history pull can never mark a real mailbox all-read, landing read, since it is
    /// history.
    Backfill {
        /// The server-side folder name, unquoted. The transport quotes it.
        folder: String,
        /// Fetch at most this many, newest first, for a trial backfill.
        limit: Option<usize>,
    },
}

impl Pull {
    /// The server-side folder.
    #[must_use]
    pub fn folder(&self) -> &str {
        match self {
            Pull::Window { folder, .. } | Pull::Backfill { folder, .. } => folder,
        }
    }

    /// The `UID SEARCH` criteria.
    #[must_use]
    pub fn criteria(&self) -> String {
        match self {
            Pull::Window { since, .. } => format!("SINCE {}", imap_date(*since)),
            Pull::Backfill { .. } => "ALL".to_owned(),
        }
    }

    /// `EXAMINE` rather than `SELECT`.
    #[must_use]
    pub fn readonly(&self) -> bool {
        match self {
            Pull::Window { .. } => false,
            Pull::Backfill { .. } => true,
        }
    }

    /// Whether messages land unread, and so enter the triage queue; read-flags are pushed back
    /// upstream exactly when they do, since only the routine pull selects its folder writable.
    #[must_use]
    pub fn unread(&self) -> bool {
        !self.readonly()
    }

    /// The cap on how many are fetched, newest first.
    #[must_use]
    pub fn limit(&self) -> Option<usize> {
        match self {
            Pull::Window { .. } => None,
            Pull::Backfill { limit, .. } => *limit,
        }
    }
}

/// What one pull did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pulled {
    /// How many messages were newly written.
    pub fetched: usize,
    /// How many messages the search found and the fetch did not return: expunged since the
    /// search, or held back by the server. None is recorded as fetched, so the next pull asks
    /// again, and the count is in the sync's line, never dropped.
    pub unreturned: usize,
    /// How many read-flags were pushed upstream.
    pub pushed: usize,
    /// The state to persist. The caller writes it; this module decides it.
    pub state: SyncState,
}

/// Run one pull: every decision here, every effect the capabilities'.
pub fn pull<S: Session + ?Sized, M: Maildir + ?Sized>(
    session: &mut S,
    maildir: &M,
    state: SyncState,
    plan: &Pull,
) -> Result<Pulled, ImapError> {
    let selected = session.select(plan.folder(), plan.readonly())?;
    let state = reconcile(state, &selected.uidvalidity);

    let found = session.uid_search(&plan.criteria())?;
    let mut wanted: Vec<u32> = found
        .into_iter()
        .filter(|uid| !state.synced.contains(uid))
        .collect();
    if let Some(cap) = plan.limit()
        && wanted.len() > cap
    {
        // Newest first: the server answers in ascending UID order.
        wanted.drain(..wanted.len() - cap);
    }

    let asked = wanted.len();
    let mut fetched: BTreeSet<u32> = BTreeSet::new();
    for uid in wanted {
        // A message expunged since the search costs one message, not the run.
        let Some(raw) = session.uid_fetch(uid)? else {
            continue;
        };
        maildir.write(&maildir_name(&raw, uid, plan.unread()), plan.unread(), &raw)?;
        fetched.insert(uid);
    }

    let mut pushed: BTreeSet<u32> = BTreeSet::new();
    if plan.unread() {
        let known: BTreeSet<u32> = state.synced.union(&fetched).copied().collect();
        let names = maildir.file_names()?;
        // Only a UID this syncer fetched can be pushed, and only once.
        let to_push: Vec<u32> = locally_seen_uids(&names)
            .into_iter()
            .filter(|uid| known.contains(uid) && !state.seen_pushed.contains(uid))
            .collect();
        for uid in to_push {
            session.uid_mark_seen(uid)?;
            pushed.insert(uid);
        }
    }

    Ok(Pulled {
        fetched: fetched.len(),
        unreturned: asked - fetched.len(),
        pushed: pushed.len(),
        state: SyncState {
            uidvalidity: state.uidvalidity,
            synced: state.synced.union(&fetched).copied().collect(),
            seen_pushed: state.seen_pushed.union(&pushed).copied().collect(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    fn inbox() -> Pull {
        Pull::Window {
            folder: INBOX.to_owned(),
            since: chrono::NaiveDate::from_ymd_opt(2026, 9, 1).expect("a date"),
        }
    }

    fn backfill(limit: Option<usize>) -> Pull {
        Pull::Backfill {
            folder: "Sent Items".to_owned(),
            limit,
        }
    }

    /// A whole IMAP server, as a value.
    struct FakeServer {
        uidvalidity: String,
        messages: BTreeMap<u32, Vec<u8>>,
        /// UIDs the server can no longer produce — expunged between the search
        /// and the fetch.
        vanished: BTreeSet<u32>,
        selected_readonly: Option<bool>,
        stored_seen: Vec<u32>,
        searched: Vec<String>,
    }

    impl FakeServer {
        fn new(uidvalidity: &str, messages: &[(u32, &str)]) -> Self {
            Self {
                uidvalidity: uidvalidity.to_owned(),
                messages: messages
                    .iter()
                    .map(|(uid, body)| (*uid, body.as_bytes().to_vec()))
                    .collect(),
                vanished: BTreeSet::new(),
                selected_readonly: None,
                stored_seen: Vec::new(),
                searched: Vec::new(),
            }
        }
    }

    impl Session for FakeServer {
        fn list(&mut self) -> Result<Vec<String>, ImapError> {
            Ok(vec![INBOX.to_owned()])
        }
        fn select(&mut self, _folder: &str, readonly: bool) -> Result<Selected, ImapError> {
            self.selected_readonly = Some(readonly);
            Ok(Selected {
                uidvalidity: self.uidvalidity.clone(),
            })
        }
        fn uid_search(&mut self, criteria: &str) -> Result<Vec<u32>, ImapError> {
            self.searched.push(criteria.to_owned());
            Ok(self.messages.keys().copied().collect())
        }
        fn uid_fetch(&mut self, uid: u32) -> Result<Option<Vec<u8>>, ImapError> {
            if self.vanished.contains(&uid) {
                return Ok(None);
            }
            Ok(self.messages.get(&uid).cloned())
        }
        fn uid_mark_seen(&mut self, uid: u32) -> Result<(), ImapError> {
            // A read-only select may not STORE, as on a real server.
            assert_ne!(
                self.selected_readonly,
                Some(true),
                "a read-only session must never STORE"
            );
            self.stored_seen.push(uid);
            Ok(())
        }
    }

    /// A maildir, as a map from name to bytes.
    #[derive(Default)]
    struct FakeMaildir {
        files: RefCell<BTreeMap<String, Vec<u8>>>,
    }

    impl FakeMaildir {
        fn with(names: &[&str]) -> Self {
            let me = Self::default();
            for name in names {
                me.files.borrow_mut().insert((*name).to_owned(), Vec::new());
            }
            me
        }
        fn names(&self) -> Vec<String> {
            self.files.borrow().keys().cloned().collect()
        }
    }

    impl Maildir for FakeMaildir {
        fn write(&self, name: &str, _unread: bool, raw: &[u8]) -> Result<(), ImapError> {
            self.files
                .borrow_mut()
                .insert(name.to_owned(), raw.to_vec());
            Ok(())
        }
        fn file_names(&self) -> Result<Vec<String>, ImapError> {
            Ok(self.names())
        }
    }

    // --- reconcile -----------------------------------------------------------

    fn state(validity: Option<&str>, synced: &[u32], pushed: &[u32]) -> SyncState {
        SyncState {
            uidvalidity: validity.map(str::to_owned),
            synced: synced.iter().copied().collect(),
            seen_pushed: pushed.iter().copied().collect(),
        }
    }

    /// The same generation keeps every UID, a changed one discards both sets, and an
    /// unrecorded one is adopted without dropping anything.
    #[test]
    fn a_uid_set_survives_exactly_its_own_generation() {
        for (held, server, reconciled) in [
            (
                state(Some("7"), &[1, 2], &[1]),
                "7",
                state(Some("7"), &[1, 2], &[1]),
            ),
            (
                state(Some("7"), &[1, 2], &[1]),
                "8",
                state(Some("8"), &[], &[]),
            ),
            (state(None, &[1], &[]), "7", state(Some("7"), &[1], &[])),
            (SyncState::default(), "7", state(Some("7"), &[], &[])),
        ] {
            assert_eq!(reconcile(held.clone(), server), reconciled, "{held:?}");
        }
    }

    // --- Names ---------------------------------------------------------------

    /// The name is the content, so a refetch rewrites one file.
    #[test]
    fn the_maildir_name_is_a_content_digest_and_carries_the_uid() {
        let unread = maildir_name(b"hello", 12345, true);
        assert!(unread.ends_with(".u12345.ms"), "{unread}");
        assert_eq!(unread.len(), 16 + ".u12345.ms".len());
        // The same bytes under the same UID are the same file.
        assert_eq!(maildir_name(b"hello", 12345, true), unread);
        // Different bytes are a different file.
        assert_ne!(maildir_name(b"hell0", 12345, true), unread);
        // Landing already-read appends the maildir flag.
        assert_eq!(
            maildir_name(b"hello", 12345, false),
            format!("{unread}:2,S")
        );
    }

    #[test]
    fn a_uid_is_read_back_out_of_a_name_with_or_without_flags() {
        assert_eq!(
            uid_of_maildir_name("a1b2c3d4e5f6a7b8.u12345.ms"),
            Some(12345)
        );
        assert_eq!(
            uid_of_maildir_name("a1b2c3d4e5f6a7b8.u12345.ms:2,S"),
            Some(12345)
        );
        // Files another syncer wrote carry none.
        assert_eq!(uid_of_maildir_name("7d05a21f12b7b0fc.ms"), None);
        assert_eq!(uid_of_maildir_name("1752513600.123456.ms:2,S"), None);
        assert_eq!(uid_of_maildir_name("x.unot.ms"), None);
    }

    #[test]
    fn the_seen_flag_is_an_s_in_the_flags_part_and_nowhere_else() {
        assert!(is_seen_name("x.u1.ms:2,S"));
        assert!(is_seen_name("x.u1.ms:2,RS"));
        assert!(!is_seen_name("x.u1.ms"));
        assert!(!is_seen_name("x.u1.ms:2,"));
        assert!(!is_seen_name("x.u1.ms:2,R"));
        // An S in the digest is not a flag.
        assert!(!is_seen_name("Sabcdef.u1.ms"));
    }

    // --- The pull ------------------------------------------------------------

    #[test]
    fn a_first_inbox_pull_fetches_everything_the_search_found() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two"), (3, "three")]);
        let maildir = FakeMaildir::default();
        let done = pull(&mut server, &maildir, SyncState::default(), &inbox()).expect("a pull");

        assert_eq!(done.fetched, 3);
        assert_eq!(done.pushed, 0);
        assert_eq!(done.state.synced, [1u32, 2, 3].into_iter().collect());
        assert_eq!(done.state.uidvalidity.as_deref(), Some("7"));
        assert_eq!(server.searched, vec!["SINCE 01-Sep-2026".to_owned()]);
        assert_eq!(server.selected_readonly, Some(false));
        assert_eq!(maildir.names().len(), 3);
    }

    /// A second pull over the same mailbox fetches nothing.
    #[test]
    fn a_second_pull_over_the_same_mailbox_fetches_nothing() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two")]);
        let maildir = FakeMaildir::default();
        let first = pull(&mut server, &maildir, SyncState::default(), &inbox()).expect("a pull");
        let again = pull(&mut server, &maildir, first.state.clone(), &inbox()).expect("a pull");
        assert_eq!(again.fetched, 0);
        assert_eq!(again.state.synced, first.state.synced);
    }

    /// A message expunged between search and fetch costs one message, not the run.
    #[test]
    fn an_expunged_message_is_skipped_and_the_rest_still_arrive() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two"), (3, "three")]);
        server.vanished.insert(2);
        let maildir = FakeMaildir::default();
        let done = pull(&mut server, &maildir, SyncState::default(), &inbox()).expect("a pull");
        assert_eq!(done.fetched, 2);
        assert_eq!(
            done.unreturned, 1,
            "the vanished message is counted, not dropped"
        );
        assert_eq!(done.state.synced, [1u32, 3].into_iter().collect());
    }

    /// Pushback needs all three: read locally, fetched by this syncer, not
    /// pushed before.
    #[test]
    fn only_locally_read_messages_this_syncer_fetched_are_pushed_once() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two")]);
        // UID 1 was read locally; UID 9 is a read file from another syncer's
        // numbering that this state has never fetched; a file with no UID at
        // all belongs to lieer.
        let maildir = FakeMaildir::with(&[
            "aaaaaaaaaaaaaaaa.u1.ms:2,S",
            "bbbbbbbbbbbbbbbb.u9.ms:2,S",
            "1752513600.7.ms:2,S",
        ]);
        let first = pull(&mut server, &maildir, SyncState::default(), &inbox()).expect("a pull");
        assert_eq!(first.pushed, 1);
        assert_eq!(server.stored_seen, vec![1]);
        assert_eq!(first.state.seen_pushed, [1u32].into_iter().collect());

        // A second run pushes nothing.
        let again = pull(&mut server, &maildir, first.state, &inbox()).expect("a pull");
        assert_eq!(again.pushed, 0);
        assert_eq!(server.stored_seen, vec![1]);
    }

    /// A backfill is read-only, lands already-read, and pushes nothing: one that
    /// pushed flags would mark a real mailbox all-read.
    #[test]
    fn a_backfill_examines_lands_read_and_never_stores() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two"), (3, "three")]);
        let maildir = FakeMaildir::with(&["aaaaaaaaaaaaaaaa.u1.ms:2,S"]);
        let done =
            pull(&mut server, &maildir, SyncState::default(), &backfill(None)).expect("a backfill");

        assert_eq!(server.selected_readonly, Some(true));
        assert_eq!(server.searched, vec!["ALL".to_owned()]);
        assert_eq!(done.fetched, 3);
        assert_eq!(done.pushed, 0);
        assert!(server.stored_seen.is_empty());
        assert!(done.state.seen_pushed.is_empty());
        // Everything landed read, so none of it is triage input.
        assert!(
            maildir
                .names()
                .iter()
                .filter(|name| name.contains(".u2.") || name.contains(".u3."))
                .all(|name| name.ends_with(":2,S")),
            "{:?}",
            maildir.names()
        );
    }

    /// A limited backfill takes the newest.
    #[test]
    fn a_limited_backfill_takes_the_newest() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two"), (3, "three"), (4, "four")]);
        let maildir = FakeMaildir::default();
        let done = pull(
            &mut server,
            &maildir,
            SyncState::default(),
            &backfill(Some(2)),
        )
        .expect("a backfill");
        assert_eq!(done.fetched, 2);
        assert_eq!(done.state.synced, [3u32, 4].into_iter().collect());
    }

    /// A UIDVALIDITY reset refetches, since the stored UIDs name other messages.
    #[test]
    fn a_uidvalidity_reset_refetches_the_window() {
        let mut server = FakeServer::new("7", &[(1, "one"), (2, "two")]);
        let maildir = FakeMaildir::default();
        let first = pull(&mut server, &maildir, SyncState::default(), &inbox()).expect("a pull");
        assert_eq!(first.fetched, 2);

        server.uidvalidity = "8".to_owned();
        let after = pull(&mut server, &maildir, first.state, &inbox()).expect("a pull");
        assert_eq!(after.fetched, 2);
        assert_eq!(after.state.uidvalidity.as_deref(), Some("8"));
        // And the maildir did not grow: the name is the content.
        assert_eq!(maildir.names().len(), 2);
    }

    /// The state file round-trips.
    #[test]
    fn the_state_round_trips_through_json() {
        let held = state(Some("7"), &[3, 1, 2], &[2]);
        let printed = serde_json::to_string(&held).expect("serialises");
        // Numbers, sorted numerically. A set of strings would write 10 before 9.
        assert!(printed.contains("\"synced\":[1,2,3]"), "{printed}");
        let read: SyncState = serde_json::from_str(&printed).expect("parses");
        assert_eq!(read, held);
        // A file missing every field is an empty state, not a refusal.
        assert_eq!(
            serde_json::from_str::<SyncState>("{}").expect("parses"),
            SyncState::default()
        );
    }
}
