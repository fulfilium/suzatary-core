//! The rule over the model: when the deployment's model does not answer, the box asks for it
//! itself. The setup agent needs a model to run, so connecting it is the one step no model
//! guides.
//!
//! What it owes, and only while the model does not answer: the subscription's sign-in. A sign-in
//! the box holds open is owed its code card whatever answers, until the session ends. Each card's
//! words name the episode, the last instant the model answered, so a model that answers and
//! later stops (an expired token) files the same card again, and the owner's no stands within
//! one episode.

use crate::mail::health::{Healths, Instant, Reading};
use crate::secretary::acts::Act;
use crate::secretary::noticed::Owed;
use crate::secretary::sign_in::SignInUrl;
use crate::secretary::store::Proposal;
use crate::settings::{Settable, Settables};

/// What the box knows of its model.
#[derive(Clone, Copy)]
pub struct Answering<'a> {
    /// The model's name, as [`crate::capability::Model::name`] spells it: the key its calls are
    /// recorded under.
    pub model: &'a str,
    /// The model's observed calls.
    pub health: &'a Healths,
    /// Every secret name declared, with what declares it.
    pub settables: &'a Settables,
    /// Whether a declared name resolves.
    pub resolves: &'a dyn Fn(&Settable) -> bool,
}

impl Answering<'_> {
    /// The secret the model reads.
    pub(crate) fn credential(&self) -> Option<Settable> {
        self.settables.model()
    }

    /// Whether the model answers: its latest observed call succeeded or, never observed, the
    /// credential it reads resolves.
    #[must_use]
    pub fn any(&self) -> bool {
        match self.health.reading(self.model) {
            Some(Reading::Ok { .. }) => true,
            Some(Reading::Failing { .. }) => false,
            None => self.credential().is_some_and(|name| (self.resolves)(&name)),
        }
    }

    /// The last instant the model answered: the episode a card belongs to.
    #[must_use]
    pub fn since(&self) -> Option<Instant> {
        self.health.succeeded()
    }
}

/// The cards the model owes: each open sign-in's code card, and while it does not answer, the
/// subscription's sign-in.
#[must_use]
pub fn owed(answering: &Answering<'_>, open: &[SignInUrl]) -> Owed {
    let codes = open
        .iter()
        .map(|url| {
            Owed::one(
                Proposal::Act(Act::SignInCode { url: url.clone() }),
                "a sign-in waits for the code its page shows",
            )
        })
        .fold(Owed::default(), Owed::join);
    if answering.any() {
        return codes;
    }
    let since = answering.since().map(|at| {
        format!(
            "no model has answered since {} UTC",
            at.format("%Y-%m-%d %H:%M")
        )
    });
    let episode = since
        .clone()
        .unwrap_or_else(|| "no model has answered yet".to_owned());
    let connect = Owed::one(
        Proposal::Act(Act::SignIn { since }),
        format!("{episode}, so nothing can answer a conversation, a review or a title"),
    );
    codes.join(connect)
}
