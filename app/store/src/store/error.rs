use suzatary_base::refusal::Refusal;

/// A store could not do what was asked, naming the store in its own words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("no {what} named {name}")]
    Absent {
        what: &'static str,
        /// The name asked for.
        name: String,
    },
    #[error("a {what} named {name} already exists")]
    Taken {
        what: &'static str,
        /// The name asked for.
        name: String,
    },
    #[error("the {store} could not be read: {why}")]
    Unreadable {
        store: &'static str,
        /// What went wrong.
        why: String,
    },
    /// The prodrome is as it was.
    #[error("the {store} could not be appended to: {why}")]
    NotAppended {
        store: &'static str,
        /// What went wrong.
        why: String,
    },
    /// The door refuses the object itself, so offered again it is refused again.
    #[error("the {store} refuses it: {why}")]
    Refused {
        store: &'static str,
        /// Why, in the door's words.
        why: String,
    },
    /// The store is as it was.
    #[error("the {store} could not be written: {why}")]
    Unwritable {
        store: &'static str,
        /// What went wrong.
        why: String,
    },
}

/// Why a store's door does not take an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shut {
    Refused(String),
    /// The door could not judge it, so it holds nothing it has not judged.
    Unjudged(String),
}

/// A store's rule for one event, run before the store holds it.
pub type Rule<E> = Box<dyn Fn(&E) -> Result<(), Shut> + Send + Sync>;

impl Shut {
    #[must_use]
    pub fn of(self, name: &str) -> Shut {
        match self {
            Shut::Refused(why) => Shut::Refused(format!("{name}: {why}")),
            Shut::Unjudged(why) => Shut::Unjudged(format!("{name}: {why}")),
        }
    }

    #[must_use]
    pub fn error(self, store: &'static str) -> StoreError {
        match self {
            Shut::Refused(why) => StoreError::Refused { store, why },
            Shut::Unjudged(why) => StoreError::not_appended(store, why),
        }
    }
}

impl StoreError {
    pub fn unreadable(store: &'static str, why: impl std::fmt::Display) -> Self {
        Self::Unreadable {
            store,
            why: why.to_string(),
        }
    }

    pub fn not_appended(store: &'static str, why: impl std::fmt::Display) -> Self {
        Self::NotAppended {
            store,
            why: why.to_string(),
        }
    }

    pub fn unwritable(store: &'static str, why: impl std::fmt::Display) -> Self {
        Self::Unwritable {
            store,
            why: why.to_string(),
        }
    }
}

impl From<prodrome_store::store::frontier::Unrecorded> for StoreError {
    fn from(failed: prodrome_store::store::frontier::Unrecorded) -> Self {
        Self::Unreadable {
            store: "recorded frontiers",
            why: failed.0,
        }
    }
}

impl From<prodrome_store::store::frontier::Unanswered> for StoreError {
    fn from(failed: prodrome_store::store::frontier::Unanswered) -> Self {
        match failed {
            prodrome_store::store::frontier::Unanswered::Unrecorded(failed) => failed.into(),
            prodrome_store::store::frontier::Unanswered::Unordered(why) => {
                Self::unreadable("the store's order", why)
            }
        }
    }
}

impl From<StoreError> for Refusal {
    fn from(failed: StoreError) -> Self {
        match failed {
            StoreError::Absent { .. } => Refusal::not_found(failed.to_string()),
            StoreError::Taken { .. } => Refusal::conflict(failed.to_string()),
            StoreError::Refused { .. } => Refusal::forbidden(failed.to_string()),
            StoreError::Unreadable { .. }
            | StoreError::NotAppended { .. }
            | StoreError::Unwritable { .. } => Refusal::internal(failed.to_string()),
        }
    }
}
