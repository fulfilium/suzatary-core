//! The cards' wire.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::todo::Neighbour;

/// `GET /api/about`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AboutPayload {
    /// `dev` when unstamped.
    pub version: String,
    /// Short git revision, or `dev` outside the flake.
    pub build: String,
    /// From `app/release.toml`, first named first.
    pub completes: Vec<Shipped>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Shipped {
    pub todo: String,
    /// `null` where the chain is not served or does not hold the todo.
    pub held: Option<HeldTodo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct HeldTodo {
    pub row: Neighbour,
    /// Plain text.
    pub title: String,
}

/// `GET /api/overview`: never a secret's name or value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Overview {
    /// In `Group::ALL`'s order; one nothing is connected to holds no line.
    pub groups: Vec<OverviewGroup>,
    pub ask: String,
    /// From the catalog, in the order Settings offers it.
    pub offers: Vec<Offer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Offer {
    /// What `POST /api/secretary/setup/<template>` takes.
    pub template: String,
    pub group: crate::connect::Group,
    /// As a sentence ends with it: `a Google account (its mail and calendars)`.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OverviewGroup {
    pub group: crate::connect::Group,
    pub lines: Vec<OverviewLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OverviewLine {
    /// Its identity within its group, never a secret's name.
    pub key: String,
    pub label: String,
    pub detail: Option<String>,
    pub standing: crate::connect::Standing,
    /// Each a `proposal:<conversation>/<n>`; every need's on one line only.
    pub cards: Vec<String>,
    /// Each stored secret it reads that a Secret card can replace.
    pub replaces: Vec<Replace>,
}

/// A stored secret a line offers to replace, by `POST /api/secretary/replace/<handle>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Replace {
    /// What the owner calls it.
    pub label: String,
    /// How the route names it: never the secret's name.
    pub handle: String,
}
