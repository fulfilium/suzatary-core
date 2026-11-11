//! Reading the web, for the secretary.
//!
//! Whether to ask before a request is the verb's decision over [`Web::allow_rules`],
//! never this trait's.

pub use crate::web::{AllowRule, WebRequest};

/// What one request answered, already text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAnswer {
    /// The URL that was requested.
    pub url: String,
    /// The HTTP status.
    pub status: u16,
    /// The page as text, the search results listed, or where a redirect
    /// pointed; capped, and saying so when cut.
    pub text: String,
}

/// Why a request produced no page.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebFailure {
    /// The network, DNS, TLS or a timeout.
    #[error("unreachable: {0}")]
    Unreachable(String),
    /// Refused before connecting: the name resolved to this box's network.
    #[error("refused: {0}")]
    Refused(String),
}

/// The web, as the secretary may read it.
pub trait Web: Sync {
    /// The rules that let a request run without a tap, read fresh each time
    /// so an "Always allow" counts at once.
    fn allow_rules(&self) -> Vec<AllowRule>;

    /// Perform a request the caller has already decided may go out.
    ///
    /// # Errors
    /// [`WebFailure`], in the fetcher's own words.
    fn perform(&self, request: &WebRequest) -> Result<WebAnswer, WebFailure>;
}
