//! What the watcher has seen move and what it has announced, stepped by each poll's readings.

use std::time::{Duration, Instant};

/// The watcher's state, without effects: a poll is a function of the readings and the instant.
///
/// What moved is kept apart from what was announced, so a store that grew is quiet once it stops
/// growing. Every count is `None` until a read succeeds, and a first count announces nothing, so
/// neither a restart nor an unreadable store announces history.
#[derive(Debug)]
pub struct Watched {
    pace: Pace,
    heads: Option<Vec<String>>,
    inbox: Inbox,
    events_announced: Option<usize>,
    inbox_announced: Option<usize>,
    motion: Motion,
}

/// What the watch knows of the inbox's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inbox {
    /// Never read: the first read is a count, not a movement.
    Unread,
    /// Read at this length.
    Seen(usize),
    /// An announcement went out while it could not be read, so the first poll that reads it is
    /// a movement, once.
    Owed,
}

/// How long a burst may be quiet, and how long it may run, before it is announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    /// Quiet this long after the last movement, a burst is over.
    pub quiet: Duration,
    /// Moving this long since the first movement, a burst is announced anyway.
    pub cap: Duration,
}

/// Whether anything has moved since the last announcement, and over which span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    /// Nothing has moved since the last announcement.
    Still,
    /// Something moved first at `first` and last at `last`.
    Moving { first: Instant, last: Instant },
}

impl Motion {
    fn moved(self, now: Instant) -> Motion {
        match self {
            Motion::Still => Motion::Moving {
                first: now,
                last: now,
            },
            Motion::Moving { first, .. } => Motion::Moving { first, last: now },
        }
    }
}

/// One announcement: what each store holds past what was announced of it.
#[derive(Debug, PartialEq, Eq)]
pub struct Fresh<E, I> {
    /// The chain's new events.
    pub events: Vec<E>,
    /// The inbox's new events; none when the inbox was not read this round.
    pub inbox: Vec<I>,
}

impl Watched {
    /// Counted from what the first reads found; an unread store counts from its first good read.
    #[must_use]
    pub fn starting(
        pace: Pace,
        heads: Option<Vec<String>>,
        events: Option<usize>,
        inbox: Option<usize>,
    ) -> Self {
        Self {
            pace,
            heads,
            inbox: inbox.map_or(Inbox::Unread, Inbox::Seen),
            events_announced: events,
            inbox_announced: inbox,
            motion: Motion::Still,
        }
    }

    /// Records a poll's readings (`None` for a failed read, which moves nothing); answers whether
    /// the heads moved, the cue to warm.
    pub fn polled(
        &mut self,
        heads: Option<Vec<String>>,
        inbox: Option<usize>,
        now: Instant,
    ) -> bool {
        let mut warm = false;
        if let Some(heads) = heads
            && self.heads.as_ref() != Some(&heads)
        {
            self.heads = Some(heads);
            self.motion = self.motion.moved(now);
            warm = true;
        }
        if let Some(length) = inbox {
            let moved = match self.inbox {
                Inbox::Unread => false,
                Inbox::Seen(seen) => seen != length,
                Inbox::Owed => true,
            };
            if moved {
                self.motion = self.motion.moved(now);
            }
            self.inbox = Inbox::Seen(length);
            self.inbox_announced.get_or_insert(length);
        }
        warm
    }

    /// Whether a burst is over by `now`: quiet for the pace's `quiet`, or moving for its `cap`.
    #[must_use]
    pub fn due(&self, now: Instant) -> bool {
        match self.motion {
            Motion::Still => false,
            Motion::Moving { first, last } => {
                now.saturating_duration_since(last) >= self.pace.quiet
                    || now.saturating_duration_since(first) >= self.pace.cap
            }
        }
    }

    /// Announce what `events` and `inbox` hold past what was announced, and begin a new burst.
    /// An unread inbox (`None`) keeps its count; its share waits for the first poll that reads it.
    pub fn announce<E: Clone, I: Clone>(
        &mut self,
        events: &[E],
        inbox: Option<&[I]>,
    ) -> Fresh<E, I> {
        self.motion = Motion::Still;
        if inbox.is_none() {
            self.inbox = Inbox::Owed;
        }
        Fresh {
            events: fresh_since(events, &mut self.events_announced),
            inbox: inbox.map_or_else(Vec::new, |inbox| {
                fresh_since(inbox, &mut self.inbox_announced)
            }),
        }
    }
}

/// What `all` holds past `seen`, then counts it to the end; nothing while `seen` is `None`.
fn fresh_since<T: Clone>(all: &[T], seen: &mut Option<usize>) -> Vec<T> {
    let fresh = seen
        .and_then(|count| all.get(count..))
        .map(<[T]>::to_vec)
        .unwrap_or_default();
    *seen = Some(all.len());
    fresh
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLL: Duration = Duration::from_secs(30);
    const QUIET: Duration = Duration::from_secs(120);
    const PACE: Pace = Pace {
        quiet: QUIET,
        cap: Duration::from_secs(600),
    };

    /// Polls `polls` rounds over a fixed chain and inbox; answers each non-empty announcement.
    fn run(
        watched: &mut Watched,
        start: Instant,
        polls: u32,
        events: &[u8],
        inbox: Option<&[u8]>,
    ) -> Vec<Fresh<u8, u8>> {
        let mut said = Vec::new();
        for round in 1..=polls {
            let now = start + POLL * round;
            watched.polled(Some(vec!["h".to_owned()]), inbox.map(<[u8]>::len), now);
            if watched.due(now) {
                let fresh = watched.announce(events, inbox);
                if !(fresh.events.is_empty() && fresh.inbox.is_empty()) {
                    said.push(fresh);
                }
            }
        }
        said
    }

    #[test]
    fn one_inbox_event_then_quiet_is_announced_exactly_once() {
        let start = Instant::now();
        let mut watched = Watched::starting(PACE, Some(vec!["h".to_owned()]), Some(0), Some(1));
        let said = run(&mut watched, start, 12, &[], Some(&[1, 2]));
        assert_eq!(
            said,
            vec![Fresh {
                events: vec![],
                inbox: vec![2]
            }]
        );
    }

    #[test]
    fn an_unreadable_inbox_holds_back_only_its_own_share() {
        let start = Instant::now();
        let mut watched = Watched::starting(PACE, Some(vec!["old".to_owned()]), Some(1), Some(1));
        assert!(watched.polled(Some(vec!["new".to_owned()]), None, start));
        let later = start + QUIET;
        assert!(watched.due(later));
        assert_eq!(
            watched.announce::<u8, u8>(&[1, 2], None),
            Fresh {
                events: vec![2],
                inbox: vec![]
            }
        );
        for round in 1..=10 {
            let now = later + POLL * round;
            watched.polled(Some(vec!["new".to_owned()]), None, now);
            assert!(!watched.due(now), "the chain is not announced again");
        }
        let read = later + POLL * 11;
        watched.polled(Some(vec!["new".to_owned()]), Some(2), read);
        for round in 1..=4 {
            let now = read + POLL * round;
            watched.polled(Some(vec!["new".to_owned()]), Some(2), now);
            assert_eq!(watched.due(now), now >= read + QUIET, "round {round}");
        }
        assert_eq!(
            watched.announce::<u8, u8>(&[1, 2], Some(&[1, 2])),
            Fresh {
                events: vec![],
                inbox: vec![2]
            }
        );
        assert!(!watched.due(read + QUIET * 10));
    }

    #[test]
    fn a_steady_stream_is_announced_every_cap() {
        let start = Instant::now();
        let mut watched = Watched::starting(PACE, Some(vec!["h".to_owned()]), Some(0), Some(0));
        let inbox: Vec<u8> = (0..200).collect();
        let mut said = 0;
        for round in 1..=200_u32 {
            let now = start + POLL * round;
            let seen = &inbox[..round as usize];
            watched.polled(Some(vec!["h".to_owned()]), Some(seen.len()), now);
            if watched.due(now) {
                assert!(!watched.announce::<u8, u8>(&[], Some(seen)).inbox.is_empty());
                said += 1;
            }
        }
        // 200 polls of 30 s are 100 minutes: one announcement per 10-minute cap.
        assert_eq!(said, 9, "announced {said} times");
    }

    #[test]
    fn what_is_fresh_is_counted_from_the_first_read_that_succeeds() {
        let start = Instant::now();
        let mut watched = Watched::starting(PACE, None, None, None);
        watched.polled(Some(vec!["h".to_owned()]), Some(3), start);
        assert_eq!(
            watched.announce::<u8, u8>(&[1, 2, 3], Some(&[1, 2, 3])),
            Fresh {
                events: vec![],
                inbox: vec![]
            }
        );
        assert_eq!(
            watched.announce::<u8, u8>(&[1, 2, 3, 4], Some(&[1, 2, 3, 4])),
            Fresh {
                events: vec![4],
                inbox: vec![4]
            }
        );
    }
}
