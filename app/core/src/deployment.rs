//! What makes this deployment the owner's, as a value: the binary parses `instance.toml` and
//! hands a [`Deployment`] down, so no library code ever reads a configuration file.
//!
//! Only what is true of the box lives here (the tailnet door, the vocabulary, the URL); the
//! trust policy and roster belong to each chain ([`suzatary_base::chains::ChainPolicy`]). The rules the
//! fields carry are applied in `suzatary_api::gate`.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;

use crate::connect::{Coding, RoutineId};
use suzatary_base::capability::Login;
use suzatary_base::chains::{ChainPolicy, Chains};
use suzatary_base::refusal::Refusal;

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
    pub coding: Option<Coding>,
    /// The owner's zone, `[persona]`'s: where every day the deployment reads is read.
    pub zone: suzatary_base::zone::Zone,
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

    /// Every line of `[web.act_as]` that stops startup, as the sentence that says why: a login
    /// that is no viewer, or an actor off a chain's roster or one the chain does not trust (a
    /// person would write and tap as, say, the triage agent).
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
    /// [`Refusal::conflict`] without `[coding]` or for any other chain.
    pub fn builder(&self, chain: &suzatary_base::chains::ChainName) -> Result<&RoutineId, Refusal> {
        let coding = self.coding.as_ref().ok_or_else(|| {
            Refusal::conflict("this deployment declares no [coding], so no todo is built")
        })?;
        let plan = crate::release::Plan::declared()
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        if chain.as_str() == plan.chain() {
            Ok(&coding.routine)
        } else {
            Err(Refusal::conflict(format!(
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

    fn chain(name: &str) -> suzatary_base::chains::ChainPolicy {
        suzatary_base::chains::ChainPolicy {
            name: suzatary_base::chains::ChainName::new(name).expect("a slug"),
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
