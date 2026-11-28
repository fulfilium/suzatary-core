//! The calendars an account lists, as every provider's list reads into them.

/// One calendar an account lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    /// Whether it is the account's own.
    pub primary: bool,
}

impl Listed {
    /// A calendar, unless its id is empty or too long for a name's digest to stand for it.
    #[must_use]
    pub fn new(id: &str, primary: bool) -> Option<Self> {
        (!id.is_empty() && id.len() <= 500).then(|| Self {
            id: id.to_owned(),
            primary,
        })
    }
}

/// The calendars an account lists, its own first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendars(Vec<Listed>);

impl Calendars {
    #[must_use]
    pub fn of(mut listed: Vec<Listed>) -> Self {
        listed.sort_by_key(|calendar| !calendar.primary);
        Self(listed)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Listed> {
        self.0.iter()
    }
}
