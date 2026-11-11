//! A watch on mail the owner expects: a sender or a subject and an expiry, set by the secretary
//! and rung once, by a push, when a matching message has synced.
//!
//! A watch's log is one [`Line::Set`], then at most one [`Line::Rang`] or [`Line::Cancelled`],
//! then at most one [`Line::Delivered`]. An expiry writes nothing: past `until` it reads as
//! [`Standing::Expired`].

use prodrome_core::fpl::Instant;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::mail::corpus::Corpus;
use crate::mail::mailbox::Query;
use crate::reference::MailHash;
use crate::refusal::Refusal;
use crate::store::{Edit, Log};

/// The longest text a watch looks for, in characters, so its query with the owner's accounts
/// stays under the corpus's cap.
pub const MAX_TEXT: usize = 100;
/// The longest a watch stands, in days.
pub const MAX_DAYS: u16 = 60;
/// How long after a ring its push is still owed, whatever the watch's expiry.
pub(crate) const GRACE: chrono::TimeDelta = chrono::TimeDelta::days(1);

/// A watch's name: the first 16 hex digits of the hash of its [`Set`] line.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchId(String);

/// A name that is not 16 lower-case hex digits.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a watch is named by 16 lower-case hex digits")]
pub struct NotAWatchId;

impl WatchId {
    /// The watch `text` names.
    ///
    /// # Errors
    /// [`NotAWatchId`] for anything else.
    pub fn new(text: &str) -> Result<Self, NotAWatchId> {
        (text.len() == 16 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
            .then(|| Self(text.to_owned()))
            .ok_or(NotAWatchId)
    }

    fn of(set: &Set) -> Self {
        let printed = serde_json::to_vec(set).expect("a set line serialises: it holds no map");
        Self(hex::encode(Sha256::digest(printed))[..16].to_owned())
    }

    /// The hex digits, which are also the log's file name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which part of a message a watch looks in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Part {
    /// The sender's address or name.
    Sender,
    /// The subject.
    Subject,
}

/// What a watch matches: words of one part of a message, as the mail index reads them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct On {
    /// Where.
    pub part: Part,
    /// What.
    pub text: Pattern,
}

/// The text a watch looks for: one line of 1 to [`MAX_TEXT`] characters with no double quote,
/// trimmed and lower-cased.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Pattern(String);

impl Pattern {
    /// `text` as a pattern.
    ///
    /// # Errors
    /// A refusal that names the rule `text` breaks.
    pub fn new(text: &str) -> Result<Self, Refusal> {
        let text = text.trim();
        if text.is_empty()
            || text.chars().count() > MAX_TEXT
            || text.chars().any(|c| c.is_control() || c == '"')
        {
            return Err(Refusal::bad_request(format!(
                "a watch looks for one line of 1 to {MAX_TEXT} characters with no double quote"
            )));
        }
        Ok(Self(text.to_lowercase()))
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Pattern {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        Self::new(&text).map_err(|refused| refused.message().to_owned())
    }
}

impl From<Pattern> for String {
    fn from(pattern: Pattern) -> Self {
        pattern.0
    }
}

/// How many days a watch stands: 1 to [`MAX_DAYS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Days(u16);

impl Days {
    /// `days` as a watch's length.
    ///
    /// # Errors
    /// A refusal for 0 or more than [`MAX_DAYS`].
    pub fn new(days: u16) -> Result<Self, Refusal> {
        (1..=MAX_DAYS)
            .contains(&days)
            .then_some(Self(days))
            .ok_or_else(|| Refusal::bad_request(format!("a watch stands for 1 to {MAX_DAYS} days")))
    }
}

/// A line of a watch's log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "line", rename_all = "snake_case")]
pub enum Line {
    /// The watch was set.
    Set(Set),
    /// A message matched, and a push of its link is owed.
    Rang {
        /// The message.
        message: MailHash,
        /// When.
        at: Instant,
    },
    /// The owner cancelled it.
    Cancelled {
        /// When.
        at: Instant,
    },
    /// The push for its ring reached the push service.
    Delivered {
        /// When.
        at: Instant,
    },
}

/// What a watch waits for, from when, and until when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Set {
    /// What it matches.
    pub on: On,
    /// When it was set.
    pub at: Instant,
    /// When it expires.
    pub until: Instant,
}

/// A watch as its log reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    /// Its name.
    pub id: WatchId,
    /// What it waits for.
    pub set: Set,
    /// What closed it, if anything.
    pub life: Life,
}

/// What closed a watch before its expiry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Life {
    /// Nothing.
    Open,
    /// A message matched.
    Rang {
        /// The message.
        message: MailHash,
        /// When.
        at: Instant,
        /// Whether its push reached the push service.
        push: Delivery,
    },
    /// The owner cancelled it.
    Cancelled,
}

/// Whether a ring's push reached the push service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Not yet.
    Owed,
    /// It did.
    Delivered,
}

/// A watch at an instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// It waits.
    Watching,
    /// Its expiry passed and nothing closed it.
    Expired,
    /// Something closed it.
    Closed,
}

impl Watch {
    /// The watch `lines` record, `None` for a log never started.
    ///
    /// # Errors
    /// An internal refusal naming the watch for a log no writer of this module writes: one that
    /// does not begin with its `Set`, or a line after a closing it cannot follow.
    pub fn of(id: WatchId, lines: &[Line]) -> Result<Option<Self>, Refusal> {
        let impossible = |line: &Line| {
            Refusal::internal(format!(
                "the watch {} holds a line it cannot hold: {line:?}",
                id.as_str()
            ))
        };
        let Some((first, rest)) = lines.split_first() else {
            return Ok(None);
        };
        let Line::Set(set) = first else {
            return Err(impossible(first));
        };
        let life = rest
            .iter()
            .try_fold(Life::Open, |life, line| match (life, line) {
                (Life::Open, Line::Rang { message, at }) => Ok(Life::Rang {
                    message: message.clone(),
                    at: *at,
                    push: Delivery::Owed,
                }),
                (Life::Open, Line::Cancelled { .. }) => Ok(Life::Cancelled),
                (
                    Life::Rang {
                        message,
                        at,
                        push: Delivery::Owed,
                    },
                    Line::Delivered { .. },
                ) => Ok(Life::Rang {
                    message,
                    at,
                    push: Delivery::Delivered,
                }),
                (Life::Open, Line::Set(_) | Line::Delivered { .. })
                | (
                    Life::Rang { .. },
                    Line::Set(_)
                    | Line::Rang { .. }
                    | Line::Cancelled { .. }
                    | Line::Delivered { .. },
                )
                | (Life::Cancelled, _) => Err(impossible(line)),
            })?;
        Ok(Some(Self {
            id,
            set: set.clone(),
            life,
        }))
    }

    /// How it stands at `now`.
    #[must_use]
    pub fn standing(&self, now: Instant) -> Standing {
        match &self.life {
            Life::Rang { .. } | Life::Cancelled => Standing::Closed,
            Life::Open if now >= self.set.until => Standing::Expired,
            Life::Open => Standing::Watching,
        }
    }

    /// The push still owed for its ring, until it is delivered or `GRACE` after the ring.
    #[must_use]
    pub fn owed(&self, now: Instant) -> Option<&MailHash> {
        match &self.life {
            Life::Rang {
                message,
                at,
                push: Delivery::Owed,
            } if now < *at + GRACE => Some(message),
            Life::Rang { .. } | Life::Open | Life::Cancelled => None,
        }
    }

    /// The corpus query for the mail this watch waits for: the part's words, dated from the
    /// set time on, from none of `own`, the owner's own addresses.
    ///
    /// # Errors
    /// A query over the corpus's cap.
    pub fn query(&self, own: &[&str]) -> Result<Query, Refusal> {
        self.set.query(own)
    }
}

impl Set {
    /// The corpus query for the mail this watch waits for; see [`Watch::query`].
    ///
    /// # Errors
    /// A query over the corpus's cap.
    pub fn query(&self, own: &[&str]) -> Result<Query, Refusal> {
        let field = match self.on.part {
            Part::Sender => "from",
            Part::Subject => "subject",
        };
        let others = own
            .iter()
            .map(|address| format!(" and not from:\"{address}\""))
            .collect::<Vec<_>>()
            .concat();
        let query = format!(
            "{field}:\"{}\" and date:@{}..{others}",
            self.on.text.as_str(),
            self.at.and_utc().timestamp()
        );
        Query::new(query).map_err(|failed| {
            Refusal::bad_request(format!(
                "this watch's search, with the owner's {} accounts left out, is too long: {failed}",
                own.len()
            ))
        })
    }
}

/// The watches' store: one log per watch.
pub type Watches = dyn Log<WatchId, Line>;

/// Set a watch on `on` for `days` from `now`; `own` is the owner's own addresses, whose mail its
/// search leaves out.
///
/// # Errors
/// A watch whose search would be over the corpus's cap, the store's own, and a conflict for
/// the same watch set at the same instant.
pub fn set(
    watches: &Watches,
    on: On,
    days: Days,
    own: &[&str],
    now: Instant,
) -> Result<Watch, Refusal> {
    let set = Set {
        on,
        at: now,
        until: now + chrono::TimeDelta::days(i64::from(days.0)),
    };
    set.query(own)?;
    let id = WatchId::of(&set);
    watches.start(&id, &Line::Set(set.clone()))?;
    Ok(Watch {
        id,
        set,
        life: Life::Open,
    })
}

/// Every watch the store holds.
///
/// # Errors
/// The store's own, and a log [`Watch::of`] refuses.
pub fn all(watches: &Watches) -> Result<Vec<Watch>, Refusal> {
    let mut all = Vec::new();
    for id in watches.keys()? {
        let lines = watches.lines_if_any(&id)?;
        all.extend(Watch::of(id, &lines)?);
    }
    Ok(all)
}

/// Every watch that waits at `now`, the soonest to expire first.
///
/// # Errors
/// As [`all`].
pub fn open(watches: &Watches, now: Instant) -> Result<Vec<Watch>, Refusal> {
    let mut open: Vec<Watch> = all(watches)?
        .into_iter()
        .filter(|watch| watch.standing(now) == Standing::Watching)
        .collect();
    open.sort_by_key(|watch| watch.set.until);
    Ok(open)
}

/// Write `line` to the watch `id` when `fits` holds of how the watch reads at `now`, and
/// answer whether it was written.
fn close(
    watches: &Watches,
    id: &WatchId,
    line: &Line,
    fits: fn(&Watch, Instant) -> bool,
    now: Instant,
) -> Result<bool, Refusal> {
    let edit = watches.edit(id, &mut |lines| {
        let watch = Watch::of(id.clone(), lines)?
            .ok_or_else(|| Refusal::not_found(format!("no watch is named {}", id.as_str())))?;
        Ok(if fits(&watch, now) {
            Edit::Push(line.clone())
        } else {
            Edit::Keep
        })
    })?;
    Ok(matches!(edit, Edit::Push(_)))
}

fn waits(watch: &Watch, now: Instant) -> bool {
    watch.standing(now) == Standing::Watching
}

/// Cancel the watch `id` at `now`.
///
/// # Errors
/// Not found for a watch the store does not hold, a conflict for one that no longer waits, and
/// the store's own.
pub fn cancel(watches: &Watches, id: &WatchId, now: Instant) -> Result<(), Refusal> {
    if close(watches, id, &Line::Cancelled { at: now }, waits, now)? {
        Ok(())
    } else {
        Err(Refusal::conflict(format!(
            "the watch {} no longer waits",
            id.as_str()
        )))
    }
}

/// Ring every watch that waits at `now` and whose query `corpus` answers with a message, the
/// newest; `own` is the owner's own addresses, whose mail never rings a watch. Answer the
/// refusal of each watch that could not be rung, so one failing watch holds no other back.
///
/// # Errors
/// As [`open`].
pub fn ring(
    watches: &Watches,
    corpus: &dyn Corpus,
    own: &[&str],
    now: Instant,
) -> Result<Vec<Refusal>, Refusal> {
    let one = |watch: &Watch| -> Result<(), Refusal> {
        let found = corpus
            .search(&watch.query(own)?, Some(1))
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        let Some(newest) = found.first() else {
            return Ok(());
        };
        let message = MailHash::new(newest.hash.as_str())
            .map_err(|refused| Refusal::internal(refused.to_string()))?;
        close(
            watches,
            &watch.id,
            &Line::Rang { message, at: now },
            waits,
            now,
        )
        .map(drop)
    };
    Ok(open(watches, now)?
        .iter()
        .filter_map(|watch| one(watch).err())
        .collect())
}

/// Every push still owed at `now`: each watch's id and the message it rang for.
///
/// # Errors
/// As [`all`].
pub fn owed(watches: &Watches, now: Instant) -> Result<Vec<(WatchId, MailHash)>, Refusal> {
    Ok(all(watches)?
        .iter()
        .filter_map(|watch| {
            watch
                .owed(now)
                .map(|message| (watch.id.clone(), message.clone()))
        })
        .collect())
}

/// Record that the push owed for the watch `id` reached the push service.
///
/// # Errors
/// The store's own.
pub fn delivered(watches: &Watches, id: &WatchId, now: Instant) -> Result<(), Refusal> {
    close(
        watches,
        id,
        &Line::Delivered { at: now },
        |watch, now| watch.owed(now).is_some(),
        now,
    )
    .map(drop)
}

/// Every watch that waits at `now`, as the Mail page draws them.
///
/// # Errors
/// As [`all`].
pub fn listed(watches: &Watches, now: Instant) -> Result<crate::wire::WatchesPayload, Refusal> {
    let shown = |watch: Watch| crate::wire::MailWatch {
        on: match watch.set.on.part {
            Part::Sender => crate::wire::WatchOn::Sender,
            Part::Subject => crate::wire::WatchOn::Subject,
        },
        text: watch.set.on.text.as_str().to_owned(),
        set: prodrome_core::fpl::iso(watch.set.at),
        until: prodrome_core::fpl::iso(watch.set.until),
        id: watch.id.0,
    };
    Ok(crate::wire::WatchesPayload {
        watches: open(watches, now)?.into_iter().map(shown).collect(),
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::capability::Nothing;
    use crate::mail::corpus::CorpusError;
    use crate::mail::state::Decision;
    use crate::mail::sync_report::SyncReport;
    use crate::mail::verbs::{Marked, Show, Summary};
    use crate::store::InMemoryLog;

    /// A corpus whose every search answers the messages that have arrived, newest first.
    struct Arrived(Vec<Summary>);

    impl Corpus for Arrived {
        fn sync(&self) -> Result<SyncReport, CorpusError> {
            Nothing.sync()
        }
        fn queue(&self, limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
            Nothing.queue(limit)
        }
        fn search(&self, _: &Query, _: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
            Ok(self.0.iter().rev().cloned().collect())
        }
        fn show(&self, id: &str) -> Result<Show, CorpusError> {
            Nothing.show(id)
        }
        fn mark(
            &self,
            hash: &str,
            decision: Decision,
            note: Option<&str>,
            native: Option<&str>,
        ) -> Result<Marked, CorpusError> {
            Nothing.mark(hash, decision, note, native)
        }
        fn last_sync(&self) -> Option<SyncReport> {
            None
        }
        fn last_triage_pass(&self) -> Option<String> {
            None
        }
    }

    fn message(n: usize) -> Summary {
        Summary {
            hash: format!("{n:06x}"),
            id: format!("{n}@library.example.test"),
            sender: "desk@library.example.test".to_owned(),
            title: "Your hold is ready".to_owned(),
            timestamp: None,
            container: "inbox".to_owned(),
            sources: 1,
            held: crate::held_value::Kinds::empty(),
        }
    }

    /// One step of an interleaving: the secretary sets a watch, the owner cancels one, a
    /// message arrives, a sync rings and delivers, or the clock moves on.
    #[derive(Debug, Clone)]
    enum Step {
        Set(u16),
        Cancel(usize),
        Arrive,
        Sync { pushed: bool },
        Tick(i64),
    }

    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            (1u16..3).prop_map(Step::Set),
            (0usize..4).prop_map(Step::Cancel),
            Just(Step::Arrive),
            any::<bool>().prop_map(|pushed| Step::Sync { pushed }),
            (1i64..40).prop_map(Step::Tick),
        ]
    }

    fn at(lines: &[Line], kind: fn(&Line) -> bool) -> Vec<usize> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, line)| kind(line))
            .map(|(n, _)| n)
            .collect()
    }

    proptest! {
        /// Whatever the order of sets, cancels, arrivals, syncs and time, each watch rings at
        /// most once, never after its cancel and never past its expiry, and its push is
        /// delivered at most once, only after its ring, and by any pushing sync within
        /// [`GRACE`] of the ring.
        #[test]
        fn a_watch_rings_at_most_once_never_after_a_cancel_or_its_expiry(
            steps in prop::collection::vec(step(), 0..40),
        ) {
            let log: InMemoryLog<WatchId, Line> = InMemoryLog::new("watch");
            let on = On {
                part: Part::Sender,
                text: Pattern::new("library.example.test").expect("a pattern"),
            };
            let mut now = chrono::NaiveDate::from_ymd_opt(2026, 10, 23)
                .and_then(|day| day.and_hms_opt(9, 0, 0))
                .expect("an instant");
            let mut ids: Vec<WatchId> = Vec::new();
            let mut arrived = Vec::new();
            for step in steps {
                match step {
                    Step::Set(days) => ids.extend(
                        set(&log, on.clone(), Days::new(days).expect("days"), &[], now)
                            .ok()
                            .map(|watch| watch.id),
                    ),
                    Step::Cancel(n) => {
                        if let Some(id) = ids.get(n) {
                            // A watch that no longer waits refuses a cancel.
                            drop(cancel(&log, id, now));
                        }
                    }
                    Step::Arrive => arrived.push(message(arrived.len())),
                    Step::Sync { pushed } => {
                        let failed = ring(&log, &Arrived(arrived.clone()), &[], now).expect("rings");
                        prop_assert!(failed.is_empty(), "{failed:?}");
                        if pushed {
                            for (id, _) in owed(&log, now).expect("reads") {
                                delivered(&log, &id, now).expect("recorded");
                            }
                            // Whatever the watch's expiry, a ring within the grace is delivered.
                            for id in &ids {
                                let lines = log.lines(id).expect("a log");
                                let rang = lines.iter().find_map(|line| match line {
                                    Line::Rang { at, .. } => Some(*at),
                                    Line::Set(_) | Line::Cancelled { .. } | Line::Delivered { .. } => None,
                                });
                                if rang.is_some_and(|at| now < at + GRACE) {
                                    prop_assert!(
                                        lines.iter().any(|line| matches!(line, Line::Delivered { .. })),
                                        "{lines:?}"
                                    );
                                }
                            }
                        }
                    }
                    Step::Tick(hours) => now += chrono::TimeDelta::hours(hours),
                }
            }
            for id in &ids {
                let lines = log.lines(id).expect("a log");
                let Some(Line::Set(set)) = lines.first() else {
                    panic!("a watch's log begins with its Set: {lines:?}");
                };
                let rings = at(&lines, |line| matches!(line, Line::Rang { .. }));
                let cancels = at(&lines, |line| matches!(line, Line::Cancelled { .. }));
                let deliveries = at(&lines, |line| matches!(line, Line::Delivered { .. }));
                prop_assert!(rings.len() + cancels.len() <= 1, "{lines:?}");
                prop_assert!(deliveries.len() <= 1, "{lines:?}");
                prop_assert!(deliveries.iter().all(|d| rings.iter().any(|r| r < d)), "{lines:?}");
                for line in &lines {
                    if let Line::Rang { at, .. } = line {
                        prop_assert!(*at < set.until, "{lines:?}");
                    }
                }
            }
        }
    }
}
