//! The rule over the deployment's secrets: a name a table of the instance file declares, with
//! nothing stored under it, is owed a Secret card the box files itself, with no model asked.
//!
//! Its identity is (name, declaration, episode). The act's words name what declares the name and,
//! once a card of the box's has ended without the owner's no (stored from the card, or withdrawn
//! when the value was stored elsewhere), that card. So asking again while nothing changed asks
//! for the same card however it was answered (one the owner dismissed stays dismissed), a
//! declaration that changes is a new card, and a value stored and then lost is a new card naming
//! the one before it.

use crate::reference::ProposalRef;
use crate::secretary::acts::Act;
use crate::secretary::inbox;
use crate::secretary::noticed::Owed;
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::{Done, Responded};
use crate::secretary::store::Proposal;
use crate::settings::{Declaration, Settable, Settables};

/// Why the box asks for a declared secret nothing stores, in the owner's words.
const MISSING: &str = "Nothing is stored for it yet, so what needs it cannot work.";

/// Why the box asks for the secret store to be unlocked, in the owner's words.
const LOCKED: &str = "The secret store did not answer, so no saved key can be read.";

/// One declared name with nothing stored under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// The name.
    pub name: Settable,
    /// Every table declaring it.
    pub declared: Vec<Declaration>,
    /// The box's card for it that last ended without the owner's no, once one has.
    pub after: Option<ProposalRef>,
}

impl Fix {
    /// The card's act: the secret, asked for in the box's words.
    #[must_use]
    pub fn act(&self) -> Act {
        let readers = self
            .declared
            .iter()
            .map(Declaration::sentence)
            .collect::<Vec<_>>()
            .join(" and ");
        Act::Secret {
            name: self.name.name().to_owned(),
            why: match &self.after {
                None => format!("{readers} reads it"),
                Some(card) => format!("{readers} reads it, and it no longer resolves after {card}"),
            },
        }
    }
}

/// Every declared name a table of the instance file names that is `missing`, each with the
/// box's card for it that last ended without the owner's no. The model's credential is no
/// table's: whether the model answers is the model's rule to ask.
#[must_use]
pub fn fixes(
    settables: &Settables,
    missing: impl Fn(&Settable) -> bool,
    inbox: &inbox::Fold,
) -> Vec<Fix> {
    settables
        .declarations()
        .filter(|(name, _)| missing(name))
        .filter_map(|(name, declared)| {
            let declared: Vec<Declaration> = declared
                .iter()
                .filter(|declaration| declaration.by_a_table() && !declaration.own_card())
                .cloned()
                .collect();
            (!declared.is_empty()).then(|| Fix {
                after: ended(inbox, |proposal| {
                    matches!(proposal, Proposal::Act(Act::Secret { name: asked, .. }) if asked == name.name())
                }),
                name,
                declared,
            })
        })
        .collect()
}

/// The cards [`fixes`] owes.
#[must_use]
pub fn owed(
    settables: &Settables,
    missing: impl Fn(&Settable) -> bool,
    inbox: &inbox::Fold,
) -> Owed {
    fixes(settables, missing, inbox)
        .into_iter()
        .map(|fix| Owed::one(Proposal::Act(fix.act()), MISSING))
        .fold(Owed::default(), Owed::join)
}

/// The card asking to unlock the store, which is locked.
#[must_use]
pub fn unlock(inbox: &inbox::Fold) -> Owed {
    let after = ended(inbox, |proposal| {
        matches!(proposal, Proposal::Act(Act::UnlockStore { .. }))
    });
    Owed::one(Proposal::Act(Act::UnlockStore { after }), LOCKED)
}

/// The box's card asking for what `asks` picks that last ended without the owner's no: acted on
/// from the card, or withdrawn by the box. The latest by its recorded instant and then its name,
/// so the answer is a function of the inbox's objects.
fn ended(inbox: &inbox::Fold, asks: impl Fn(&Proposal) -> bool) -> Option<ProposalRef> {
    inbox
        .objects()
        .filter(|object| {
            object.asked.proposer == Proposer::Box
                && match object.reading.shown().map(|answer| &answer.response) {
                    Some(Responded::Done(Done::Acted { .. }) | Responded::Withdrawn(_)) => true,
                    None
                    | Some(
                        Responded::Tried { .. }
                        | Responded::Done(
                            Done::Sent { .. } | Done::Applied { .. } | Done::MayHaveGone { .. },
                        )
                        | Responded::Checked(_)
                        | Responded::Dismissed { .. },
                    ) => false,
                }
                && asks(&object.asked.proposal)
        })
        .max_by_key(|object| (object.reading.shown().map(|answer| answer.at), &object.id))
        .map(|object| object.asked.asked.clone())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::capability::Datetime;
    use crate::secretary::inbox_event::InboxEvent;
    use crate::secretary::responded::Performed;
    use crate::store::InMemory;
    use crate::store::Reads as _;

    fn at(minute: u32) -> Datetime {
        Datetime::new(2026, 10, 7, 9, minute, 0, 0).expect("an instant")
    }

    use crate::connect::Channel;

    fn declared(channels: &[Channel]) -> Settables {
        Settables::derive(
            crate::settings::MODEL_TOKEN,
            channels.iter().copied(),
            None,
            &[],
        )
    }

    struct World {
        conversations: crate::secretary::conversations::InMemory,
        inbox: InMemory<InboxEvent>,
        stored: BTreeSet<String>,
    }

    impl World {
        fn new() -> Self {
            Self {
                conversations: crate::secretary::conversations::InMemory::default(),
                inbox: InMemory::default(),
                stored: BTreeSet::new(),
            }
        }

        fn fold(&self) -> inbox::Fold {
            inbox::fold(&self.inbox).expect("reads")
        }

        /// The box's one filing: what the reading owes, at `minute`.
        fn file(&self, settables: &Settables, minute: u32) -> crate::secretary::noticed::Filed {
            owed(
                settables,
                |name| !self.stored.contains(name.name()),
                &self.fold(),
            )
            .file(self.conversations.conversations(), &self.inbox, at(minute))
            .expect("filed")
        }

        /// The open cards of the box's, by the name each asks for.
        fn open(&self) -> Vec<String> {
            let mut names: Vec<String> = self
                .fold()
                .objects()
                .filter(|object| object.asked.proposer == Proposer::Box && object.reading.open())
                .filter_map(|object| match &object.asked.proposal {
                    Proposal::Act(Act::Secret { name, .. }) => Some(name.clone()),
                    _ => None,
                })
                .collect();
            names.sort();
            names
        }

        fn card_for(&self, name: &str) -> Option<inbox::ProposalId> {
            self.fold()
                .objects()
                .find(|object| {
                    object.reading.open()
                        && matches!(&object.asked.proposal,
                            Proposal::Act(Act::Secret { name: asked, .. }) if asked == name)
                })
                .map(|object| object.id.clone())
        }

        /// The owner taps the card for `name`, and the value is stored.
        fn tap(&mut self, name: &str, minute: u32) {
            let id = self.card_for(name).expect("a card");
            let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
            inbox::submit(&self.inbox, &id, &owner, at(minute), |_| {
                Ok(Performed::Done(Done::Acted {
                    what: format!("stored {name}"),
                }))
            })
            .expect("taps");
            self.stored.insert(name.to_owned());
        }

        fn dismiss(&self, name: &str, minute: u32) {
            let id = self.card_for(name).expect("a card");
            let owner = prodrome_core::event::Actor::new("ada").expect("an actor");
            inbox::dismiss(&self.inbox, &id, &owner, at(minute), "not now").expect("dismissed");
        }
    }

    /// One step of the world around the box.
    #[derive(Debug, Clone)]
    enum Step {
        Declare(Vec<Channel>),
        StoreElsewhere(&'static str),
        Lose(&'static str),
        Tap(&'static str),
        Dismiss(&'static str),
        File,
    }

    const NAMES: [&str; 2] = ["NTFY_TOPIC", "FCM_SERVICE_ACCOUNT_B64"];

    fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
        use proptest::prelude::*;
        let name = proptest::sample::select(NAMES.to_vec());
        proptest::collection::vec(
            prop_oneof![
                proptest::sample::subsequence(Channel::ALL.to_vec(), 0..=2).prop_map(Step::Declare),
                name.clone().prop_map(Step::StoreElsewhere),
                name.clone().prop_map(Step::Lose),
                name.clone().prop_map(Step::Tap),
                name.prop_map(Step::Dismiss),
                Just(Step::File),
            ],
            0..24,
        )
    }

    proptest::proptest! {
        /// After any history, a filing leaves exactly one open card for each declared name that
        /// does not resolve and was not answered no in this episode, none for any other, and
        /// filing again writes nothing.
        #[test]
        fn a_filing_leaves_one_open_card_per_unresolved_name(script in steps()) {
            let mut held = World::new();
            let mut settables = declared(&[]);
            let mut minute = 0;
            for step in script {
                minute += 1;
                match step {
                    Step::Declare(channels) => settables = declared(&channels),
                    Step::StoreElsewhere(name) => { held.stored.insert(name.to_owned()); }
                    Step::Lose(name) => { held.stored.remove(name); }
                    Step::Tap(name) => if held.card_for(name).is_some() { held.tap(name, minute) },
                    Step::Dismiss(name) => if held.card_for(name).is_some() { held.dismiss(name, minute) },
                    Step::File => { held.file(&settables, minute); }
                }
            }
            held.file(&settables, minute + 1);
            let answered_no = |name: &str| {
                let fold = held.fold();
                let fix = fixes(&settables, |_| true, &fold)
                    .into_iter()
                    .find(|fix| fix.name.name() == name);
                fix.is_some_and(|fix| fold.objects().any(|object| {
                    object.asked.proposal == Proposal::Act(fix.act())
                        && matches!(object.reading.shown().map(|answer| &answer.response),
                            Some(Responded::Dismissed { .. }))
                }))
            };
            let mut expected: Vec<String> = settables
                .names()
                .into_iter()
                .filter(|name| NAMES.contains(name) && !held.stored.contains(*name))
                .filter(|name| !answered_no(name))
                .map(str::to_owned)
                .collect();
            expected.sort();
            proptest::prop_assert_eq!(held.open(), expected);
            let objects = held.inbox.objects().expect("reads").len();
            held.file(&settables, minute + 2);
            proptest::prop_assert_eq!(held.inbox.objects().expect("reads").len(), objects);
        }
    }
}
