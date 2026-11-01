//! The lines a tab's core holds, whole: the prints and keys read through [`HeldLines`], and the
//! skeleton's nodes read through [`Outlines`]. What it was handed is the skeleton and the keys,
//! so that pair is what its snapshot prints ([`crate::snapshot`]); the prints are content, kept
//! by name on the device and handed over again as a reading needs them.

use serde::{Deserialize, Serialize};

use crate::secretary::held_lines::HeldLines;
use crate::secretary::key_ring::KeyRing;
use crate::secretary::locked_text::KeyName;
use crate::secretary::outlines::{Outlines, Parsed};
use crate::secretary::step::Step;
use crate::snapshot::Held;
use crate::wire::{NodeHeader, SkeletonNode};

/// The lines held, and the skeleton they are read beside.
#[derive(Default)]
pub struct HeldReplica {
    /// The prints and the keys of their words.
    pub lines: HeldLines,
    /// The skeleton's nodes.
    pub outlines: Outlines,
}

/// What a replica was handed: every node of its skeleton, by name, and its keys.
#[derive(Serialize, Deserialize)]
pub struct Given {
    nodes: Vec<Node>,
    keys: KeyRing,
}

/// A node in the snapshot's form. The wire's header and step are tagged inside their fields,
/// which a binary form without field names cannot read back, so they are given here by their
/// constructor instead, each field of the wire's own type checked against it by serde.
#[derive(Serialize, Deserialize)]
struct Node(#[serde(with = "NodeForm")] SkeletonNode);

#[derive(Serialize, Deserialize)]
#[serde(remote = "SkeletonNode")]
struct NodeForm {
    hash: String,
    parents: Vec<String>,
    size: usize,
    #[serde(with = "HeaderForm")]
    header: NodeHeader,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "NodeHeader")]
enum HeaderForm {
    Structure {
        kind: String,
    },
    Line {
        conversation: String,
        at: String,
        kind: String,
        keys: Vec<String>,
        #[serde(with = "StepForm")]
        step: Step,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Step")]
enum StepForm {
    Opened,
    Asks,
    Noted,
    Answers,
    Rewound {
        to: usize,
    },
    Proposed,
    Coding {
        proposal: usize,
    },
    Titled,
    Resealed {
        of: KeyName,
        from: KeyName,
    },
    Erased {
        keys: std::collections::BTreeSet<KeyName>,
    },
    Still,
}

impl Held for HeldReplica {
    type Given = Given;
    /// 1: the skeleton's nodes as the wire's, and each key's bytes under its name.
    const FORM: u32 = 1;

    fn given(&self) -> Given {
        Given {
            nodes: self.outlines.given().cloned().map(Node).collect(),
            keys: self.lines.key_ring().clone(),
        }
    }

    fn of_given(given: Given) -> Result<Self, String> {
        let nodes = given
            .nodes
            .into_iter()
            .map(|Node(node)| Parsed::of(node))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut outlines = Outlines::default();
        outlines.hold(nodes, &[]);
        Ok(Self {
            lines: HeldLines::of_keys(given.keys),
            outlines,
        })
    }
}
