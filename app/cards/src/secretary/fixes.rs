//! A declared secret with nothing stored is owed a Secret card the box files itself.
//!
//! A card's identity is (name, declaration, episode); an episode begins when the box's card for
//! the name ends without the owner's no.

use crate::secretary::noticed::Owed;
use suzatary_base::capability::Datetime;
use suzatary_connect::settings::{Declaration, Settable, Settables};
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::need::Need;
use suzatary_inbox::secretary::proposal::Proposal;
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded::{Done, Responded};

const MISSING: &str = "Nothing is stored for it yet, so what needs it cannot work.";

const LOCKED: &str = "The secret store did not answer, so no saved key can be read.";

/// One declared name with nothing stored under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub name: Settable,
    /// Every table declaring it.
    pub declared: Vec<Declaration>,
    /// Where its episode begins.
    pub since: Option<Datetime>,
}

impl Fix {
    /// The card's act.
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
            why: format!("{readers} reads it"),
        }
    }

    /// Its act, in its episode.
    #[must_use]
    pub fn owed(&self) -> Owed {
        let owed = Owed::one(Proposal::Act(self.act()), MISSING);
        match self.since {
            Some(since) => owed.since(since),
            None => owed,
        }
    }
}

/// Every table-declared name that is `missing`. The model's credential is no table's; the
/// model's own rule asks for it.
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
                .filter(|declaration| declaration.by_a_secret_card())
                .cloned()
                .collect();
            (!declared.is_empty()).then(|| Fix {
                since: ended(inbox, |proposal| {
                    proposal.need() == Need::Secret(name.name())
                })
                .and_then(|object| object.reading.shown().map(|answer| answer.at)),
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
        .iter()
        .map(Fix::owed)
        .fold(Owed::default(), Owed::join)
}

/// The act's words for a secret the owner asked to replace.
pub(crate) const REPLACING: &str = "you asked to replace it";

const REPLACE: &str = "You asked to replace it from Settings; what reads it keeps the stored \
                       value until a new one is stored here.";

/// The card asking for a new value of `name`, in an episode beginning at `at`.
#[must_use]
pub fn replace(name: &Settable, at: Datetime) -> Owed {
    Owed::one(
        Proposal::Act(Act::Secret {
            name: name.name().to_owned(),
            why: REPLACING.to_owned(),
        }),
        REPLACE,
    )
    .since(at)
}

/// Every waiting replace card of the box's: the card is the request, owed until answered.
#[must_use]
pub fn replacing(settables: &Settables, inbox: &inbox::Fold) -> Owed {
    inbox
        .objects()
        .filter(|object| object.asked.proposer == Proposer::Box && object.reading.open())
        .filter_map(|object| match &object.asked.proposal {
            Proposal::Act(Act::Secret { name, why }) if why == REPLACING => {
                Some(replace(&Settable::mint(settables, name).ok()?, object.at))
            }
            Proposal::Act(_)
            | Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Changes(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => None,
        })
        .fold(Owed::default(), Owed::join)
}

/// The card asking to unlock the store.
#[must_use]
pub fn unlock(inbox: &inbox::Fold) -> Owed {
    let after = ended(inbox, |proposal| {
        matches!(proposal, Proposal::Act(Act::UnlockStore { .. }))
    })
    .map(|object| object.asked.asked.clone());
    Owed::one(Proposal::Act(Act::UnlockStore { after }), LOCKED)
}

/// The box's latest card matching `asks` that ended without the owner's no, ordered by instant
/// then name so the answer is a function of the inbox.
fn ended(inbox: &inbox::Fold, asks: impl Fn(&Proposal) -> bool) -> Option<&inbox::Object> {
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
}
