//! A turn without its words: who said it, what it pointed at, which verbs it named, which files
//! it held. Everything that reads a turn for what it IS (its taint, whether it closes a round,
//! whether a rewind may hide it) reads this, so a turn whose words are gone reads as it read
//! whole.
//!
//! [`split`] and [`join`] are a lawful pair: `join(split(t)) == t` for every turn, and a slot
//! that did not open joins as [`Turn::Erased`], its shape intact.

use serde::{Deserialize, Serialize};

use crate::attachment::{Attachment, AttachmentId};
use crate::enclosed::{Enclosed, Enclosure};
use crate::secretary::line_words::{AttachmentWords, LineWords};
use crate::secretary::model::{ToolCall, Turn};
use crate::secretary::outcome::Outcome;
use crate::secretary::tool_name::ToolName;
use crate::secretary::words::{Gone, Words};
use suzatary_base::reference::Reference;

/// A turn's shape, by who said it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "who", rename_all = "lowercase")]
pub enum TurnShape {
    /// A person's: what they pointed at, and each attachment's id and type.
    Person {
        /// What they pointed at.
        references: Vec<Reference>,
        /// Each attachment, in order.
        attachments: Vec<AttachmentShape>,
    },
    /// The model's: the verb of each call it asked for, in order.
    Model {
        /// The calls' verbs.
        calls: Vec<ToolName>,
    },
    /// A verb's answer: which verb, how the call ended, each file it handed over.
    Tool {
        /// The verb.
        verb: ToolName,
        /// How the call ended, a remembered fact's receipt included.
        outcome: Outcome,
        /// Each file, in order.
        files: Vec<FileShape>,
    },
    /// An act: what it was about.
    Act {
        /// What it was about.
        references: Vec<Reference>,
    },
}

/// An attachment without its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentShape {
    /// Its name in the store.
    pub id: AttachmentId,
    /// Its type.
    pub media_type: Enclosure,
}

/// A file a verb handed over, without its name or its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileShape {
    /// Its name in the store.
    pub id: AttachmentId,
    /// What it is.
    pub media: Enclosure,
    /// Its size in bytes.
    pub size: u64,
}

impl TurnShape {
    /// How many slots of words a turn of this shape holds: one, or for a model turn its prose and
    /// then each call's arguments.
    #[must_use]
    pub fn slots(&self) -> usize {
        match self {
            TurnShape::Model { calls } => 1 + calls.len(),
            TurnShape::Person { .. } | TurnShape::Tool { .. } | TurnShape::Act { .. } => 1,
        }
    }
}

/// A turn's shape and its words, one entry per slot; no words for a turn that is already
/// [`Turn::Erased`], whose words are not all here.
#[must_use]
pub fn split(turn: &Turn) -> (TurnShape, Option<Vec<LineWords>>) {
    let (shape, words) = match turn {
        Turn::Person {
            text,
            references,
            attachments,
        } => (
            TurnShape::Person {
                references: references.clone(),
                attachments: attachments
                    .iter()
                    .map(|attached| AttachmentShape {
                        id: attached.id.clone(),
                        media_type: attached.media_type,
                    })
                    .collect(),
            },
            vec![LineWords::Person {
                text: text.clone(),
                attachments: attachments
                    .iter()
                    .map(|attached| AttachmentWords {
                        name: attached.name.clone(),
                        private: attached.private.clone(),
                    })
                    .collect(),
            }],
        ),
        Turn::Model { text, calls } => (
            TurnShape::Model {
                calls: calls.iter().map(|call| call.verb.clone()).collect(),
            },
            std::iter::once(LineWords::Model { text: text.clone() })
                .chain(calls.iter().map(|call| LineWords::Args {
                    args: call.args.clone(),
                }))
                .collect(),
        ),
        Turn::Tool {
            verb,
            answer,
            outcome,
            files,
        } => (
            TurnShape::Tool {
                verb: verb.clone(),
                outcome: *outcome,
                files: files
                    .iter()
                    .map(|file| FileShape {
                        id: file.id.clone(),
                        media: file.media,
                        size: file.size,
                    })
                    .collect(),
            },
            vec![LineWords::Tool {
                answer: answer.clone().into(),
                names: files.iter().map(|file| file.name.clone()).collect(),
                texts: files.iter().map(|file| file.text.clone()).collect(),
            }],
        ),
        Turn::Act { text, references } => (
            TurnShape::Act {
                references: references.clone(),
            },
            vec![LineWords::Act { text: text.clone() }],
        ),
        Turn::Erased { shape, .. } => return (shape.clone(), None),
    };
    (shape, Some(words))
}

/// The turn a shape and its slots' words make: the whole turn when every slot opened to words of
/// its kind, else [`Turn::Erased`] with each slot as it read. A slot whose words are of another
/// kind, or of another count of files, reads as [`Gone::Missing`]: no writer made it.
#[must_use]
pub fn join(shape: TurnShape, mut words: Vec<Words<LineWords>>) -> Turn {
    words.resize_with(shape.slots(), || Gone::Missing.into());
    match whole(&shape, &words) {
        Some(turn) => turn,
        None => Turn::Erased {
            words: words
                .into_iter()
                .enumerate()
                .map(|(slot, read)| match read {
                    Words::Kept { kept } if fits(&shape, slot, &kept) => Words::kept(kept),
                    Words::Kept { .. } => Gone::Missing.into(),
                    gone @ Words::Gone { .. } => gone,
                })
                .collect(),
            shape,
        },
    }
}

/// Whether `words` are words of slot `slot` of a turn shaped `shape`.
fn fits(shape: &TurnShape, slot: usize, words: &LineWords) -> bool {
    match (shape, slot, words) {
        (TurnShape::Person { attachments, .. }, 0, LineWords::Person { attachments: w, .. }) => {
            w.len() == attachments.len()
        }
        (TurnShape::Model { .. }, 0, LineWords::Model { .. })
        | (TurnShape::Model { .. }, 1.., LineWords::Args { .. })
        | (TurnShape::Act { .. }, 0, LineWords::Act { .. }) => true,
        (TurnShape::Tool { files, .. }, 0, LineWords::Tool { names, texts, .. }) => {
            names.len() == files.len() && texts.len() == files.len()
        }
        _ => false,
    }
}

/// The turn, when every slot is kept and fits.
fn whole(shape: &TurnShape, words: &[Words<LineWords>]) -> Option<Turn> {
    let kept: Vec<&LineWords> = words
        .iter()
        .enumerate()
        .map(|(slot, read)| read.get().filter(|words| fits(shape, slot, words)))
        .collect::<Option<_>>()?;
    Some(match (shape, kept.as_slice()) {
        (
            TurnShape::Person {
                references,
                attachments,
            },
            [
                LineWords::Person {
                    text,
                    attachments: w,
                },
            ],
        ) => Turn::Person {
            text: text.clone(),
            references: references.clone(),
            attachments: attachments
                .iter()
                .zip(w)
                .map(|(shape, words)| Attachment {
                    id: shape.id.clone(),
                    media_type: shape.media_type,
                    name: words.name.clone(),
                    private: words.private.clone(),
                })
                .collect(),
        },
        (TurnShape::Model { calls }, [LineWords::Model { text }, args @ ..]) => Turn::Model {
            text: text.clone(),
            calls: calls
                .iter()
                .zip(args)
                .map(|(verb, args)| match args {
                    LineWords::Args { args } => Some(ToolCall {
                        verb: verb.clone(),
                        args: args.clone(),
                    }),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        },
        (
            TurnShape::Tool {
                verb,
                outcome,
                files,
            },
            [
                LineWords::Tool {
                    answer,
                    names,
                    texts,
                },
            ],
        ) => Turn::Tool {
            verb: verb.clone(),
            answer: answer.as_str().to_owned(),
            outcome: *outcome,
            files: files
                .iter()
                .zip(names.iter().zip(texts))
                .map(|(file, (name, text))| Enclosed {
                    id: file.id.clone(),
                    name: name.clone(),
                    media: file.media,
                    size: file.size,
                    text: text.clone(),
                })
                .collect(),
        },
        (TurnShape::Act { references }, [LineWords::Act { text }]) => Turn::Act {
            text: text.clone(),
            references: references.clone(),
        },
        _ => return None,
    })
}

/// What a reader is told of a turn's prose, its first slot: the words where kept, else the box's
/// sentence for why they are gone, bracketed so it never reads as somebody's words.
#[must_use]
pub fn prose(words: &[Words<LineWords>]) -> String {
    match words.first() {
        Some(Words::Kept { kept }) => kept.text().into_owned(),
        Some(Words::Gone { gone }) => format!("[{}]", gone.sentence()),
        None => String::new(),
    }
}

/// The shape of any turn, whole or not.
#[must_use]
pub fn shape(turn: &Turn) -> TurnShape {
    split(turn).0
}
