//! What a tab wrote to the lines and the box has not served back yet: each line it sealed, with
//! its node as the box computes it, its print and its slots' keys, and each signature of one by
//! the device's key; an own line reads back from its print and its keys alone ([`OwnLine::read`]).

use std::collections::BTreeMap;

use prodrome_core::event::{Envelope, Hash, parse_envelope};
use sha2::{Digest as _, Sha256};

use crate::secretary::line_event::{LineEvent, LineVocabulary};
use crate::secretary::line_form;
use crate::secretary::outlines::Parsed;
use suzatary_memory::secretary::key_ring::{KeyRing, spelled};
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_store::replica::held::Node;
use suzatary_store::replica::writes::Role;

/// What a line of the person's gives back when the box refuses it for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    /// A question whole: its words, what it points at, and its files.
    Question {
        text: String,
        references: Vec<suzatary_base::reference::Reference>,
        attachments: Vec<suzatary_held::attachment::Attachment>,
    },
    /// A title: a rename's, or a topic's.
    Title(String),
    /// An edit's question whole, and the question it edits.
    Edit {
        question: Hash,
        text: String,
        references: Vec<suzatary_base::reference::Reference>,
        attachments: Vec<suzatary_held::attachment::Attachment>,
    },
}

impl Words {
    /// Its words as a sentence quotes them.
    #[must_use]
    pub fn quoted(&self) -> &str {
        match self {
            Words::Question { text, .. } | Words::Edit { text, .. } => text,
            Words::Title(title) => title,
        }
    }
}

/// One object a tab wrote to the lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnLine {
    Line {
        /// Its node, as the box would serve it.
        node: Box<Parsed>,
        /// Its canonical print.
        print: String,
        keys: KeyRing,
    },
    /// The device's signature of a line.
    Signature {
        name: Hash,
        /// The line it signs, the one name it rests on.
        signs: [Hash; 1],
        /// Its canonical print.
        print: String,
    },
}

impl OwnLine {
    /// The object `print` is, with the keys `keys` (by name, in hex) of a line's slots.
    pub fn read(print: String, keys: &BTreeMap<KeyName, String>) -> Result<Self, String> {
        let name = Hash::new(hex::encode(Sha256::digest(print.as_bytes())))
            .unwrap_or_else(|_| unreachable!("a SHA-256 in lowercase hex is a name"));
        match parse_envelope::<LineEvent>(&LineVocabulary, &print).map_err(|e| e.to_string())? {
            Envelope::Signed(signed) => Ok(OwnLine::Signature {
                name,
                signs: [signed.object],
                print,
            }),
            envelope @ Envelope::Change(_) => {
                let keys = KeyRing::of(keys);
                let line = envelope.event().unwrap_or_else(|| unreachable!("a change"));
                if let Some(missing) = line
                    .keys()
                    .find(|key| keys.iter().all(|(name, _)| name != *key))
                {
                    return Err(format!("no key for the slot locked under {missing}"));
                }
                let node = Parsed::of(line_form::skeleton_node(&name, &envelope))
                    .map_err(|e| e.to_string())?;
                Ok(OwnLine::Line {
                    node: Box::new(node),
                    print,
                    keys,
                })
            }
            other => Err(format!(
                "a tab writes a line or its signature, not a {}",
                other.name()
            )),
        }
    }

    /// `line` sealed by this tab, opened by `keys`, as an own line.
    ///
    /// # Errors
    /// [`OwnLine::read`]'s, which a line just sealed never meets.
    pub fn sealed(line: &Envelope<LineEvent>, keys: &[TextKey]) -> Result<Self, String> {
        let keys = keys.iter().map(|key| (key.name(), spelled(key))).collect();
        Self::read(prodrome_core::event::canonical_envelope(line), &keys)
    }

    #[must_use]
    pub fn print(&self) -> &str {
        match self {
            OwnLine::Line { print, .. } | OwnLine::Signature { print, .. } => print,
        }
    }

    /// Each key of its slots, in hex by name; none for a signature.
    #[must_use]
    pub fn keys(&self) -> BTreeMap<KeyName, String> {
        match self {
            OwnLine::Line { keys, .. } => keys
                .iter()
                .map(|(name, bytes)| (name.clone(), hex::encode(bytes)))
                .collect(),
            OwnLine::Signature { .. } => BTreeMap::new(),
        }
    }

    /// What the person wrote in it, opened with its own keys: a question whole, or a title (a
    /// rename's, or a topic's it was opened under); none for a line that holds neither, or a
    /// signature.
    #[must_use]
    pub fn words(&self) -> Option<Words> {
        use suzatary_conversations::secretary::model::Turn;
        use suzatary_conversations::secretary::store::{Entry, Title};
        let OwnLine::Line { print, keys, .. } = self else {
            return None;
        };
        let envelope = parse_envelope::<LineEvent>(&LineVocabulary, print).ok()?;
        // A title is words only where it says something: a topic opened with none gives nothing back.
        let title = |title: Option<&str>| {
            title
                .filter(|title| !title.is_empty())
                .map(|title| Words::Title(title.to_owned()))
        };
        match envelope.event()?.entry(&|name: &KeyName| keys.key(name)) {
            Entry::Said {
                turn:
                    Turn::Person {
                        text,
                        references,
                        attachments,
                    },
                ..
            } => Some(Words::Question {
                text,
                references,
                attachments,
            }),
            Entry::Titled { title: words, .. } => title(words.get().map(Title::as_str)),
            Entry::Opened { title: words, .. } => title(words.get().map(String::as_str)),
            _ => None,
        }
    }

    #[must_use]
    pub fn opens(&self) -> bool {
        matches!(
            self,
            OwnLine::Line { node, .. }
                if matches!(&node.given().header, crate::wire::lines::NodeHeader::Line { step: crate::secretary::step::Step::Opened, .. })
        )
    }

    /// Whether it is an edit's own question.
    pub(crate) fn edits(&self) -> bool {
        matches!(
            self,
            OwnLine::Line { node, .. }
                if matches!(&node.given().header, crate::wire::lines::NodeHeader::Line { step: crate::secretary::step::Step::Edits, .. })
        )
    }

    /// The line its ruling is drawn on: an `Again`'s question, which it makes owed; else itself.
    #[must_use]
    pub fn drawn_on(&self) -> &Hash {
        match self {
            OwnLine::Line { node, .. } => match &node.given().header {
                crate::wire::lines::NodeHeader::Line {
                    step: crate::secretary::step::Step::Again { question, .. },
                    ..
                } => question,
                crate::wire::lines::NodeHeader::Line { .. }
                | crate::wire::lines::NodeHeader::Structure { .. } => node.name(),
            },
            OwnLine::Signature { name, .. } => name,
        }
    }

    /// The conversation it is a line of; none for a signature.
    #[must_use]
    pub fn conversation(&self) -> Option<&str> {
        match self {
            OwnLine::Line { node, .. } => match &node.given().header {
                crate::wire::lines::NodeHeader::Line { conversation, .. } => Some(conversation),
                crate::wire::lines::NodeHeader::Structure { .. } => None,
            },
            OwnLine::Signature { .. } => None,
        }
    }

    /// Its writer's instant, as its node spells it; none for a signature.
    #[must_use]
    pub fn at(&self) -> Option<&str> {
        match self {
            OwnLine::Line { node, .. } => match &node.given().header {
                crate::wire::lines::NodeHeader::Line { at, .. } => Some(at),
                crate::wire::lines::NodeHeader::Structure { .. } => None,
            },
            OwnLine::Signature { .. } => None,
        }
    }

    /// What it is to a post.
    #[must_use]
    pub fn role(&self) -> Role {
        match self {
            OwnLine::Line { .. } => Role::Change,
            OwnLine::Signature { signs: [line], .. } => Role::Signs(line.clone()),
        }
    }
}

impl Node for OwnLine {
    fn name(&self) -> &Hash {
        match self {
            OwnLine::Line { node, .. } => node.name(),
            OwnLine::Signature { name, .. } => name,
        }
    }

    fn rests_on(&self) -> &[Hash] {
        match self {
            OwnLine::Line { node, .. } => node.parents(),
            OwnLine::Signature { signs, .. } => signs,
        }
    }

    fn prodrome(&self) -> Option<&Hash> {
        None
    }
}
