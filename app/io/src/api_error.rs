//! The failure both model APIs answer with: `{"error": {"message": …}}`, from the Messages
//! API's error status and stream `error` event alike, and from an OpenAI-compatible endpoint.

use serde::Deserialize;

/// An error answer's body.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApiError {
    pub error: Said,
}

/// The endpoint's own words for a failure.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Said {
    pub message: String,
}

/// The endpoint's message, or the whole body when it answered in another shape.
#[must_use]
pub fn message_of(body: &str) -> String {
    serde_json::from_str::<ApiError>(body).map_or_else(|_| body.to_owned(), |e| e.error.message)
}
