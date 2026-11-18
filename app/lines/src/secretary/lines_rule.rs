//! The lines' own rule for what a tab posts to them ([`Admission`]): a tab writes a rename, an
//! opening and a setup, each sealed as the box seals it, under the current redaction rules, by
//! the actor its signer proves, and each opening under its own conversation's id.
//!
//! What it checks is what the post proves, never what a line claims: each slot is opened with
//! the key posted beside it and scrubbed again, so no unscrubbed word is taken; a rename's
//! namer is the poster, whom the shared rule has already proven the signer; an opening's id is
//! computed again from its clear fields ([`LineEvent::begun_id`]).

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::change::Change;
use prodrome_core::event::Hash;
use suzatary_base::conversation_id::ConversationId;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_store::admission::{Admission, Beside, Inadmissible, Post};
use suzatary_store::receipt::Grounds;

use crate::secretary::line_event::LineEvent;
use crate::secretary::line_shape::LineShape;
use suzatary_conversations::secretary::store::{Kind, Namer, Title};
use suzatary_memory::secretary::line_words::LineWords;

/// The lines' rule.
#[derive(Debug, Clone, Copy)]
pub enum LinesRule {}

/// Each slot's key, in slot order, by the name of the line it opens.
#[derive(Debug, Default)]
pub struct SlotKeys(BTreeMap<Hash, Vec<TextKey>>);

impl SlotKeys {
    /// The keys of the line named `name`, in slot order; none for a line posted without any.
    #[must_use]
    pub fn of(&self, name: &Hash) -> &[TextKey] {
        self.0.get(name).map_or(&[], Vec::as_slice)
    }
}

impl Beside for SlotKeys {
    fn parse(posted: &BTreeMap<String, Vec<String>>) -> Result<Self, String> {
        posted
            .iter()
            .map(|(name, keys)| {
                let name = Hash::new(name.clone()).map_err(|_| "a key's line is no object name")?;
                let keys = keys
                    .iter()
                    .map(|key| TextKey::from_hex(key).ok_or("a slot's key is not 64 hex digits"))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((name, keys))
            })
            .collect::<Result<_, &str>>()
            .map(Self)
            .map_err(str::to_owned)
    }
}

/// What the lines' rule reads of the lines: every conversation they hold, and the name of every
/// key a line of theirs was sealed under, the keys an erasure destroyed among them.
#[derive(Debug, Default)]
pub struct Held {
    conversations: BTreeSet<ConversationId>,
    keys: BTreeSet<KeyName>,
}

impl Admission for LinesRule {
    type Schema = LineEvent;
    type Beside = SlotKeys;
    type Held = Held;

    fn held<'e>(lines: impl Iterator<Item = &'e LineEvent>) -> Held {
        let mut held = Held::default();
        for line in lines {
            held.conversations.insert(line.conversation().clone());
            held.keys.extend(line.keys().cloned());
        }
        held
    }

    fn refuses(change: &Change<LineEvent>, post: &Post<'_, Self>) -> Option<Inadmissible> {
        let line = &change.event;
        begins(line, post)
            .err()
            .or_else(|| stamped(line, change).err())
            .or_else(|| fresh(line, &post.held.keys).err())
            .or_else(|| sealed(line, post.beside.of(post.name)).err())
    }
}

/// Whether every key `line` was sealed under is one no line the store holds names: a fresh key.
/// A key a held line names is a key the box keeps for that line's words, or one an erasure
/// destroyed; taking it again would open those words to whoever posted it, so no post brings a
/// key back.
fn fresh(line: &LineEvent, held: &BTreeSet<KeyName>) -> Result<(), Inadmissible> {
    if line.keys().any(|key| held.contains(key)) {
        Err(Inadmissible::finally(
            "its words are sealed under a key a line the box holds already names; a tab seals under a key of its own",
        ))
    } else {
        Ok(())
    }
}

/// Whether `line` is a shape a tab writes, and begins or names its conversation rightly: an
/// opening under its own id, of a conversation the store does not hold; a rename of one the
/// store holds or the post begins.
fn begins(line: &LineEvent, post: &Post<'_, LinesRule>) -> Result<(), Inadmissible> {
    let conversation = line.conversation();
    match line.shape() {
        LineShape::Opened { kind, .. } if *kind != Some(Kind::Topic) => Err(Inadmissible::finally(
            "a tab opens a topic, and the box opens every other kind of conversation",
        )),
        LineShape::Opened { model, .. } if !model.is_empty() => Err(Inadmissible::finally(
            "a tab's opening names no model: the box says which model answers",
        )),
        LineShape::Opened { .. } | LineShape::SetUp { .. } => {
            if *conversation != line.begun_id() {
                Err(Inadmissible::finally(
                    "a new conversation is named by its opening line, and this one names another",
                ))
            } else if post.held.conversations.contains(conversation) {
                Err(Inadmissible::finally(format!(
                    "the conversation {conversation} already exists"
                )))
            } else {
                Ok(())
            }
        }
        LineShape::Titled { .. } => {
            let begun = post.changes.iter().any(|other| {
                other.event.conversation() == conversation
                    && matches!(
                        other.event.shape(),
                        LineShape::Opened { .. } | LineShape::SetUp { .. }
                    )
                    && other.event.begun_id() == *conversation
            });
            if post.held.conversations.contains(conversation) || begun {
                Ok(())
            } else {
                Err(Inadmissible::finally(format!(
                    "no conversation {conversation} to rename"
                )))
            }
        }
        LineShape::Said { .. }
        | LineShape::Proposed { .. }
        | LineShape::Reviewed { .. }
        | LineShape::ReviewFailed { .. }
        | LineShape::Stopped { .. }
        | LineShape::Rewound { .. }
        | LineShape::MemorySeen { .. }
        | LineShape::Session { .. }
        | LineShape::Unheard { .. }
        | LineShape::Kept { .. }
        | LineShape::Scoped { .. }
        | LineShape::Coding { .. }
        | LineShape::Erased { .. }
        | LineShape::Resealed { .. } => Err(Inadmissible::finally(format!(
            "the box alone writes a {} line",
            line.shape().name()
        ))),
    }
}

/// Whether `line`'s stamp is what its signer proves: a rename names its poster as the person who
/// gave it, the actor the shared rule took the change as. An opening carries no stamp.
fn stamped(line: &LineEvent, change: &Change<LineEvent>) -> Result<(), Inadmissible> {
    use prodrome_core::schema::Schema as _;
    let LineShape::Titled { by } = line.shape() else {
        return Ok(());
    };
    match by {
        Namer::Person { actor } if actor.as_str() == change.event.actor().as_str() => Ok(()),
        Namer::Person { .. } | Namer::Job { .. } => Err(Inadmissible::finally(
            "a rename is the poster's own, and this one names someone else",
        )),
    }
}

/// Whether each slot of `line` opens with its posted key and holds words as the box would seal
/// them: under the current rules (else outdated), and unchanged by a scrub under them.
fn sealed(line: &LineEvent, keys: &[TextKey]) -> Result<(), Inadmissible> {
    let slots = line.shape().slots();
    if keys.len() != slots {
        return Err(Inadmissible::finally(format!(
            "a {} line has {slots} slots of words, and {} keys came with it",
            line.shape().name(),
            keys.len()
        )));
    }
    for (slot, (key, under)) in keys.iter().zip(line.scrubbed_under()).enumerate() {
        let Some(words) = line.unscrubbed(slot, key) else {
            return Err(Inadmissible::finally(format!(
                "slot {slot} does not open with the key posted for it"
            )));
        };
        if !under.is_current(()) {
            return Err(Inadmissible {
                grounds: Grounds::Outdated,
                reason: "this page is older than the box: reload to send it".to_owned(),
            });
        }
        if !matches!(words.scrubbed(), Ok(scrubbed) if scrubbed == words) {
            return Err(Inadmissible::finally(format!(
                "slot {slot}'s words are not as the box would seal them"
            )));
        }
        fits(line.shape(), &words)?;
    }
    Ok(())
}

/// Whether `words` are what a line of `shape` says, read as the box reads it: a rename's are a
/// [`Title`]; an opening's are empty or a [`Title`]; a setup's are its template's own name, which
/// the box gave it when it set one up.
fn fits(shape: &LineShape, words: &LineWords) -> Result<(), Inadmissible> {
    let LineWords::Title { title } = words else {
        return Err(Inadmissible::finally("its words are no title"));
    };
    let titled = || match Title::new(title.as_str()) {
        Ok(read) if read.as_str() == title => Ok(()),
        Ok(_) => Err(Inadmissible::finally(
            "a title is held as it reads, with no space around it",
        )),
        Err(refused) => Err(Inadmissible::finally(format!(
            "its title is refused: {refused}"
        ))),
    };
    match shape {
        LineShape::Opened { .. } if title.is_empty() => Ok(()),
        LineShape::Titled { .. } | LineShape::Opened { .. } => titled(),
        LineShape::SetUp { template } if *title == template.template().called() => Ok(()),
        LineShape::SetUp { .. } => Err(Inadmissible::finally(
            "a setup is titled by its template, and this one names another title",
        )),
        LineShape::Said { .. }
        | LineShape::Proposed { .. }
        | LineShape::Reviewed { .. }
        | LineShape::ReviewFailed { .. }
        | LineShape::Stopped { .. }
        | LineShape::Rewound { .. }
        | LineShape::MemorySeen { .. }
        | LineShape::Session { .. }
        | LineShape::Unheard { .. }
        | LineShape::Kept { .. }
        | LineShape::Scoped { .. }
        | LineShape::Coding { .. }
        | LineShape::Erased { .. }
        | LineShape::Resealed { .. } => Err(Inadmissible::finally(format!(
            "the box alone writes a {} line",
            shape.name()
        ))),
    }
}
