use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::signatures::RefusalKind;
use suzatary_store::wire::store::SinceRead;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StoredObject {
    /// sha256 of the literal, rehashed by the receiver.
    pub hash: String,
    pub literal: String,
}

/// The digest the box recorded its heads under, named back as `since`, and its object count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Frontier {
    /// 64 hex digits.
    pub digest: String,
    pub count: usize,
}

/// `GET /api/objects?since=<frontier>`: what the caller lacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ObjectsPayload {
    /// Sorted.
    pub heads: Vec<String>,
    pub untrusted: Vec<String>,
    /// The root key a device key's signatures count under; `null` for a store the box alone writes.
    pub root: Option<String>,
    /// Sorted: a replica drops any other store it holds.
    pub geneses: Vec<String>,
    /// Bumped whenever `geneses` changes and by nothing else (`suzatary_core::epoch`).
    #[ts(type = "number")]
    pub epoch: u64,
    /// Parents first.
    pub objects: Vec<StoredObject>,
    pub since: SinceRead,
    /// To name as the next `since`.
    pub frontier: Frontier,
}

/// An object's literal, or the husk of a line no key the box holds opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "held", rename_all = "lowercase")]
pub enum Content {
    Literal { hash: String, literal: String },
    Husk { hash: String, husk: String },
}

/// What a fold reads of an object without its literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SkeletonNode {
    pub hash: String,
    /// Its prodrome's genesis included, sorted.
    pub parents: Vec<String>,
    /// The bytes of its literal, so a tab can ask for content in answers of a bounded size.
    pub size: usize,
    pub header: NodeHeader,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "node", rename_all = "lowercase")]
pub enum NodeHeader {
    Structure {
        kind: String,
    },
    Line {
        conversation: String,
        /// By its writer's clock.
        at: String,
        kind: String,
        /// In slot order.
        keys: Vec<String>,
        step: crate::secretary::step::Step,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "are", rename_all = "lowercase")]
pub enum Heads {
    /// The heads the asker's `since` names, so they are not sent again.
    Held,
    These {
        /// Sorted.
        heads: Vec<String>,
    },
}

/// `GET /api/stores/lines/skeleton?since=<heads>`: the lines without literals; no erased key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SkeletonPayload {
    pub heads: Heads,
    /// Sorted.
    pub geneses: Vec<String>,
    #[ts(type = "number")]
    pub epoch: u64,
    /// Parents first.
    pub nodes: Vec<SkeletonNode>,
    pub since: SinceRead,
    /// To name as the next `since`.
    pub frontier: Frontier,
    #[ts(type = "Record<string, string>")]
    pub keys: BTreeMap<String, String>,
    pub rest: usize,
    /// The bytes of the nodes lacked, by a piece's weight.
    pub size: usize,
    /// By the same weight, so `size` less this is what is left.
    pub weight: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RefusedObject {
    /// `""` when the bytes were not an object.
    pub hash: String,
    pub reason: String,
    pub kind: RefusalKind,
}

/// `POST /api/objects`. The lists are always present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReceiptPayload {
    /// Sorted.
    pub heads: Vec<String>,
    /// Parents first.
    pub adopted: Vec<String>,
    /// What makes a re-post free.
    pub already: Vec<String>,
    /// In arrival order.
    pub refused: Vec<RefusedObject>,
    pub since: SinceRead,
    /// What it held at `since` and everything adopted, with all they rest on.
    pub frontier: Frontier,
}
