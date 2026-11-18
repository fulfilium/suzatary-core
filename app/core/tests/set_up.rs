//! The set-up conversation's laws, over the conversations held in core's memory.

use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_connect::connect::TemplateId;
use suzatary_conversations::secretary::store::Entry;
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_lines::secretary::transcript;
use suzatary_memory::secretary::words::Words;
use suzatary_topics::secretary::rules::{Conversational, RULES};
use suzatary_topics::secretary::set_up::*;
use suzatary_triage::rule::Engine;

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 11, 17, 9, 0, second, 0).expect("an instant")
}

/// THE RESTART LAW: under any interleaving of two workers' engines, each told of the
/// conversation and performing what is due, and of restarts that make an engine anew and
/// resume what is owed, a set-up conversation gets exactly one opening note once any worker
/// runs after it was taken; a conversation not set up gets none.
#[test]
fn each_setup_gets_exactly_one_opening_across_restarts() {
    for seed in 1..=60u64 {
        let mut r = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut next = |n: u64| {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            r % n
        };
        let store = suzatary_core::stores::conversations::InMemory::default();
        let lines = store.conversations();
        let set_up = transcript::begin(
            lines,
            &Entry::SetUp {
                at: at(0),
                title: Words::kept("Connect Gmail".to_owned()),
                template: TemplateId::Gmail,
            },
        )
        .expect("begun");
        let topic = transcript::begin(
            lines,
            &Entry::Opened {
                at: at(1),
                title: Words::kept("Trip".to_owned()),
                model: String::new(),
                kind: suzatary_conversations::secretary::store::Kind::Topic,
            },
        )
        .expect("begun");
        let mut engines = [Engine::new(RULES.to_vec()), Engine::new(RULES.to_vec())];
        let mut ran = false;
        for step in 0..12 {
            if next(4) == 0 {
                let n = usize::try_from(next(2)).expect("small");
                engines[n] = Engine::new(RULES.to_vec());
                assert!(owing(lines).expect("reads").iter().all(|id| *id == set_up));
            } else {
                let n = usize::try_from(next(2)).expect("small");
                for id in [&set_up, &topic] {
                    let reading = transcript::read(lines, id).expect("reads");
                    if let Some((Conversational::Opening, _)) = engines[n].due(id, &reading) {
                        open(lines, id, at(2 + step), |t| {
                            t.template()
                                .opening(suzatary_connect::secrets::SecretState::Missing)
                        })
                        .expect("written");
                    }
                }
                ran = true;
            }
        }
        if !ran {
            for id in owing(lines).expect("reads") {
                open(lines, &id, at(30), |t| {
                    t.template()
                        .opening(suzatary_connect::secrets::SecretState::Missing)
                })
                .expect("written");
            }
        }
        let notes = |id: &ConversationId| {
            transcript::read(lines, id)
                .expect("reads")
                .entries()
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        Entry::Said {
                            by: Some(Stamp::Box),
                            ..
                        }
                    )
                })
                .count()
        };
        assert_eq!(notes(&set_up), 1, "seed {seed}");
        assert_eq!(notes(&topic), 0, "seed {seed}");
        assert!(owing(lines).expect("reads").is_empty(), "seed {seed}");
    }
}
