//! The chains read once, so every reading of one request (a page's rows, a proposal's price, the
//! document) sees one instant of every chain and folds each once; and read again only where a
//! chain's heads moved, so a write to one chain re-reads that chain alone.

use std::sync::Arc;

use prodrome_core::dag::Dag;
use prodrome_core::event::{Envelope, Hash};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use prodrome_core::registers::Node;

use super::{ChainSet, Held, Remembered};
use suzatary_base::chains::{ChainName, ChainPolicy};
use suzatary_record::record::TodoEvent;
use suzatary_store::store::Prodrome;

/// Every chain's objects and their nodes, or why they did not read.
#[derive(Debug, Clone, Default)]
pub struct Read(Vec<Arc<Chain>>);

#[derive(Debug)]
struct Chain {
    name: ChainName,
    untrusted: Untrusted,
    /// The store's heads when it was read, which name these objects; none where they were
    /// handed over rather than read, or the heads did not read, so nothing is kept by them.
    heads: Option<Vec<Hash>>,
    dag: Dag<TodoEvent>,
    nodes: Result<Vec<Node<TodoEvent>>, String>,
    remembered: Remembered,
}

impl Chain {
    fn of(
        name: ChainName,
        untrusted: Untrusted,
        heads: Option<Vec<Hash>>,
        objects: Result<Vec<(Hash, Envelope<TodoEvent>)>, String>,
    ) -> Chain {
        let (dag, nodes) = match objects {
            Ok(objects) => {
                let dag = Dag::from_iter(objects);
                let nodes = dag.nodes().map_err(|failed| failed.to_string());
                (dag, nodes)
            }
            Err(failed) => (Dag::from_iter(Vec::new()), Err(failed)),
        };
        Chain {
            name,
            untrusted,
            heads,
            dag,
            nodes,
            remembered: Remembered::default(),
        }
    }

    /// Whether this is `policy`'s chain as its store's `heads` name it.
    fn is(&self, policy: &ChainPolicy, heads: &[Hash]) -> bool {
        self.name == policy.name
            && self.untrusted == policy.untrusted
            && self.heads.as_deref() == Some(heads)
    }
}

impl Read {
    /// Each chain's objects as given, ordered once.
    #[must_use]
    pub fn of_objects(
        chains: impl IntoIterator<Item = (ChainName, Untrusted, Vec<(Hash, Envelope<TodoEvent>)>)>,
    ) -> Read {
        Read(
            chains
                .into_iter()
                .map(|(name, untrusted, objects)| {
                    Arc::new(Chain::of(name, untrusted, None, Ok(objects)))
                })
                .collect(),
        )
    }

    /// Each store read once, under its chain's policy; a store that does not read is a chain
    /// held as that failure, never a refusal of the others.
    #[must_use]
    pub fn of_stores<'s>(
        chains: impl IntoIterator<Item = (&'s ChainPolicy, &'s dyn Prodrome<TodoEvent>)>,
    ) -> Read {
        Read::default().moved(chains)
    }

    /// These chains as the stores hold them now: a chain whose store's heads and policy are
    /// what this read was taken at is this read's, its readings kept; every other is read again.
    #[must_use]
    pub fn moved<'s>(
        &self,
        chains: impl IntoIterator<Item = (&'s ChainPolicy, &'s dyn Prodrome<TodoEvent>)>,
    ) -> Read {
        Read(
            chains
                .into_iter()
                .map(|(policy, store)| {
                    // Heads first: objects written between the two are read now and kept under
                    // the older heads, so the next ask sees the heads moved and reads again.
                    let heads = store.tips().ok();
                    let kept = heads
                        .as_deref()
                        .and_then(|heads| self.0.iter().find(|chain| chain.is(policy, heads)));
                    kept.cloned().unwrap_or_else(|| {
                        Arc::new(Chain::of(
                            policy.name.clone(),
                            policy.untrusted.clone(),
                            heads,
                            store.objects().map_err(|failed| failed.to_string()),
                        ))
                    })
                })
                .collect(),
        )
    }

    /// The set these chains make at `at`, each chain's reading there folded once.
    #[must_use]
    pub fn set(&self, at: Datetime) -> ChainSet<'_> {
        self.0
            .iter()
            .fold(ChainSet::new(at), |set, chain| match &chain.nodes {
                Ok(nodes) => set.hold_remembered(
                    Held {
                        name: &chain.name,
                        dag: &chain.dag,
                        nodes,
                        untrusted: &chain.untrusted,
                    },
                    &chain.remembered,
                ),
                Err(why) => set.fail(&chain.name, why.as_str()),
            })
    }

    /// Whether `other` holds this read's chain `name`, the same reading, not a copy of it.
    #[cfg(any(test, feature = "nothing"))]
    #[must_use]
    pub fn shares(&self, other: &Read, name: &ChainName) -> bool {
        let of = |read: &Read| read.0.iter().find(|chain| chain.name == *name).cloned();
        matches!((of(self), of(other)), (Some(a), Some(b)) if Arc::ptr_eq(&a, &b))
    }
}
