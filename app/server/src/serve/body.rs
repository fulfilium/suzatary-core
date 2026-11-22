//! A route's request body, capped, read by serde as its wire type, and parsed once.

use serde::de::DeserializeOwned;
use suzatary_base::refusal::Refusal;

use super::State;

/// A route's typed body.
pub(crate) trait Body: Sized + Send + 'static {
    /// The most bytes read.
    const CAP: usize;

    /// The body as the wire spells it: a JSON object whose every field it names, refusing any
    /// other.
    type Wire: DeserializeOwned;

    fn parse(wire: Self::Wire, state: &State) -> Result<Self, Refusal>;

    /// Read, cap and parse the request's body; a [`Refusal::bad_request`] for any fault.
    fn read(
        body: axum::body::Body,
        state: &State,
    ) -> impl Future<Output = Result<Self, Refusal>> + Send {
        async move {
            let object = object(body, Self::CAP)
                .await
                .map_err(NotAnObject::refusal)?;
            let wire = serde_json::from_value(object)
                .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
            Self::parse(wire, state)
        }
    }
}

/// A write's body is an action line, not a document.
pub(crate) const MAX_ACT_BYTES: usize = 16_384;

/// A route that reads no body: whatever the request carries is left unread, though the door
/// still reads up to an action line of it to check a signature over its bytes.
impl Body for () {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = ();

    fn parse((): (), _: &State) -> Result<Self, Refusal> {
        Ok(())
    }

    fn read(_: axum::body::Body, _: &State) -> impl Future<Output = Result<Self, Refusal>> + Send {
        std::future::ready(Ok(()))
    }
}

/// Why a body is no capped JSON object: each route says which status each answers with.
#[derive(Debug)]
pub(super) enum NotAnObject {
    /// Over the cap, in bytes.
    Over(usize),
    NotJson,
    /// JSON of another type, named.
    Other(&'static str),
}

impl NotAnObject {
    /// The sentence.
    pub(super) fn said(&self) -> String {
        match self {
            NotAnObject::Over(cap) => format!("the body is capped at {cap} bytes"),
            NotAnObject::NotJson => "the body must be JSON".to_owned(),
            NotAnObject::Other(kind) => format!("the body must be a JSON object, got {kind}"),
        }
    }

    /// A [`Refusal::bad_request`], as most routes answer it.
    pub(super) fn refusal(self) -> Refusal {
        Refusal::bad_request(self.said())
    }
}

/// The body, capped and parsed as one JSON object.
pub(super) async fn object(
    body: axum::body::Body,
    cap: usize,
) -> Result<serde_json::Value, NotAnObject> {
    let bytes = axum::body::to_bytes(body, cap)
        .await
        .map_err(|_| NotAnObject::Over(cap))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| NotAnObject::NotJson)?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(NotAnObject::Other(suzatary_base::refusal::json_type(
            &value,
        )))
    }
}
