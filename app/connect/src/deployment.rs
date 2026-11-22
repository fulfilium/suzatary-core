//! The deployment as a value: the binary parses `instance.toml`, so no library code reads a
//! configuration file.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;

use crate::connect::{Coding, RoutineId};
use suzatary_base::capability::Login;
use suzatary_base::chains::{ChainName, ChainPolicy, Chains};
use suzatary_base::refusal::Refusal;

/// Ordered maps and sets throughout, so a payload that carries the roster is byte-identical
/// between two runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    /// Who may open the app over the shared tailnet: Tailscale logins, not actors.
    pub viewers: BTreeSet<Login>,
    /// The value is an event's `actor`, never a request field, so a page cannot claim an actor.
    pub actors: BTreeMap<Login, Actor>,
    /// In the instance file's order; the first is what an unnamed chain means.
    pub chains: Chains,
    /// Name to stable id; a new todo's `category` must be one of these.
    pub categories: BTreeMap<String, u32>,
    /// Where a phone reaches the app; notification clicks and deep links are built from it.
    pub app_url: String,
    /// `None` refuses a tap that starts a coding session.
    pub coding: Option<Coding>,
    /// `[upkeep] chain`: where the box files todos about itself; `None` files none.
    pub upkeep: Option<ChainName>,
    /// `[persona] zone`: every day the deployment reads is a day in this zone.
    pub zone: suzatary_base::zone::Zone,
}

impl Deployment {
    /// Every actor a login writes as: who can mint a tap, so whose word the digest believes.
    #[must_use]
    pub fn writers(&self) -> BTreeSet<Actor> {
        self.actors.values().cloned().collect()
    }

    /// By stable id.
    #[must_use]
    pub fn category_order(&self) -> Vec<String> {
        suzatary_record::decoration::categories_in_order(&self.categories)
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// `None` when it may only read; this is the only way to obtain an actor for a request.
    #[must_use]
    pub fn actor_of(&self, login: &Login) -> Option<&Actor> {
        self.actors.get(login)
    }

    #[must_use]
    pub fn may_view(&self, login: &Login) -> bool {
        self.viewers.contains(login)
    }

    /// Every line of `[web.act_as]` that stops startup: a login that is no viewer, or an actor off
    /// a chain's roster or untrusted by it; on a public chain the actor's public name is tested.
    #[must_use]
    pub fn refused_writers(&self) -> Vec<String> {
        let unseen = self
            .actors
            .keys()
            .filter(|login| !self.viewers.contains(*login));
        let unseen =
            unseen.map(|login| format!("{:?} may write but is not a viewer", login.as_str()));
        let per_chain = self.chains.iter().flat_map(|policy| {
            self.actors.iter().filter_map(move |(login, actor)| {
                let actor = &policy.named(actor).ok()?;
                let whom = if !policy.roster.contains(actor) {
                    "'s roster does not name"
                } else if policy.untrusted.actors().contains(actor) {
                    " does not trust"
                } else {
                    return None;
                };
                let (login, actor, chain) = (login.as_str(), actor.as_str(), policy.name.as_str());
                Some(format!(
                    "{login:?} writes as {actor:?}, whom chain {chain:?}{whom}"
                ))
            })
        });
        unseen.chain(per_chain).collect()
    }

    /// The first chain when none is named; `None` for a name this deployment does not serve.
    #[must_use]
    pub fn chain(&self, named: Option<&str>) -> Option<&ChainPolicy> {
        match named {
            None => Some(self.chains.first()),
            Some(name) => self.chains.get(name),
        }
    }

    /// The routine "Build it" fires, only for a todo of `plan_store`, whose todos a pull request's
    /// `Completes:` line closes.
    pub fn builder(
        &self,
        chain: &ChainName,
        plan_store: Option<&ChainName>,
    ) -> Result<&RoutineId, Refusal> {
        let coding = self.coding.as_ref().ok_or_else(|| {
            Refusal::conflict("this deployment declares no [coding], so no todo is built")
        })?;
        match plan_store {
            Some(store) if store == chain => Ok(&coding.routine),
            Some(store) => Err(Refusal::conflict(format!(
                "only todos of {store} are built: a pull request's Completes: line closes those"
            ))),
            None => Err(Refusal::conflict(
                "no chain this deployment mounts holds a release plan, so no todo is built",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str) -> Actor {
        Actor::new(name).expect("a roster name")
    }

    fn chain(name: &str) -> suzatary_base::chains::ChainPolicy {
        suzatary_base::chains::ChainPolicy {
            name: suzatary_base::chains::ChainName::new(name).expect("a slug"),
            untrusted: prodrome_core::policy::Untrusted::of([actor("triage")]),
            roster: [actor("ada"), actor("triage")].into_iter().collect(),
            root: crate::cast::keys::root(),
            naming: suzatary_base::chains::Naming::AsActs,
        }
    }

    fn deployment() -> Deployment {
        Deployment {
            viewers: [Login::from_whois("her@example.test")]
                .into_iter()
                .collect(),
            actors: [(Login::from_whois("her@example.test"), actor("ada"))]
                .into_iter()
                .collect(),
            chains: Chains::of(vec![chain("life"), chain("suzatary")]).expect("two chains"),
            categories: [("home".to_owned(), 3)].into_iter().collect(),
            app_url: "https://example.test".to_owned(),
            coding: None,
            upkeep: None,
            zone: suzatary_base::zone::Zone::UTC,
        }
    }

    #[test]
    fn the_actor_is_the_mappings_value_and_there_is_no_other_door() {
        let deployed = deployment();
        let hers = Login::from_whois("her@example.test");
        let stranger = Login::from_whois("someone@else.test");
        assert_eq!(deployed.actor_of(&hers).map(Actor::as_str), Some("ada"));
        assert_eq!(deployed.actor_of(&stranger), None);
        assert!(deployed.may_view(&hers));
        assert!(!deployed.may_view(&stranger));
    }
}
