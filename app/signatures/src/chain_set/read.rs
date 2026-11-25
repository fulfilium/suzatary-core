//! The chains read once per request, so every reading sees one instant of every chain; a chain is
//! read again only when its heads moved.

use std::sync::Arc;

use prodrome_dag::dag::Dag;
use prodrome_dag::registers::Node;
use prodrome_data::event::{Envelope, Hash};
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;

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
    /// The store's heads when it was read; `None` for objects handed over or unread heads.
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

    fn is(&self, policy: &ChainPolicy, heads: &[Hash]) -> bool {
        self.name == policy.name
            && self.untrusted == policy.untrusted
            && self.heads.as_deref() == Some(heads)
    }
}

impl Read {
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

    #[must_use]
    pub fn with(
        mut self,
        name: ChainName,
        untrusted: Untrusted,
        objects: Result<Vec<(Hash, Envelope<TodoEvent>)>, String>,
    ) -> Read {
        self.0
            .push(Arc::new(Chain::of(name, untrusted, None, objects)));
        self
    }

    /// A store that does not read is held as that failure, never a refusal of the others.
    #[must_use]
    pub fn of_stores<'s>(
        chains: impl IntoIterator<Item = (&'s ChainPolicy, &'s dyn Prodrome<TodoEvent>)>,
    ) -> Read {
        Read::default().moved(chains)
    }

    /// Keeps each chain whose heads and policy did not move, and reads every other again.
    #[must_use]
    pub fn moved<'s>(
        &self,
        chains: impl IntoIterator<Item = (&'s ChainPolicy, &'s dyn Prodrome<TodoEvent>)>,
    ) -> Read {
        Read(
            chains
                .into_iter()
                .map(|(policy, store)| {
                    // Heads first: a write between the two keeps under older heads, so the next
                    // ask reads again.
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

    /// Whether `other` holds the same reading of chain `name`, not a copy.
    #[cfg(any(test, feature = "nothing"))]
    #[must_use]
    pub fn shares(&self, other: &Read, name: &ChainName) -> bool {
        let of = |read: &Read| read.0.iter().find(|chain| chain.name == *name).cloned();
        matches!((of(self), of(other)), (Some(a), Some(b)) if Arc::ptr_eq(&a, &b))
    }
}
