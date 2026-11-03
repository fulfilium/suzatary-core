//! A replica's rows in the device's form: what every store's snapshot and frames print of a row
//! and of a move of its rows, each name as its 32 bytes rather than 64 hex characters. A store
//! gives how one of its nodes and its keys print ([`RowForm`]'s `N` and `K`); the generation, the
//! box's words and the frontier print the same for every store.

use std::collections::BTreeSet;

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};

use crate::replica::frontier::Frontier;
use crate::replica::held::{Held, Node};
use crate::replica::pulling::Delta;
use crate::replica::row::{Keys, Row};
use crate::replica::word::Word;

/// A name's 32 bytes.
pub type Name = [u8; 32];

/// A row: its nodes as `N` print, its keys as `K`.
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

/// A move of the rows, its rows as `R` print and each key a purge let go as `G`.
#[derive(Serialize, Deserialize)]
pub enum DeltaForm<R, G> {
    /// [`Delta::Piece`].
    Piece {
        /// The landings before it.
        landings: u64,
        /// What it brought.
        row: R,
    },
    /// [`Delta::Landed`].
    Landed {
        /// The landings, this one included.
        landings: u64,
        /// What landed.
        row: R,
    },
    /// [`Delta::StartedOver`].
    StartedOver {
        /// The generation started.
        generation: u64,
    },
    /// [`Delta::Forgot`].
    Forgot(Vec<G>),
}

/// `row` as it prints, each node through `node` and its keys through `keys`.
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

/// The row `form` prints, each node read through `node` and its keys through `keys`.
///
/// # Errors
/// Why a node, the keys, a word or the frontier is not one a row holds.
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

/// `delta` as it prints, its rows through `row` and each key it let go through `gone`.
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

/// The delta `form` prints, its rows read through `row` and each key through `gone`.
///
/// # Errors
/// Why a row or a key does not read.
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

/// The 32 bytes 64 hex digits spell. Every name printed here is a [`Hash`](struct@Hash) or a key's name, or
/// was parsed as one where its node arrived, so each is 64 lowercase hex and every digit reads.
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

/// The name 32 bytes spell.
///
/// # Errors
/// Never for 32 bytes; kept as [`Hash::new`]'s refusal.
pub fn hash(bytes: &Name) -> Result<Hash, String> {
    Hash::new(hex::encode(bytes)).map_err(|e| e.to_string())
}

/// The names each 32 bytes spell.
///
/// # Errors
/// [`hash`]'s.
pub fn hashes(names: &[Name]) -> Result<BTreeSet<Hash>, String> {
    names.iter().map(hash).collect()
}
