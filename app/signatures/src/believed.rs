//! What a chain believes at an instant: the Prodrome's register projections, read once for
//! every todo.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::change::mk_change;
use prodrome_core::dag::Dag;
use prodrome_core::event::{Hash, TodoId, mk_sealed, seal_hash};
use prodrome_core::fold;
use prodrome_core::fpl::{self, Candidates, Env, FplError, Outcome};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::{Datetime, ProdromeError};
use prodrome_core::policy::Untrusted;
use prodrome_core::registers;
use prodrome_core::term::Term;

use suzatary_record::record::{Authored, Envelope, Node, TodoEvent};

/// Every todo's candidates at one instant, under one policy.
///
/// A register with more than one candidate is a conflict and reads as all of them: the price is
/// the most urgent world's, and nothing here picks a winner. A reading that can show only one
/// record takes [`Believed::record`], the candidate of the least object name.
#[derive(Debug, Clone, Default)]
pub struct Believed {
    /// Candidate outcomes of every todo not simply open, and the tendings.
    pub env: Env,
    /// Each todo's candidate specs.
    pub specs: BTreeMap<TodoId, Vec<Term>>,
    /// Each todo's candidate records, whoever wrote them.
    pub content: BTreeMap<TodoId, Vec<Authored>>,
    /// Each todo's one function, `Least` over its worlds.
    pub functions: BTreeMap<TodoId, Term>,
    known: BTreeSet<TodoId>,
}

impl Believed {
    /// Fold `nodes` at `at` under `untrusted`, across every prodrome they hold.
    ///
    /// # Errors
    /// A spec that cannot be flattened.
    pub fn of(nodes: &[Node], at: Datetime, untrusted: &Untrusted) -> Result<Believed, FplError> {
        let state = registers::fold(nodes);
        let now = fpl::instant_of(at);
        let mut believed = Believed::default();
        for prodrome in state.prodromes().values() {
            let env = fold::env(prodrome, now, untrusted);
            believed.env.outcomes.extend(env.outcomes);
            believed.env.tended.extend(env.tended);
            believed.specs.extend(fold::specs(prodrome, now, untrusted));
            believed.content.extend(fold::content(prodrome, now));
            believed
                .functions
                .extend(fold::flatten(prodrome, now, untrusted)?);
            believed.known.extend(prodrome.keys().cloned());
        }
        Ok(believed)
    }

    /// The candidate outcomes of `todo`, `{None}` for an open one.
    #[must_use]
    pub fn outcome(&self, todo: &TodoId) -> Candidates {
        self.env
            .outcomes
            .get(todo.as_str())
            .cloned()
            .unwrap_or_else(|| Candidates::from([None]))
    }

    /// Is `todo` open in every world? A conflict with an open candidate is open.
    #[must_use]
    pub fn is_open(&self, todo: &TodoId) -> bool {
        self.outcome(todo).contains(&None)
    }

    /// The one record a single-record reading shows (see the type's note).
    #[must_use]
    pub fn record(&self, todo: &TodoId) -> Option<&Authored> {
        self.content.get(todo).and_then(|records| records.first())
    }

    /// Every function as [`fpl::link`] reads them: `Absent` for a todo the chain knows with none.
    #[must_use]
    pub fn linkable(&self) -> BTreeMap<String, Term> {
        fold::link_specs(&self.functions, &self.known)
    }

    /// `todo`'s value at `at`: `None` for `∅`, a todo with no function, or one that does not link.
    #[must_use]
    pub fn value(&self, todo: &TodoId, at: Datetime) -> Option<f64> {
        let closed = fpl::link(self.functions.get(todo)?, &self.linkable()).ok()?;
        fpl::fulfillment(&closed, fpl::instant_of(at), &self.env)
    }
}

/// Every todo's candidate records at `at`, whoever wrote them: [`Believed::content`] alone.
#[must_use]
pub fn records(nodes: &[Node], at: Datetime) -> BTreeMap<TodoId, Vec<Authored>> {
    let now = fpl::instant_of(at);
    registers::fold(nodes)
        .prodromes()
        .values()
        .flat_map(|prodrome| fold::content(prodrome, now))
        .collect()
}

/// A history in memory, as its objects and the nodes a DAG places them as: what a preview folds
/// before anything is written, and never flushed.
#[derive(Debug, Clone, Default)]
pub struct Preview {
    objects: Vec<(Hash, Envelope)>,
    nodes: Vec<Node>,
}

impl Preview {
    /// The history these objects make.
    ///
    /// # Errors
    /// A cycle, which only a hash collision could make.
    pub fn of(objects: Vec<(Hash, Envelope)>) -> Result<Self, ProdromeError> {
        let nodes = Dag::from_iter(objects.clone()).nodes()?;
        Ok(Self { objects, nodes })
    }

    /// The nodes the folds read.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The objects, the history's own and every one written ahead.
    #[must_use]
    pub fn objects(&self) -> &[(Hash, Envelope)] {
        &self.objects
    }

    /// `event` where the store's `append` would put it: nowhere when an object of the writer's
    /// prodrome already carries it, else a `Change` over its todo's heads, the bytes a store
    /// holding these objects would write. A history with no genesis is begun by one first, as
    /// an empty store is.
    ///
    /// # Errors
    /// The append's refusal, as the store would give it.
    pub fn ahead(&mut self, event: TodoEvent) -> Result<(), ProdromeError> {
        let dag = Dag::from_iter(self.objects.clone());
        // The prodrome the store's own append writes into, so a preview never picks another.
        let home = crate::chain_set::home(&dag).map_err(|several| {
            ProdromeError::Store(format!(
                "the history holds {several} geneses: name the writer's"
            ))
        })?;
        let genesis = if let Some(genesis) = home {
            genesis
        } else {
            let begun = Envelope::Genesis(mk_genesis("preview", &"0".repeat(32))?);
            let name = seal_hash(&begun);
            self.objects.push((name.clone(), begun));
            name
        };
        let prodrome = dag.key(&genesis);
        let folded = registers::fold(&self.nodes);
        let twin = self
            .nodes
            .iter()
            .any(|node| *node.genesis() == prodrome && node.event() == Some(&event));
        if !twin {
            let deps = registers::deps_for(&folded, &prodrome, &event)?;
            let change = Envelope::Change(mk_change(genesis, deps, event)?);
            self.objects.push((seal_hash(&change), change));
        }
        *self = Self::of(std::mem::take(&mut self.objects))?;
        Ok(())
    }

    /// A list of events as a legacy chain holds them, each object on the one before: what a log
    /// built in memory means to the folds.
    ///
    /// # Panics
    /// Never: each object rests on the one before it, which is held.
    #[must_use]
    pub fn chain(events: &[TodoEvent]) -> Self {
        let mut prev = None;
        let objects = events
            .iter()
            .map(|event| {
                let object = mk_sealed(prev.take(), event.clone());
                let name = seal_hash(&object);
                prev = Some(name.clone());
                (name, object)
            })
            .collect();
        Self::of(objects).expect("a chain")
    }
}

/// The one outcome a reading that shows one takes: open while any candidate is, else the least
/// binding.
#[must_use]
pub fn one_outcome(candidates: &Candidates) -> Option<Outcome> {
    if candidates.contains(&None) {
        None
    } else {
        candidates.iter().flatten().next().copied()
    }
}

/// The candidates as the core's `Entry::state` spells them: lowercased, `open` for none, and
/// `|` between the candidates of a conflict.
#[must_use]
pub fn spelled(candidates: &Candidates) -> String {
    candidates
        .iter()
        .map(|outcome| match outcome {
            None => "open",
            Some(Outcome::Completed(_)) => "completed",
            Some(Outcome::Cancelled(_)) => "cancelled",
        })
        .collect::<Vec<_>>()
        .join("|")
}
