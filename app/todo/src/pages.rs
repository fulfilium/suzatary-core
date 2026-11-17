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
}

/// A reference's text after `todo:`, `<chain>/<id>`, and the page it leads to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Linked {
    pub reference: String,
    /// A path relative to the todo's page.
    pub page: String,
}
