//! The lines' replica in the device's form: its snapshot ([`crate::snapshot::Held`]) and its
//! frames, each name printed as its 32 bytes rather than 64 hex characters. A node reads back
//! through [`Parsed::of`], as the box's are read, so a snapshot holds nothing a pull could not;
//! a husk reads back through [`Husk::read`], which reads no words.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};

use crate::replica::frontier::Frontier;
use crate::replica::held::Held;
use crate::replica::pulling::{Delta, Pulling};
use crate::replica::row::Row;
use crate::replica::word::Word;
use crate::secretary::key_ring::KeyRing;
use crate::secretary::line_event::Husk;
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

/// A line the device keeps as its husk ([`Husk`]): its name, the names it rests on, and the husk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Husked {
    /// The line's object's name.
    pub name: Hash,
    /// The names it rests on.
    pub parents: Vec<Hash>,
    /// The line, its words dropped.
    pub husk: Husk,
}

/// The lines' replica as the device keeps it: the rows, and the husk of every line each key of
/// whose words it let go, by name. A husk stands where its print stood, so it is never fetched
/// again.
#[derive(Debug, Default, PartialEq)]
pub struct LineReplica {
    /// The rows.
    pub rows: LinePulling,
    /// The husks.
    pub husks: BTreeMap<Hash, Husked>,
}

/// One move of the replica: the rows' delta, and the lines it husked.
#[derive(Debug, Clone)]
pub struct LineFrame {
    /// The rows' move.
    pub delta: LineDelta,
    /// The lines it husked.
    pub husked: Vec<Husked>,
}

impl LineReplica {
    /// `frame` taken: another tab's, or one read back from the log.
    pub fn apply(&mut self, frame: &LineFrame) {
        self.rows.apply(&frame.delta);
        for husked in &frame.husked {
            self.husks.insert(husked.name.clone(), husked.clone());
        }
    }
}

/// What a snapshot of the lines' replica prints.
#[derive(Serialize, Deserialize)]
pub struct Given {
    landed: RowForm,
    landings: u64,
    pending: RowForm,
    husks: Vec<HuskForm>,
}

#[derive(Serialize, Deserialize)]
struct HuskForm {
    name: Name,
    parents: Vec<Name>,
    line: String,
}

#[derive(Serialize, Deserialize)]
struct FrameForm {
    delta: DeltaForm,
    husked: Vec<HuskForm>,
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

impl snapshot::Held for LineReplica {
    type Given = Given;
    /// 3: the landed row and the pending pieces, each with its nodes, keys, words and frontier,
    /// every name as its 32 bytes, and the husks, each line's print with its words dropped.
    const FORM: u32 = 3;

    fn given(&self) -> Given {
        Given {
            landed: row_form(self.rows.landed()),
            landings: self.rows.landings(),
            pending: row_form(self.rows.pending()),
            husks: self.husks.values().map(husk_form).collect(),
        }
    }

    fn of_given(given: Given) -> Result<Self, String> {
        Ok(Self {
            rows: Pulling::of(
                row_of(given.landed)?,
                given.landings,
                row_of(given.pending)?,
            ),
            husks: given
                .husks
                .iter()
                .map(|form| husked(form).map(|h| (h.name.clone(), h)))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// `frame` as a frame of the log: the snapshot's form, then the frame in postcard.
///
/// # Errors
/// [`Unprinted`] where it refuses to print.
pub fn frame(frame: &LineFrame) -> Result<Vec<u8>, Unprinted> {
    let form = FrameForm {
        delta: delta_form(&frame.delta),
        husked: frame.husked.iter().map(husk_form).collect(),
    };
    snapshot::print(<LineReplica as snapshot::Held>::FORM, &form)
}

fn delta_form(delta: &LineDelta) -> DeltaForm {
    match delta {
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
    }
}

/// The frame `bytes` prints, or none for a frame of another form, which this build passes over.
///
/// # Errors
/// [`NotASnapshot`]: a frame of this form cut short, unread, or holding what no replica holds.
pub fn unframe(bytes: &[u8]) -> Result<Option<LineFrame>, NotASnapshot> {
    let form: FrameForm = match snapshot::read(<LineReplica as snapshot::Held>::FORM, bytes) {
        Ok(form) => form,
        Err(NotASnapshot::Form { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(Some(LineFrame {
        delta: delta_of(form.delta).map_err(NotASnapshot::Refused)?,
        husked: form
            .husked
            .iter()
            .map(husked)
            .collect::<Result<_, _>>()
            .map_err(NotASnapshot::Refused)?,
    }))
}

fn delta_of(form: DeltaForm) -> Result<LineDelta, String> {
    Ok(match form {
        DeltaForm::Piece { landings, row } => Delta::Piece {
            landings,
            row: row_of(row)?,
        },
        DeltaForm::Landed { landings, row } => Delta::Landed {
            landings,
            row: row_of(row)?,
        },
        DeltaForm::StartedOver { generation } => Delta::StartedOver { generation },
        DeltaForm::Forgot(gone) => {
            Delta::Forgot(gone.iter().map(key_name).collect::<Result<_, _>>()?)
        }
    })
}

fn husk_form(husked: &Husked) -> HuskForm {
    HuskForm {
        name: bytes(husked.name.as_str()),
        parents: husked.parents.iter().map(|p| bytes(p.as_str())).collect(),
        line: husked.husk.print(),
    }
}

fn husked(form: &HuskForm) -> Result<Husked, String> {
    Ok(Husked {
        name: hash(&form.name)?,
        parents: form.parents.iter().map(hash).collect::<Result<_, _>>()?,
        husk: Husk::read(&form.line).map_err(|e| e.to_string())?,
    })
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
