//! `state.json`, the sole dedup authority and the one mail state the data repository pushes: each
//! decision sits under the name [`DedupKey::name`] gives its message, so the file says of no
//! message whether this box received it.
//!
//! The printed shape is stored data: key order (`version`, `seen`, the declaration order of the
//! derived `Serialize` structs), two-space indent, sorted names, a decision's `todo_id` only on a
//! todo, and a trailing newline.

use std::collections::BTreeMap;

use prodrome_core::event::TodoId;
use serde::{Deserialize, Serialize};

use suzatary_mail::mail::Disposed;
use suzatary_mail::mail::dedup_key::{DedupKey, Keyed};

pub(crate) const SCHEMA_VERSION: u64 = 4;

/// A closed set, so a fourth spelling is a parse failure rather than a badge nobody renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Todo,
    /// Automated, marketing or bot mail to ignore wholesale.
    Noise,
    /// Seen, not actionable, not noise: a one-off notice.
    Skip,
}

impl Disposition {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Disposition::Todo => "todo",
            Disposition::Noise => "noise",
            Disposition::Skip => "skip",
        }
    }

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

/// A todo names the todo it became, and nothing else names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub enum Decision {
    Todo {
        /// As `todo_add` named it.
        todo_id: TodoId,
    },
    Noise,
    Skip,
}

impl From<&Decision> for Disposed {
    fn from(decision: &Decision) -> Self {
        match decision {
            Decision::Todo { todo_id } => Self::Todo {
                todo: todo_id.as_str().to_owned(),
            },
            Decision::Noise => Self::Noise,
            Decision::Skip => Self::Skip,
        }
    }
}

impl Decision {
    pub fn parse(disposition: &str, todo_id: Option<&str>) -> Result<Self, StateError> {
        Self::of(Disposition::parse(disposition)?, todo_id.map(str::to_owned))
    }

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

    #[must_use]
    pub fn disposition(&self) -> Disposition {
        match self {
            Decision::Todo { .. } => Disposition::Todo,
            Decision::Noise => Disposition::Noise,
            Decision::Skip => Disposition::Skip,
        }
    }

    #[must_use]
    pub fn todo_id(&self) -> Option<&TodoId> {
        match self {
            Decision::Todo { todo_id } => Some(todo_id),
            Decision::Noise | Decision::Skip => None,
        }
    }
}

/// Field order is the stored key order.
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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StateError {
    #[error("state.json is not JSON of its shape: {0}")]
    Unparseable(String),
    #[error("{0:?} is not a disposition (todo, noise, skip)")]
    Disposition(String),
    #[error("a todo decision names the todo it became: pass todo_id")]
    NoTodo,
    #[error("{0:?} is not a todo id (a-z, 0-9, _ and -)")]
    TodoId(String),
    #[error("a {disposition} decision names no todo, and {todo_id:?} was given")]
    NotATodo {
        disposition: Disposition,
        todo_id: String,
    },
    #[error("state.json is version {0}, and this program reads version {SCHEMA_VERSION}")]
    Version(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriageState {
    /// By keyed name, never by hash.
    pub seen: BTreeMap<Keyed, Decision>,
}

/// In the order it is written.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u64,
    seen: BTreeMap<Keyed, Decision>,
}

impl TriageState {
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

    /// What an absent file reads as, not a failure.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Trailing newline included.
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

    #[must_use]
    pub fn mark(mut self, key: &DedupKey, hash: &str, decision: Decision) -> Self {
        self.seen.insert(key.name(hash), decision);
        self
    }

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

    /// The file's text is the contract, byte for byte.
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

    /// No version is refused rather than read as empty.
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
        /// The pushed file holds no message's hash and no sender.
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

    /// A row with no date prints it as `""`.
    #[test]
    fn a_row_prints_its_disposition_as_a_tagged_value() {
        let row = suzatary_mail::mail::MailRow {
            hash: "abc".to_owned(),
            id: "id@x".to_owned(),
            sender: "her@example.test".to_owned(),
            subject: "hello".to_owned(),
            date: None,
            container: "inbox".to_owned(),
            account: "me@example.test".to_owned(),
            disposition: Disposed::Undecided,
            held: suzatary_held::held_value::Kinds::empty(),
        };
        assert_eq!(
            serde_json::to_value(&row).expect("serialises"),
            serde_json::json!({"hash": "abc", "id": "id@x", "sender": "her@example.test",
                "subject": "hello", "date": "", "container": "inbox",
                "account": "me@example.test", "disposition": {"kind": "undecided"},
                "held": []})
        );
        let decided = |decision: &Decision| {
            serde_json::to_value(suzatary_mail::mail::MailRow {
                date: Some(
                    suzatary_base::capability::Datetime::new(2026, 9, 1, 8, 30, 0, 0)
                        .expect("a real instant"),
                ),
                disposition: Disposed::from(decision),
                ..row.clone()
            })
            .expect("serialises")
        };
        let json = decided(&Decision::Noise);
        assert_eq!(json["date"], "2026-09-01T08:30:00");
        for printed in [
            serde_json::to_value(&row).expect("serialises"),
            json.clone(),
        ] {
            let read: suzatary_mail::mail::MailRow =
                serde_json::from_value(printed.clone()).expect("reads back");
            assert_eq!(serde_json::to_value(read).expect("serialises"), printed);
        }
        assert_eq!(json["disposition"], serde_json::json!({"kind": "noise"}));
        let todo = Decision::parse("todo", Some("renewcard")).expect("a decision");
        assert_eq!(
            decided(&todo)["disposition"],
            serde_json::json!({"kind": "todo", "todo": "renewcard"})
        );
    }
}
