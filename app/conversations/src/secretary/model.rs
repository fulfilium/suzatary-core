//! The model as a capability: a conversation in, a turn out, its calls as values so the
//! authority stays on this side; the model is the least trusted author in the application.

pub mod laws;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::secretary::tool_name::ToolName;
use suzatary_base::call_log::TokenUsage;
use suzatary_base::reference::Reference;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "who", rename_all = "lowercase")]
pub enum Turn {
    Person {
        text: String,
        #[serde(default)]
        references: Vec<Reference>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<suzatary_held::attachment::Attachment>,
    },
    Model {
        /// Its words; empty when it only wanted a tool run.
        text: String,
        calls: Vec<ToolCall>,
    },
    /// What a verb answered, one per call, in the order they were made.
    Tool {
        verb: ToolName,
        answer: String,
        /// How the call ended; a refusal is still a turn the model must see.
        outcome: crate::secretary::outcome::Outcome,
        /// Files the answer hands the model as themselves, named by their digest.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<suzatary_held::enclosed::Enclosed>,
    },
    /// What the owner did rather than said, such as dismissing a proposal. Not a `Person` turn,
    /// since a person's would leave a page waiting on a model nobody started.
    Act {
        /// What he did, in the box's words, never the model's.
        text: String,
        #[serde(default)]
        references: Vec<Reference>,
    },
    /// A stored turn some of whose words are gone.
    Erased {
        /// Who said it and what it named.
        shape: crate::secretary::turn_shape::TurnShape,
        words: Vec<
            suzatary_memory::secretary::words::Words<
                suzatary_memory::secretary::line_words::LineWords,
            >,
        >,
    },
}

/// One tool call the model wants made: a name resolved by the dispatcher, and arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub verb: ToolName,
    pub args: BTreeMap<String, serde_json::Value>,
}

/// What one call is sent, whole: nothing of an earlier call is kept by the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub system: String,
    pub context: Context,
    /// Whole rounds before the one being answered, oldest first.
    pub earlier: Vec<Earlier>,
    /// The round being answered, oldest first, ending in the newest turn; a file in it is sent
    /// as itself.
    pub turns: Vec<Turn>,
}

/// One turn of a round before the one being answered, as a call sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Earlier {
    /// A turn that is no verb's answer, as itself; a file it names goes by its marker.
    Said(Turn),
    /// A verb's answer as itself, beside the call that has it again, so a file it handed over
    /// is named by its marker with that call; none where no call does (its call's words are
    /// gone, or what the owner pointed at is no verb's).
    Answered { call: Option<ToolCall>, turn: Turn },
    /// A verb's answer too long to send again: its marker, the call that has it again, and the
    /// answer's bytes where its words are here.
    Long {
        verb: crate::secretary::tool_name::ToolName,
        call: Option<ToolCall>,
        bytes: Option<usize>,
    },
}

impl Earlier {
    /// The turn it stands for, when it is sent as itself.
    #[must_use]
    pub fn turn(&self) -> Option<&Turn> {
        match self {
            Earlier::Said(turn) | Earlier::Answered { turn, .. } => Some(turn),
            Earlier::Long { .. } => None,
        }
    }
}

/// What a call is told beside its conversation: `cached` before its turns (the facts and the
/// record up to its frozen point), `rest` after the call's one mark (the record's newest parts,
/// then the topics in use).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Context {
    pub cached: Vec<String>,
    pub rest: Vec<String>,
}

impl Conversation {
    /// `turns` under `system`, told nothing before them.
    #[must_use]
    pub fn bare(system: impl Into<String>, turns: Vec<Turn>) -> Self {
        Self {
            system: system.into(),
            context: Context::default(),
            earlier: Vec::new(),
            turns,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reply {
    /// What to say to the owner; empty when the model only wants a call.
    pub say: String,
    pub calls: Vec<ToolCall>,
    /// What the model said this turn cost; `None` when unreported.
    pub usage: Option<TokenUsage>,
}

/// A plain-path answer and what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plain {
    pub text: String,
    pub usage: Option<TokenUsage>,
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
    /// Total: text that is not the envelope becomes `say` with no calls.
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
    #[error("the model could not be reached: {0}")]
    Unreachable(String),
    /// It ran and failed: a non-zero exit or a rejected request.
    #[error("the model refused or failed: {0}")]
    Failed(String),
    /// It refused the credential it was run with: the one failure a new credential can mend.
    /// A rate limit, a timeout or an outage is never this.
    #[error("the model refused its credential: {0}")]
    Refused(String),
    #[error("the model did not answer within {0:?}")]
    TimedOut(std::time::Duration),
    #[error("{0}")]
    Limited(suzatary_health::health::Limited),
    /// Its service is overloaded: no pause, since it is over in minutes.
    #[error("the model's service is overloaded: {0}")]
    Overloaded(String),
    #[error("what the model was sent is longer than it reads: {0}")]
    TooLong(String),
    /// A [`Halt`] was pulled while it ran, and the process was ended.
    #[error("the answer was stopped")]
    Halted,
}

/// A round's stop, set once by the actor who pulled it, beside the run its round drew, so a stop
/// pulls the halt of a round it ends and of no other.
#[derive(Debug, Clone, Default)]
pub struct Halt(std::sync::Arc<Pulled>);

#[derive(Debug, Default)]
struct Pulled {
    by: std::sync::OnceLock<prodrome_data::event::Actor>,
    run: std::sync::OnceLock<crate::secretary::store::RunId>,
}

impl Halt {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pull it, as `by`; only the first pull counts.
    pub fn stop(&self, by: prodrome_data::event::Actor) {
        let _first = self.0.by.set(by);
    }

    /// Its round drew `run`, before the round claims anything under it; only the first counts.
    pub fn claims(&self, run: crate::secretary::store::RunId) {
        let _first = self.0.run.set(run);
    }

    /// The run its round drew; none before the round has drawn one.
    #[must_use]
    pub fn run(&self) -> Option<&crate::secretary::store::RunId> {
        self.0.run.get()
    }

    #[must_use]
    pub fn by(&self) -> Option<&prodrome_data::event::Actor> {
        self.0.by.get()
    }

    #[must_use]
    pub fn halted(&self) -> bool {
        self.0.by.get().is_some()
    }
}

/// The halt of the round asking a model on this thread, for the e2e fixture's scripted model,
/// which is handed none: a scene's Stop decides when the round answers.
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

prodrome_data::text_literal!(
    SessionId => |id| id.as_str().to_owned(),
);

/// A retired model session's id, as its stored line names it, parsed as a UUID.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a session id is a UUID, nothing else: {0:?}")]
pub struct NotAUuid(pub String);

impl SessionId {
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

pub trait Model: Sync {
    /// Which model this is, stored on the conversation and never matched on.
    fn name(&self) -> String;

    /// Ask for one turn; `tools` is the closed verb list the prompt names. Each fragment of the
    /// reply goes to `on_delta` as it arrives, and the call ends when `halt` is pulled; a model
    /// that cannot stream answers whole, and one that cannot be stopped says so by ignoring it.
    ///
    /// # Errors
    /// [`ModelError::Halted`] when `halt` is pulled. Unparseable text is not an error.
    fn streamed(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError>;

    /// [`Model::streamed`] for a job nobody watches or stops.
    ///
    /// # Errors
    /// As [`Model::streamed`].
    fn turn(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        self.streamed(conversation, tools, &|_| {}, &Halt::new())
    }

    /// The limit its account is under now, which every call refuses with while it stands; none
    /// for a model whose calls the box records nowhere.
    fn limited(&self) -> Option<suzatary_health::health::Limited> {
        None
    }
}

impl<M: Model + ?Sized> Model for &M {
    fn name(&self) -> String {
        (**self).name()
    }

    fn limited(&self) -> Option<suzatary_health::health::Limited> {
        (**self).limited()
    }

    fn streamed(
        &self,
        conversation: &Conversation,
        tools: &serde_json::Value,
        on_delta: &dyn Fn(&str),
        halt: &Halt,
    ) -> Result<Reply, ModelError> {
        (**self).streamed(conversation, tools, on_delta, halt)
    }
}

/// The model one round asks, picked once for that round and owned by it: nothing of one
/// round's pick reaches another round or another conversation.
pub type Pick<'a> = Box<dyn Model + 'a>;

/// Which model a round asks.
pub trait Picker: Sync {
    /// The model this round asks, read now.
    fn pick(&self) -> Pick<'_>;
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
