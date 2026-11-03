//! The request an act makes of the owner: everything it would write except its pricing, the price
//! and the rationale that explains it. A writer re-reads a price off the date on every pass (a
//! deadline nearing moves 62% to 61%), so two acts that differ only in their pricing are the same
//! request, and a triage card is known by its acts' requests ([`crate::secretary::filed`]).

use chrono::NaiveDate;

use crate::record::SubTodo;
use crate::secretary::changes::{Act, Provenance};
use crate::todo::{Patch, TodoFields, TodoPatch};

/// An act with its pricing left out.
#[derive(Debug, PartialEq, Eq)]
pub enum Request<'a> {
    /// A new todo, whatever it is priced at.
    Add {
        /// Its id.
        todo: &'a str,
        /// The action line.
        body: &'a str,
        /// The block kind.
        kind: &'a str,
        /// The category.
        category: &'a str,
        /// The detail.
        detail: &'a str,
        /// The blocker.
        waiting_on: &'a str,
        /// The message, the day and the parent.
        from: &'a Provenance,
    },
    /// A change to a todo: the fields it names, and whether it moves the pricing at all.
    Update {
        /// Which todo.
        todo: &'a str,
        /// A new action line.
        body: &'a Patch<String>,
        /// A new detail.
        detail: &'a Patch<String>,
        /// A paragraph under the detail.
        detail_append: &'a Patch<String>,
        /// A new blocker.
        waiting_on: &'a Patch<String>,
        /// Whether the blocker is cleared.
        clear_waiting_on: bool,
        /// A new category.
        category: &'a Patch<String>,
        /// A new checklist.
        subtodos: &'a Patch<Vec<SubTodo>>,
        /// A deferral.
        defer_to: &'a Patch<NaiveDate>,
        /// Whether it reprices, to whatever.
        pricing: Pricing,
    },
    /// A new price for a todo, whatever it is.
    Reprice {
        /// Which todo.
        todo: &'a str,
    },
    /// An act that carries no pricing, which is its own request.
    Unpriced(&'a Act),
}

/// Whether an update names a price or a rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pricing {
    /// It leaves both as they are.
    Kept,
    /// It names a price, a rationale, or both.
    Moved,
}

impl Act {
    /// The request this act makes.
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
