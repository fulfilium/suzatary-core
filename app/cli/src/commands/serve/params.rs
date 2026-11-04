//! A route's parameters, parsed once out of its path and query string.

use std::collections::HashMap;

use axum::extract::RawPathParams;
use suzatary_core::refusal::Refusal;

use super::Chained;

/// A route's typed parameters.
pub trait Params: Sized + Send + 'static {
    /// # Errors
    /// A [`Refusal`] naming the parameter that did not parse.
    fn parse(asked: &Asked, chain: &Chained) -> Result<Self, Refusal>;
}

impl Params for () {
    fn parse(_: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(())
    }
}

/// A request's path segments and query string, as text.
pub struct Asked {
    segments: Vec<(String, String)>,
    query: HashMap<String, String>,
}

impl Asked {
    #[must_use]
    pub fn of(segments: &RawPathParams, query: HashMap<String, String>) -> Self {
        Self {
            segments: segments
                .iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            query,
        }
    }

    /// The chain a path named; absent on a bare path, which means the first chain.
    #[must_use]
    pub fn chain(&self) -> Option<&str> {
        self.segments
            .iter()
            .find(|(name, _)| name == "chain")
            .map(|(_, value)| value.as_str())
    }

    /// # Errors
    /// [`Refusal::internal`] when the pattern has no such segment, since tables get edited.
    pub fn segment(&self, key: &str) -> Result<&str, Refusal> {
        self.segments
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| Refusal::internal(format!("this route matched without a {key} segment")))
    }

    #[must_use]
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(String::as_str)
    }
}
