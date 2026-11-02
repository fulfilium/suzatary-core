//! The submit law: any interleaving of submits, resubmits, dismissals and acknowledgements,
//! concurrent ones included, performs each effect at most once, and every submit of a completed
//! proposal returns its completion's response. Over a scripted transport that counts sends and an
//! in-memory inbox whose one lock is the interpreter's.

use std::sync::{Barrier, Mutex};
use std::time::Duration;

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use proptest::prelude::*;

use suzatary_core::secretary::inbox::{self, ProposalId, Response};
use suzatary_core::secretary::inbox_reading::Reading;
use suzatary_core::secretary::responded::{Done, Performed, Responded, Tried};
use suzatary_core::secretary::store::{Draft, Proposal, Reasons};
use suzatary_core::store::InMemory;

/// The inbox, in memory; its hold is the one lock the law is about.
type Memory = InMemory<suzatary_core::secretary::inbox_event::InboxEvent>;

fn now() -> Datetime {
    Datetime::new(2026, 9, 28, 9, 0, 0, 0).expect("an instant")
}

/// What the far end answers a send, in turn.
#[derive(Debug, Clone, Copy)]
enum Answer {
    Sent,
    NotSent,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Submit(usize),
    Dismiss(usize),
    Acknowledge(usize),
}

/// What happened, in the order it happened under the lock.
#[derive(Debug, Clone)]
enum Happened {
    Sent(usize, Performed),
    Dismissed(usize),
    Checked(usize),
}

fn proposal(n: usize) -> Proposal {
    Proposal::Email(Draft {
        account: "ada@example.com".to_owned(),
        to: vec!["mira@example.com".to_owned()],
        cc: Vec::new(),
        subject: format!("proposal {n}"),
        body: "Thursday works.".to_owned(),
        attachments: Vec::new(),
    })
}

fn filed(memory: &Memory, n: usize) -> ProposalId {
    let asked = inbox::Asked {
        proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
        proposal: proposal(n),
        reasons: Reasons::new(
            "she asked",
            vec!["mail:abcdef012345".parse().expect("a ref")],
        )
        .expect("reasons"),
        asked: format!("proposal:default/{n}").parse().expect("a ref"),
        price: inbox::Price::unpriced(),
    };
    let by = Actor::new("assistant").expect("an actor");
    inbox::ask(memory, asked, &by, now()).expect("filed")
}

fn answer(script: &[Answer], call: usize, n: usize) -> Performed {
    match script[call.min(script.len() - 1)] {
        Answer::Sent => Performed::Done(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: format!("<{n}.{call}@box>"),
        }),
        Answer::NotSent => Performed::Tried(Tried::NotSent {
            reason: format!("refused, call {call}"),
        }),
        Answer::Unknown => Performed::Done(Done::MayHaveGone {
            reason: format!("no answer, call {call}"),
        }),
    }
}

/// What a perform settled, when it settled anything.
fn completion(performed: &Performed) -> Option<&Done> {
    match performed {
        Performed::Done(done) => Some(done),
        Performed::Tried(_) => None,
    }
}

/// The send a submit's answer says completed it, a check answering the send it checks.
fn answered(response: &Response) -> Option<Done> {
    let said = match response {
        Response::Ran(responded) => responded.clone(),
        Response::Recorded(reading) => match reading.as_ref() {
            Reading::One(answer) => match &answer.response {
                Responded::Checked(check) => {
                    return Some(Done::MayHaveGone {
                        reason: check.reason().to_owned(),
                    });
                }
                Responded::Done(done) => Responded::Done(done.clone()),
                _ => return None,
            },
            _ => return None,
        },
    };
    match said {
        Responded::Done(done) => Some(done),
        _ => None,
    }
}

fn answers() -> impl Strategy<Value = Answer> {
    prop_oneof![
        Just(Answer::Sent),
        Just(Answer::NotSent),
        Just(Answer::Unknown)
    ]
}

fn ops(proposals: usize) -> impl Strategy<Value = Vec<Vec<Op>>> {
    let op = (0..proposals, 0..3_u8).prop_map(|(n, which)| match which {
        0 => Op::Dismiss(n),
        1 => Op::Acknowledge(n),
        _ => Op::Submit(n),
    });
    prop::collection::vec(prop::collection::vec(op, 1..4), 1..6)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn a_submit_performs_at_most_once_and_answers_what_was_recorded(
        scripts in prop::collection::vec(prop::collection::vec(answers(), 1..4), 1..4),
        rounds in ops(3),
    ) {
        let memory = Memory::default();
        let ids: Vec<ProposalId> = (0..scripts.len()).map(|n| filed(&memory, n)).collect();
        let calls: Vec<Mutex<usize>> = scripts.iter().map(|_| Mutex::new(0)).collect();
        let log: Mutex<Vec<Happened>> = Mutex::new(Vec::new());
        let responses: Mutex<Vec<(usize, Response)>> = Mutex::new(Vec::new());
        let owner = Actor::new("ada").expect("an actor");

        for round in &rounds {
            let round: Vec<Op> = round.iter().copied().filter(|op| match op {
                Op::Submit(n) | Op::Dismiss(n) | Op::Acknowledge(n) => *n < ids.len(),
            }).collect();
            let barrier = Barrier::new(round.len());
            std::thread::scope(|scope| {
                for op in &round {
                    let (memory, ids, scripts, calls, log, responses, owner, barrier) =
                        (&memory, &ids, &scripts, &calls, &log, &responses, &owner, &barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        match *op {
                            Op::Submit(n) => {
                                let response = inbox::submit(memory, &ids[n], owner, now(), |_| {
                                    let mut call = calls[n].lock().expect("calls");
                                    std::thread::sleep(Duration::from_millis(2));
                                    let outcome = answer(&scripts[n], *call, n);
                                    *call += 1;
                                    log.lock().expect("log").push(Happened::Sent(n, outcome.clone()));
                                    Ok(outcome)
                                })
                                .expect("a submit answers");
                                responses.lock().expect("responses").push((n, response));
                            }
                            Op::Dismiss(n) => {
                                if let Ok(Response::Ran(_)) =
                                    inbox::dismiss(memory, &ids[n], owner, now(), "not this one")
                                {
                                    log.lock().expect("log").push(Happened::Dismissed(n));
                                }
                            }
                            Op::Acknowledge(n) => {
                                if let Ok(Response::Ran(_)) =
                                    inbox::acknowledge(memory, &ids[n], owner, now())
                                {
                                    log.lock().expect("log").push(Happened::Checked(n));
                                }
                            }
                        }
                    });
                }
            });
        }

        let log = log.into_inner().expect("log");
        let fold = inbox::fold(&memory).expect("the inbox reads");
        for (n, id) in ids.iter().enumerate() {
            let mine: Vec<&Happened> = log.iter().filter(|happened| match happened {
                Happened::Sent(m, _) | Happened::Dismissed(m) | Happened::Checked(m) => *m == n,
            }).collect();
            // At most one effect, and after it or a dismissal only the owner's one check of a
            // send that may have gone.
            let ended = mine.iter().position(|happened| match happened {
                Happened::Sent(_, performed) => completion(performed).is_some(),
                Happened::Dismissed(_) => true,
                Happened::Checked(_) => false,
            });
            match ended {
                Some(end) => {
                    let uncertain = matches!(
                        mine[end],
                        Happened::Sent(_, Performed::Done(Done::MayHaveGone { .. }))
                    );
                    let after = &mine[end + 1..];
                    prop_assert!(
                        after.is_empty() || (uncertain && after.len() == 1 && matches!(after[0], Happened::Checked(_))),
                        "proposal {} ran after it ended: {:?}", n, mine
                    );
                }
                None => prop_assert!(
                    !mine.iter().any(|happened| matches!(happened, Happened::Checked(_))),
                    "proposal {} was checked with nothing uncertain to check: {:?}", n, mine
                ),
            }
            let completed = mine.iter().find_map(|happened| match happened {
                Happened::Sent(_, performed) => completion(performed).cloned(),
                _ => None,
            });
            // Every answer of a completion is the one recorded.
            for (m, response) in responses.lock().expect("responses").iter() {
                if *m == n && let Some(done) = answered(response) {
                    prop_assert_eq!(Some(done), completed.clone());
                }
            }
            // And the fold holds what happened, note for note, each try numbered in turn.
            let object = fold.get(id).expect("filed");
            let mut tries = 0;
            let happened: Vec<Responded> = mine.iter().map(|happened| match happened {
                Happened::Sent(_, performed) => {
                    tries += u32::from(completion(performed).is_none());
                    Responded::performed(performed.clone(), tries)
                }
                Happened::Dismissed(_) => Responded::Dismissed {
                    reason: "not this one".to_owned(),
                },
                Happened::Checked(_) => Responded::Checked(()),
            }).collect();
            prop_assert_eq!(&object.responses, &happened);
        }
    }
}
