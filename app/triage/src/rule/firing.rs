//! One firing: which rule, about what, on which reading.

/// A rule fired on a reading of one key: what the box did and why, as a log line says it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Firing {
    rule: &'static str,
    key: String,
    on: crate::rule::ReadingName,
}

impl Firing {
    /// `rule` on the reading `on` of `key`.
    #[must_use]
    pub fn new(rule: &'static str, key: String, on: crate::rule::ReadingName) -> Self {
        Self { rule, key, on }
    }

    /// The rule's name.
    #[must_use]
    pub fn rule(&self) -> &'static str {
        self.rule
    }

    /// The key it fired about.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The reading it fired on.
    #[must_use]
    pub fn on(&self) -> &crate::rule::ReadingName {
        &self.on
    }
}

impl std::fmt::Display for Firing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} fired on {} at {}", self.rule, self.key, self.on)
    }
}
