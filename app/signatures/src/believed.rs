//! What a chain believes at an instant: the register projections, read once for every todo.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::dag::Dag;
use prodrome_dag::registers;
use prodrome_data::change::mk_change;
use prodrome_data::event::{Hash, TodoId, mk_sealed, seal_hash};
use prodrome_data::genesis::mk_genesis;
use prodrome_data::instant::FplError;
use prodrome_data::literal::{Datetime, ProdromeError};
use prodrome_data::policy::Untrusted;
use prodrome_fpl::fpl::{self};
use prodrome_reading::fold;
use prodrome_term::fpl::{Candidates, Env, Outcome};
use prodrome_term::term::Term;

use suzatary_record::record::{Authored, Envelope, Node, TodoEvent};

/// Every todo's candidates at one instant, under one policy.
///
/// A conflict reads as all its candidates and prices as the most urgent world; a single-record
/// reading takes [`Believed::record`], the candidate of the least object name.
#[derive(Debug, Clone, Default)]
pub struct Believed {
    /// Candidate outcomes of every todo not simply open, and the tendings.
    pub env: Env,
    pub specs: BTreeMap<TodoId, Vec<Term>>,
    /// Each todo's candidate records, whoever wrote them.
    pub content: BTreeMap<TodoId, Vec<Authored>>,
    /// Each todo's one function, `Least` over its worlds.
    pub functions: BTreeMap<TodoId, Term>,
    known: BTreeSet<TodoId>,
}

impl Believed {
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

    /// A conflict with an open candidate is open.
    #[must_use]
    pub fn is_open(&self, todo: &TodoId) -> bool {
        self.outcome(todo).contains(&None)
    }

    /// The candidate of the least object name.
    #[must_use]
    pub fn record(&self, todo: &TodoId) -> Option<&Authored> {
        self.content.get(todo).and_then(|records| records.first())
    }

    /// Every function as [`fpl::link`] reads them: `Absent` for a todo the chain knows with none.
    #[must_use]
    pub fn linkable(&self) -> BTreeMap<String, Term> {
        fold::link_specs(&self.functions, &self.known)
    }

    /// `None` for `∅`, a todo with no function, or one that does not link.
    #[must_use]
    pub fn value(&self, todo: &TodoId, at: Datetime) -> Option<f64> {
        let closed = fpl::link(self.functions.get(todo)?, &self.linkable()).ok()?;
        fpl::fulfillment(&closed, fpl::instant_of(at), &self.env)
    }
}

/// [`Believed::content`] alone.
#[must_use]
pub fn records(nodes: &[Node], at: Datetime) -> BTreeMap<TodoId, Vec<Authored>> {
    let now = fpl::instant_of(at);
    registers::fold(nodes)
        .prodromes()
        .values()
        .flat_map(|prodrome| fold::content(prodrome, now))
        .collect()
}

/// A history in memory that a preview folds before anything is written; never flushed.
#[derive(Debug, Clone, Default)]
pub struct Preview {
    objects: Vec<(Hash, Envelope)>,
    nodes: Vec<Node>,
}

impl Preview {
    /// # Errors
    /// A cycle, which only a hash collision could make.
    pub fn of(objects: Vec<(Hash, Envelope)>) -> Result<Self, ProdromeError> {
        let nodes = Dag::from_iter(objects.clone()).nodes()?;
        Ok(Self { objects, nodes })
    }

    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The history's own objects and every one written ahead.
    #[must_use]
    pub fn objects(&self) -> &[(Hash, Envelope)] {
        &self.objects
    }

    /// Writes `event` as the store's `append` would: nothing for a twin, else a `Change` over its
    /// todo's heads, after a `Genesis` for an empty history.
    pub fn ahead(&mut self, event: TodoEvent) -> Result<(), ProdromeError> {
        let dag = Dag::from_iter(self.objects.clone());
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

    /// Events as a legacy chain holds them, each object on the one before.
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

/// Open while any candidate is, else the least binding.
#[must_use]
pub fn one_outcome(candidates: &Candidates) -> Option<Outcome> {
    if candidates.contains(&None) {
        None
    } else {
        candidates.iter().flatten().next().copied()
    }
}

/// As the core's `Entry::state` spells them: `open` for none, `|` between conflicting candidates.
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
