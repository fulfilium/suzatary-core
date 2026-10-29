//! `POST /api/held/reveal`: one held value (`suzatary_core::held_value`), a number or a file, for
//! the owner's tap.
//!
//! The answer carries the value or the file once. The box keeps only `{"type":"NotKept"}` for a
//! replay of the request, and nothing logs, stores or pushes the value.

use axum::Json;
use axum::response::{IntoResponse as _, Response};
use suzatary_core::held_value::{Handle, Holder, Holders, Place};
use suzatary_core::refusal::Refusal;
use suzatary_core::secretary::opening::Opening;
use suzatary_core::secretary::read_holders::ReadHolders;
use suzatary_core::secretary::store::ConversationId;
use suzatary_core::secretary::tap::Tap;
use suzatary_core::wire::{self, RevealPayload};

use super::State;
use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::reply::{Keeps, Outcome};

/// What the box keeps for a replay of a reveal, in the value's place.
pub const NOT_KEPT: &str = r#"{"type":"NotKept"}"#;

/// What a reveal names.
pub enum Revealing {
    /// A number in a message: the owner's own mail, which a mail page shows.
    InMail(Place),
    /// A file, or a number in one, which only a conversation that read the file reveals: a
    /// handle carries a file's source, so a model could write one naming any file.
    Read {
        /// The handle.
        handle: Handle,
        /// The conversation the placeholder is drawn in.
        conversation: ConversationId,
    },
}

impl Body for Revealing {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = wire::Revealing;

    fn parse(revealing: wire::Revealing, _: &State) -> Result<Self, Refusal> {
        let handle = Handle::parse(&revealing.handle)
            .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        match (handle, revealing.conversation) {
            (Handle::Value(place), _) if matches!(place.holder(), Holder::Message(_)) => {
                Ok(Self::InMail(place))
            }
            (handle, Some(conversation)) => Ok(Self::Read {
                handle,
                conversation: ConversationId::new(conversation)
                    .map_err(|refused| Refusal::bad_request(refused.to_string()))?,
            }),
            (_, None) => Err(Refusal::bad_request(
                "a file's handle is revealed only in the conversation that read the file; name it",
            )),
        }
    }
}

/// A reveal's answer, which the box never keeps.
pub struct Revealed(Result<RevealPayload, Refusal>);

impl Outcome for Revealed {
    const KEEPS: Keeps = Keeps::Instead(NOT_KEPT);

    fn outcome(self) -> Result<Response, Refusal> {
        self.0.map(|payload| Json(payload).into_response())
    }
}

/// `POST /api/held/reveal`: the value or file a handle names, read again from what holds it, for
/// a trusted device of an actor the gate maps.
pub fn reveal(call: &Call<Tap, (), Revealing>) -> Revealed {
    Revealed(revealed(call))
}

fn revealed(call: &Call<Tap, (), Revealing>) -> Result<RevealPayload, Refusal> {
    let caps = &call.state.first.caps.secretary;
    let (handle, read) = match &call.body {
        Revealing::InMail(place) => (Handle::Value(place.clone()), ReadHolders::default()),
        Revealing::Read {
            handle,
            conversation,
        } => {
            let read = ReadHolders::of(&suzatary_core::secretary::read(
                caps.conversations,
                conversation,
            )?);
            if !read.holds(handle) {
                return Err(Refusal::forbidden(
                    "that conversation never read the file this handle names, so it is not shown",
                ));
            }
            (handle.clone(), read)
        }
    };
    let opening = Opening { caps, read: &read };
    let holders = Holders {
        mail: call.state.reveal.as_ref(),
        files: &opening,
    };
    holders
        .read(&handle)
        .map_err(|failed| Refusal::internal(failed.to_string()))
        .and_then(RevealPayload::of)
}
