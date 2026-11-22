//! A store's static pages (`suzatary prodrome pages`): the value each template is called with,
//! each the reading `todos open` or `todo show` prints, with the instant in words and where its
//! links lead.

use schemars::JsonSchema;
use serde::Serialize;

use crate::shown::{OpenRow, Shown};

/// The open list (`rendering::TODOS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TodosPage {
    /// The instant every price is taken at, in words.
    pub at: String,
    pub rows: Vec<Listed>,
}

/// One row and the page it links to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Listed {
    /// A path relative to the list's page.
    pub page: String,
    pub row: OpenRow,
}

/// One todo whole (`rendering::TODO`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TodoPage {
    /// The instant its price is taken at, in words.
    pub at: String,
    /// The list's page, relative to this one.
    pub list: String,
    /// Each reference that names a todo of this store, and its page.
    pub links: Vec<Linked>,
    pub shown: Shown,
    /// Each candidate's markup fields, in `shown.candidates`' order.
    pub drawn: Vec<Drawn>,
}

/// What a todo's page shows of one candidate's markup fields. The page evaluates no writer's
/// markup: a field's drawing, compiled alone as door 1 read it, is set into the page after the
/// page's compile, where the page holds its marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Drawn {
    pub body: Field,
    pub detail: Field,
    /// Each todo of this store the fields' drawn text names (`todo:<chain>/<id>`), and its page.
    pub refers: Vec<Linked>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Field {
    Empty,
    /// It draws; its drawing replaces the element whose `data-drawn` is `marker`.
    Drawn {
        marker: String,
    },
    /// It does not draw in the closed world, so its source is shown as text.
    Source,
}

/// A reference's text after `todo:`, `<chain>/<id>`, and the page it leads to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Linked {
    pub reference: String,
    /// A path relative to the todo's page.
    pub page: String,
}
