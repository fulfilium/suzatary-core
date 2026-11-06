//! The list's tree, grown here so the pages' module links no term and no trie: the members a
//! page places, grouped as it asks, each node with its term (`suzatary_core::tree`), for the
//! worker to price beside the reading it keeps.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::TodoId;
use prodrome_core::fpl::LinkError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suzatary_core::term_json;
use suzatary_core::tree::{
    self, By, Classifier, Member, Members, Monoid, Place, Segment, Tree, Trie,
};
use wasm_bindgen::prelude::*;

use crate::{json, refused};

/// A todo the page places, as it reads its row.
#[derive(Deserialize)]
struct Placed {
    todo: String,
    title: String,
    category: String,
    /// Its own value: a number, `null` for `∅`, or why it does not link.
    worth: Worth,
    /// Its candidate specs, as the wire prints a term.
    specs: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Worth {
    Value { v: Option<f64> },
    Unlinked,
}

/// A node as the page draws it: where it stands, and its term.
#[derive(Serialize)]
struct Node {
    place: Spot,
    term: Value,
    children: Vec<Node>,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "v")]
enum Spot {
    All,
    Category(String),
    Band(&'static str),
    Todo(String),
}

/// The tree of `members` (JSON, [`Placed`]) grouped by `by` (`category`, `urgency` or `flat`)
/// and then by ancestry, a parent being a member any of whose specs refers to the child.
#[wasm_bindgen]
pub fn grow(members: &str, by: &str) -> Result<String, JsError> {
    let placed: Vec<Placed> =
        serde_json::from_str(members).map_err(|e| refused(format!("members: {e}")))?;
    let ids: BTreeSet<&str> = placed.iter().map(|p| p.todo.as_str()).collect();
    let mut parents: BTreeMap<String, BTreeSet<TodoId>> = BTreeMap::new();
    for p in &placed {
        let Ok(parent) = TodoId::new(p.todo.clone()) else {
            continue;
        };
        for spec in p.specs.iter().filter_map(|s| term_json::from_json(s).ok()) {
            for child in tree::refs(&spec).into_iter().filter(|c| ids.contains(c)) {
                parents
                    .entry(child.to_owned())
                    .or_default()
                    .insert(parent.clone());
            }
        }
    }
    let members: Members = placed
        .into_iter()
        .filter_map(|p| {
            let worth = match p.worth {
                Worth::Value { v } => Ok(v),
                Worth::Unlinked => Err(LinkError::Unknown(p.todo.clone())),
            };
            let member = Member {
                title: p.title,
                category: p.category,
                worth,
                parents: parents.remove(&p.todo).unwrap_or_default(),
            };
            Some((TodoId::new(p.todo).ok()?, member))
        })
        .collect();
    let classifier = match by {
        "category" => Classifier::by(By::Category),
        "urgency" => Classifier::by(By::Urgency),
        "flat" => Classifier::empty(),
        other => return Err(refused(format!("no grouping {other}"))),
    }
    .combine(Classifier::by(By::Ancestry));
    let grown = Trie::of(&members, &classifier).tree().scan(&mut tree::term);
    json(&node(grown))
}

fn node(t: Tree<(Place, prodrome_core::term::Term)>) -> Node {
    let (place, term) = t.node;
    Node {
        place: match place {
            Place::All => Spot::All,
            Place::At(Segment::Category(c)) => Spot::Category(c),
            Place::At(Segment::Band(b)) => Spot::Band(b.label()),
            Place::At(Segment::Todo(id)) => Spot::Todo(id.as_str().to_owned()),
        },
        term: term_json::to_json(&term),
        children: t.children.into_iter().map(node).collect(),
    }
}
