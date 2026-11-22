//! The request an act makes of the owner: everything it would write except its pricing, the price
//! and the rationale that explains it. A writer re-reads a price off the date on every pass (a
//! deadline nearing moves 62% to 61%), so two acts that differ only in their pricing are the same
//! request, and a triage card is known by its acts' requests (`suzatary_topics::secretary::filed`).

use chrono::NaiveDate;

use crate::secretary::changes::{Act, Provenance};
use suzatary_record::record::SubTodo;
use suzatary_record::todo::{Patch, TodoFields, TodoPatch};

#[derive(Debug, PartialEq, Eq)]
pub enum Request<'a> {
    /// A new todo, whatever it is priced at.
    Add {
        todo: &'a str,
        body: &'a str,
        kind: &'a str,
        category: &'a str,
        detail: &'a str,
        waiting_on: &'a str,
        /// The message, the day and the parent.
        from: &'a Provenance,
    },
    /// A change to a todo: the fields it names, and whether it moves the pricing at all.
    Update {
        todo: &'a str,
        body: &'a Patch<String>,
        detail: &'a Patch<String>,
        /// A paragraph under the detail.
        detail_append: &'a Patch<String>,
        waiting_on: &'a Patch<String>,
        clear_waiting_on: bool,
        category: &'a Patch<String>,
        subtodos: &'a Patch<Vec<SubTodo>>,
        defer_to: &'a Patch<NaiveDate>,
        /// Whether it reprices, to whatever.
        pricing: Pricing,
    },
    /// A new price for a todo, whatever it is.
    Reprice { todo: &'a str },
    /// An act that carries no pricing, which is its own request.
    Unpriced(&'a Act),
}

/// Whether an update names a price or a rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pricing {
    Kept,
    Moved,
}

impl Act {
    #[must_use]
    pub fn request(&self) -> Request<'_> {
        match self {
            Act::Add { todo, fields, from } => {
                let TodoFields {
                    body,
                    rationale: _,
                    kind,
                    category,
                    price: _,
                    detail,
                    waiting_on,
                } = fields;
                Request::Add {
                    todo,
                    body,
                    kind,
                    category,
                    detail,
                    waiting_on,
                    from,
                }
            }
            Act::Update { todo, fields } => {
                let TodoPatch {
                    body,
                    detail,
                    detail_append,
                    rationale,
                    price,
                    waiting_on,
                    clear_waiting_on,
                    category,
                    subtodos,
                    defer_to,
                } = fields;
                Request::Update {
                    todo,
                    body,
                    detail,
                    detail_append,
                    waiting_on,
                    clear_waiting_on: *clear_waiting_on,
                    category,
                    subtodos,
                    defer_to,
                    pricing: if price.is_keep() && rationale.is_keep() {
                        Pricing::Kept
                    } else {
                        Pricing::Moved
                    },
                }
            }
            Act::Reprice {
                todo,
                priority: _,
                rationale: _,
            } => Request::Reprice { todo },
            Act::Complete { .. } | Act::Cancel { .. } | Act::Reopen { .. } | Act::Tend { .. } => {
                Request::Unpriced(self)
            }
        }
    }
}
