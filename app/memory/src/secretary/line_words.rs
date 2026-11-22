//! The words of one slot of a conversation line: what is locked under one key.
//!
//! Each slot has its own key, so a forgetting can erase the one call that wrote a fact and leave
//! the prose beside it.
//! Every slot's words go through [`Scrubbed`] before `line_event::seal` locks them.
//! Reading a print only parses it; the line scrubs a slot under older rules again as it opens.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::secretary::salted_text::Words;
use suzatary_held::redact::Scrubbed;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "lowercase")]
pub enum LineWords {
    Person {
        text: String,
        attachments: Vec<AttachmentWords>,
    },
    /// A model turn's prose.
    Model { text: String },
    Args {
        args: BTreeMap<String, serde_json::Value>,
    },
    /// A verb's answer, and each file it handed over: its name and its text layer.
    Tool {
        answer: StoredAnswer,
        names: Vec<String>,
        texts: Vec<String>,
    },
    /// An act's sentence.
    Act { text: String },
    /// A conversation's name.
    Title { title: String },
    /// A review's reason.
    Why { why: String },
    /// Why a review could not run.
    Reason { reason: String },
    Report {
        report: crate::coding_report::Report,
    },
    /// Why the owner let a coding session go.
    Dropped {
        /// His words.
        reason: String,
    },
}

/// The most bytes of a verb's answer one line stores; the model reads the whole answer only in the
/// round that asked.
pub const STORED_ANSWER_BYTES: usize = 64 * 1024;

/// Bytes kept at the end of a cut answer for the sentence saying how much was left out.
const NOTE_BYTES: usize = 128;

/// A verb's answer as a line stores it: at most [`STORED_ANSWER_BYTES`] bytes, for writer and
/// reader alike.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct StoredAnswer(String);

impl From<String> for StoredAnswer {
    fn from(answer: String) -> Self {
        if answer.len() <= STORED_ANSWER_BYTES {
            return Self(answer);
        }
        let mut cut = STORED_ANSWER_BYTES - NOTE_BYTES;
        while !answer.is_char_boundary(cut) {
            cut -= 1;
        }
        let left_out = answer.len() - cut;
        Self(format!(
            "{}\n[{left_out} more bytes of this answer were not stored]",
            &answer[..cut]
        ))
    }
}

impl From<StoredAnswer> for String {
    fn from(answer: StoredAnswer) -> Self {
        answer.0
    }
}

impl StoredAnswer {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What of an attachment is words; its id and type are the line's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentWords {
    /// `None` for a photo, which has no name.
    pub name: Option<String>,
    pub private: Option<suzatary_held::vault::PrivateCopy>,
}

/// Printed as canonical JSON: struct fields in declared order, maps sorted.
impl Words for LineWords {
    fn print(&self) -> Cow<'_, str> {
        serde_json::to_string(self)
            .expect("a slot's words are plain data")
            .into()
    }

    fn read(print: &str) -> Option<Self> {
        serde_json::from_str(print).ok()
    }
}

impl LineWords {
    /// The slot's prose; a call's arguments as their JSON.
    #[must_use]
    pub fn text(&self) -> std::borrow::Cow<'_, str> {
        match self {
            LineWords::Person { text, .. }
            | LineWords::Model { text }
            | LineWords::Act { text }
            | LineWords::Title { title: text }
            | LineWords::Why { why: text }
            | LineWords::Reason { reason: text }
            | LineWords::Dropped { reason: text } => text.as_str().into(),
            LineWords::Report { report } => report.text.as_str().into(),
            LineWords::Tool { answer, .. } => answer.as_str().into(),
            LineWords::Args { args } => serde_json::to_string(args).unwrap_or_default().into(),
        }
    }

    /// These words through the redaction door.
    ///
    /// # Errors
    /// For words whose scrubbed strings no longer make them.
    pub fn scrubbed(&self) -> Result<Self, suzatary_held::redact::Unscrubbable> {
        Scrubbed::of(self).map(Scrubbed::into_inner)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn line_words() -> impl Strategy<Value = LineWords> {
        let text = "(?s).{0,40}";
        prop_oneof![
            (
                text,
                proptest::collection::vec(proptest::option::of("[a-z.]{1,8}"), 0..3)
            )
                .prop_map(|(text, names)| LineWords::Person {
                    text,
                    attachments: names
                        .into_iter()
                        .map(|name| AttachmentWords {
                            name,
                            private: None
                        })
                        .collect(),
                }),
            text.prop_map(|text| LineWords::Model { text }),
            proptest::collection::btree_map("[a-z]{1,6}", "[ -~]{0,12}", 0..3).prop_map(|args| {
                LineWords::Args {
                    args: args
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect(),
                }
            }),
            (text, proptest::collection::vec("[a-z]{1,6}", 0..2)).prop_map(|(answer, names)| {
                LineWords::Tool {
                    texts: names.iter().map(|name| format!("text of {name}")).collect(),
                    answer: answer.into(),
                    names,
                }
            }),
            text.prop_map(|text| LineWords::Act { text }),
            text.prop_map(|title| LineWords::Title { title }),
            text.prop_map(|why| LineWords::Why { why }),
            text.prop_map(|reason| LineWords::Reason { reason }),
            text.prop_map(|reason| LineWords::Dropped { reason }),
        ]
    }

    proptest! {
        /// The text store's law: words read back as their print.
        #[test]
        fn words_read_back_as_their_print(words in line_words()) {
            prop_assert_eq!(LineWords::read(&words.print()), Some(words));
        }
    }

    /// An oversized stored answer opens bounded, because the reader builds the same type.
    #[test]
    fn an_oversized_stored_answer_opens_bounded() {
        let print = serde_json::json!({
            "slot": "tool",
            "answer": "x".repeat(3 * STORED_ANSWER_BYTES),
            "names": [],
            "texts": [],
        })
        .to_string();
        let Some(LineWords::Tool { answer, .. }) = LineWords::read(&print) else {
            panic!("a tool slot");
        };
        assert!(answer.as_str().len() <= STORED_ANSWER_BYTES);
        assert!(
            answer.as_str().ends_with(&format!(
                "[{} more bytes of this answer were not stored]",
                2 * STORED_ANSWER_BYTES + 128
            )),
            "{}",
            &answer.as_str()[answer.as_str().len() - 80..]
        );
    }
}
