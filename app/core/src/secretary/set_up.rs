//! A conversation set up from a template (`SetUp`) is owed the box's opening note: the template's
//! steps the box still owes, as an act line stamped by the box. Owed is a reading of the
//! conversation, a `SetUp` with no such note after it, and the note is written under the lines'
//! lock only while it is still owed, so each setup gets one note whatever restarts and workers
//! meet it.

use crate::secretary::conversations::Conversations;
use crate::secretary::model::Turn;
use crate::secretary::stamp::Stamp;
use crate::secretary::store::{Begun, Entry};
use crate::secretary::transcript::{self, Transcript};
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_connect::connect::TemplateId;
use suzatary_connect::secrets::SecretState;
use suzatary_store::store::StoreError;

/// The template whose opening `reading` is owed: none for a conversation not set up, or one the
/// box has noted in.
#[must_use]
pub fn owed(reading: &Transcript) -> Option<TemplateId> {
    let entries = reading.entries();
    let Begun::SetUp(template) = entries.first()?.start()?.begun else {
        return None;
    };
    let noted = entries.iter().skip(1).any(|entry| {
        matches!(
            entry,
            Entry::Said {
                turn: Turn::Act { .. },
                by: Some(Stamp::Box),
                ..
            }
        )
    });
    (!noted).then_some(template)
}

/// The opening `note` of `id`'s template written at `at`, where it is still owed; whether it
/// was.
///
/// # Errors
/// The stores'.
pub fn open(
    conversations: Conversations<'_>,
    id: &ConversationId,
    at: Datetime,
    note: impl FnOnce(TemplateId) -> String,
) -> Result<bool, StoreError> {
    transcript::push_owed(conversations, id, |reading| {
        owed(reading).map(|template| Entry::noted(at, note(template), Vec::new()))
    })
}

/// Every conversation owed its opening, as a restart finds them: the note a worker was to write
/// when the box stopped.
///
/// # Errors
/// The store's own.
pub fn owing(conversations: Conversations<'_>) -> Result<Vec<ConversationId>, StoreError> {
    Ok(transcript::each(conversations)?
        .into_iter()
        .filter(|(_, read)| read.as_ref().is_ok_and(|read| owed(read).is_some()))
        .map(|(id, _)| id)
        .collect())
}

/// Whether the box's Google client is stored, its id and its secret both.
#[must_use]
pub fn client_kept(
    secrets: &dyn suzatary_connect::secrets::Secrets,
    settables: &suzatary_connect::settings::Settables,
) -> SecretState {
    SecretState::all(
        [
            suzatary_google::google::CLIENT_ID,
            suzatary_google::google::CLIENT_SECRET,
        ]
        .map(|name| {
            suzatary_connect::settings::Settable::mint(settables, name)
                .map_or(SecretState::Missing, |name| secrets.secret_state(&name))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::rules::{Conversational, RULES};
    use crate::secretary::store::Entry;
    use suzatary_memory::secretary::words::Words;
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
            let store = crate::secretary::conversations::InMemory::default();
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
                    kind: crate::secretary::store::Kind::Topic,
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
}
