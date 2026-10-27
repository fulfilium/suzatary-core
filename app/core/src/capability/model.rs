//! The model as a capability: a conversation in, a turn out.
//!
//! A [`Reply`] carries what to say and the calls it wants as values, so the authority stays
//! on this side; the model is the least trusted author in the application.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::reference::Reference;
use crate::secretary::tool_name::ToolName;

/// Who said a turn: a person, the model, a verb this side ran, or an act the person took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "who", rename_all = "lowercase")]
pub enum Turn {
    /// What the owner typed.
    Person {
        /// His words.
        text: String,
        /// What he pointed at while typing them; `default` so older transcripts still read.
        #[serde(default)]
        references: Vec<Reference>,
        /// The photos he attached; absent on transcripts written before photos existed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<crate::attachment::Attachment>,
    },
    /// What the model answered: its words, and the calls it wants made.
    Model {
        /// Its words; empty when it only wanted a tool run.
        text: String,
        /// The calls, in the order it asked for them.
        calls: Vec<ToolCall>,
    },
    /// What a verb answered, one per call, in the order they were made.
    Tool {
        /// The verb's name, echoed so a transcript reads without the call beside it.
        verb: ToolName,
        /// What the verb answered, as text.
        answer: String,
        /// How the call ended: answered, refused (still a turn the model must see), or a fact
        /// remembered.
        outcome: crate::secretary::outcome::Outcome,
        /// Files the answer hands the model as themselves, named by their digest; absent on
        /// transcripts written before a verb could.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<crate::enclosed::Enclosed>,
    },
    /// What the owner did rather than said, such as dismissing a proposal.
    ///
    /// Not a `Person` turn, because the answering state is read off the last turn and a person's
    /// would leave a page waiting on a model nobody started.
    Act {
        /// What he did, in the box's words, never the model's.
        text: String,
        /// What the act was about.
        #[serde(default)]
        references: Vec<Reference>,
    },
    /// A stored turn some of whose words are gone: its shape, which every reading of what the
    /// turn IS reads, and each slot of its words as it read.
    Erased {
        /// Who said it and what it named.
        shape: crate::secretary::turn_shape::TurnShape,
        /// Each slot, kept or gone.
        words: Vec<crate::secretary::words::Words<crate::secretary::line_words::LineWords>>,
    },
}

/// One tool call the model wants made: a name resolved by the dispatcher, and arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The verb's name, read once where the model said it.
    pub verb: ToolName,
    /// Its arguments.
    pub args: BTreeMap<String, serde_json::Value>,
}

/// A whole conversation, handed to the model every time, with no session handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    /// The rules the secretary works under.
    pub system: String,
    /// Every turn so far, oldest first.
    pub turns: Vec<Turn>,
}

/// What one turn of the model answered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reply {
    /// What to say to the owner; empty when the model only wants a call.
    pub say: String,
    /// The calls it wants made, in order.
    pub calls: Vec<ToolCall>,
    /// What the model said this turn cost, attached by the interpreter; `None` when unreported.
    pub usage: Option<TokenUsage>,
}

/// Token counts for one turn; `cached_prompt_tokens` is `None` when the model does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens the prompt cost.
    pub prompt_tokens: u64,
    /// Tokens the reply cost.
    pub completion_tokens: u64,
    /// Prompt tokens served from a cache, when the model reports the figure.
    pub cached_prompt_tokens: Option<u64>,
}

/// The envelope the model answers in: `{"say": "...", "calls": [{"verb": "...", "args": {...}}]}`.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    say: String,
    #[serde(default)]
    calls: Vec<RawCall>,
}

#[derive(Debug, Deserialize)]
struct RawCall {
    verb: String,
    #[serde(default)]
    args: BTreeMap<String, serde_json::Value>,
}

impl Reply {
    /// Read the envelope out of whatever the model said, unwrapping a fenced block first.
    ///
    /// Total by design: text that is not the envelope becomes `say` with no calls.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let candidate = unfence(text.trim());
        match serde_json::from_str::<Envelope>(candidate) {
            Ok(envelope) => Self {
                say: envelope.say,
                calls: envelope
                    .calls
                    .into_iter()
                    .map(|call| ToolCall {
                        verb: call.verb.into(),
                        args: call.args,
                    })
                    .collect(),
                usage: None,
            },
            Err(_) => Self {
                say: text.trim().to_owned(),
                calls: Vec::new(),
                usage: None,
            },
        }
    }

    /// Attach usage read from the transport, since it never lives in the reply text.
    #[must_use]
    pub fn with_usage(mut self, usage: Option<TokenUsage>) -> Self {
        self.usage = usage;
        self
    }
}

/// The inside of a fenced code block, or the text unchanged.
fn unfence(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("```") else {
        return text;
    };
    let Some((_, body)) = rest.split_once('\n') else {
        return text;
    };
    body.trim_end()
        .strip_suffix("```")
        .map_or(text, str::trim_end)
}

/// The model could not be asked, or did not answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// The interpreter could not reach the model at all.
    #[error("the model could not be reached: {0}")]
    Unreachable(String),
    /// It ran and failed: a non-zero exit or a rejected request.
    #[error("the model refused or failed: {0}")]
    Failed(String),
    /// It refused the credential it was run with, in the interpreter's words: the one failure a
    /// new credential can mend. A rate limit, a timeout or an outage is never this.
    #[error("the model refused its credential: {0}")]
    Refused(String),
    /// It did not answer within the interpreter's time limit, and was ended.
    #[error("the model did not answer within {0:?}")]
    TimedOut(std::time::Duration),
    /// What it was sent is longer than it reads, in the interpreter's own words.
    #[error("what the model was sent is longer than it reads: {0}")]
    TooLong(String),
    /// A [`SessionModel::resume`] named a session the CLI no longer knows.
    #[error("the session is gone: {0}")]
    Lost(String),
    /// A [`Halt`] was pulled while it ran, and the process was ended.
    #[error("the answer was stopped")]
    Halted,
}

/// A round's stop, set once by the actor who pulled it so the round records a person.
#[derive(Debug, Clone, Default)]
pub struct Halt(std::sync::Arc<std::sync::OnceLock<prodrome_core::event::Actor>>);

impl Halt {
    /// A halt nobody has pulled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pull it, as `by`; only the first pull counts.
    pub fn stop(&self, by: prodrome_core::event::Actor) {
        let _first = self.0.set(by);
    }

    /// Who pulled it, if anyone has.
    #[must_use]
    pub fn by(&self) -> Option<&prodrome_core::event::Actor> {
        self.0.get()
    }

    /// Whether it has been pulled.
    #[must_use]
    pub fn halted(&self) -> bool {
        self.0.get().is_some()
    }
}

/// The halt of the round asking a model on this thread, for the e2e fixture's scripted model,
/// which is handed no halt: a round it keeps waiting ends once its own round's halt is pulled,
/// whether before the model was asked or while it waits, so a scene's Stop decides when the
/// round answers and nothing races it.
#[cfg(feature = "fixture")]
pub mod pulled {
    use std::cell::RefCell;

    use super::Halt;

    thread_local! {
        static ASKING: RefCell<Option<Halt>> = const { RefCell::new(None) };
    }

    /// While it lives, `halt` is the one this thread's model is asked under.
    #[derive(Debug)]
    pub struct Asking(());

    impl Asking {
        /// Name `halt` as this thread's until the guard drops.
        #[must_use]
        pub fn of(halt: &Halt) -> Self {
            ASKING.with(|asking| *asking.borrow_mut() = Some(halt.clone()));
            Self(())
        }
    }

    impl Drop for Asking {
        fn drop(&mut self) {
            ASKING.with(|asking| *asking.borrow_mut() = None);
        }
    }

    /// Whether the halt of the round asking on this thread has been pulled; `false` outside a
    /// round.
    #[must_use]
    pub fn halted() -> bool {
        ASKING.with(|asking| asking.borrow().as_ref().is_some_and(Halt::halted))
    }
}

prodrome_core::text_literal!(
    SessionId => |id| id.as_str().to_owned(),
);

/// A model session's id, parsed as a UUID so a bad one fails here rather than in the interpreter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId(String);

/// Text that is not a UUID.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a session id is a UUID, nothing else: {0:?}")]
pub struct NotAUuid(pub String);

impl SessionId {
    /// Parse one.
    ///
    /// # Errors
    /// [`NotAUuid`] for anything but 36 lowercase hex characters hyphenated at 8, 13, 18 and 23.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAUuid> {
        let text = text.into();
        if is_uuid(&text) {
            Ok(Self(text))
        } else {
            Err(NotAUuid(text))
        }
    }

    /// Mint one from `seed` by hashing, since `suzatary-core` holds no source of randomness.
    #[must_use]
    pub fn minted(seed: &str) -> Self {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(seed.as_bytes());
        let hex = hex::encode(&digest[..16]);
        let minted = format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        );
        debug_assert!(is_uuid(&minted), "a minted id is a UUID by construction");
        Self(minted)
    }

    /// The id, as the interpreter is handed it and answers it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SessionId {
    type Error = NotAUuid;
    fn try_from(text: String) -> Result<Self, NotAUuid> {
        Self::new(text)
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> String {
        id.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// RFC 4122's printed shape only; the version and variant bits are never read, so not checked.
fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit() && !c.is_ascii_uppercase()
            }
        })
}

/// A model session as the application holds one: its id, model and directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The id `--session-id` minted it under.
    pub id: SessionId,
    /// The model it was opened with.
    pub model: String,
    /// The directory it was opened in.
    pub cwd: String,
    /// When it was minted.
    pub created: String,
    /// When this store last heard of it.
    pub last: String,
    /// The digest of the manual and verb list it was opened with; a mismatch or `None` reopens it.
    pub surface: Option<String>,
}

/// What one turn of a sessioned model answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sessioned {
    /// The reply, read by the same [`Reply::parse`] as every model's answer.
    pub reply: Reply,
    /// The session it came back on, which is always the one asked for.
    pub session: Session,
}

/// What a resumed session is sent: the turns it has not read, never none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta(Vec<Turn>);

impl Delta {
    /// `first`, then `rest`, in that order.
    #[must_use]
    pub fn new(first: Turn, rest: impl IntoIterator<Item = Turn>) -> Self {
        Self(std::iter::once(first).chain(rest).collect())
    }

    /// The turns, oldest first; never empty.
    #[must_use]
    pub fn turns(&self) -> &[Turn] {
        &self.0
    }
}

/// The model as a conversation it remembers.
///
/// A second trait rather than a method on [`Model`], so a sessionless model fakes nothing.
pub trait SessionModel: Model {
    /// Start a session under a caller-minted id, so the first turn is already resumable.
    ///
    /// # Errors
    /// [`ModelError`] as for [`Model::turn`], and [`ModelError::Halted`] when `halt` is pulled.
    fn open(
        &self,
        id: &SessionId,
        conversation: &Conversation,
        tools: &serde_json::Value,
        halt: &Halt,
    ) -> Result<Sessioned, ModelError>;

    /// Continue a session with only the turns it has not read, streaming text to `on_delta`.
    ///
    /// # Errors
    /// [`ModelError::Lost`] when the CLI no longer knows the session, else as for `open`.
    fn resume(
        &self,
        session: &Session,
        delta: &Delta,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Sessioned, ModelError>;
}

/// The model, as the application asks it.
pub trait Model: Sync {
    /// Which model this is, stored on the conversation and never matched on.
    fn name(&self) -> String;

    /// Ask for one turn; `tools` is the closed verb list the prompt names.
    ///
    /// # Errors
    /// [`ModelError`] when the model could not be reached; unparseable text is not an error.
    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError>;
}

/// A model lent: the same model, asked through a reference.
impl<M: Model + ?Sized> Model for &M {
    fn name(&self) -> String {
        (**self).name()
    }

    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        (**self).turn(conversation, tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call with no `verb` fails the whole envelope, so the text stands as prose.
    #[test]
    fn a_call_without_a_verb_is_not_half_read() {
        let reply = Reply::parse(r#"{"say": "x", "calls": [{"args": {}}]}"#);
        assert!(reply.calls.is_empty());
        assert!(reply.say.contains("calls"));
    }

    /// Two different seeds mint two different valid ids.
    #[test]
    fn a_minted_session_id_is_valid_and_seed_determined() {
        let a = SessionId::minted("2026-09-09-1/0");
        assert!(SessionId::new(a.as_str().to_owned()).is_ok());
        assert_eq!(a, SessionId::minted("2026-09-09-1/0"));
        assert_ne!(a, SessionId::minted("2026-09-09-1/1"));
    }

    #[test]
    fn a_turn_round_trips_through_its_stored_shape() {
        let turn = Turn::Model {
            text: "asking".to_owned(),
            calls: vec![ToolCall {
                verb: "list_open_todos".into(),
                args: BTreeMap::new(),
            }],
        };
        let printed = serde_json::to_string(&turn).expect("a turn serialises");
        assert!(printed.contains(r#""who":"model""#));
        let read: Turn = serde_json::from_str(&printed).expect("and reads back");
        assert_eq!(read, turn);
    }
}
