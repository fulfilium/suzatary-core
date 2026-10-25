use crate::refusal::Refusal;

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
    /// The store could not be written; it is as it was.
    #[error("the {store} could not be written: {why}")]
    Unwritable {
        /// Which store.
        store: &'static str,
        /// What went wrong.
        why: String,
    },
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

impl From<crate::frontier::Unrecorded> for StoreError {
    fn from(failed: crate::frontier::Unrecorded) -> Self {
        Self::Unreadable {
            store: "recorded frontiers",
            why: failed.0,
        }
    }
}

impl From<crate::frontier::Unrecorded> for Refusal {
    fn from(failed: crate::frontier::Unrecorded) -> Self {
        StoreError::from(failed).into()
    }
}

impl From<StoreError> for Refusal {
    fn from(failed: StoreError) -> Self {
        match failed {
            StoreError::Absent { .. } => Refusal::not_found(failed.to_string()),
            StoreError::Taken { .. } => Refusal::conflict(failed.to_string()),
            StoreError::Unreadable { .. }
            | StoreError::NotAppended { .. }
            | StoreError::Unwritable { .. } => Refusal::internal(failed.to_string()),
        }
    }
}
