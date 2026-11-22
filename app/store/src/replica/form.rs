//! A replica's rows and moves as the device stores them, each name as its 32 bytes rather than
//! 64 hex characters.

use std::collections::BTreeSet;

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};

use crate::replica::frontier::Frontier;
use crate::replica::held::{Held, Node};
use crate::replica::pulling::Delta;
use crate::replica::row::{Keys, Row};
use crate::replica::word::Word;

pub type Name = [u8; 32];

#[derive(Serialize, Deserialize)]
pub struct RowForm<N, K> {
    generation: u64,
    nodes: Vec<N>,
    keys: K,
    words: Vec<WordForm>,
    frontier: Option<FrontierForm>,
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
pub enum DeltaForm<R, G> {
    Piece {
        /// The landings before it.
        landings: u64,
        row: R,
    },
    Landed {
        /// The landings, this one included.
        landings: u64,
        row: R,
    },
    StartedOver {
        /// The generation started.
        generation: u64,
    },
    Forgot(Vec<G>),
}

pub fn row_form<N: Node, K: Keys, NF, KF>(
    row: &Row<N, K>,
    node: impl Fn(&N) -> NF,
    keys: impl FnOnce(&K) -> KF,
) -> RowForm<NF, KF> {
    let replica = row.replica();
    RowForm {
        generation: replica.generation(),
        nodes: replica.nodes().map(node).collect(),
        keys: keys(row.keys()),
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

pub fn row_of<N: Node, K: Keys, NF, KF>(
    form: RowForm<NF, KF>,
    node: impl Fn(NF) -> Result<N, String>,
    keys: impl FnOnce(KF) -> Result<K, String>,
) -> Result<Row<N, K>, String> {
    let nodes = form
        .nodes
        .into_iter()
        .map(node)
        .collect::<Result<Vec<_>, _>>()?;
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
        keys(form.keys)?,
    ))
}

pub fn delta_form<N: Node, K: Keys, R, G>(
    delta: &Delta<N, K>,
    row: impl Fn(&Row<N, K>) -> R,
    gone: impl Fn(&K::Name) -> G,
) -> DeltaForm<R, G> {
    match delta {
        Delta::Piece { landings, row: r } => DeltaForm::Piece {
            landings: *landings,
            row: row(r),
        },
        Delta::Landed { landings, row: r } => DeltaForm::Landed {
            landings: *landings,
            row: row(r),
        },
        Delta::StartedOver { generation } => DeltaForm::StartedOver {
            generation: *generation,
        },
        Delta::Forgot(names) => DeltaForm::Forgot(names.iter().map(gone).collect()),
    }
}

pub fn delta_of<N: Node, K: Keys, R, G>(
    form: DeltaForm<R, G>,
    row: impl Fn(R) -> Result<Row<N, K>, String>,
    gone: impl Fn(&G) -> Result<K::Name, String>,
) -> Result<Delta<N, K>, String> {
    Ok(match form {
        DeltaForm::Piece { landings, row: r } => Delta::Piece {
            landings,
            row: row(r)?,
        },
        DeltaForm::Landed { landings, row: r } => Delta::Landed {
            landings,
            row: row(r)?,
        },
        DeltaForm::StartedOver { generation } => Delta::StartedOver { generation },
        DeltaForm::Forgot(names) => {
            Delta::Forgot(names.iter().map(gone).collect::<Result<_, _>>()?)
        }
    })
}

/// The 32 bytes 64 lowercase hex digits spell; every caller passes a [`Hash`](struct@Hash) or a
/// key's name.
#[must_use]
pub fn bytes(hex: &str) -> Name {
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

/// # Errors
/// Never for 32 bytes; kept as [`Hash::new`]'s refusal.
pub fn hash(bytes: &Name) -> Result<Hash, String> {
    Hash::new(hex::encode(bytes)).map_err(|e| e.to_string())
}

pub fn hashes(names: &[Name]) -> Result<BTreeSet<Hash>, String> {
    names.iter().map(hash).collect()
}
