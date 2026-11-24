//! A plugin's items learned as the calendar keeps them: the host's side of a plugin's door.

use prodrome_declared::Declared;
use suzatary_store::store::{Learnt, Prodrome, StoreError, learn};

use super::Occurrence;

/// What a store learned of a plugin's items, and how many it left out unread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    pub learnt: Learnt,
    /// Items the calendar cannot read as an occurrence, which no store learns.
    pub unread: usize,
}

/// Learn each of `items` into `store` as [`Occurrence::kept`] prints it, so an attendee whose name
/// is not an `AttendeeName` is counted and never stored, whatever the plugin wrote. An item the
/// calendar cannot read is left out and counted, so one odd item does not stop the rest.
///
/// # Errors
/// The store's refusal.
pub fn learn_kept(store: &dyn Prodrome<Declared>, items: &[Declared]) -> Result<Kept, StoreError> {
    let kept: Vec<Declared> = items
        .iter()
        .filter_map(|item| Occurrence::kept(item).ok())
        .collect();
    let unread = items.len() - kept.len();
    Ok(Kept {
        learnt: learn(store, kept)?,
        unread,
    })
}
