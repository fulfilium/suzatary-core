//! The claim's laws ([`Claims`]), over generated interleavings of requests, claim steps, joins,
//! answers and runs that die, as the box writes them: one chain, every line resting on the one
//! before. The laws:
//! - settled means sent: a waking request within the bound is settled only by a closed round that
//!   covered it;
//! - one run covers a request once, and the rounds of at most two runs cover it;
//! - the box closes only when two lost rounds covered a request it settles, or when no waking
//!   request it settles is within the bound: the bound decides whether a round starts, never
//!   what a started round covers;
//! - once every run is gone and the claim steps have run, every waking request is settled.

use super::{Asked, Bound, Claims, Due, End, Line, Mark, Retry, Standing, Wakes};
use proptest::prelude::*;

/// One event of an interleaving.
#[derive(Debug, Clone)]
enum Event {
    /// A request arrives.
    Ask(Wakes, Bound),
    /// A claim step runs.
    Step,
    /// The live round of the `n`th run joins what is queued.
    Join(usize),
    /// The live round of the `n`th run writes a model's turn with calls, or a verb's answer.
    Calls(usize),
    /// A verb's answer is written outside any round, as a tap writes one.
    Tapped,
    /// The live round of the `n`th run closes.
    Answer(usize),
    /// The `n`th run's process dies.
    Die(usize),
}

fn events() -> impl Strategy<Value = Vec<Event>> {
    proptest::collection::vec(
        prop_oneof![
            3 => (
                prop_oneof![3 => Just(Wakes::Model), 1 => Just(Wakes::RidesAlong)],
                prop_oneof![4 => Just(Bound::Within), 1 => Just(Bound::Past)],
            )
                .prop_map(|(wakes, bound)| Event::Ask(wakes, bound)),
            3 => Just(Event::Step),
            1 => (0usize..4).prop_map(Event::Join),
            1 => (0usize..4).prop_map(Event::Calls),
            1 => Just(Event::Tapped),
            2 => (0usize..4).prop_map(Event::Answer),
            2 => (0usize..4).prop_map(Event::Die),
        ],
        0..40,
    )
}

/// The lines so far, every one resting on the one before, the requests asked, and the runs.
#[derive(Default)]
struct Chain {
    marks: Vec<Mark<u32>>,
    parents: Vec<Vec<usize>>,
    asked: Vec<Asked>,
    runs: u32,
    gone: std::collections::BTreeSet<u32>,
    /// Each box close, with each request it settled and that request's attempts.
    closes: Vec<Vec<(Asked, usize)>>,
    /// The answers a round wrote for itself: its own lines, which nobody asked for.
    own: std::collections::BTreeSet<usize>,
}

impl Chain {
    fn push(&mut self, mark: Mark<u32>) -> usize {
        let at = self.marks.len();
        self.parents.push(at.checked_sub(1).into_iter().collect());
        self.marks.push(mark);
        at
    }

    /// A model's or a verb's turn, which the box's reading lists among what a claim step reads.
    fn answer(&mut self) -> usize {
        let at = self.push(Mark::Answer);
        self.asked.push(Asked {
            at,
            wakes: Wakes::Model,
            bound: Bound::Within,
        });
        at
    }

    fn claims(&self) -> Claims<u32> {
        let lines: Vec<Line<'_, u32>> = self
            .marks
            .iter()
            .zip(&self.parents)
            .map(|(mark, rests_on)| Line { mark, rests_on })
            .collect();
        Claims::of(&lines)
    }

    /// The live round of run `n` by its order among the runs drawn.
    fn live(&self, n: usize) -> Option<u32> {
        let claims = self.claims();
        let mut open: Vec<u32> = claims
            .rounds()
            .iter()
            .filter(|round| round.end == End::Open && !self.gone.contains(&round.run))
            .map(|round| round.run)
            .collect();
        open.dedup();
        (!open.is_empty()).then(|| open[n % open.len()])
    }

    fn step(&mut self) {
        let claims = self.claims();
        match claims.due(&self.asked, |run| self.gone.contains(run), Retry::Allowed) {
            Due::Nothing => {}
            Due::Claim(covers) | Due::Retry(covers) => {
                self.runs += 1;
                let run = self.runs;
                self.push(Mark::Claim { run, covers });
            }
            Due::Close => {
                let lost = claims
                    .rounds()
                    .iter()
                    .rposition(|round| round.end == End::Open);
                let settled = self
                    .asked
                    .iter()
                    .filter(|asked| match (claims.standing(asked.at), lost) {
                        (Standing::Claimed { latest, .. }, Some(lost)) => latest == lost,
                        (Standing::Queued, None) => true,
                        _ => false,
                    })
                    .map(|asked| (*asked, claims.standing(asked.at).attempts()))
                    .collect();
                self.closes.push(settled);
                self.push(Mark::Close);
            }
        }
    }

    fn apply(&mut self, event: &Event) {
        match event {
            Event::Ask(wakes, bound) => {
                let at = self.push(Mark::Request);
                self.asked.push(Asked {
                    at,
                    wakes: *wakes,
                    bound: *bound,
                });
            }
            Event::Step => self.step(),
            Event::Join(n) => {
                if let Some(run) = self.live(*n) {
                    let covers = self.claims().joins(&self.asked, &run);
                    if !covers.is_empty() {
                        self.push(Mark::Claim { run, covers });
                    }
                }
            }
            Event::Calls(n) => {
                if self.live(*n).is_some() {
                    let at = self.answer();
                    self.own.insert(at);
                }
            }
            Event::Tapped => {
                let open = self
                    .claims()
                    .rounds()
                    .iter()
                    .any(|round| round.end == End::Open);
                let at = self.answer();
                if open {
                    // Written while a round is open, it is that round's, as the reading says.
                    self.own.insert(at);
                }
            }
            Event::Answer(n) => {
                if self.live(*n).is_some() {
                    self.push(Mark::Close);
                }
            }
            Event::Die(n) => {
                if let Some(run) = self.live(*n) {
                    self.gone.insert(run);
                }
            }
        }
    }

    /// Every run dies, and claim steps run until none owes anything.
    fn quiesce(&mut self) {
        for _ in 0..64 {
            self.gone.extend(1..=self.runs);
            let before = self.marks.len();
            self.step();
            if self.marks.len() == before {
                return;
            }
        }
        panic!("the claim steps never stopped appending");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn the_claim_s_laws_hold_over_every_interleaving(events in events()) {
        let mut held = Chain::default();
        for event in &events {
            held.apply(event);
        }
        held.quiesce();
        let claims = held.claims();

        // One run covers a request once, and the rounds of at most two runs cover it.
        for asked in held.asked.iter().filter(|asked| !held.own.contains(&asked.at)) {
            let mut runs = Vec::new();
            for mark in &held.marks {
                if let Mark::Claim { run, covers } = mark {
                    let times = covers.iter().filter(|&&at| at == asked.at).count();
                    prop_assert!(times <= 1, "a claim names a request twice");
                    if times == 1 {
                        prop_assert!(!runs.contains(run), "run {run} covers {} twice", asked.at);
                        runs.push(*run);
                    }
                }
            }
            prop_assert!(runs.len() <= 2, "{} covered by {runs:?}", asked.at);
        }

        // The box closes only after two lost rounds, or when nothing it settles may start a round.
        for settled in &held.closes {
            prop_assert!(
                settled.iter().any(|(_, attempts)| *attempts >= 2)
                    || !settled.iter().any(|(asked, _)| {
                        asked.wakes == Wakes::Model && asked.bound == Bound::Within
                    }),
                "the box closed {settled:?}"
            );
        }

        for asked in held.asked.iter().filter(|asked| asked.wakes == Wakes::Model) {
            // Every request is settled once the runs are gone and the steps have run.
            prop_assert_eq!(claims.standing(asked.at), Standing::Settled, "{:?}", asked);
            // Settled means sent: a waking request within the bound was covered by a round
            // that closed.
            if asked.bound == Bound::Within {
                prop_assert!(
                    claims.rounds().iter().any(|round| {
                        round.covers.contains(&asked.at) && matches!(round.end, End::Closed(_))
                    }),
                    "{:?} settled unsent", asked
                );
            }
        }
    }
}

/// A request said after the round's last claim and before its close is not settled by that close:
/// the next claim step owes it.
#[test]
fn a_request_after_the_last_claim_is_owed_after_the_close() {
    let mut held = Chain::default();
    held.apply(&Event::Ask(Wakes::Model, Bound::Within));
    held.apply(&Event::Step);
    held.apply(&Event::Ask(Wakes::Model, Bound::Within));
    held.apply(&Event::Answer(0));
    let claims = held.claims();
    assert_eq!(claims.standing(0), Standing::Settled);
    assert_eq!(claims.standing(2), Standing::Queued);
    assert_eq!(
        claims.due(&held.asked, |_| true, Retry::Allowed),
        Due::Claim(vec![2])
    );
}

/// A lost round is retried once, a second loss is closed by the box, and a retry takes no joins.
#[test]
fn a_lost_round_is_retried_once_then_closed() {
    let mut held = Chain::default();
    held.apply(&Event::Ask(Wakes::Model, Bound::Within));
    held.apply(&Event::Step);
    held.apply(&Event::Die(0));
    held.apply(&Event::Step);
    assert!(matches!(&held.marks[2], Mark::Claim { run: 2, covers } if covers == &[0]));
    held.apply(&Event::Ask(Wakes::Model, Bound::Within));
    assert!(
        held.claims().joins(&held.asked, &2).is_empty(),
        "a retry joins nothing"
    );
    held.apply(&Event::Die(0));
    assert_eq!(
        held.claims().due(&held.asked, |_| true, Retry::Allowed),
        Due::Close
    );
    assert_eq!(
        held.claims().due(&held.asked, |_| true, Retry::Held),
        Due::Close
    );
}

/// With no claim anywhere, a close settles everything beneath it, as every close did before
/// claims were written.
#[test]
fn a_close_with_no_claim_settles_what_it_rests_on() {
    let mut held = Chain::default();
    held.apply(&Event::Ask(Wakes::Model, Bound::Within));
    held.push(Mark::Answer);
    held.push(Mark::Close);
    let claims = held.claims();
    assert_eq!(claims.standing(0), Standing::Settled);
    assert_eq!(claims.standing(1), Standing::Settled);
}
