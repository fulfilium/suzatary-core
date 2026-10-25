//! One exchange with a model: ask, append its turn, run the verbs it called, until it answers
//! without a call or reaches its bound.

use serde_json::{Map, Value};

use crate::capability::{Clock, Halt, ModelError, ToolCall, Turn};
use crate::conversation::ask::{Answered, Ask, Asked, Sent};
use crate::escape::escape_markup;
use crate::refusal::Refusal;
use crate::secretary::queue;
use crate::secretary::store::{ConversationId, Entry, Scope, Stored};
use crate::secretary::surface::{SecretaryCall, Subset};
use crate::secretary::tool_name::ToolName;
use crate::secretary::transcript;
use crate::secretary::verb_answer::VerbAnswer;
use crate::verb::Request;

/// Who says the turn that ends an exchange the model did not finish.
const BOX: &str = "the box";

/// One exchange's state: what the last ask sent, the calls its reply made, every line the round
/// appended, in order, and how many times it asked.
#[derive(Debug, Default)]
pub struct Round {
    /// What the last ask sent the model.
    pub sent: Option<Sent>,
    /// The calls the last reply made, in the order it made them.
    pub calls: Vec<ToolCall>,
    /// Every line this round appended.
    pub appended: Vec<Entry>,
    /// How many times the model was asked.
    pub steps: usize,
}

/// Everything that differs between the rounds of different callers.
pub struct Rules<'a> {
    /// How the model is asked.
    pub ask: Ask<'a>,
    /// The manual it works under.
    pub system: &'a str,
    /// The verbs it is shown and may call.
    pub verbs: &'a Subset,
    /// Runs one call of a verb in `verbs`; a refusal is written down as the call's answer.
    pub run: &'a Run<'a>,
    /// How many times the model may be asked.
    pub bound: usize,
    /// What the round streams, and the stop it obeys.
    pub sink: &'a Sink<'a>,
    /// Runs once something waits, before the model is first asked: the secretary loads what the
    /// question points at. A refusal ends the round like any other, with the box's words.
    pub load: &'a Load<'a>,
}

/// Runs one call of a verb the round's surface lists, read whole.
pub type Run<'a> = dyn Fn(Request<SecretaryCall>) -> Result<VerbAnswer, Refusal> + 'a;

/// What a round does before it first asks the model.
pub type Load<'a> = dyn Fn() -> Result<(), Refusal> + 'a;

/// Loads nothing, for a round whose question points at nothing.
pub const LOAD_NOTHING: &Load<'static> = &|| Ok(());

/// What a round feeds as it goes, and the stop it obeys.
pub struct Sink<'a> {
    /// Each fragment of a resumed session's reply, verbatim, as it arrives.
    pub on_delta: &'a dyn Fn(&str),
    /// A verb's name, just before it runs.
    pub on_working: &'a dyn Fn(&str),
    /// Pulled by `POST …/stop`: the model process ends and the round writes
    /// [`Entry::Stopped`], which closes the question by itself.
    pub halt: &'a Halt,
}

impl<'a> Sink<'a> {
    /// Streams nothing, for a caller nobody watches.
    #[must_use]
    pub fn quiet(halt: &'a Halt) -> Self {
        fn nothing(_: &str) {}
        Self {
            on_delta: &nothing,
            on_working: &nothing,
            halt,
        }
    }
}

/// The one conversation a round reads and appends to, and the clock that stamps its lines.
#[derive(Clone, Copy)]
pub struct Transcript<'a> {
    /// The store.
    pub conversations: crate::secretary::conversations::Conversations<'a>,
    /// Stamps each line.
    pub clock: &'a dyn Clock,
    /// The conversation.
    pub id: &'a ConversationId,
}

/// How a round ended. Every round that ran ends one of these two ways, and an [`Ending::Ended`]
/// round's last line is the box's, written from its [`Why`] by [`Round::answer`] alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// The model's last turn answered the owner in words.
    Answered,
    /// The round stopped short of an answer, and the box said why in the conversation's last line.
    Ended {
        /// Why it stopped.
        why: Why,
    },
}

impl Ending {
    /// For a job nobody reads as a conversation (a review, a sweep): `Err` when the round never
    /// had a working model or store, so the job fails as it should; a silent model, the bound
    /// and a stop are endings it records, not failures.
    ///
    /// # Errors
    /// The model's failure as a refusal, or the store's own.
    pub fn ran(&self) -> Result<(), Refusal> {
        match self {
            Ending::Answered
            | Ending::Ended {
                why: Why::Silent { .. } | Why::Bound(_) | Why::Stopped(_),
            } => Ok(()),
            Ending::Ended {
                why: Why::Model { failed, .. },
            } => Err(Refusal::internal(failed.to_string())),
            Ending::Ended {
                why: Why::Refused(refusal),
            } => Err(refusal.clone()),
        }
    }
}

/// Why a round stopped short of the model's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The model failed when asked. `after` is the verb whose answer it was asked about, if any.
    Model {
        /// How it failed.
        failed: ModelError,
        /// The verb that answered last before the ask.
        after: Option<ToolName>,
    },
    /// The model replied with no words and no call. `after` is as for [`Why::Model`].
    Silent {
        /// The verb that answered last before the ask.
        after: Option<ToolName>,
    },
    /// The model asked for verbs this many times without answering.
    Bound(usize),
    /// The store refused, or what the question points at would not load.
    Refused(Refusal),
    /// Somebody pulled the round's [`Halt`]; the line is an [`Entry::Stopped`] naming them.
    Stopped(prodrome_core::event::Actor),
}

impl From<Refusal> for Why {
    fn from(refusal: Refusal) -> Self {
        Why::Refused(refusal)
    }
}

impl Why {
    /// What happened and what the owner can do next, in plain words: the box's last turn, and
    /// the line a caller logs.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Why::Model { failed, after } => model_failed(failed, after.as_ref()),
            Why::Silent { after: Some(verb) } => format!(
                "I stopped after {verb} answered, without an answer for you. Ask again, or ask \
                 me to narrow it."
            ),
            Why::Silent { after: None } => {
                "The model replied with nothing to say. Ask again.".to_owned()
            }
            Why::Bound(bound) => format!(
                "I asked for a verb {bound} times without finishing, so this exchange stopped \
                 here. Say something to continue it."
            ),
            Why::Refused(refusal) => {
                format!("I could not finish: {}. Ask again.", refusal.message())
            }
            Why::Stopped(by) => format!("{} stopped this answer.", by.as_str()),
        }
    }
}

/// The box's words for a model that failed after `after` answered.
fn model_failed(failed: &ModelError, after: Option<&ToolName>) -> String {
    match (failed, after) {
        (ModelError::TooLong(_), Some(verb)) => format!(
            "I could not finish: {verb} answered more than I can read. Ask me to narrow it."
        ),
        (ModelError::TooLong(_), None) => "I could not finish: this conversation is longer than \
             the model reads. Start a new conversation, or ask me to narrow it."
            .to_owned(),
        (ModelError::TimedOut(limit), _) => format!(
            "I stopped: the model did not answer within {} s. Ask again, or ask for less at once.",
            limit.as_secs()
        ),
        (ModelError::Unreachable(words), _) => format!(
            "I could not finish: the model could not be reached ({words}). Ask again in a minute."
        ),
        (ModelError::Refused(words), _) => format!(
            "I could not finish: the model refused its sign-in ({words}). Sign in again from \
             Settings."
        ),
        (ModelError::Failed(words), _) => {
            format!("I could not finish: the model failed ({words}). Ask again.")
        }
        (ModelError::Lost(_), _) => "I could not finish: the model lost this conversation's \
             session. Ask again and I will open a new one."
            .to_owned(),
        (ModelError::Halted, _) => "I stopped: the model's run was ended. Ask again.".to_owned(),
    }
}

/// How [`Round::run`] came back without a [`Why`].
enum Ran {
    /// Nothing waited, so no model was asked; or the question was closed by another line (a
    /// rewind) while the round ran, which leaves nothing to answer.
    Nothing,
    /// The model's last turn answered in words.
    Answered,
}

/// A round that ran: what it appended and how it ended.
#[derive(Debug)]
pub struct Closed {
    /// The round's lines and steps.
    pub round: Round,
    /// How it ended.
    pub ending: Ending,
}

impl Round {
    /// Answer what waits in the conversation; `None` when nothing does, and no model is asked.
    /// A round that ran ends on the model's answer or on the box's line saying why it did not,
    /// so the conversation never ends on a call or a verb's answer and nothing is asked again; a
    /// stop in one write, so no reading between two writes finds its question queued again.
    ///
    /// # Errors
    /// The store's, when it refuses the box's closing line too.
    pub fn answer(rules: &Rules<'_>, at: &Transcript<'_>) -> Result<Option<Closed>, Refusal> {
        let mut round = Round::default();
        let ending = match round.run(rules, at) {
            Ok(Ran::Nothing) => return Ok(None),
            Ok(Ran::Answered) => Ending::Answered,
            Err(why) => {
                round.close(at, &why)?;
                Ending::Ended { why }
            }
        };
        Ok(Some(Closed { round, ending }))
    }

    /// Step until the model answers without a call, or say why it did not.
    fn run(&mut self, rules: &Rules<'_>, at: &Transcript<'_>) -> Result<Ran, Why> {
        let tools = rules.verbs.tools();
        loop {
            let stored = read(at)?;
            // A round with nothing new does not run: a continuation would buy an answer to nothing.
            let Some(waiting) = queue::Pending::of(&stored) else {
                return Ok(Ran::Nothing);
            };
            // When a stop arrives before the model is asked, the round asks the model nothing.
            obeyed(rules.sink.halt)?;
            if self.steps == 0 {
                (rules.load)()?;
            }
            if self.steps == rules.bound {
                return Err(Why::Bound(rules.bound));
            }
            self.steps += 1;
            let after = self.calls.last().map(|called| called.verb.clone());
            #[cfg(feature = "fixture")]
            let _asking = crate::capability::pulled::Asking::of(rules.sink.halt);
            let asked = rules
                .ask
                .ask(at.id, rules.system, &tools, &stored, &waiting, rules.sink)
                .map_err(|failed| unasked(failed, after.clone(), rules.sink.halt))?;
            obeyed(rules.sink.halt)?;
            let silent = asked.reply.say.trim().is_empty() && asked.reply.calls.is_empty();
            self.said(at, &stored, asked, rules.ask.scope())?;
            if silent {
                return Err(Why::Silent { after });
            }
            if self.calls.is_empty() {
                return Ok(Ran::Answered);
            }
            for called in self.calls.clone() {
                obeyed(rules.sink.halt)?;
                (rules.sink.on_working)(called.verb.as_str());
                self.answered(rules, at, &called)?;
            }
        }
    }

    /// The model's turn, after the memory it was told and the session it opened, if either moved.
    fn said(
        &mut self,
        at: &Transcript<'_>,
        stored: &Stored,
        asked: Asked,
        scope: Scope,
    ) -> Result<(), Refusal> {
        if let Some(version) = asked.seen
            && stored.memory_seen != Some(version)
        {
            self.append(
                at,
                Entry::MemorySeen {
                    at: at.clock.now(),
                    version,
                },
            )?;
        }
        match &asked.answered {
            Answered::Opened(_, kept) => self.append(at, kept.line(at.clock.now()))?,
            Answered::Stateless { by, .. } => self.unheard(at, stored, by)?,
            Answered::Resumed(_) => {}
        }
        if scope != stored.scope {
            self.append(
                at,
                Entry::Scoped {
                    at: at.clock.now(),
                    scope,
                },
            )?;
        }
        self.sent = Some(asked.answered.sent());
        self.calls.clone_from(&asked.reply.calls);
        self.append(
            at,
            Entry::Said {
                at: at.clock.now(),
                turn: Turn::Model {
                    text: asked.reply.say,
                    calls: asked.reply.calls,
                },
                by: None,
            },
        )
    }

    /// One call's answer, refused unless the round's verbs list it.
    fn answered(
        &mut self,
        rules: &Rules<'_>,
        at: &Transcript<'_>,
        called: &ToolCall,
    ) -> Result<(), Refusal> {
        let args: Map<String, Value> = called
            .args
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let stamped = at.clock.now();
        let answer = rules
            .verbs
            .find(&called.verb)
            .and_then(|verb| verb.request(&args))
            .and_then(rules.run);
        self.append(
            at,
            Entry::Said {
                at: stamped,
                turn: VerbAnswer::turn(called.verb.clone(), answer),
                by: None,
            },
        )
    }

    /// The line that ends a round short of an answer. A stop is its own line; every other
    /// ending is a model turn with no calls, since "over" is derived from the transcript, holding
    /// `why`'s words escaped, since they quote a backend's or a verb's text and are not markup.
    /// The box says it, not the session, so the session never hears it nor the turns it closes.
    fn close(&mut self, at: &Transcript<'_>, why: &Why) -> Result<(), Refusal> {
        match why {
            Why::Stopped(by) => self.append(
                at,
                Entry::Stopped {
                    at: at.clock.now(),
                    by: by.as_str().to_owned(),
                },
            ),
            Why::Model { .. } | Why::Silent { .. } | Why::Bound(_) | Why::Refused(_) => {
                self.unheard(at, &read(at)?, BOX)?;
                self.append(
                    at,
                    Entry::Said {
                        at: at.clock.now(),
                        turn: Turn::Model {
                            text: escape_markup(&why.said()).as_str().to_owned(),
                            calls: Vec::new(),
                        },
                        by: None,
                    },
                )
            }
        }
    }

    /// When `stored` keeps a session, that it did not hear what `by` says next: a session is sent
    /// only the turns since its last, so the next held round must open one with the whole window.
    fn unheard(&mut self, at: &Transcript<'_>, stored: &Stored, by: &str) -> Result<(), Refusal> {
        let Some(kept) = &stored.session else {
            return Ok(());
        };
        self.append(
            at,
            Entry::Unheard {
                at: at.clock.now(),
                session: kept.session.id.clone(),
                model: by.to_owned(),
            },
        )
    }

    fn append(&mut self, at: &Transcript<'_>, entry: Entry) -> Result<(), Refusal> {
        transcript::push(at.conversations, at.id, &entry)?;
        self.appended.push(entry);
        Ok(())
    }
}

fn read(at: &Transcript<'_>) -> Result<Stored, Refusal> {
    crate::secretary::read(at.conversations, at.id)
}

/// `Err` once the halt is pulled, naming who pulled it.
fn obeyed(halt: &Halt) -> Result<(), Why> {
    halt.by().map_or(Ok(()), |by| Err(Why::Stopped(by.clone())))
}

/// A model that failed while its halt was pulled was stopped, whatever the interpreter says.
fn unasked(failed: ModelError, after: Option<ToolName>, halt: &Halt) -> Why {
    match obeyed(halt) {
        Err(stopped) => stopped,
        Ok(()) => Why::Model { failed, after },
    }
}
