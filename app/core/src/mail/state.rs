//! `state.json`, the sole dedup authority and the one mail state the data repository pushes:
//! what triage decided about each message, under the name [`DedupKey::name`] gives it, so the
//! file says how many messages were decided and how, and of no message whether this box
//! received it. What else the box knows of a message (where it lives, the agent's note, the
//! senders it ignores) is [`crate::mail::local_state`], which no push carries.
//!
//! The printed shape is stored data: key order (`version`, `seen`), two-space indent, sorted
//! names, a decision's `todo_id` only on a todo, trailing newline. Key order comes from the
//! declaration order of derived `Serialize` structs.
//!
//! Updates are pure: each returns a new state for the caller to write.

use std::collections::BTreeMap;

use prodrome_core::event::TodoId;
use serde::{Deserialize, Serialize};

use crate::mail::dedup_key::{DedupKey, Keyed};

/// The version this program reads and writes.
pub(crate) const SCHEMA_VERSION: u64 = 4;

/// The word a decision is stored and answered under; a closed set, so a fourth spelling is a
/// parse failure rather than a badge nobody renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    /// Actionable: a todo was authored.
    Todo,
    /// Automated, marketing or bot mail to ignore wholesale.
    Noise,
    /// Seen, not actionable, not noise — a one-off notice.
    Skip,
}

impl Disposition {
    /// Its wire spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Disposition::Todo => "todo",
            Disposition::Noise => "noise",
            Disposition::Skip => "skip",
        }
    }

    /// Read one, or the reason this text is not one.
    pub fn parse(text: &str) -> Result<Self, StateError> {
        match text {
            "todo" => Ok(Disposition::Todo),
            "noise" => Ok(Disposition::Noise),
            "skip" => Ok(Disposition::Skip),
            other => Err(StateError::Disposition(other.to_owned())),
        }
    }
}

impl std::fmt::Display for Disposition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The recorded decision about a message: a todo names the todo it became, and nothing else
/// names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub enum Decision {
    /// Actionable: this todo was authored for it.
    Todo {
        /// The todo, as `todo_add` named it.
        todo_id: TodoId,
    },
    /// Automated, marketing or bot mail.
    Noise,
    /// Seen, and neither.
    Skip,
}

impl Decision {
    /// Parse a decision as a verb is told it: a disposition word and, for `todo` alone, the
    /// todo it became.
    ///
    /// # Errors
    /// A word that is not a disposition, a todo with no id or an id that is not one, and an
    /// id beside a disposition that names none.
    pub fn parse(disposition: &str, todo_id: Option<&str>) -> Result<Self, StateError> {
        Self::of(Disposition::parse(disposition)?, todo_id.map(str::to_owned))
    }

    /// The decision a disposition and an optional todo make, or why they make none.
    ///
    /// # Errors
    /// As [`Decision::parse`], past the word.
    pub fn of(disposition: Disposition, todo_id: Option<String>) -> Result<Self, StateError> {
        match (disposition, todo_id) {
            (Disposition::Todo, Some(id)) => TodoId::new(id.clone())
                .map(|todo_id| Decision::Todo { todo_id })
                .map_err(|_| StateError::TodoId(id)),
            (Disposition::Todo, None) => Err(StateError::NoTodo),
            (Disposition::Noise, None) => Ok(Decision::Noise),
            (Disposition::Skip, None) => Ok(Decision::Skip),
            (other @ (Disposition::Noise | Disposition::Skip), Some(id)) => {
                Err(StateError::NotATodo {
                    disposition: other,
                    todo_id: id,
                })
            }
        }
    }

    /// Its word.
    #[must_use]
    pub fn disposition(&self) -> Disposition {
        match self {
            Decision::Todo { .. } => Disposition::Todo,
            Decision::Noise => Disposition::Noise,
            Decision::Skip => Disposition::Skip,
        }
    }

    /// The todo it became, `None` for noise and skip.
    #[must_use]
    pub fn todo_id(&self) -> Option<&TodoId> {
        match self {
            Decision::Todo { todo_id } => Some(todo_id),
            Decision::Noise | Decision::Skip => None,
        }
    }
}

/// A decision as the file spells it; field order is the stored key order.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    disposition: Disposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    todo_id: Option<String>,
}

impl TryFrom<Stored> for Decision {
    type Error = StateError;
    fn try_from(stored: Stored) -> Result<Self, StateError> {
        Decision::of(stored.disposition, stored.todo_id)
    }
}

impl From<Decision> for Stored {
    fn from(decision: Decision) -> Self {
        Stored {
            disposition: decision.disposition(),
            todo_id: decision.todo_id().map(|id| id.as_str().to_owned()),
        }
    }
}

/// `state.json` could not be read, or a value in it is not one.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StateError {
    /// The file is not JSON at all, or not this shape.
    #[error("state.json is not JSON of its shape: {0}")]
    Unparseable(String),
    /// A disposition this program does not know.
    #[error("{0:?} is not a disposition (todo, noise, skip)")]
    Disposition(String),
    /// A `todo` decision that names no todo.
    #[error("a todo decision names the todo it became: pass todo_id")]
    NoTodo,
    /// A todo id that is not one.
    #[error("{0:?} is not a todo id (a-z, 0-9, _ and -)")]
    TodoId(String),
    /// A todo named beside a disposition that is not `todo`.
    #[error("a {disposition} decision names no todo, and {todo_id:?} was given")]
    NotATodo {
        /// The disposition.
        disposition: Disposition,
        /// The id given beside it.
        todo_id: String,
    },
    /// Another version.
    #[error("state.json is version {0}, and this program reads version {SCHEMA_VERSION}")]
    Version(u64),
}

/// The dedup authority as it is pushed: decisions under keyed names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriageState {
    /// What was decided about each message, by its keyed name.
    pub seen: BTreeMap<Keyed, Decision>,
}

/// The file's shape, in the order it is written.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u64,
    seen: BTreeMap<Keyed, Decision>,
}

impl TriageState {
    /// Read a state file of this version.
    ///
    /// # Errors
    /// [`StateError::Unparseable`] for text that is not the document's shape, and
    /// [`StateError::Version`] for a document of any other version.
    pub fn parse(text: &str) -> Result<Self, StateError> {
        let document: Document =
            serde_json::from_str(text).map_err(|e| StateError::Unparseable(e.to_string()))?;
        if document.version != SCHEMA_VERSION {
            return Err(StateError::Version(document.version));
        }
        Ok(Self {
            seen: document.seen,
        })
    }

    /// What an absent file reads as: nothing triaged yet, not a failure.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The file's exact text, trailing newline included.
    #[must_use]
    #[expect(
        clippy::missing_panics_doc,
        reason = "unreachable: every field is a string, a u64 or a map with string keys"
    )]
    pub fn print(&self) -> String {
        let document = Document {
            version: SCHEMA_VERSION,
            seen: self.seen.clone(),
        };
        let mut text =
            serde_json::to_string_pretty(&document).expect("a state document always serialises");
        text.push('\n');
        text
    }

    /// Record a decision about the message whose canonical hash is `hash`.
    #[must_use]
    pub fn mark(mut self, key: &DedupKey, hash: &str, decision: Decision) -> Self {
        self.seen.insert(key.name(hash), decision);
        self
    }

    /// The decision recorded for the message whose canonical hash is `hash`.
    #[must_use]
    pub fn decided(&self, key: &DedupKey, hash: &str) -> Option<&Decision> {
        self.seen.get(&key.name(hash))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use suzatary_base::capability::Secret;

    fn key() -> DedupKey {
        DedupKey::of(&Secret::new("ab".repeat(32))).expect("a key")
    }

    fn todo(id: &str) -> Decision {
        Decision::parse("todo", Some(id)).expect("a todo decision")
    }

    /// The file's text is the contract: key order, indent, sorting, omission, newline; a
    /// todo's id is printed beside it and nowhere else.
    #[test]
    fn the_printed_document_is_its_shape_byte_for_byte() {
        let state = TriageState::empty()
            .mark(&key(), "bbbb", todo("renewinsurance"))
            .mark(&key(), "aaaa", Decision::Skip);
        let (a, b) = (key().name("aaaa"), key().name("bbbb"));
        let (first, second) = if a < b { (a, b) } else { (b, a) };
        let entry = |name: &Keyed| {
            if *name == key().name("bbbb") {
                format!(
                    "    \"{}\": {{\n      \"disposition\": \"todo\",\n      \"todo_id\": \"renewinsurance\"\n    }}",
                    name.as_str()
                )
            } else {
                format!(
                    "    \"{}\": {{\n      \"disposition\": \"skip\"\n    }}",
                    name.as_str()
                )
            }
        };
        assert_eq!(
            state.print(),
            format!(
                "{{\n  \"version\": 4,\n  \"seen\": {{\n{},\n{}\n  }}\n}}\n",
                entry(&first),
                entry(&second)
            )
        );
    }

    #[test]
    fn an_empty_state_prints_the_empty_document() {
        assert_eq!(
            TriageState::empty().print(),
            "{\n  \"version\": 4,\n  \"seen\": {}\n}\n"
        );
    }

    /// Another version, or no version, is refused rather than read as empty.
    #[test]
    fn another_version_is_refused() {
        assert_eq!(
            TriageState::parse(r#"{"version": 3, "seen": {}}"#),
            Err(StateError::Version(3))
        );
        assert!(matches!(
            TriageState::parse(r#"{"seen": {}}"#),
            Err(StateError::Unparseable(_))
        ));
    }

    /// The pushed form holds nothing box-local and no watermark.
    #[test]
    fn the_pushed_form_refuses_what_is_box_local_and_the_watermark() {
        let named = key().name("abc123");
        for field in ["note", "native_id", "last_thread_hash"] {
            let text = format!(
                r#"{{"version": 4, "seen": {{"{}": {{"disposition": "skip", "{field}": "x"}}}}}}"#,
                named.as_str()
            );
            assert!(TriageState::parse(&text).is_err(), "{field}");
        }
        for extra in [r#""noise_senders": [], "#, r#""last_triaged": null, "#] {
            let text = format!(r#"{{"version": 4, {extra}"seen": {{}}}}"#);
            assert!(TriageState::parse(&text).is_err(), "{extra}");
        }
        assert!(
            TriageState::parse(r#"{"version": 4, "seen": {"abc123": {"disposition": "skip"}}}"#)
                .is_err(),
            "a bare hash is not a keyed name"
        );
    }

    #[test]
    fn an_unknown_disposition_refuses_the_file() {
        let text = format!(
            r#"{{"version": 4, "seen": {{"{}": {{"disposition": "maybe"}}}}}}"#,
            key().name("a").as_str()
        );
        assert!(TriageState::parse(&text).is_err());
        assert_eq!(Disposition::parse("skip"), Ok(Disposition::Skip));
        assert!(Disposition::parse("Skip").is_err());
    }

    /// A todo names its todo, and nothing else names one, whether a verb is told it or the
    /// file holds it.
    #[test]
    fn a_todo_names_its_todo_and_nothing_else_names_one() {
        assert_eq!(Decision::parse("todo", None), Err(StateError::NoTodo));
        assert_eq!(
            Decision::parse("todo", Some("Not An Id")),
            Err(StateError::TodoId("Not An Id".to_owned()))
        );
        assert_eq!(
            Decision::parse("skip", Some("renewcard")),
            Err(StateError::NotATodo {
                disposition: Disposition::Skip,
                todo_id: "renewcard".to_owned()
            })
        );
        assert_eq!(Decision::parse("noise", None), Ok(Decision::Noise));
        assert_eq!(
            todo("renewcard").todo_id().map(TodoId::as_str),
            Some("renewcard")
        );
        assert_eq!(todo("renewcard").disposition(), Disposition::Todo);
        let named = key().name("a");
        for entry in [
            r#"{"disposition": "todo"}"#,
            r#"{"disposition": "noise", "todo_id": "renewcard"}"#,
            r#"{"disposition": "todo", "todo_id": ""}"#,
        ] {
            let text = format!(
                r#"{{"version": 4, "seen": {{"{}": {entry}}}}}"#,
                named.as_str()
            );
            assert!(TriageState::parse(&text).is_err(), "{entry}");
        }
    }

    fn a_decision() -> impl Strategy<Value = Decision> {
        prop_oneof![
            "[a-z0-9_-]{1,12}".prop_map(|id| todo(&id)),
            Just(Decision::Noise),
            Just(Decision::Skip),
        ]
    }

    proptest! {
        /// Whatever was decided about whichever messages, the pushed file holds no message's
        /// hash, no todo note and no sender, and reads back as itself.
        #[test]
        fn the_pushed_form_holds_no_hash_and_round_trips(
            marks in prop::collection::vec(("[0-9a-f]{64}", a_decision()), 0..20),
        ) {
            let state = marks.iter().fold(TriageState::empty(), |state, (hash, decision)| {
                state.mark(&key(), hash, decision.clone())
            });
            let printed = state.print();
            for (hash, _) in &marks {
                prop_assert!(!printed.contains(hash.as_str()));
                prop_assert!(!printed.contains(&hash[..12]));
                prop_assert!(state.decided(&key(), hash).is_some());
            }
            prop_assert!(!printed.contains('@'));
            prop_assert_eq!(TriageState::parse(&printed).expect("parses"), state);
        }
    }
}
