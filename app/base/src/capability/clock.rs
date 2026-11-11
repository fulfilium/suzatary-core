//! The wall clock, as a capability rather than a call.

use super::Datetime;

/// The wall clock, taken as a capability so a test can fold at a fixed instant.
pub trait Clock: Sync {
    /// Now, as the Prodrome spells an instant: naive, and UTC by definition.
    fn now(&self) -> Datetime;
}
