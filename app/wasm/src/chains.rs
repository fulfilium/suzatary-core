//! The todo chains a tab holds, as the set every row and proposal is priced through
//! (`suzatary_signatures::chain_set`): each chain's objects read once, its name and its policy, and the
//! set read at the instant a reading asks for, each chain's reading there folded once until the
//! chain is held again. A chain whose objects or policy do not read is held as that failure, so
//! it moves nothing that does not refer into it and never refuses an act.

use std::collections::BTreeMap;

use prodrome_core::fpl::{Stores, datetime_of, link_in};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use prodrome_core::term::Term;
use prodrome_wasm_exports::wire::{parse_instant, parse_specs, parse_term, parse_untrusted};
use prodrome_wasm_exports::{Replica, json};
use suzatary_base::chains::ChainName;
use suzatary_signatures::chain_set::{self, ChainSet, Remembered};
use suzatary_signatures::wire::signatures::FailedChain;
use wasm_bindgen::prelude::*;

use crate::{Event, json, printed, refused};

/// One chain's replica and policy, or why they did not read.
type Held = Result<(Replica<Event>, Untrusted), String>;

/// Every chain the tab holds, by name, each beside its last reading.
#[wasm_bindgen]
#[derive(Default)]
pub struct Chains(Vec<(ChainName, Held, Remembered)>);

#[wasm_bindgen]
impl Chains {
    /// No chain: every reference into one reads `∅`.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Chains {
        Chains::default()
    }

    /// Hold the chain `name`: its objects, `[{hash, text}]`, read once, under `untrusted` (a
    /// JSON array of actor names, as the box reports it with the objects). Objects or a policy
    /// that do not read are held as that failure; only a name that is no chain's is refused.
    /// Answers what was held: `{excluded}`, what the chain's reading leaves out as `tips`
    /// answers it, or `{why}`, why the chain did not read.
    pub fn hold(&mut self, name: &str, objects: &str, untrusted: &str) -> Result<String, JsError> {
        let name = ChainName::new(name).map_err(|e| refused(e.to_string()))?;
        let read = Replica::of(objects)
            .and_then(|replica| parse_untrusted(untrusted).map(|untrusted| (replica, untrusted)));
        let answer = read
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|(replica, _)| prodrome_wasm_exports::tips(replica))
            .unwrap_or_else(|why| serde_json::json!({ "why": why }).to_string());
        self.0.retain(|(held, ..)| *held != name);
        self.0.push((name, read, Remembered::default()));
        Ok(answer)
    }

    /// Hold the chain `name` no more: every reference into it reads `∅`.
    pub fn drop(&mut self, name: &str) {
        self.0.retain(|(held, ..)| held.as_str() != name);
    }

    /// The set these chains make at `at` (ISO), read once, to link every term of one reading
    /// in a chain whose functions are `specs` (`{todo: term}`). The caller frees it.
    pub fn linker(&self, specs: &str, at: &str) -> Result<Linker, JsError> {
        let specs = parse_specs(specs).map_err(refused)?;
        let at = datetime_of(parse_instant("at", at).map_err(refused)?)
            .map_err(|e| refused(format!("at: {e}")))?;
        let set = self.set(at, None);
        Ok(Linker {
            specs,
            failed: set.failed(),
            stores: set.into_stores(),
        })
    }
}

/// The set at one instant, read once, and the functions of the chain a term is read in.
#[wasm_bindgen]
pub struct Linker {
    specs: BTreeMap<String, Term>,
    stores: Stores,
    failed: Vec<FailedChain>,
}

#[wasm_bindgen]
impl Linker {
    /// `term` with every reference bound: a bare one to the chain's functions, a qualified one to
    /// its chain's function in the set, compiled against that chain's own environment, as the box
    /// binds a row. A chain not held, or held as a failure, reads `∅`; a loop is the refusal.
    pub fn link(&self, term: &str) -> Result<String, JsError> {
        let term = parse_term("term", term).map_err(refused)?;
        let linked =
            link_in(&term, &self.specs, &self.stores).map_err(|e| refused(format!("link: {e}")))?;
        printed(&json::to_json(linked.term()))
    }

    /// Every chain this set holds as a failure, `[{chain, why}]`: what a reference into it read
    /// `∅` for, from the same reading as every link.
    pub fn failed(&self) -> Result<String, JsError> {
        json(&self.failed)
    }
}

impl Chains {
    /// The set at `at`: these chains, with `own` in place of the one of its name.
    pub(crate) fn set<'s>(
        &'s self,
        at: Datetime,
        own: Option<chain_set::Held<'s>>,
    ) -> ChainSet<'s> {
        let others = self
            .0
            .iter()
            .filter(|(name, ..)| own.is_none_or(|own| own.name != name));
        let set = others.fold(
            ChainSet::new(at),
            |set, (name, read, remembered)| match read
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|(replica, untrusted)| replica.read().map(|read| (read, untrusted)))
            {
                Ok((read, untrusted)) => set.hold_remembered(
                    chain_set::Held {
                        name,
                        dag: read.dag,
                        nodes: read.nodes,
                        untrusted,
                    },
                    remembered,
                ),
                Err(why) => set.fail(name, why),
            },
        );
        match own {
            Some(own) => set.hold(own),
            None => set,
        }
    }
}
