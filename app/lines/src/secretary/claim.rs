//! Which requests a round covers, by which runs, and which are settled: a reading of the claims
//! and closes in one sequence of lines, and what a claim step owes it.
//!
//! A round is the claims of one run up to the first close after them, and the answers written
//! while it is open, which are its own. A close ends every round open before it and settles what
//! they cover; a close with no round open, or a stop, settles everything beneath it. An answer
//! written while no round is open is a request of its own.

use std::collections::BTreeSet;

/// What one line of the sequence is to the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mark<R> {
    /// A turn a round may cover.
    Request,
    /// A turn a round writes for itself (a model's or a verb's), settled by any close that rests
    /// on it.
    Answer,
    /// Run `run` covers the requests at `covers`, by their positions in the sequence.
    Claim {
        run: R,
        covers: Vec<usize>,
    },
    Close,
    /// A stop: it settles everything beneath it and ends every open round.
    Stop,
    /// A close or a stop a later line set aside: the rounds it ends are withdrawn, which settles
    /// nothing and spends no attempt.
    Withdrawn,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Line<'a, R> {
    pub(crate) mark: &'a Mark<R>,
    pub(crate) rests_on: &'a [usize],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Open,
    /// By the close or stop at this position.
    Closed(usize),
    Withdrawn,
}

/// The claims of one run up to their close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Round<R> {
    pub(crate) run: R,
    /// The positions of its claims, the first opening it.
    pub(crate) claims: Vec<usize>,
    /// What its claims cover and the answers written while it was open, each once, in order.
    pub(crate) covers: Vec<usize>,
    pub(crate) end: End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    Settled,
    /// No round covers it.
    Queued,
    /// Open rounds cover it: `latest` by its index in [`Claims::rounds`], and `earlier` more.
    Claimed {
        latest: usize,
        earlier: usize,
    },
}

impl Standing {
    /// How many rounds covered it.
    #[must_use]
    pub(crate) fn attempts(self) -> usize {
        match self {
            Standing::Settled | Standing::Queued => 0,
            Standing::Claimed { earlier, .. } => earlier + 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Claims<R> {
    rounds: Vec<Round<R>>,
    /// Each line's standing; [`Standing::Settled`] for a line that is neither request nor answer.
    standing: Vec<Standing>,
}

impl<R: PartialEq + Clone> Claims<R> {
    /// The reading of `lines`, in sequence order.
    #[must_use]
    pub(crate) fn of(lines: &[Line<'_, R>]) -> Self {
        let mut rounds: Vec<Round<R>> = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        let mut beneath_all: Vec<usize> = Vec::new();
        let mut beneath_answers: Vec<usize> = Vec::new();
        for (at, line) in lines.iter().enumerate() {
            match line.mark {
                Mark::Request | Mark::Other => {}
                Mark::Answer => {
                    if let Some(&round) = open.last() {
                        rounds[round].covers.push(at);
                    }
                }
                Mark::Claim { run, covers } => {
                    let round = if let Some(&round) =
                        open.iter().find(|&&round| rounds[round].run == *run)
                    {
                        round
                    } else {
                        open.push(rounds.len());
                        rounds.push(Round {
                            run: run.clone(),
                            claims: Vec::new(),
                            covers: Vec::new(),
                            end: End::Open,
                        });
                        rounds.len() - 1
                    };
                    let round = &mut rounds[round];
                    round.claims.push(at);
                    for &covered in covers {
                        if covered < at && !round.covers.contains(&covered) {
                            round.covers.push(covered);
                        }
                    }
                }
                Mark::Close => {
                    if open.is_empty() {
                        beneath_all.push(at);
                    } else {
                        beneath_answers.push(at);
                    }
                    for round in open.drain(..) {
                        rounds[round].end = End::Closed(at);
                    }
                }
                Mark::Stop => {
                    beneath_all.push(at);
                    for round in open.drain(..) {
                        rounds[round].end = End::Closed(at);
                    }
                }
                Mark::Withdrawn => {
                    for round in open.drain(..) {
                        rounds[round].end = End::Withdrawn;
                    }
                }
            }
        }
        let all = beneath(lines, &beneath_all);
        let answers = beneath(lines, &beneath_answers);
        let mut standing: Vec<Standing> = lines
            .iter()
            .enumerate()
            .map(|(at, line)| match line.mark {
                Mark::Request if !all.contains(&at) => Standing::Queued,
                Mark::Answer if !all.contains(&at) && !answers.contains(&at) => Standing::Queued,
                Mark::Request | Mark::Answer => Standing::Settled,
                Mark::Claim { .. } | Mark::Close | Mark::Stop | Mark::Withdrawn | Mark::Other => {
                    Standing::Settled
                }
            })
            .collect();
        for (index, round) in rounds.iter().enumerate() {
            for &covered in &round.covers {
                let now = &mut standing[covered];
                *now = match (round.end, *now) {
                    (End::Withdrawn, now) | (_, now @ Standing::Settled) => now,
                    (End::Closed(_), _) => Standing::Settled,
                    (End::Open, Standing::Queued) => Standing::Claimed {
                        latest: index,
                        earlier: 0,
                    },
                    (End::Open, Standing::Claimed { earlier, .. }) => Standing::Claimed {
                        latest: index,
                        earlier: earlier + 1,
                    },
                };
            }
        }
        Self { rounds, standing }
    }

    /// Every round, in the order its first claim stands.
    #[must_use]
    pub(crate) fn rounds(&self) -> &[Round<R>] {
        &self.rounds
    }

    #[must_use]
    pub(crate) fn standing(&self, at: usize) -> Standing {
        self.standing[at]
    }

    /// The round of `run` still open, by its index.
    #[must_use]
    pub(crate) fn open(&self, run: &R) -> Option<usize> {
        self.rounds
            .iter()
            .position(|round| round.end == End::Open && round.run == *run)
    }
}

/// Every position beneath the lines at `from`, directly or not.
fn beneath<R>(lines: &[Line<'_, R>], from: &[usize]) -> BTreeSet<usize> {
    let mut found = BTreeSet::new();
    let mut pending: Vec<usize> = from
        .iter()
        .flat_map(|&at| lines[at].rests_on.iter().copied())
        .collect();
    while let Some(at) = pending.pop() {
        if found.insert(at) {
            pending.extend(lines[at].rests_on);
        }
    }
    found
}

/// Whether a request starts a round by itself, or only rides along with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wakes {
    Model,
    RidesAlong,
}

/// Whether a request was made within the bound of the claim step's instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    Within,
    Past,
}

/// One live request as a claim step reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Asked {
    pub(crate) at: usize,
    pub(crate) wakes: Wakes,
    pub(crate) bound: Bound,
}

impl Asked {
    fn starts(&self) -> bool {
        self.wakes == Wakes::Model && self.bound == Bound::Within
    }
}

/// Whether a lost round may be claimed again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    Allowed,
    /// The model failed its last call: a lost round is closed, never retried.
    Held,
}

/// What a claim step appends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Due {
    /// Nothing is owed, or a round is running.
    Nothing,
    /// A new run's first claim, of the requests at these positions.
    Claim(Vec<usize>),
    /// A new run's claim of what a lost round covered and nothing settled: a retry, which takes
    /// no joins.
    Retry(Vec<usize>),
    /// The box's close: of a lost round it will not retry, or of waking requests none of which
    /// is within the bound.
    Close,
}

impl<R: PartialEq + Clone> Claims<R> {
    /// What a claim step owes, over `asked`, the live requests in sequence order; `gone` says
    /// whether a run has ended. The bound decides whether a round may start, never what a started
    /// round covers.
    #[must_use]
    pub(crate) fn due(&self, asked: &[Asked], gone: impl Fn(&R) -> bool, retry: Retry) -> Due {
        let open: Vec<usize> = (0..self.rounds.len())
            .filter(|&round| self.rounds[round].end == End::Open)
            .collect();
        if open.iter().any(|&round| !gone(&self.rounds[round].run)) {
            return Due::Nothing;
        }
        if let Some(&lost) = open.last() {
            let unsettled: Vec<&Asked> = asked
                .iter()
                .filter(|asked| matches!(self.standing[asked.at], Standing::Claimed { latest, .. } if latest == lost))
                .collect();
            if !unsettled.is_empty() {
                let again = retry == Retry::Allowed
                    && unsettled
                        .iter()
                        .all(|asked| self.standing[asked.at].attempts() == 1)
                    && unsettled.iter().any(|asked| asked.starts());
                return if again {
                    Due::Retry(unsettled.iter().map(|asked| asked.at).collect())
                } else {
                    Due::Close
                };
            }
        }
        let queued: Vec<&Asked> = asked
            .iter()
            .filter(|asked| self.standing[asked.at] == Standing::Queued)
            .collect();
        if queued.iter().any(|asked| asked.starts()) {
            Due::Claim(queued.iter().map(|asked| asked.at).collect())
        } else if queued.iter().any(|asked| asked.wakes == Wakes::Model) {
            Due::Close
        } else {
            Due::Nothing
        }
    }

    /// What the running round of `run` joins before its next ask: every request queued. None for
    /// a retry, or for a run with no round open.
    #[must_use]
    pub(crate) fn joins(&self, asked: &[Asked], run: &R) -> Vec<usize> {
        let Some(round) = self.open(run) else {
            return Vec::new();
        };
        if self.retries(round) {
            return Vec::new();
        }
        asked
            .iter()
            .filter(|asked| self.standing[asked.at] == Standing::Queued)
            .map(|asked| asked.at)
            .collect()
    }

    /// Whether round `round` is a retry: its first claim covers a request an earlier round
    /// covered.
    fn retries(&self, round: usize) -> bool {
        let first = self.rounds[round].claims[0];
        self.rounds[..round].iter().any(|earlier| {
            earlier.end != End::Withdrawn
                && earlier.covers.iter().any(|covered| {
                    self.rounds[round].covers.contains(covered)
                        && earlier.claims.iter().any(|&claim| claim < first)
                })
        })
    }
}

// The claim's laws over generated interleavings.
#[cfg(test)]
#[path = "../../tests/support/claim.rs"]
mod laws;
