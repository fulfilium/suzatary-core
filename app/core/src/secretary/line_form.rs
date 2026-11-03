//! The lines' replica in the device's form: its snapshot ([`crate::snapshot::Held`]) and its
//! deltas, each name printed as its 32 bytes rather than 64 hex characters. A node reads back
//! through [`Parsed::of`], as the box's are read, so a snapshot holds nothing a pull could not.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};

use crate::replica::frontier::Frontier;
use crate::replica::held::Held;
use crate::replica::pulling::{Delta, Pulling};
use crate::replica::row::Row;
use crate::replica::word::Word;
use crate::secretary::key_ring::KeyRing;
use crate::secretary::locked_text::{KEY_BYTES, KeyName};
use crate::secretary::outlines::Parsed;
use crate::secretary::step::Step;
use crate::snapshot::{self, NotASnapshot, Unprinted};
use crate::wire::{NodeHeader, SkeletonNode};

/// The lines' rows.
pub type LinePulling = Pulling<Parsed, KeyRing>;

/// One move of the lines' rows.
pub type LineDelta = Delta<Parsed, KeyRing>;

/// A name's 32 bytes: an object's, or a key's.
type Name = [u8; 32];

/// What a snapshot of the lines' rows prints.
#[derive(Serialize, Deserialize)]
pub struct Given {
    landed: RowForm,
    landings: u64,
    pending: RowForm,
}

#[derive(Serialize, Deserialize)]
struct RowForm {
    generation: u64,
    nodes: Vec<NodeForm>,
    keys: Vec<(Name, [u8; KEY_BYTES])>,
    words: Vec<WordForm>,
    frontier: Option<FrontierForm>,
}

#[derive(Serialize, Deserialize)]
struct NodeForm {
    name: Name,
    parents: Vec<Name>,
    size: usize,
    header: HeaderForm,
}

#[derive(Serialize, Deserialize)]
enum HeaderForm {
    Structure {
        kind: String,
    },
    Line {
        conversation: String,
        at: String,
        kind: String,
        keys: Vec<Name>,
        step: StepForm,
    },
}

#[derive(Serialize, Deserialize)]
enum StepForm {
    Opened,
    Asks,
    Noted,
    Answers,
    Rewound { to: usize },
    Proposed,
    Coding { proposal: usize },
    Titled,
    Resealed { of: Name, from: Name },
    Erased { keys: Vec<Name> },
    Still,
}

#[derive(Serialize, Deserialize)]
struct WordForm {
    epoch: u64,
    heads: Vec<Name>,
    geneses: Vec<Name>,
    fetched_at: u64,
}

#[derive(Serialize, Deserialize)]
struct FrontierForm {
    era: u64,
    count: u64,
    digest: Name,
}

#[derive(Serialize, Deserialize)]
enum DeltaForm {
    Piece { landings: u64, row: RowForm },
    Landed { landings: u64, row: RowForm },
    StartedOver { generation: u64 },
    Forgot(Vec<Name>),
}

impl snapshot::Held for LinePulling {
    type Given = Given;
    /// 2: the landed row and the pending pieces, each with its nodes, keys, words and frontier,
    /// every name as its 32 bytes.
    const FORM: u32 = 2;

    fn given(&self) -> Given {
        Given {
            landed: row_form(self.landed()),
            landings: self.landings(),
            pending: row_form(self.pending()),
        }
    }

    fn of_given(given: Given) -> Result<Self, String> {
        Ok(Pulling::of(
            row_of(given.landed)?,
            given.landings,
            row_of(given.pending)?,
        ))
    }
}

/// `delta` as a frame of the log: the snapshot's form, then the delta in postcard.
///
/// # Errors
/// [`Unprinted`] where it refuses to print.
pub fn frame(delta: &LineDelta) -> Result<Vec<u8>, Unprinted> {
    let form = match delta {
        Delta::Piece { landings, row } => DeltaForm::Piece {
            landings: *landings,
            row: row_form(row),
        },
        Delta::Landed { landings, row } => DeltaForm::Landed {
            landings: *landings,
            row: row_form(row),
        },
        Delta::StartedOver { generation } => DeltaForm::StartedOver {
            generation: *generation,
        },
        Delta::Forgot(gone) => DeltaForm::Forgot(gone.iter().map(|k| bytes(k.as_str())).collect()),
    };
    snapshot::print(<LinePulling as snapshot::Held>::FORM, &form)
}

/// The delta a frame prints, or none for a frame of another form, which this build passes over.
///
/// # Errors
/// [`NotASnapshot`]: a frame of this form cut short, unread, or holding what no replica holds.
pub fn unframe(bytes: &[u8]) -> Result<Option<LineDelta>, NotASnapshot> {
    let form = match snapshot::read(<LinePulling as snapshot::Held>::FORM, bytes) {
        Ok(form) => form,
        Err(NotASnapshot::Form { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let delta = match form {
        DeltaForm::Piece { landings, row } => Delta::Piece {
            landings,
            row: row_of(row).map_err(NotASnapshot::Refused)?,
        },
        DeltaForm::Landed { landings, row } => Delta::Landed {
            landings,
            row: row_of(row).map_err(NotASnapshot::Refused)?,
        },
        DeltaForm::StartedOver { generation } => Delta::StartedOver { generation },
        DeltaForm::Forgot(gone) => Delta::Forgot(
            gone.iter()
                .map(key_name)
                .collect::<Result<_, _>>()
                .map_err(NotASnapshot::Refused)?,
        ),
    };
    Ok(Some(delta))
}

fn row_form(row: &Row<Parsed, KeyRing>) -> RowForm {
    let replica = row.replica();
    RowForm {
        generation: replica.generation(),
        nodes: replica.nodes().map(|n| node_form(n.given())).collect(),
        keys: row
            .keys()
            .iter()
            .map(|(name, key)| (bytes(name.as_str()), *key))
            .collect(),
        words: replica
            .words()
            .iter()
            .map(|w| WordForm {
                epoch: w.epoch,
                heads: w.heads.iter().map(|h| bytes(h.as_str())).collect(),
                geneses: w.geneses.iter().map(|h| bytes(h.as_str())).collect(),
                fetched_at: w.fetched_at,
            })
            .collect(),
        frontier: match replica.frontier() {
            Frontier::Nothing => None,
            Frontier::Named { era, count, digest } => Some(FrontierForm {
                era: *era,
                count: *count,
                digest: bytes(digest.as_str()),
            }),
        },
    }
}

fn row_of(form: RowForm) -> Result<Row<Parsed, KeyRing>, String> {
    let nodes = form
        .nodes
        .into_iter()
        .map(|n| Parsed::of(node_of(n)?).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let keys: BTreeMap<KeyName, [u8; KEY_BYTES]> = form
        .keys
        .iter()
        .map(|(name, key)| Ok((key_name(name)?, *key)))
        .collect::<Result<_, String>>()?;
    let keys = KeyRing::try_from(keys).map_err(|e| e.to_string())?;
    let words = form
        .words
        .into_iter()
        .map(|w| {
            Ok(Word {
                epoch: w.epoch,
                heads: hashes(&w.heads)?,
                geneses: hashes(&w.geneses)?,
                fetched_at: w.fetched_at,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let frontier = match form.frontier {
        None => Frontier::Nothing,
        Some(f) => Frontier::Named {
            era: f.era,
            count: f.count,
            digest: hash(&f.digest)?,
        },
    };
    Ok(Row::of(
        Held::of(form.generation, nodes, words, frontier),
        keys,
    ))
}

fn node_form(node: &SkeletonNode) -> NodeForm {
    NodeForm {
        name: bytes(&node.hash),
        parents: node.parents.iter().map(|p| bytes(p)).collect(),
        size: node.size,
        header: match &node.header {
            NodeHeader::Structure { kind } => HeaderForm::Structure { kind: kind.clone() },
            NodeHeader::Line {
                conversation,
                at,
                kind,
                keys,
                step,
            } => HeaderForm::Line {
                conversation: conversation.clone(),
                at: at.clone(),
                kind: kind.clone(),
                keys: keys.iter().map(|k| bytes(k)).collect(),
                step: step_form(step),
            },
        },
    }
}

fn node_of(form: NodeForm) -> Result<SkeletonNode, String> {
    Ok(SkeletonNode {
        hash: hex::encode(form.name),
        parents: form.parents.iter().map(hex::encode).collect(),
        size: form.size,
        header: match form.header {
            HeaderForm::Structure { kind } => NodeHeader::Structure { kind },
            HeaderForm::Line {
                conversation,
                at,
                kind,
                keys,
                step,
            } => NodeHeader::Line {
                conversation,
                at,
                kind,
                keys: keys.iter().map(hex::encode).collect(),
                step: step_of(step)?,
            },
        },
    })
}

fn step_form(step: &Step) -> StepForm {
    match step {
        Step::Opened => StepForm::Opened,
        Step::Asks => StepForm::Asks,
        Step::Noted => StepForm::Noted,
        Step::Answers => StepForm::Answers,
        Step::Rewound { to } => StepForm::Rewound { to: *to },
        Step::Proposed => StepForm::Proposed,
        Step::Coding { proposal } => StepForm::Coding {
            proposal: *proposal,
        },
        Step::Titled => StepForm::Titled,
        Step::Resealed { of, from } => StepForm::Resealed {
            of: bytes(of.as_str()),
            from: bytes(from.as_str()),
        },
        Step::Erased { keys } => StepForm::Erased {
            keys: keys.iter().map(|k| bytes(k.as_str())).collect(),
        },
        Step::Still => StepForm::Still,
    }
}

fn step_of(form: StepForm) -> Result<Step, String> {
    let name = key_name;
    Ok(match form {
        StepForm::Opened => Step::Opened,
        StepForm::Asks => Step::Asks,
        StepForm::Noted => Step::Noted,
        StepForm::Answers => Step::Answers,
        StepForm::Rewound { to } => Step::Rewound { to },
        StepForm::Proposed => Step::Proposed,
        StepForm::Coding { proposal } => Step::Coding { proposal },
        StepForm::Titled => Step::Titled,
        StepForm::Resealed { of, from } => Step::Resealed {
            of: name(&of)?,
            from: name(&from)?,
        },
        StepForm::Erased { keys } => Step::Erased {
            keys: keys.iter().map(name).collect::<Result<_, _>>()?,
        },
        StepForm::Still => Step::Still,
    })
}

/// The 32 bytes 64 hex digits spell. Every name printed here is a [`Hash`] or a [`KeyName`], or
/// was parsed as one where its node arrived, so each is 64 lowercase hex and every digit reads.
fn bytes(hex: &str) -> Name {
    let mut out = [0; 32];
    for (byte, pair) in out.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = pair.iter().fold(0, |acc, digit| acc << 4 | nibble(*digit));
    }
    out
}

fn nibble(digit: u8) -> u8 {
    match digit {
        b'0'..=b'9' => digit - b'0',
        b'a'..=b'f' => digit - b'a' + 10,
        _ => 0,
    }
}

fn hash(bytes: &Name) -> Result<Hash, String> {
    Hash::new(hex::encode(bytes)).map_err(|e| e.to_string())
}

fn hashes(names: &[Name]) -> Result<BTreeSet<Hash>, String> {
    names.iter().map(hash).collect()
}

fn key_name(bytes: &Name) -> Result<KeyName, String> {
    KeyName::new(hex::encode(bytes)).map_err(|e| e.to_string())
}
