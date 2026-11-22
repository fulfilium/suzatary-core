//! What one try came to: a success, or a failure of a class the box parsed where it saw it.

use serde::{Deserialize, Serialize};

/// A success, or a failure of class `C`; its stored form is `"succeeded"` or `{"failed": C}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<C> {
    Succeeded,
    Failed(C),
}

impl<C> Outcome<C> {
    #[must_use]
    pub fn map<D>(self, f: impl FnOnce(C) -> D) -> Outcome<D> {
        match self {
            Outcome::Succeeded => Outcome::Succeeded,
            Outcome::Failed(class) => Outcome::Failed(f(class)),
        }
    }
}
