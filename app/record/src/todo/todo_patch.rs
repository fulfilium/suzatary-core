//! A change to a todo that exists.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::record::SubTodo;
use crate::todo::{Patch, Price};

/// The fields a change names, each kept unless set, so a writer that forgot a field cannot blank
/// it. The serde form is an `update` act's as stored before acts were literals: its seven fields
/// print even when kept, and the three only the app and the CLI set are omitted when kept.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct TodoPatch {
    /// A new action line, never blank.
    #[ts(as = "Option<String>", optional)]
    pub body: Patch<String>,
    /// A new detail; `""` clears it. Exclusive with `detail_append`.
    #[ts(as = "Option<String>", optional)]
    pub detail: Patch<String>,
    /// A paragraph added under the detail, keeping the markup already there.
    #[ts(as = "Option<String>", optional)]
    pub detail_append: Patch<String>,
    /// Why the price is what it is, required whenever the price moves.
    #[ts(as = "Option<String>", optional)]
    pub rationale: Patch<String>,
    /// A new price.
    #[serde(rename = "priority")]
    #[ts(as = "Option<Price>", optional)]
    pub price: Patch<Price>,
    /// A new blocker.
    #[ts(as = "Option<String>", optional)]
    pub waiting_on: Patch<String>,
    /// Set the blocker back to nothing; it wins over `waiting_on`.
    #[ts(as = "Option<bool>", optional)]
    pub clear_waiting_on: bool,
    /// A new category, one of this deployment's.
    #[serde(skip_serializing_if = "Patch::is_keep")]
    #[ts(as = "Option<String>", optional)]
    pub category: Patch<String>,
    /// A new checklist, whole: ticking one line is the same list with one line flipped.
    #[serde(skip_serializing_if = "Patch::is_keep")]
    #[ts(as = "Option<Vec<crate::wire::record::SubTodo>>", optional)]
    pub subtodos: Patch<Vec<SubTodo>>,
    /// Nothing owed before this date, then what it was. Exclusive with `price`.
    #[serde(skip_serializing_if = "Patch::is_keep")]
    #[ts(as = "Option<String>", optional)]
    pub defer_to: Patch<NaiveDate>,
}

impl TodoPatch {
    /// Whether it names nothing, which is not a request.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == TodoPatch::default()
    }
}
