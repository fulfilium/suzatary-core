//! One exchange with a model: ask, append its turn, run the verbs it called, until it answers
//! without a call or reaches its bound.

use serde_json::{Map, Value};

use crate::conversation::ask::{Ask, Asked};
use crate::secretary::surface::{SecretaryCall, Subset};
use crate::secretary::verb_answer::VerbAnswer;
use crate::verb::Request;
use suzatary_base::capability::Clock;
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::escape::escape_markup;
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::model::{Conversation, Halt, ModelError, ToolCall, Turn};
use suzatary_conversations::secretary::queue;
use suzatary_conversations::secretary::store::RunId;
use suzatary_conversations::secretary::store::{Entry, Scope, Stored};
use suzatary_conversations::secretary::taint::Taint;
use suzatary_conversations::secretary::tool_name::ToolName;
use suzatary_lines::secretary::answer::{Answer, BoxLine};
use suzatary_lines::secretary::conversations;
use suzatary_lines::secretary::transcript;
use suzatary_lines::secretary::transcript::Retry;

#[derive(Debug, Default)]
pub struct Round {
    /// What the model was last sent.
    pub sent: Option<Conversation>,
    /// The calls the last reply made, in the order it made them.
    pub calls: Vec<ToolCall>,
    pub appended: Vec<Entry>,
    /// How many times the model was asked.
    pub steps: usize,
    /// What the round's calls carried, joined as each is BUILT, before it is sent, so a call
    /// that fails carried it too: clean before the first is built. Its model turns, its closing
    /// turn among them, store it, and its verbs' gate reads it: held is sent by construction.
    pub carries: Taint,
}

/// Everything that differs between the rounds of different callers.
pub struct Rules<'a> {
    pub ask: Ask<'a>,
    pub system: &'a str,
    pub verbs: &'a Subset,
    /// Runs one call of a verb in `verbs`; a refusal is written down as the call's answer.
    pub run: &'a Run<'a>,
    /// How many times the model may be asked.
    pub bound: usize,
    pub sink: &'a Sink<'a>,
    /// Runs once something waits, before the model is first asked; a refusal ends the round like
    /// any other, with the box's words.
    pub load: &'a Load<'a>,
    /// Whether a round a lost run left may be claimed again: not while the model is failing.
    pub retry: Retry,
    /// Whether the todos a turn names are confirmed, as the chains read when a claim judges
    /// another writer's turn.
    pub confirms: &'a transcript::Confirms<'a>,
}

/// Runs one call of a verb the round's surface lists, read whole, beside what the round's asks
/// were sent carried, the one fact its gate reads.
pub type Run<'a> = dyn Fn(Request<SecretaryCall>, Taint) -> Result<VerbAnswer, Refusal> + 'a;

pub type Load<'a> = dyn Fn() -> Result<(), Refusal> + 'a;

pub const LOAD_NOTHING: &Load<'static> = &|| Ok(());

pub struct Sink<'a> {
    /// Each fragment of the model's reply, verbatim, as it arrives.
    pub on_delta: &'a dyn Fn(&str),
    /// A verb's name, just before it runs.
    pub on_working: &'a dyn Fn(&str),
    /// Pulled by `POST …/stop`: the model process ends and the round writes
    /// [`Entry::Stopped`], which closes the question by itself.
    pub halt: &'a Halt,
}

impl<'a> Sink<'a> {
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
    pub conversations: conversations::Conversations<'a>,
    pub clock: &'a dyn Clock,
    pub id: &'a ConversationId,
}

/// How a round ended. Every round that ran ends one of these two ways, and an [`Ending::Ended`]
/// round's last line is the box's, written from its [`Why`] by [`Round::answer`] alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    Answered,
    Ended {
        /// Why it stopped.
        why: Why,
    },
}

impl Ending {
    /// For a job nobody reads as a conversation (a review, a sweep): `Err` when the round never
    /// had a working model or store, so the job fails as it should; a silent model, the bound
    /// and a stop are endings it records, not failures.
    pub fn ran(&self) -> Result<(), Refusal> {
        match self {
            Ending::Answered
            | Ending::Ended {
                why: Why::Silent { .. } | Why::Bound(_) | Why::Stopped(_) | Why::Unanswered,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// The model failed when asked. `after` is the verb whose answer it was asked about, if any.
    Model {
        failed: ModelError,
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
    /// No round answered in time: two runs that covered it were lost, or it was asked longer
    /// ago than [`transcript::ANSWERED_WITHIN`].
    Unanswered,
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
            Why::Unanswered => UNANSWERED.to_owned(),
        }
    }
}

/// The box's words for a model that failed after `after` answered.
fn model_failed(failed: &ModelError, after: Option<&ToolName>) -> String {
    match (failed, after) {
        (ModelError::TooLong(_), Some(verb)) => format!(
            "I could not finish: {verb} answered more than I can read. Ask me to narrow it; \
             your next message is sent without that answer."
        ),
        (ModelError::TooLong(_), None) => "I could not finish: this message, with what it \
             brought, is longer than the model reads. Ask again in smaller parts. Nothing is cut: \
             your next message is sent with this one whole, a long answer it brought by its \
             marker, or without it when even that is too long."
            .to_owned(),
        (ModelError::TimedOut(limit), _) => format!(
            "I stopped: the model did not answer within {} s. Ask again, or ask for less at once.",
            limit.as_secs()
        ),
        (ModelError::Unreachable(words), _) => format!(
            "I could not finish: the model could not be reached ({words}). Ask again in a minute."
        ),
        (ModelError::Limited(limited), _) => {
            format!("I could not finish: {limited}. Ask again after then.")
        }
        (ModelError::Overloaded(_), _) => {
            "I could not finish: the model's service is overloaded. Ask again in a few minutes."
                .to_owned()
        }
        (ModelError::Refused(words), _) => format!(
            "I could not finish: the model refused its sign-in ({words}). Sign in again from \
             Settings."
        ),
        (ModelError::Failed(words), _) => {
            format!("I could not finish: the model failed ({words}). Ask again.")
        }
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
    pub round: Round,
    pub ending: Ending,
}

impl Round {
    /// Answer what waits in the conversation; `None` when nothing does, and no model is asked.
    ///
    /// The round claims what it answers under the lines' lock before it spends anything
    /// ([`transcript::claim`]), so one run answers a turn at a time, and a run lost with its round
    /// open is retried once, then closed by the box; the conversation never ends on a call.
    pub fn answer(rules: &Rules<'_>, at: &Transcript<'_>) -> Result<Option<Closed>, Refusal> {
        let held = at.conversations.runs.begin()?;
        let run = &held.run;
        // Named before the claim, so a stop taken after the claim finds this round by its run.
        rules.sink.halt.claims(run.clone());
        let now = at.clock.now();
        match transcript::claim(
            at.conversations,
            at.id,
            run,
            now,
            rules.retry,
            rules.confirms,
            |_| closing(now, &Why::Unanswered, Taint::Clean),
        )? {
            transcript::Claimed::Nothing => return Ok(None),
            transcript::Claimed::Closed => {
                return Ok(Some(Closed {
                    round: Round::default(),
                    ending: Ending::Ended {
                        why: Why::Unanswered,
                    },
                }));
            }
            transcript::Claimed::Began => {}
        }
        let mut round = Round::default();
        let ending = match round.run(rules, at, run) {
            Ok(Ran::Nothing) => return Ok(None),
            Ok(Ran::Answered) => Ending::Answered,
            Err(why) => match round.close(at, run, &why)? {
                transcript::Answering::Written => Ending::Ended { why },
                transcript::Answering::Answered => return Ok(None),
            },
        };
        Ok(Some(Closed { round, ending }))
    }

    fn run(&mut self, rules: &Rules<'_>, at: &Transcript<'_>, run: &RunId) -> Result<Ran, Why> {
        let tools = rules.verbs.tools();
        loop {
            // A turn said since the last ask joins the round before the model reads it.
            transcript::join(at.conversations, at.id, run, at.clock.now(), rules.confirms)
                .map_err(Refusal::from)?;
            let read = transcript::read(at.conversations, at.id).map_err(Refusal::from)?;
            let stored = read.stored();
            if !queue::waits(&read.sending(run)) {
                return Ok(Ran::Nothing);
            }
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
            let _asking =
                suzatary_conversations::secretary::model::pulled::Asking::of(rules.sink.halt);
            let call = rules.ask.carrying(rules.system, &read);
            self.carries = self.carries.join(call.carries);
            let reply = rules
                .ask
                .send(call.call, &tools, rules.sink)
                .map_err(|failed| unasked(failed, after.clone(), rules.sink.halt))?;
            obeyed(rules.sink.halt)?;
            let silent = reply.reply.say.trim().is_empty() && reply.reply.calls.is_empty();
            if self.said(at, stored, reply, rules.ask.scope(), run)?
                == transcript::Answering::Answered
            {
                return Ok(Ran::Nothing);
            }
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

    /// The model's turn, after the scope it was asked over, if that moved; nothing of the turn's
    /// own once the round of `run` is closed.
    fn said(
        &mut self,
        at: &Transcript<'_>,
        stored: &Stored,
        asked: Asked,
        scope: Scope,
        run: &RunId,
    ) -> Result<transcript::Answering, Refusal> {
        if scope != stored.scope {
            self.append(
                at,
                Entry::Scoped {
                    at: at.clock.now(),
                    scope,
                },
            )?;
        }
        self.sent = Some(asked.sent);
        self.calls.clone_from(&asked.reply.calls);
        let answer = Answer::after(
            at.clock.now(),
            asked.reply.say,
            asked.reply.calls,
            self.carries,
        );
        self.answering(at, run, vec![BoxLine::Answer(answer)])
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
        let sent = self.carries;
        let answer = rules
            .verbs
            .find(&called.verb)
            .and_then(|verb| verb.request(&args))
            .and_then(|request| (rules.run)(request, sent));
        self.append(
            at,
            Entry::Said {
                at: stamped,
                turn: VerbAnswer::turn(called.verb.clone(), answer),
                by: None,
            },
        )
    }

    /// The lines that end the round of `run` short of an answer, while it is open.
    fn close(
        &mut self,
        at: &Transcript<'_>,
        run: &RunId,
        why: &Why,
    ) -> Result<transcript::Answering, Refusal> {
        let now = at.clock.now();
        let lines = match why {
            Why::Stopped(by) => vec![BoxLine::Entry(Entry::Stopped {
                at: now,
                by: by.as_str().to_owned(),
            })],
            Why::Model { .. }
            | Why::Silent { .. }
            | Why::Bound(_)
            | Why::Refused(_)
            | Why::Unanswered => closing(now, why, self.carries),
        };
        self.answering(at, run, lines)
    }

    /// `lines`, appended in one hold of the lock while the round of `run` is open, or once this
    /// round has written a model turn: a silent one closed the round, and the box's words that
    /// end it follow.
    fn answering(
        &mut self,
        at: &Transcript<'_>,
        run: &RunId,
        lines: Vec<BoxLine>,
    ) -> Result<transcript::Answering, Refusal> {
        if self.appended.iter().any(|written| {
            matches!(
                written,
                Entry::Said {
                    turn: Turn::Model { .. },
                    ..
                }
            )
        }) {
            for line in lines {
                match &line {
                    BoxLine::Answer(answer) => {
                        transcript::answer(at.conversations, at.id, answer)?;
                    }
                    BoxLine::Entry(entry) => transcript::push(at.conversations, at.id, entry)?,
                }
                self.appended.push(line.entry());
            }
            return Ok(transcript::Answering::Written);
        }
        let answering = transcript::push_open(at.conversations, at.id, run, &lines)?;
        if answering == transcript::Answering::Written {
            self.appended.extend(lines.iter().map(BoxLine::entry));
        }
        Ok(answering)
    }

    fn append(&mut self, at: &Transcript<'_>, entry: Entry) -> Result<(), Refusal> {
        transcript::push(at.conversations, at.id, &entry)?;
        self.appended.push(entry);
        Ok(())
    }
}

/// The box's own sentence for a question no round answered in time.
pub const UNANSWERED: &str = "This was not answered in time; ask again.";

/// The line that ends an exchange the model did not finish: a model turn with no calls, since
/// "over" is derived from the transcript, carrying `carries`, the round's join of every call it
/// built. A total function: after a call was built, its words may quote the model CLI's or a
/// verb's (escaped, since they are no markup), and it carries what that call was sent; before
/// any call was built its words are the box's own (a store's refusal, the bound's count), and
/// it carries the round's join so far, clean; the unanswered question's is a constant
/// ([`Answer::own`]).
fn closing(now: Datetime, why: &Why, carries: Taint) -> Vec<BoxLine> {
    let answer = match why {
        Why::Unanswered => Answer::own(now, UNANSWERED),
        Why::Model { .. }
        | Why::Silent { .. }
        | Why::Bound(_)
        | Why::Refused(_)
        | Why::Stopped(_) => Answer::after(
            now,
            escape_markup(&why.said()).as_str().to_owned(),
            Vec::new(),
            carries,
        ),
    };
    vec![BoxLine::Answer(answer)]
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
