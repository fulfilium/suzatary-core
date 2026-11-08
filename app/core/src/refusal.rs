//! A request this application declines to do work for: a value every route returns, turned by
//! the axum layer into the one body shape the frontend reads ([`crate::wire::ErrorPayload`]).
//!
//! The sentence is the contract: the frontend shows it verbatim and tests assert on it, so a
//! refusal names which field was wrong rather than saying something friendlier.

use std::fmt;

use crate::wire::ErrorPayload;

/// The HTTP status a refusal carries: a closed set rather than a `u16`, so only a status
/// somebody decided to answer with can be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// The request is malformed; refused before anything touches the chain.
    BadRequest,
    /// The gate said no: not a viewer, not an actor, or missing the CSRF triple. An unknown
    /// caller is refused, never passed as anonymous.
    Forbidden,
    /// Nothing here; distinct from an empty result, which looks identical on a page and means
    /// the opposite.
    NotFound,
    /// The right resource, the wrong method: a GET under `/api/act/`.
    MethodNotAllowed,
    /// The request contradicts the chain (an id already taken, a `base` this store never held).
    Conflict,
    /// The request named a real resource whose own content refuses (Typst that will not
    /// compile); unlike [`Status::BadRequest`], the fault is not the request's.
    UnprocessableEntity,
    /// Past a per-login budget (a browser's error reports, `crate::clienterror`).
    TooManyRequests,
    /// A page whose build writes by a contract older than the box takes (`crate::signatures::CLIENT_MIN`):
    /// nothing is performed, and the page offers the box's build, to be sent again after Reload.
    UpgradeRequired,
    /// A request signed by a device key its history revoked: the device deletes what it holds
    /// (the tab's `wipe`), and asks again as a new device if its person wants it back.
    Wipe,
    /// This application failed, not the caller.
    Internal,
}

impl Status {
    /// The status as an HTTP code.
    #[must_use]
    pub fn code(self) -> u16 {
        match self {
            Status::BadRequest => 400,
            Status::Forbidden => 403,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::Conflict => 409,
            Status::UnprocessableEntity => 422,
            Status::TooManyRequests => 429,
            Status::UpgradeRequired => 426,
            Status::Wipe => 410,
            Status::Internal => 500,
        }
    }
}

/// One refusal: a status and the server's own words for it.
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

    /// 400 — the request is malformed.
    #[must_use]
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(Status::BadRequest, message)
    }

    /// 403 — the gate, or the CSRF triple.
    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(Status::Forbidden, message)
    }

    /// 404 — nothing here.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(Status::NotFound, message)
    }

    /// 405 — the right path, the wrong method.
    #[must_use]
    pub fn method_not_allowed(message: impl Into<String>) -> Self {
        Self::new(Status::MethodNotAllowed, message)
    }

    /// 409 — the request contradicts the chain.
    #[must_use]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(Status::Conflict, message)
    }

    /// 422 — the resource was found; its own content refuses. `typst`'s
    /// message, quoted.
    #[must_use]
    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(Status::UnprocessableEntity, message)
    }

    /// 429 — past a per-login budget.
    #[must_use]
    pub fn too_many(message: impl Into<String>) -> Self {
        Self::new(Status::TooManyRequests, message)
    }

    /// 410 — the request was signed by a revoked device key, which deletes what it holds.
    #[must_use]
    pub fn wipe(message: impl Into<String>) -> Self {
        Self::new(Status::Wipe, message)
    }

    /// 500 — this application failed.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(Status::Internal, message)
    }

    /// Its status.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }

    /// Its sentence, shown to the reader verbatim.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The body every refusal answers with.
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

/// What a route answers with.
pub type Answered<T> = Result<T, Refusal>;

/// A value quoted as `CPython`'s `repr` quotes a plain string: single quotes, unless the text
/// holds one and no double quote.
///
/// The quotes are part of pinned refusal sentences, so `{:?}` would not do. Control characters
/// and non-ASCII are not escaped: the values quoted have already failed a character class.
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
