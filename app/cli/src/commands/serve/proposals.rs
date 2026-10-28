//! Proposals: the inbox of everything proposed, and the owner's answers to one proposal.

use axum::Json;
use suzatary_core::reference::ProposalRef;
use suzatary_core::refusal::Refusal;
use suzatary_core::secretary::routes::{self as secretary_routes, Effectors, TapAnswer};
use suzatary_core::secretary::store::ConversationId;
use suzatary_core::secretary::tap::Tap;
use suzatary_core::settings::EditError;
use suzatary_core::wire;

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::Reader;
use super::params::{Asked, Params};
use super::secretary::conversation_named;
use super::{Chained, State};

/// Proposals of one conversation, named by their indices.
pub struct Drawn {
    id: ConversationId,
    indices: Vec<usize>,
}

/// `GET …/<id>/drawn?indices=<n>,…`: the proposals named, drawn against the todo chains in one
/// read.
pub fn drawn(call: &Call<Reader, Drawn, ()>) -> Result<Json<wire::DrawnPayload>, Refusal> {
    let Drawn { id, indices } = &call.params;
    call.state
        .with_cards(|caps, cards| secretary_routes::drawn(caps, cards, id, indices))
        .map(Json)
}

/// The owner's answer to a tap: a line, and at most one of a secret's value, "always allow"
/// and a device's endorsement.
pub struct Answer {
    note: String,
    answer: TapAnswer,
}

/// One proposal: its conversation and its index there.
impl Params for ProposalRef {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let id = conversation_named(asked)?;
        let text = asked.segment("index")?;
        let index = text
            .parse()
            .map_err(|_| Refusal::not_found(format!("proposal {text} is not an index")))?;
        Ok(Self {
            conversation: id,
            index,
        })
    }
}

impl Params for Drawn {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let id = conversation_named(asked)?;
        let Some(asked) = asked.query("indices").filter(|text| !text.is_empty()) else {
            return Err(Refusal::bad_request(
                "name the proposals to draw: ?indices=<n>,…",
            ));
        };
        let indices = asked
            .split(',')
            .map(|text| {
                text.parse()
                    .map_err(|_| Refusal::bad_request(format!("proposal {text} is not an index")))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { id, indices })
    }
}

impl Body for Answer {
    const CAP: usize = MAX_ACT_BYTES;

    /// The value is read once into a [`suzatary_core::capability::Secret`], and the body dropped
    /// with this call; a body naming two of `value`, `always` and `endorsement` is refused.
    type Wire = wire::Tapping;

    fn parse(tapping: wire::Tapping, _: &State) -> Result<Self, Refusal> {
        let wire::Tapping {
            note,
            value,
            always,
            endorsement,
        } = tapping;
        let endorsement = endorsement.as_ref().map(endorsement_of).transpose()?;
        let answer = match (value, always, endorsement) {
            (None, false, None) => TapAnswer::Plain,
            (Some(value), false, None) => TapAnswer::Value(value),
            (None, true, None) => TapAnswer::Always,
            (None, false, Some(endorsement)) => TapAnswer::Endorsed(endorsement),
            (Some(_), true, _) | (Some(_), _, Some(_)) | (_, true, Some(_)) => {
                return Err(Refusal::bad_request(
                    "a tap carries at most one of value, always and endorsement",
                ));
            }
        };
        Ok(Self { note, answer })
    }
}

/// A signing key and a signature, each checked as what it says it is.
fn endorsement_of(
    endorsing: &wire::Endorsing,
) -> Result<suzatary_core::signatures::Endorsement, Refusal> {
    Ok(suzatary_core::signatures::Endorsement {
        key: prodrome_core::sign::PublicKey::new(&endorsing.key)
            .map_err(|e| Refusal::bad_request(format!("endorsement.key: {e}")))?,
        signature: prodrome_core::sign::Signature::new(&endorsing.signature)
            .map_err(|e| Refusal::bad_request(format!("endorsement.signature: {e}")))?,
    })
}

impl Body for wire::Reasoning {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(reasoning: Self, _: &State) -> Result<Self, Refusal> {
        Ok(reasoning)
    }
}

/// `POST …/proposals/<n>/tap`: perform one proposal, the one irreversible route.
pub fn tap(
    call: &Call<Tap, ProposalRef, Answer>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    let state = &call.state;
    let card = &call.params;
    let (id, index) = (&card.conversation, &card.index);
    let Answer { note, answer } = &call.body;
    let chains = state.chain_stores();
    let started = std::time::Instant::now();
    let edits = Noting::new(state.edits.as_ref());
    let answered = state
        .with_cards(|caps, cards| {
            secretary_routes::tap(
                caps,
                &Effectors {
                    post: state.postman.as_ref(),
                    reveal: state.reveal.as_ref(),
                    performer: state.acting.as_ref(),
                    secrets: state.secrets.as_ref(),
                    verify: state.verify.as_ref(),
                    checks: state.checks.as_ref(),
                    signing: state.signing.as_ref(),
                    edits: &edits,
                    forget: state.forget.as_ref(),
                    enrolling: state.keys.as_deref().map(|keys| {
                        suzatary_core::signatures::Enrolling {
                            chains: &chains,
                            root: keys.root(),
                        }
                    }),
                    consents: state.consenting.as_ref(),
                },
                &call.guard,
                cards,
                card,
                note,
                answer,
            )
        })
        .inspect(|_| {
            state.heard(id);
            // A connection or a setting written or removed moves what the box reads of the file.
            if edits.moved() {
                state.reconnected(call.guard.who().as_str());
            }
            // A stored secret, a sign-in or a connection moves what the box owes.
            state.notice();
        });
    // A tap whose answer never reached the page is answered from this line: what happened, and how long.
    let ms = started.elapsed().as_millis();
    match &answered {
        Ok(payload) => {
            let now = payload
                .proposals
                .iter()
                .find(|p| p.index == *index)
                .map(|p| p.state);
            tracing::info!(conversation = %id, index, state = ?now, ms, "tap");
        }
        Err(refusal) => tracing::warn!(conversation = %id, index, ms, refused = %refusal, "tap"),
    }
    answered.map(Json)
}

/// The instance file's editor, noting whether a tap moved the file.
struct Noting<'a> {
    edits: &'a dyn suzatary_core::settings::Edits,
    moved: std::sync::atomic::AtomicBool,
}

impl<'a> Noting<'a> {
    fn new(edits: &'a dyn suzatary_core::settings::Edits) -> Self {
        Self {
            edits,
            moved: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Whether an edit went through.
    fn moved(&self) -> bool {
        self.moved.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn noted<T>(&self, done: Result<T, EditError>) -> Result<T, EditError> {
        if done.is_ok() {
            self.moved.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        done
    }
}

impl suzatary_core::settings::Edits for Noting<'_> {
    fn allow_web(&self, rule: &suzatary_core::web::AllowRule) -> Result<(), EditError> {
        self.noted(self.edits.allow_web(rule))
    }

    fn allow_drive(&self, folder: &suzatary_core::drive::DrivePath) -> Result<(), EditError> {
        self.noted(self.edits.allow_drive(folder))
    }

    fn add_connection(
        &self,
        connection: &suzatary_core::connect::Connection,
    ) -> Result<(), EditError> {
        self.noted(self.edits.add_connection(connection))
    }

    fn remove_connection(
        &self,
        connection: &suzatary_core::connect::Connection,
    ) -> Result<(), EditError> {
        self.noted(self.edits.remove_connection(connection))
    }

    fn add_origin(
        &self,
        plugin: &suzatary_core::plugin::PluginName,
        origin: &suzatary_core::plugin::Origin,
    ) -> Result<(), EditError> {
        self.noted(self.edits.add_origin(plugin, origin))
    }
}

impl suzatary_core::settings::Configuration for Noting<'_> {
    fn declared(&self) -> Result<suzatary_core::connect::Declared, EditError> {
        self.edits.declared()
    }
}

/// `POST …/proposals/<n>/dismiss`: say no, and say why.
pub fn dismiss(
    call: &Call<Tap, ProposalRef, wire::Reasoning>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    let card = &call.params;
    let id = &card.conversation;
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::dismiss(caps, &call.guard, cards, card, &call.body.reason)
        })
        .inspect(|_| call.state.heard(id))
        .map(Json)
}

/// `POST …/proposals/<n>/acknowledge`: the owner checked Sent for a send that may have
/// gone; performs nothing and wakes nobody. Its act line is a notice the answer rule would
/// owe, and a check must not cost a model round, so it is not heard until notices have a line
/// kind of their own.
pub fn acknowledge(
    call: &Call<Tap, ProposalRef, ()>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    let card = &call.params;
    call.state
        .with_cards(|caps, cards| secretary_routes::acknowledge(caps, &call.guard, cards, card))
        .map(Json)
}

/// `POST …/proposals/<n>/drop`: let a coding session go, with a required reason.
pub fn drop_session(
    call: &Call<Tap, ProposalRef, wire::Reasoning>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    let card = &call.params;
    let id = &card.conversation;
    call.state
        .with_cards(|caps, cards| {
            secretary_routes::drop_session(caps, &call.guard, cards, card, &call.body.reason)
        })
        .inspect(|_| call.state.heard(id))
        .map(Json)
}
