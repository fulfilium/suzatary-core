//! Proposals: the inbox, and the owner's answers to one proposal.

use crate::secretary::routes::{self as secretary_routes, Effectors, TapAnswer};
use axum::Json;
use suzatary_api::wire::Reasoning;
use suzatary_api::wire::Tapping;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::ProposalRef;
use suzatary_base::refusal::Refusal;
use suzatary_signatures::signatures::Tap;
use suzatary_signatures::signatures::{Endorsement, Keeping};
use suzatary_topics::wire::topics as wire;

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::Reader;
use super::params::{Asked, Params};
use super::secretary::conversation_named;
use super::{Chained, State};
use prodrome_data::sign;

pub(crate) struct Drawn {
    id: ConversationId,
    indices: Vec<usize>,
}

/// `GET …/<id>/drawn?indices=<n>,…`: the proposals named, drawn in one read.
pub(crate) fn drawn(
    call: &Call<Reader, Drawn, ()>,
) -> Result<Json<suzatary_api::wire::DrawnPayload>, Refusal> {
    let Drawn { id, indices } = &call.params;
    call.state
        .with_cards(|caps, cards| secretary_routes::drawn(caps, cards, id, indices))
        .map(Json)
}

/// The owner's answer to a tap: a line and at most one extra, or the tap of the card's fix.
pub(crate) enum Answer {
    Tap { note: String, answer: TapAnswer },
    Fix,
}

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

    /// The value is read once into a [`suzatary_base::capability::Secret`], and the body dropped
    /// with this call; a blank value, and a body naming two of `value`, `always` and
    /// `endorsement`, are refused.
    type Wire = Tapping;

    fn parse(tapping: Tapping, _: &State) -> Result<Self, Refusal> {
        let Tapping {
            note,
            value,
            always,
            endorsement,
            fix,
        } = tapping;
        if fix {
            return if note.is_empty() && value.is_none() && !always && endorsement.is_none() {
                Ok(Answer::Fix)
            } else {
                Err(Refusal::bad_request("a fix's tap carries nothing else"))
            };
        }
        let endorsement = endorsement.as_ref().map(endorsement_of).transpose()?;
        // The store trims what it keeps: a blank value would reach it as nothing.
        if value
            .as_ref()
            .is_some_and(|value| value.expose().trim().is_empty())
        {
            return Err(Refusal::bad_request(
                "value is empty: there is nothing to keep",
            ));
        }
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
        Ok(Answer::Tap { note, answer })
    }
}

fn endorsement_of(endorsing: &suzatary_api::wire::Endorsing) -> Result<Endorsement, Refusal> {
    Ok(Endorsement {
        key: sign::PublicKey::new(&endorsing.key)
            .map_err(|e| Refusal::bad_request(format!("endorsement.key: {e}")))?,
        signature: sign::Signature::new(&endorsing.signature)
            .map_err(|e| Refusal::bad_request(format!("endorsement.signature: {e}")))?,
    })
}

impl Body for Reasoning {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(reasoning: Self, _: &State) -> Result<Self, Refusal> {
        Ok(reasoning)
    }
}

/// `POST …/proposals/<n>/tap`: perform one proposal, the one irreversible route.
pub(crate) fn tap(
    call: &Call<Tap, ProposalRef, Answer>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    let state = &call.state;
    let card = &call.params;
    let (id, index) = (&card.conversation, &card.index);
    // The door admits no tap where no chain keeps keys.
    let keyed = match state.keeping() {
        Keeping::Keyed(keyed) => Some(keyed.map(|chain| chain.store)),
        Keeping::Keyless => None,
    };
    let enrolling = state
        .keys
        .as_deref()
        .zip(keyed.as_ref())
        .map(
            |(keys, chains)| suzatary_signatures::signatures::Enrolling {
                chains,
                root: keys.root(),
            },
        );
    let started = std::time::Instant::now();
    let answered = state
        .editing(call.guard.who().as_str(), |edits| {
            state.with_cards(|caps, cards| {
                let effectors = Effectors {
                    post: state.postman.as_ref(),
                    reveal: state.reveal.as_ref(),
                    performer: state.acting.as_ref(),
                    secrets: state.secrets.as_ref(),
                    verify: state.verify.as_ref(),
                    checks: state.checks.as_ref(),
                    signing: state.signing.as_ref(),
                    edits,
                    forget: state.forget.as_ref(),
                    enrolling,
                    consents: state.consenting.as_ref(),
                };
                match &call.body {
                    Answer::Tap { note, answer } => secretary_routes::tap(
                        caps,
                        &effectors,
                        &call.guard,
                        cards,
                        card,
                        note,
                        answer,
                    ),
                    Answer::Fix => {
                        secretary_routes::fix(caps, &effectors, &call.guard, cards, card)
                    }
                }
            })
        })
        .inspect(|after| {
            state.heard(id);
            if let Some(opened) = &after.opened {
                state.heard(opened);
            }
            state.notice();
        });
    // A tap whose answer never reached the page is diagnosed from this line.
    let ms = started.elapsed().as_millis();
    match &answered {
        Ok(after) => {
            let now = after
                .conversation
                .proposals
                .iter()
                .find(|p| p.index == *index)
                .map(|p| p.state);
            tracing::info!(conversation = %id, index, state = ?now, ms, "tap");
        }
        Err(refusal) => tracing::warn!(conversation = %id, index, ms, refused = %refusal, "tap"),
    }
    answered.map(|after| Json(after.conversation))
}

/// `POST …/proposals/<n>/dismiss`: say no, and say why.
pub(crate) fn dismiss(
    call: &Call<Tap, ProposalRef, Reasoning>,
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

/// `POST …/proposals/<n>/acknowledge`: the owner checked Sent for a send that may have gone.
pub(crate) fn acknowledge(
    call: &Call<Tap, ProposalRef, ()>,
) -> Result<Json<wire::ConversationPayload>, Refusal> {
    let card = &call.params;
    call.state
        .with_cards(|caps, cards| secretary_routes::acknowledge(caps, &call.guard, cards, card))
        .map(Json)
}

/// `POST …/proposals/<n>/drop`: let a coding session go, with a required reason.
pub(crate) fn drop_session(
    call: &Call<Tap, ProposalRef, Reasoning>,
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
