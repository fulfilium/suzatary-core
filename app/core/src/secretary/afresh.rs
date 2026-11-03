//! The owner's request, asked again in a fresh conversation. A [`Request`] is a turn that the
//! gate stamped as the owner's, with words and no file, so it is clean on its own. It is the only
//! line that crosses from a conversation that read more into the fresh one.

use crate::capability::{Datetime, Turn};
use crate::refusal::Refusal;
use crate::secretary::queue;
use crate::secretary::store::{ConversationId, Entry, Said, Stored};
use crate::secretary::taint::{self, Taint};

/// One turn of the owner's own words, and where it was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    turn: usize,
    said: Said,
}

impl Request {
    /// The owner's request in the round that `stored` answers: its last turn of his own clean
    /// words. `None` when the round holds none, as in a round the box opened, or one whose words
    /// carry a file or point at an unconfirmed todo.
    #[must_use]
    pub fn in_round(stored: &Stored) -> Option<Self> {
        queue::delta(stored)
            .into_iter()
            .rev()
            .find(|(_, said)| own(said))
            .map(|(turn, said)| Request {
                turn,
                said: said.clone(),
            })
    }

    /// Turn `turn` of `stored`, read again when the card is tapped, because the owner can erase
    /// a turn after the card is filed.
    ///
    /// # Errors
    /// [`Refusal::conflict`] when that turn is no longer the owner's own clean words.
    pub fn at_tap(stored: &Stored, turn: usize) -> Result<Self, Refusal> {
        stored
            .turns
            .get(turn)
            .filter(|said| own(said))
            .map(|said| Request {
                turn,
                said: said.clone(),
            })
            .ok_or_else(|| {
                Refusal::conflict(format!(
                    "turn {turn} of {} is no longer your own words alone, so no conversation \
                     was opened",
                    stored.id
                ))
            })
    }

    /// Its index among its conversation's turns.
    #[must_use]
    pub fn turn(&self) -> usize {
        self.turn
    }

    /// The fresh conversation's first line: the same turn under the same stamp, said at `at`.
    #[must_use]
    pub fn entry(&self, at: Datetime) -> Entry {
        Entry::Said {
            at,
            turn: self.said.turn.clone(),
            by: self.said.by.clone(),
        }
    }

    /// The act that asks it again, filed in `conversation`.
    #[must_use]
    pub fn act(&self, conversation: &ConversationId) -> crate::secretary::acts::Act {
        crate::secretary::acts::Act::Afresh {
            conversation: conversation.clone(),
            turn: self.turn,
        }
    }
}

/// Whether `said` is a person's words that are clean on their own: an erased turn has no words,
/// and the taint of a person's turn is clean only under the gate's stamp of the owner, with no
/// file.
fn own(said: &Said) -> bool {
    matches!(said.turn, Turn::Person { .. }) && taint::turn(said) == Taint::Clean
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ToolCall;
    use crate::secretary::stamp::{Speaker, Stamp};

    fn at() -> Datetime {
        Datetime::new(2026, 11, 1, 9, 0, 0, 0).expect("an instant")
    }

    fn said(turn: Turn, by: Option<Stamp>) -> Entry {
        Entry::Said { at: at(), turn, by }
    }

    fn person(text: &str, attachments: Vec<crate::attachment::Attachment>) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments,
        }
    }

    fn owner() -> Stamp {
        Stamp::Actor(Speaker::device("ada"))
    }

    fn stored(lines: Vec<Entry>) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: at(),
            title: crate::secretary::words::Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: crate::secretary::store::Kind::Topic,
        }];
        entries.extend(lines);
        Stored::of(&ConversationId::new("default").expect("a slug"), &entries)
            .expect("a conversation")
    }

    /// Only a person's turn under the owner's stamp with no file is a request; an unstamped
    /// turn, an unconfirmed pointer, an act line and the model's words never are, so nothing
    /// that a round read can be carried as his.
    #[test]
    fn only_the_owners_clean_words_are_a_request() {
        let calling = Turn::Model {
            text: "reading".to_owned(),
            calls: vec![ToolCall {
                verb: "search".into(),
                args: std::collections::BTreeMap::new(),
            }],
        };
        let relayed = Turn::Act {
            text: "do this".to_owned(),
            references: Vec::new(),
        };
        let conversation = stored(vec![
            said(person("start a session", Vec::new()), Some(owner())),
            said(calling, None),
            said(person("an unstamped line", Vec::new()), None),
            said(
                person("a pointer", Vec::new()),
                Some(Stamp::Unconfirmed(Speaker::device("ada"))),
            ),
            said(relayed, Some(owner())),
        ]);
        let request = Request::in_round(&conversation).expect("his request");
        assert_eq!(request.turn(), 0);
        for turn in 1..conversation.turns.len() {
            assert!(Request::at_tap(&conversation, turn).is_err(), "turn {turn}");
        }
        assert_eq!(Request::at_tap(&conversation, 0), Ok(request));
        assert!(Request::at_tap(&conversation, conversation.turns.len()).is_err());
        assert_eq!(
            Request::in_round(&stored(vec![said(
                person(
                    "see the file",
                    vec![crate::attachment::Attachment {
                        id: crate::attachment::AttachmentId::of(b"a scan"),
                        media_type: crate::enclosed::Enclosure::Pdf,
                        name: None,
                        private: None,
                    }]
                ),
                Some(owner())
            )])),
            None,
            "a file is not his words"
        );
    }
}
