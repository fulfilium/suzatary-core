//! A route's request body, capped, read by serde as its wire type, and parsed once.

use serde::de::DeserializeOwned;
use suzatary_base::refusal::Refusal;

use super::State;

/// A route's typed body.
pub trait Body: Sized + Send + 'static {
    /// The most bytes read.
    const CAP: usize;

    /// The body as the wire spells it: a JSON object whose every field it names, refusing any
    /// other.
    type Wire: DeserializeOwned;

    /// The body, out of its wire form.
    ///
    /// # Errors
    /// A [`Refusal::bad_request`] naming the field that did not parse.
    fn parse(wire: Self::Wire, state: &State) -> Result<Self, Refusal>;

    /// Read, cap and parse the request's body.
    ///
    /// # Errors
    /// A [`Refusal::bad_request`] for a body over the cap, not JSON, not an object, not
    /// [`Body::Wire`], or refused by [`Body::parse`].
    fn read(
        body: axum::body::Body,
        state: &State,
    ) -> impl Future<Output = Result<Self, Refusal>> + Send {
        async move {
            let object = object(body, Self::CAP).await?;
            let wire = serde_json::from_value(object)
                .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
            Self::parse(wire, state)
        }
    }
}

/// A write's body is an action line, not a document.
pub const MAX_ACT_BYTES: usize = 16_384;

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

/// The body, capped and parsed as one JSON object.
async fn object(body: axum::body::Body, cap: usize) -> Result<serde_json::Value, Refusal> {
    let bytes = axum::body::to_bytes(body, cap)
        .await
        .map_err(|_| Refusal::bad_request(format!("the body is capped at {cap} bytes")))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| Refusal::bad_request("the body must be JSON"))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(Refusal::bad_request(format!(
            "the body must be a JSON object, got {}",
            suzatary_base::refusal::json_type(&value)
        )))
    }
}
