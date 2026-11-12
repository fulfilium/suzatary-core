//! A new todo's fields.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::todo::Price;

/// Everything a writer says about a new todo: `body` and `detail` as markup `build_todo` admits
/// through the door, the rest as plain text it escapes. The serde form, whose key for the price is
/// `priority`, is a verb's argument, and an `add` act's as stored before acts were literals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TodoFields {
    /// The action line.
    pub body: String,
    /// Why that price, in one sentence: the only record of the reasoning.
    pub rationale: String,
    /// The block kind, `todo` when a writer names none.
    #[serde(default = "plain_kind")]
    #[ts(as = "Option<String>", optional)]
    pub kind: String,
    /// One of this deployment's categories, "" for a kind that sets its own.
    pub category: String,
    /// What it is worth.
    #[serde(rename = "priority")]
    pub price: Price,
    /// The detail, "" for none.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub detail: String,
    /// What it is blocked on, "" for nothing.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub waiting_on: String,
}

/// The kind a todo is when its writer names none.
#[must_use]
pub fn plain_kind() -> String {
    "todo".to_owned()
}
