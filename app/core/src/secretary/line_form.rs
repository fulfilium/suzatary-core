//! The lines' replica in the device's form: its snapshot ([`suzatary_store::snapshot::Held`]) and its
//! frames, each name printed as its 32 bytes rather than 64 hex characters. A node reads back
//! through [`Parsed::of`], as the box's are read, so a snapshot holds nothing a pull could not;
//! a husk reads back through [`Husk::read`], which reads no words.

use std::collections::BTreeMap;

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};

use crate::secretary::key_ring::KeyRing;
use crate::secretary::line_event::LineEvent;
use crate::secretary::locked_text::{KEY_BYTES, KeyName};
use crate::secretary::outlines::Parsed;
use crate::secretary::step::Step;
use crate::wire::lines::{NodeHeader, SkeletonNode};
use suzatary_store::replica::form::{self, DeltaForm, Name, RowForm, bytes, hash};
use suzatary_store::replica::pulling::{Delta, Pulling};
use suzatary_store::replica::row::Row;
use suzatary_store::snapshot::{self, NotASnapshot, Unprinted};
use suzatary_store::store::Husk;

/// The lines' rows.
pub type LinePulling = Pulling<Parsed, KeyRing>;

/// One move of the lines' rows.
pub type LineDelta = Delta<Parsed, KeyRing>;

/// A line the device keeps as its husk ([`Husk`]): its name, the names it rests on, and the husk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Husked {
    pub name: Hash,
    pub parents: Vec<Hash>,
    /// The line, its words dropped.
    pub husk: Husk<LineEvent>,
}

/// The lines' replica as the device keeps it: the rows, and the husk of every line each key of
/// whose words it let go, by name. A husk stands where its print stood, so it is never fetched
/// again.
#[derive(Debug, Default, PartialEq)]
pub struct LineReplica {
    pub rows: LinePulling,
    pub husks: BTreeMap<Hash, Husked>,
}

/// One move of the replica: the rows' delta, and the lines it husked.
#[derive(Debug, Clone)]
pub struct LineFrame {
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
    landed: LineRowForm,
    landings: u64,
    pending: LineRowForm,
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
    delta: DeltaForm<LineRowForm, Name>,
    husked: Vec<HuskForm>,
}

/// A row of the lines: each node with its header, and each key's bytes by its name.
type LineRowForm = RowForm<NodeForm, Vec<(Name, [u8; KEY_BYTES])>>;

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
        delta: form::delta_form(&frame.delta, row_form, |k| bytes(k.as_str())),
        husked: frame.husked.iter().map(husk_form).collect(),
    };
    snapshot::print(<LineReplica as snapshot::Held>::FORM, &form)
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
        delta: form::delta_of(form.delta, row_of, key_name).map_err(NotASnapshot::Refused)?,
        husked: form
            .husked
            .iter()
            .map(husked)
            .collect::<Result<_, _>>()
            .map_err(NotASnapshot::Refused)?,
    }))
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

fn row_form(row: &Row<Parsed, KeyRing>) -> LineRowForm {
    form::row_form(
        row,
        |n| node_form(n.given()),
        |keys| {
            keys.iter()
                .map(|(name, key)| (bytes(name.as_str()), *key))
                .collect()
        },
    )
}

fn row_of(form: LineRowForm) -> Result<Row<Parsed, KeyRing>, String> {
    form::row_of(
        form,
        |n| Parsed::of(node_of(n)?).map_err(|e| e.to_string()),
        |keys| {
            let keys: BTreeMap<KeyName, [u8; KEY_BYTES]> = keys
                .iter()
                .map(|(name, key)| Ok((key_name(name)?, *key)))
                .collect::<Result<_, String>>()?;
            KeyRing::try_from(keys).map_err(|e| e.to_string())
        },
    )
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

fn key_name(bytes: &Name) -> Result<KeyName, String> {
    KeyName::new(hex::encode(bytes)).map_err(|e| e.to_string())
}

/// A line's header, what a fold reads of it without its words: its husk's is its own.
#[must_use]
pub fn line_header(line: &crate::secretary::line_event::LineEvent) -> NodeHeader {
    NodeHeader::Line {
        conversation: line.conversation().as_str().to_owned(),
        at: prodrome_core::fpl::iso(prodrome_core::fpl::instant_of(line.at())),
        kind: line.shape().name().to_owned(),
        keys: line.keys().map(|key| key.as_str().to_owned()).collect(),
        step: Step::of(line.shape()),
    }
}
