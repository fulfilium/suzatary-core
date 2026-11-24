//! What a wait points at: one rule from the store it is written in, its reference and the stores
//! held, to a blocker or nothing ([`blocker`]).

use prodrome_data::event::Hash;
use serde::Serialize;
use suzatary_base::chains::ChainName;
use suzatary_base::reference::{Found, GlobalTodo, Reference, TodoRef};

/// Who reads a store, and so whose words it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// This deployment alone (a private chain): its chain names are the deployment's.
    Deployment,
    /// Anyone (a public or a followed store): anyone may have written its words, and a
    /// deployment's chain name there means nothing.
    Anyone,
}

/// Whose a store is and who reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    /// This deployment's own chain, read by it alone.
    Private,
    /// This deployment's own chain, published.
    Public,
    /// Another repository's store, copied here.
    Followed,
}

impl Kept {
    #[must_use]
    pub fn audience(self) -> Audience {
        match self {
            Kept::Private => Audience::Deployment,
            Kept::Public | Kept::Followed => Audience::Anyone,
        }
    }

    /// Whether the store is this deployment's own chain.
    #[must_use]
    pub fn own(self) -> bool {
        match self {
            Kept::Private | Kept::Public => true,
            Kept::Followed => false,
        }
    }
}

/// Where a wait points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wait {
    /// The task, in the store that holds it, which counts if it is open.
    On(TodoRef),
    /// Text: no todo, or a deployment's name in a store anyone reads.
    Label,
    /// A todo it names and does not move, which is said.
    Missing(Missing),
}

/// Why a reference to a todo moves no task held here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "type")]
pub enum Missing {
    /// No store held is the one it names, or that store holds nothing yet.
    Unheld,
    /// Its genesis prefix starts each of these held geneses, not one alone of them begun by this
    /// deployment's own chain, so it names none.
    Ambiguous { geneses: Vec<String> },
    /// These held stores begin with the one genesis it names, not one alone of them this
    /// deployment's own chain, so it names none of them.
    Replicas { stores: Vec<ChainName> },
    /// Words anyone may write name a task of a private chain, which they do not move.
    Private,
}

/// The stores held, as a wait reads them: each by this deployment's name for it, with the
/// prodrome it begins and whose it is.
#[derive(Debug, Clone, Default)]
pub struct Held {
    stores: Vec<(ChainName, Hash, Kept)>,
}

impl Held {
    /// These stores and `name`, the deployment's name for a store that begins `prodrome`.
    #[must_use]
    pub fn with(mut self, name: ChainName, prodrome: Hash, kept: Kept) -> Held {
        self.stores.push((name, prodrome, kept));
        self
    }
}

/// The rule.
/// - A todo named by a chain counts only where the deployment is the audience, and then in
///   exactly that chain; where anyone reads the store it is a label.
/// - A todo named by its genesis counts in a store that begins that genesis. This deployment's
///   own chain wins over a followed store: where a prefix starts several held geneses, it names
///   the one begun by an own chain, and where several held stores begin the genesis (replicas
///   of one store), it names the one own chain among them; otherwise it names none.
/// - Words anyone may write move no private chain's task: written where the audience is
///   anyone, it counts only into a store anyone reads.
#[must_use]
pub fn blocker(written_in: Audience, reference: &Reference, held: &Held) -> Wait {
    match reference {
        Reference::Global(global) => match by_genesis(written_in, global, held) {
            Ok(todo) => Wait::On(todo),
            Err(missing) => Wait::Missing(missing),
        },
        Reference::Todo(local) => match written_in {
            Audience::Anyone => Wait::Label,
            Audience::Deployment if held.stores.iter().any(|(name, ..)| *name == local.chain) => {
                Wait::On(local.clone())
            }
            Audience::Deployment => Wait::Missing(Missing::Unheld),
        },
        Reference::Mail { .. }
        | Reference::Thread { .. }
        | Reference::Briefing { .. }
        | Reference::Proposal(_)
        | Reference::Batch { .. }
        | Reference::Target(_) => Wait::Label,
    }
}

/// The task `global` names, written where `written_in` says, by [`blocker`]'s rule.
///
/// # Errors
/// Why it moves no task held here.
pub fn by_genesis(
    written_in: Audience,
    global: &GlobalTodo,
    held: &Held,
) -> Result<TodoRef, Missing> {
    let own = |prodrome: &Hash| {
        held.stores
            .iter()
            .any(|(_, begins, kept)| begins == prodrome && kept.own())
    };
    let prodrome = match global.genesis.among(held.stores.iter().map(|(_, p, _)| p)) {
        Found::One(prodrome) => prodrome,
        Found::Unheld => return Err(Missing::Unheld),
        Found::Several(geneses) => {
            match geneses.iter().filter(|g| own(g)).collect::<Vec<_>>()[..] {
                [one] => *one,
                _ => {
                    return Err(Missing::Ambiguous {
                        geneses: geneses.iter().map(|g| g.as_str().to_owned()).collect(),
                    });
                }
            }
        }
    };
    let readable: Vec<&(ChainName, Hash, Kept)> = held
        .stores
        .iter()
        .filter(|(_, begins, kept)| {
            begins == prodrome
                && (written_in == Audience::Deployment || kept.audience() == Audience::Anyone)
        })
        .collect();
    let owned: Vec<&&(ChainName, Hash, Kept)> =
        readable.iter().filter(|(_, _, kept)| kept.own()).collect();
    let chain = match (owned.as_slice(), readable.as_slice()) {
        (_, []) => return Err(Missing::Private),
        ([(one, ..)], _) | (_, [(one, ..)]) => one,
        (_, several) => {
            return Err(Missing::Replicas {
                stores: several.iter().map(|(name, ..)| name.clone()).collect(),
            });
        }
    };
    Ok(TodoRef {
        chain: chain.clone(),
        id: global.id.clone(),
    })
}
