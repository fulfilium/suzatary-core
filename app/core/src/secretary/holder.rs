//! The registry of every store that holds a copy of a conversation's words.

/// A store holding a copy of a conversation's words. For each slot it kills,
/// [`crate::secretary::transcript::sweep`] empties each [`Fate::Deleted`] holder in order; a
/// copy it could not delete keeps the key for the next sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// The words opened in memory.
    Opened,
    /// The pages the box drew from the model's answers.
    Renders,
    /// The model CLI's session files, by the session ids the lines record.
    ModelSessions,
    /// The keys that open the sealed words, destroyed last.
    Keys,
    /// The keys in the vault's daily snapshots, which open the data repository's ciphertext.
    Vault,
    /// The proposals and briefings a conversation led to, pushed in the clear.
    DataHistory,
}

/// What an erasure leaves in a holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    /// The sweep deletes the copy before it destroys the key.
    Deleted,
    /// Out of the box's reach, for as long as this says.
    Remains(&'static str),
}

impl Holder {
    /// In the order the sweep empties them.
    pub const ALL: [Holder; 6] = [
        Holder::Opened,
        Holder::Renders,
        Holder::ModelSessions,
        Holder::Keys,
        Holder::Vault,
        Holder::DataHistory,
    ];

    /// As the erase question names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Holder::Opened => "the words in memory",
            Holder::Renders => "the pages drawn from them",
            Holder::ModelSessions => "the model's session files",
            Holder::Keys => "the keys that open them",
            Holder::Vault => "the keys in the vault's encrypted snapshots on Drive",
            Holder::DataHistory => {
                "the proposals and briefings it led to, in the data repository's history"
            }
        }
    }

    #[must_use]
    pub const fn fate(self) -> Fate {
        match self {
            Holder::Opened | Holder::Renders | Holder::ModelSessions | Holder::Keys => {
                Fate::Deleted
            }
            // `scripts/vault-backup.sh` keeps seven daily snapshots.
            Holder::Vault => Fate::Remains("for up to seven days"),
            Holder::DataHistory => Fate::Remains("until that history is rewritten"),
        }
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::literal::Datetime;

    use crate::secretary::conversations::InMemory;
    use crate::secretary::erasure::Erasure;
    use crate::secretary::line_event::LineEvent;
    use crate::secretary::model::{SessionId, Turn};
    use crate::secretary::scrubbed_under::ScrubbedUnder;
    use crate::secretary::store::{Entry, Kind};
    use crate::secretary::transcript;
    use crate::secretary::words::Words;
    use suzatary_base::conversation_id::ConversationId;
    use suzatary_store::store::{Doc as _, Gets as _, Prodrome as _};

    const SESSION: &str = "6f1c2b0e-8a4d-4c55-9f3e-2b7a1d9e0c41";

    fn at(second: u32) -> Datetime {
        Datetime::new(2026, 11, 8, 9, 0, second, 0).expect("an instant")
    }

    /// A conversation the model held a session over, written under older rules, so a reseal
    /// finds its owner's words stale; answers the key of those words.
    fn spoken(held: &InMemory, id: &ConversationId) -> crate::secretary::locked_text::KeyName {
        let entries = [
            Entry::Opened {
                at: at(0),
                title: Words::kept("Folders".to_owned()),
                model: "a-cli/small".to_owned(),
                kind: Kind::Topic,
            },
            Entry::Session {
                at: at(1),
                session: SessionId::new(SESSION).expect("a uuid"),
                model: "a-cli/small".to_owned(),
                cwd: "/srv/code".to_owned(),
                surface: None,
            },
            Entry::Said {
                at: at(2),
                turn: Turn::Person {
                    text: "reset at https://example.test/r?token=s3cret".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            },
        ];
        let mut said = None;
        for (count, entry) in entries.iter().enumerate() {
            let (line, keys) = LineEvent::written_under(
                &ScrubbedUnder::Unversioned,
                id.clone(),
                transcript::writer(),
                u32::try_from(count).expect("a count"),
                entry,
                &held.entropy,
            )
            .expect("a line");
            for key in &keys {
                held.keys.set(&key.name(), key).expect("kept");
            }
            said = keys
                .last()
                .map(crate::secretary::locked_text::TextKey::name);
            held.lines.append(line).expect("appended");
        }
        said.expect("the owner's words have a key")
    }

    /// Law: the erase question names every holder, and how long each copy out of the box's reach
    /// stays.
    #[test]
    fn the_erase_question_names_every_holder_and_what_remains() {
        let asked = Erasure::for_good();
        for holder in super::Holder::ALL {
            let named = match holder.fate() {
                super::Fate::Deleted => holder.name().to_owned(),
                super::Fate::Remains(until) => format!("{}, {until}", holder.name()),
            };
            assert!(asked.contains(&named), "{named} is not in: {asked}");
        }
    }

    /// Law: each act that kills a slot, an erasure and a reseal by `migrate-scrub`, deletes its
    /// conversation's model session in the same act, and the slot's key is gone after it.
    #[test]
    fn killing_a_slot_deletes_its_conversation_s_model_session() {
        let id = ConversationId::new("2026-11-08-1").expect("a slug");
        for act in ["erase", "migrate-scrub"] {
            let held = InMemory::default();
            let key = spoken(&held, &id);
            match act {
                "erase" => transcript::erase(
                    held.conversations(),
                    &id,
                    [key.clone()].into(),
                    Erasure::Asked {
                        by: transcript::writer(),
                    },
                    at(9),
                )
                .map(|_| ()),
                _ => crate::secretary::rescrub::run(held.conversations(), at(9)).map(|_| ()),
            }
            .expect("the act runs");
            assert!(
                held.forgot
                    .sources()
                    .contains(&format!("session {SESSION} in /srv/code")),
                "{act}: {:?}",
                held.forgot.sources()
            );
            assert!(held.keys.get(&key).expect("reads").is_none(), "{act}");
        }
    }
}
