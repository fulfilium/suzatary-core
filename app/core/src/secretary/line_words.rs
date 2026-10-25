//! The words of one slot of a conversation line: what is locked under one key.
//!
//! A line holds at most a few slots ([`crate::secretary::line_shape::LineShape::slots`]): a
//! turn's words, or a model turn's text and then each call's arguments, each under a key of its
//! own, so a forgetting can erase the one call that wrote a fact and leave the prose beside it.
//! Every slot's words go through the redaction door ([`Scrubbed`]) before they are locked
//! (`line_event::seal`), and the line records the rules they passed. Reading a print only parses
//! it; the line scrubs again a slot whose rules are not the current ones as it opens.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::redact::Scrubbed;
use crate::secretary::salted_text::Words;

/// The words of one slot, by what the slot is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "lowercase")]
pub enum LineWords {
    /// A person's turn: their words and each attachment's, in order.
    Person {
        /// The words.
        text: String,
        /// Each attachment's words.
        attachments: Vec<AttachmentWords>,
    },
    /// A model turn's prose.
    Model {
        /// The words.
        text: String,
    },
    /// One call's arguments.
    Args {
        /// The arguments, as the model passed them.
        args: BTreeMap<String, serde_json::Value>,
    },
    /// A verb's answer, and each file it handed over: its name and its text layer.
    Tool {
        /// The answer, bounded.
        answer: StoredAnswer,
        /// Each file's name, in order.
        names: Vec<String>,
        /// Each file's text layer, in order.
        texts: Vec<String>,
    },
    /// An act's sentence.
    Act {
        /// The sentence.
        text: String,
    },
    /// A conversation's name.
    Title {
        /// The name.
        title: String,
    },
    /// A review's reason.
    Why {
        /// The reason.
        why: String,
    },
    /// Why a review could not run.
    Reason {
        /// The reason.
        reason: String,
    },
    /// A coding session's report: its file, its text and the branch's diff stat.
    Report {
        /// The report.
        report: crate::coding::Report,
    },
    /// Why the owner let a coding session go.
    Dropped {
        /// His words.
        reason: String,
    },
}

/// The most bytes of a verb's answer one line stores: 64 KiB. Ordinary answers fit whole (a
/// mail body as most mail prints, a page of a file, a list of todos); an answer past it is a
/// sweep that printed its whole population, which no later round reads usefully and which a
/// phone must pull with the rest of the conversations. The model reads the whole answer in the
/// round that asked for it; later rounds read the stored head and can call the verb again.
pub const STORED_ANSWER_BYTES: usize = 64 * 1024;

/// Room kept at the end of a cut answer for the sentence saying how much was left out.
const NOTE_BYTES: usize = 128;

/// A verb's answer as a line stores it: at most [`STORED_ANSWER_BYTES`] bytes. Built only from
/// a `String`, by writer and reader alike, so no stored or opened tool answer is longer,
/// whatever the verb returned. A longer answer keeps its head and ends in one line saying how
/// many bytes were left out.
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
    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What of an attachment is words: the file's name and, for a private file, where it is kept and
/// its redacted text. Its id and type are the line's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentWords {
    /// The name it was picked under; `None` for a photo, which has none.
    pub name: Option<String>,
    /// Where a private file is kept, and its redacted text.
    pub private: Option<crate::vault::PrivateCopy>,
}

/// Printed as canonical JSON (struct fields in declared order, maps sorted), and read back as
/// parsed, so `read(print(w)) == Some(w)` for every `w`.
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
    /// The slot's prose: what was said, answered or done, a name or a reason; a call's arguments
    /// as their JSON.
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
    /// [`crate::redact::Unscrubbable`] for words whose scrubbed strings no longer make them.
    pub fn scrubbed(&self) -> Result<Self, crate::redact::Unscrubbable> {
        Scrubbed::of(self).map(Scrubbed::into_inner)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    /// Any slot's words, of the shapes a line holds.
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

    /// An answer of `len` bytes or a little more, of one-, two-, three- and four-byte characters,
    /// so a cut can land inside a character.
    fn answer_of(len: usize) -> String {
        "aé€😀"
            .chars()
            .cycle()
            .scan(0, |bytes, c| {
                (*bytes < len).then(|| {
                    *bytes += c.len_utf8();
                    c
                })
            })
            .collect()
    }

    proptest! {
        /// No stored tool turn exceeds the cap, whatever the verb answered; an answer within it
        /// is stored whole, and a longer one keeps its head and says how much it left out.
        #[test]
        fn a_stored_tool_answer_is_bounded(len in prop_oneof![
            0..STORED_ANSWER_BYTES + 1,
            STORED_ANSWER_BYTES - 8..STORED_ANSWER_BYTES + 8,
            STORED_ANSWER_BYTES..4 * STORED_ANSWER_BYTES,
        ]) {
            let answer = answer_of(len);
            let turn = crate::capability::Turn::Tool {
                verb: crate::secretary::tool_name::ToolName::from("junk_digest"),
                answer: answer.clone(),
                outcome: crate::secretary::outcome::Outcome::Answered,
                files: Vec::new(),
            };
            let (_, words) = crate::secretary::turn_shape::split(&turn);
            let words = words.expect("a whole turn has words");
            let [LineWords::Tool { answer: stored, .. }] = words.as_slice() else {
                panic!("a tool turn is one tool slot: {words:?}");
            };
            prop_assert!(stored.as_str().len() <= STORED_ANSWER_BYTES);
            if answer.len() <= STORED_ANSWER_BYTES {
                prop_assert_eq!(stored.as_str(), answer.as_str());
            } else {
                let (head, note) = stored.as_str().rsplit_once('\n').expect("a note");
                prop_assert!(answer.starts_with(head));
                prop_assert_eq!(
                    note,
                    format!("[{} more bytes of this answer were not stored]", answer.len() - head.len())
                );
            }
            let opened = LineWords::read(&words[0].print()).expect("reads back");
            prop_assert_eq!(&opened, &words[0]);
        }
    }

    /// A line stored before the bound opens bounded: the reader builds the same type.
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
