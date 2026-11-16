use suzatary_base::refusal::Refusal;

/// A store could not do what was asked, naming the store in its own words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// Nothing by that name.
    #[error("no {what} named {name}")]
    Absent {
        /// What the store holds, one of.
        what: &'static str,
        /// The name asked for.
        name: String,
    },
    /// One by that name exists, and making it again would be an edit.
    #[error("a {what} named {name} already exists")]
    Taken {
        /// What the store holds, one of.
        what: &'static str,
        /// The name asked for.
        name: String,
    },
    /// The store could not be read.
    #[error("the {store} could not be read: {why}")]
    Unreadable {
        /// Which store.
        store: &'static str,
        /// What went wrong.
        why: String,
    },
    /// An object could not be added to a prodrome; it is as it was.
    #[error("the {store} could not be appended to: {why}")]
    NotAppended {
        /// Which store.
        store: &'static str,
        /// What went wrong.
        why: String,
    },
    /// The store's door refuses the object itself, so offered again it is refused again; the
    /// store is as it was.
    #[error("the {store} refuses it: {why}")]
    Refused {
        /// Which store.
        store: &'static str,
        /// Why, in the door's words.
        why: String,
    },
    /// The store could not be written; it is as it was.
    #[error("the {store} could not be written: {why}")]
    Unwritable {
        /// Which store.
        store: &'static str,
        /// What went wrong.
        why: String,
    },
}

/// Why a store's door does not take an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shut {
    /// The object itself: offered again, it is refused again.
    Refused(String),
    /// The door could not judge it, so it holds nothing it has not judged.
    Unjudged(String),
}

/// A store's rule for one event, run before the store holds it.
pub type Rule<E> = Box<dyn Fn(&E) -> Result<(), Shut> + Send + Sync>;

impl Shut {
    /// The same refusal, said of the object `name`.
    #[must_use]
    pub fn of(self, name: &str) -> Shut {
        match self {
            Shut::Refused(why) => Shut::Refused(format!("{name}: {why}")),
            Shut::Unjudged(why) => Shut::Unjudged(format!("{name}: {why}")),
        }
    }

    /// The refusal as `store`'s error: [`StoreError::Refused`], or a store that took nothing.
    #[must_use]
    pub fn error(self, store: &'static str) -> StoreError {
        match self {
            Shut::Refused(why) => StoreError::Refused { store, why },
            Shut::Unjudged(why) => StoreError::not_appended(store, why),
        }
    }
}

impl StoreError {
    /// An unreadable `store`, for any failure that says why.
    pub fn unreadable(store: &'static str, why: impl std::fmt::Display) -> Self {
        Self::Unreadable {
            store,
            why: why.to_string(),
        }
    }

    /// A prodrome `store` that took nothing, for any failure that says why.
    pub fn not_appended(store: &'static str, why: impl std::fmt::Display) -> Self {
        Self::NotAppended {
            store,
            why: why.to_string(),
        }
    }

    /// An unwritable `store`, for any failure that says why.
    pub fn unwritable(store: &'static str, why: impl std::fmt::Display) -> Self {
        Self::Unwritable {
            store,
            why: why.to_string(),
        }
    }
}

impl From<prodrome_core::store::frontier::Unrecorded> for StoreError {
    fn from(failed: prodrome_core::store::frontier::Unrecorded) -> Self {
        Self::Unreadable {
            store: "recorded frontiers",
            why: failed.0,
        }
    }
}

impl From<prodrome_core::store::frontier::Unanswered> for StoreError {
    fn from(failed: prodrome_core::store::frontier::Unanswered) -> Self {
        match failed {
            prodrome_core::store::frontier::Unanswered::Unrecorded(failed) => failed.into(),
            prodrome_core::store::frontier::Unanswered::Unordered(why) => {
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
