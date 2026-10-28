//! What makes this deployment the owner's, as a value: the binary parses `instance.toml` and
//! hands a [`Deployment`] down, so no library code ever reads a configuration file.
//!
//! Only what is true of the box lives here (the tailnet door, the vocabulary, the URL); the
//! trust policy and roster belong to each chain ([`crate::chains::ChainPolicy`]). The rules the
//! fields carry are applied in [`crate::gate`].

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;

use crate::capability::Login;
use crate::chains::{ChainPolicy, Chains};

/// This deployment, as the application reads it.
///
/// Ordered maps and sets throughout, so a payload that carries the roster is byte-identical
/// between two runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    /// Who may open the app over the shared tailnet: Tailscale logins, not actors.
    pub viewers: BTreeSet<Login>,
    /// Who may write, and as which actor. The value is what lands in an event's `actor`, never
    /// a field of the request, so a page cannot claim an actor.
    pub actors: BTreeMap<Login, Actor>,
    /// Every chain this deployment serves, in the instance file's order; the first is what an
    /// unnamed chain means.
    pub chains: Chains,
    /// The category vocabulary, name to its stable id. A new todo's `category` must be one of these.
    pub categories: BTreeMap<String, u32>,
    /// Where the app answers, as a phone reaches it. A notification's click
    /// URL and the app's deep links are built from this.
    pub app_url: String,
    /// `[coding]`, when this deployment starts coding sessions; `None` refuses a tap on one.
    pub coding: Option<crate::coding::Coding>,
    /// The owner's zone, `[persona]`'s: where every day the deployment reads is read.
    pub zone: crate::zone::Zone,
}

impl Deployment {
    /// Every actor a login writes as: who can mint a tap, and so whose word about standing the
    /// digest believes.
    #[must_use]
    pub fn writers(&self) -> BTreeSet<Actor> {
        self.actors.values().cloned().collect()
    }

    /// The category vocabulary in its order, by stable id.
    #[must_use]
    pub fn category_order(&self) -> Vec<String> {
        crate::decoration::categories_in_order(&self.categories)
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// The actor this peer writes as, or `None` when it may only read; there is no other way to
    /// obtain an actor for a request.
    #[must_use]
    pub fn actor_of(&self, login: &Login) -> Option<&Actor> {
        self.actors.get(login)
    }

    /// May this login open the app at all?
    #[must_use]
    pub fn may_view(&self, login: &Login) -> bool {
        self.viewers.contains(login)
    }

    /// Writers whose login the gate would refuse: dead config, checked at startup against the
    /// actual file rather than asserted in a test.
    #[must_use]
    pub fn dead_writers(&self) -> Vec<&Login> {
        self.actors
            .keys()
            .filter(|login| !self.viewers.contains(*login))
            .collect()
    }

    /// Writers some chain's roster does not know, per chain, since a write goes to one store.
    #[must_use]
    pub fn unrostered_writers(&self) -> Vec<(&str, &Actor)> {
        self.chains
            .iter()
            .flat_map(|chain| {
                self.actors
                    .values()
                    .filter(|actor| !chain.roster.contains(*actor))
                    .map(|actor| (chain.name.as_str(), actor))
            })
            .collect()
    }

    /// The chain a request or a verb named, or the first when it named none; `None` is a name
    /// this deployment does not serve.
    #[must_use]
    pub fn chain(&self, named: Option<&str>) -> Option<&ChainPolicy> {
        match named {
            None => Some(self.chains.first()),
            Some(name) => self.chains.get(name),
        }
    }

    /// The routine "Build it" fires for a todo of `chain`: `[coding]`'s, on the chain whose todos a
    /// pull request's `Completes:` line closes.
    ///
    /// # Errors
    /// [`crate::refusal::Refusal::conflict`] without `[coding]` or for any other chain.
    pub fn builder(
        &self,
        chain: &crate::chains::ChainName,
    ) -> Result<&crate::coding::RoutineId, crate::refusal::Refusal> {
        let coding = self.coding.as_ref().ok_or_else(|| {
            crate::refusal::Refusal::conflict(
                "this deployment declares no [coding], so no todo is built",
            )
        })?;
        let plan = crate::release::Plan::declared()
            .map_err(|failed| crate::refusal::Refusal::internal(failed.to_string()))?;
        if chain.as_str() == plan.chain() {
            Ok(&coding.routine)
        } else {
            Err(crate::refusal::Refusal::conflict(format!(
                "only todos of {} are built: a pull request's Completes: line closes those",
                plan.chain()
            )))
        }
    }

    /// This project's own chain: second in the instance file's order of meaning, or the only
    /// one a deployment of one chain has.
    #[must_use]
    pub fn project(&self) -> &ChainPolicy {
        self.chains.iter().nth(1).unwrap_or(self.chains.first())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(name: &str) -> Actor {
        Actor::new(name).expect("a roster name")
    }

    fn chain(name: &str) -> crate::chains::ChainPolicy {
        crate::chains::ChainPolicy {
            name: crate::chains::ChainName::new(name).expect("a slug"),
            untrusted: prodrome_core::policy::Untrusted::of([actor("triage")]),
            roster: [actor("ada"), actor("triage")].into_iter().collect(),
            root: crate::cast::keys::root(),
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
            zone: crate::zone::Zone::UTC,
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
