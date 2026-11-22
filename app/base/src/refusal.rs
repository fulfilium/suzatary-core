//! A refusal every route returns; the axum layer turns it into [`crate::wire::base::ErrorPayload`].
//!
//! The frontend shows the sentence verbatim and tests assert on it, so it names the wrong field.

use std::fmt;

use crate::wire::base::ErrorPayload;

/// A closed set rather than a `u16`, so only a status somebody chose can be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// The request is malformed; refused before anything touches the chain.
    BadRequest,
    /// The gate said no; an unknown caller is refused, never passed as anonymous.
    Forbidden,
    /// Nothing here; a page shows an empty result the same way, and it means the opposite.
    NotFound,
    /// The right resource, the wrong method: a GET under `/api/act/`.
    MethodNotAllowed,
    /// The request contradicts the chain (an id already taken, a `base` this store never held).
    Conflict,
    /// A real resource whose own content refuses, such as Typst that will not compile.
    /// The page also refuses a tab store's lines for good on this status alone, so a box of
    /// another build answers its own 400 instead.
    UnprocessableEntity,
    TooLarge,
    /// Past a per-login budget (a browser's error reports, `crate::clienterror`).
    TooManyRequests,
    /// The page writes by a contract older than `crate::signatures::CLIENT_MIN`; nothing is done.
    /// The page offers the box's build and sends the act again after a reload.
    UpgradeRequired,
    /// A post this box's build does not read; the page posts it again once the builds agree.
    OtherBuild,
    /// A request signed by a revoked device key: the device deletes what it holds.
    Wipe,
    Internal,
}

impl Status {
    #[must_use]
    pub fn code(self) -> u16 {
        match self {
            Status::BadRequest => 400,
            Status::Forbidden => 403,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::Conflict | Status::OtherBuild => 409,
            Status::UnprocessableEntity => 422,
            Status::TooLarge => 413,
            Status::TooManyRequests => 429,
            Status::UpgradeRequired => 426,
            Status::Wipe => 410,
            Status::Internal => 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    status: Status,
    message: String,
}

impl Refusal {
    /// A refusal with a status picked from a value; routes otherwise use the named constructors.
    #[must_use]
    pub fn new(status: Status, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(Status::BadRequest, message)
    }

    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(Status::Forbidden, message)
    }

    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(Status::NotFound, message)
    }

    #[must_use]
    pub fn method_not_allowed(message: impl Into<String>) -> Self {
        Self::new(Status::MethodNotAllowed, message)
    }

    #[must_use]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(Status::Conflict, message)
    }

    #[must_use]
    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(Status::UnprocessableEntity, message)
    }

    #[must_use]
    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(Status::TooLarge, message)
    }

    #[must_use]
    pub fn too_many(message: impl Into<String>) -> Self {
        Self::new(Status::TooManyRequests, message)
    }

    #[must_use]
    pub fn wipe(message: impl Into<String>) -> Self {
        Self::new(Status::Wipe, message)
    }

    #[must_use]
    pub fn other_build(message: impl Into<String>) -> Self {
        Self::new(Status::OtherBuild, message)
    }

    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(Status::Internal, message)
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }

    /// Its sentence, shown to the reader verbatim.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            error: self.message.clone(),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.status.code(), self.message)
    }
}

impl std::error::Error for Refusal {}

pub type Answered<T> = Result<T, Refusal>;

/// A value quoted as `CPython`'s `repr` quotes a plain string: single quotes, unless the text
/// holds one and no double quote.
/// The quotes are pinned in refusal sentences; control characters are not escaped.
#[must_use]
pub fn shown(text: &str) -> String {
    if text.contains('\'') && !text.contains('"') {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\'', "\\'"))
    }
}

/// A JSON value's type in `CPython`'s `type(x).__name__` vocabulary, which pinned refusal
/// sentences quote (`NoneType`, not `null`).
#[must_use]
pub fn json_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "NoneType",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(n) if n.is_f64() => "float",
        serde_json::Value::Number(_) => "int",
        serde_json::Value::String(_) => "str",
        serde_json::Value::Array(_) => "list",
        serde_json::Value::Object(_) => "dict",
    }
}
