//! The owner's request, asked again in a fresh conversation: a turn the gate stamped as the
//! owner's, with words and no file, is the only line that crosses from a conversation that read
//! more into the fresh one.

use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::queue;
use suzatary_conversations::secretary::store::{Entry, Said, Stored};
use suzatary_conversations::secretary::taint;
use suzatary_conversations::secretary::taint::Taint;
use suzatary_inbox::secretary::acts;
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_lines::secretary::spoken::Plain;
use suzatary_lines::secretary::spoken::Spoken;

/// One turn of the owner's own words, and where it was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    turn: usize,
    said: Spoken,
    by: Option<Stamp>,
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
            .find_map(|(turn, said)| own(said).map(|own| Request::new(turn, own, said)))
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
            .and_then(|said| own(said).map(|own| Request::new(turn, own, said)))
            .ok_or_else(|| {
                Refusal::conflict(format!(
                    "turn {turn} of {} is no longer your own words alone, so no conversation \
                     was opened",
                    stored.id
                ))
            })
    }

    fn new(turn: usize, said: Spoken, stamped: &Said) -> Self {
        Request {
            turn,
            said,
            by: stamped.by.clone(),
        }
    }

    #[must_use]
    pub fn turn(&self) -> usize {
        self.turn
    }

    /// The fresh conversation's first line: the same turn under the same stamp, said at `at`.
    #[must_use]
    pub fn entry(&self, at: Datetime) -> Plain {
        Entry::Said {
            at,
            turn: self.said.clone(),
            by: self.by.clone(),
        }
    }

    #[must_use]
    pub fn act(&self, conversation: &ConversationId) -> acts::Act {
        acts::Act::Afresh {
            conversation: conversation.clone(),
            turn: self.turn,
        }
    }
}

/// `said` as a person's words, where they are clean on their own: only under the gate's stamp of
/// the owner, with no file.
fn own(said: &Said) -> Option<Spoken> {
    match &said.turn {
        Turn::Person {
            text,
            references,
            attachments,
        } if taint::turn(said) == Taint::Clean => Some(Spoken::person(
            text.clone(),
            references.clone(),
            attachments.clone(),
        )),
        Turn::Person { .. }
        | Turn::Model { .. }
        | Turn::Tool { .. }
        | Turn::Act { .. }
        | Turn::Erased { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_conversations::secretary::model::ToolCall;
    use suzatary_inbox::secretary::stamp::{Speaker, Stamp};

    fn at() -> Datetime {
        Datetime::new(2026, 11, 1, 9, 0, 0, 0).expect("an instant")
    }

    fn said(turn: Turn, by: Option<Stamp>) -> Entry {
        Entry::Said { at: at(), turn, by }
    }

    fn person(text: &str, attachments: Vec<suzatary_held::attachment::Attachment>) -> Turn {
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
            title: suzatary_memory::secretary::words::Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        }];
        entries.extend(lines);
        Stored::of(&ConversationId::new("default").expect("a slug"), &entries)
            .expect("a conversation")
    }

    /// Only a person's turn under the owner's stamp with no file is a request, so nothing that a
    /// round read can be carried as his.
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
                    vec![suzatary_held::attachment::Attachment {
                        id: suzatary_held::attachment::AttachmentId::of(b"a scan"),
                        media_type: suzatary_held::enclosed::Enclosure::Pdf,
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
