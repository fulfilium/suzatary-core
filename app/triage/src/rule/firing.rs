#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Firing {
    rule: &'static str,
    key: String,
    on: crate::rule::ReadingName,
}

impl Firing {
    #[must_use]
    pub fn new(rule: &'static str, key: String, on: crate::rule::ReadingName) -> Self {
        Self { rule, key, on }
    }

    #[must_use]
    pub fn rule(&self) -> &'static str {
        self.rule
    }

    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

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
