//! The cards the box files itself, with no model asked, kept equal to the set a reading owes.
//!
//! Filing is idempotent: an open card for the same need holds it. A waiting card of a kind the
//! set `owns` that it no longer holds is withdrawn, unless the set is [holding](Owed::holding).

use suzatary_base::capability::Datetime;
use suzatary_base::reference::{ProposalRef, Reference};
use suzatary_base::refusal::Refusal;
use suzatary_connect::secrets::SecretState;
use suzatary_connect::settings::Settable;
use suzatary_conversations::secretary::store::Entry;
use suzatary_health::health::Healths;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded::Withdrawal;
use suzatary_lines::parts;
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::{conversations, transcript};
use suzatary_store::store::Prodrome;
use suzatary_topics::secretary::filed::{self, Filing};

/// Why a stale card is withdrawn.
pub const WITHDRAWN: &str = "the box no longer asks for this: what it was needed for is in \
                             place, or what declared it is gone";

/// One proposal a reading owes, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Noticed {
    pub proposal: Proposal,
    /// Why the reading owes it.
    pub why: String,
    /// When its episode began; an ended card from before is asked again.
    pub since: Option<Datetime>,
    /// Earlier cards it rests on, named in its sources.
    pub about: Vec<Reference>,
}

/// The set of proposals the box owes; the empty set that holds nothing is its identity.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Owed {
    owed: Vec<Noticed>,
    stale: Stale,
}

/// What a filing does with a waiting card the set does not hold; the join takes the larger.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
enum Stale {
    /// Withdrawn, saying why.
    #[default]
    Withdrawn,
    /// Left as it stands.
    Held,
}

/// What one filing did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filed {
    /// Every owed proposal, filed now or before.
    pub owed: Vec<ProposalId>,
    pub withdrawn: Vec<ProposalId>,
}

impl Owed {
    /// One proposal, owed.
    #[must_use]
    pub fn one(proposal: Proposal, why: impl Into<String>) -> Self {
        Self {
            owed: vec![Noticed {
                proposal,
                why: why.into(),
                since: None,
                about: Vec::new(),
            }],
            stale: Stale::Withdrawn,
        }
    }

    /// Each proposal's episode begun at `at`.
    #[must_use]
    pub fn since(mut self, at: Datetime) -> Self {
        for noticed in &mut self.owed {
            noticed.since = Some(at);
        }
        self
    }

    /// Each proposal resting on `cards` too.
    #[must_use]
    pub fn about(mut self, cards: &[Reference]) -> Self {
        for noticed in &mut self.owed {
            noticed.about.extend_from_slice(cards);
        }
        self
    }

    /// Drops the secret of any link whose removal rests on that link's card, so the box never
    /// asks to set a link it retires.
    #[must_use]
    pub(crate) fn retiring(mut self) -> Self {
        use suzatary_inbox::secretary::acts::Act;
        let removed: Vec<String> = self
            .owed
            .iter()
            .filter_map(|noticed| {
                let Proposal::Act(Act::Disconnect { connection }) = &noticed.proposal else {
                    return None;
                };
                (!noticed.about.is_empty()).then(|| connection.secrets())
            })
            .flatten()
            .collect();
        self.owed.retain(|noticed| {
            !matches!(&noticed.proposal, Proposal::Act(Act::Secret { name, .. }) if removed.contains(name))
        });
        self
    }

    /// This set, withdrawing nothing when filed.
    #[must_use]
    pub fn holding(self) -> Self {
        Self {
            stale: Stale::Held,
            ..self
        }
    }

    /// The union; a shared need keeps the first one's words, and it holds if either does.
    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        for noticed in other.owed {
            if !self.holds(&noticed.proposal) {
                self.owed.push(noticed);
            }
        }
        self.stale = self.stale.max(other.stale);
        self
    }

    /// Whether `proposal`'s need is owed, in whatever words.
    #[must_use]
    pub fn holds(&self, proposal: &Proposal) -> bool {
        self.owed
            .iter()
            .any(|noticed| noticed.proposal.need() == proposal.need())
    }

    /// Every proposal owed.
    pub fn iter(&self) -> impl Iterator<Item = &Noticed> {
        self.owed.iter()
    }

    /// File every owed proposal in [`SETUP`](suzatary_base::conversation_id::SETUP) and withdraw the
    /// stale ones.
    pub fn file(
        &self,
        conversations: Conversations<'_>,
        inbox: &dyn Prodrome<InboxEvent>,
        at: Datetime,
    ) -> Result<Filed, Refusal> {
        let conversation = &*suzatary_base::conversation_id::SETUP;
        let mut done = Filed::default();
        // Read once: this filing adds only other proposals, so it changes no answer here.
        let before = parts::part("refold", || inbox::fold(inbox))?;
        for noticed in &self.owed {
            let held = before
                .held(&noticed.proposal, noticed.since)
                .map(|object| object.id.clone());
            if let Some(held) = held {
                done.owed.push(held);
                continue;
            }
            // Opened by the first card, so a box that owes nothing has no conversation.
            parts::part("file.open", || {
                filed::opened(conversations, conversation, "Setup", &Proposer::Box, at)
            })?;
            let card = Reference::Proposal(ProposalRef {
                conversation: conversation.clone(),
                index: parts::part("file.read", || {
                    conversations::read(conversations, conversation)
                })?
                .proposals
                .len(),
            });
            let why = suzatary_base::spelling::said(&noticed.why);
            parts::part("file.note", || {
                conversations::append(
                    conversations,
                    conversation,
                    &Entry::noted(at, why.clone(), vec![card.clone()]),
                )
            })?;
            let reasons = Reasons::new(
                why,
                [card].into_iter().chain(noticed.about.clone()).collect(),
            )
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
            let (_, object) = parts::part("file.card", || {
                filed::file(
                    conversations,
                    inbox,
                    &[],
                    at,
                    Filing {
                        conversation,
                        proposer: Proposer::Box,
                        proposal: noticed.proposal.clone(),
                        reasons,
                        serves: None,
                        revises: None,
                    },
                )
            })?;
            done.owed.push(object);
        }
        let stale: Vec<ProposalId> = before
            .objects()
            .filter(|object| {
                self.stale == Stale::Withdrawn
                    && object.asked.proposer == Proposer::Box
                    && owns(&object.asked.proposal)
                    && object.reading.performs()
                    && !self.holds(&object.asked.proposal)
            })
            .map(|object| object.id.clone())
            .collect();
        for id in stale {
            parts::part("withdraw", || {
                inbox::withdraw(
                    inbox,
                    &id,
                    &transcript::writer(),
                    at,
                    Withdrawal::NoLongerNeeded {
                        why: WITHDRAWN.to_owned(),
                    },
                )
            })?;
            done.withdrawn.push(id);
        }
        Ok(done)
    }
}

/// Whether a box card asking for `proposal` is this set's to file and withdraw.
pub(crate) fn owns(proposal: &Proposal) -> bool {
    use suzatary_inbox::secretary::acts::Act;
    match proposal {
        Proposal::Act(
            Act::Secret { .. }
            | Act::UnlockStore { .. }
            | Act::SignIn { .. }
            | Act::SignInCode { .. }
            | Act::Auth { .. }
            | Act::AuthSend { .. }
            | Act::GoogleSignIn { .. }
            | Act::GoogleClient { .. }
            | Act::Grant { .. }
            | Act::Connect { .. }
            | Act::Disconnect { .. },
        ) => true,
        Proposal::Act(
            Act::Web { .. }
            | Act::Patch { .. }
            | Act::DriveRead { .. }
            | Act::CodingSession { .. }
            | Act::Device(_)
            | Act::Erase { .. }
            | Act::Retract { .. }
            | Act::Afresh { .. }
            | Act::CalendarEvent { .. }
            | Act::Noise { .. }
            | Act::Restore { .. },
        )
        | Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Changes(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => false,
    }
}

/// Each rule's input, one record.
#[derive(Clone, Copy)]
pub struct Seen<'a> {
    /// Every declared secret name.
    pub settables: &'a suzatary_connect::settings::Settables,
    /// Where each stands.
    pub secrets: &'a dyn suzatary_connect::secrets::Secrets,
    /// The model's name, as [`suzatary_conversations::secretary::model::Model::name`] spells it.
    pub model: &'a str,
    /// The model's observed calls.
    pub answered: &'a Healths<String>,
    /// The sign-ins the box holds open.
    pub open: &'a [suzatary_inbox::secretary::sign_in::SignInUrl],
    /// The connection tables now; `None` (unreadable) holds every card.
    pub declared: Option<&'a suzatary_connect::connect::Declared>,
    /// Every source's results; `None` (unreadable) holds every card.
    pub health: Option<&'a Healths<suzatary_health::health::SourceKey>>,
    /// The calendars with an occurrence within [`super::failing::quiet`] of now.
    pub heard: Option<&'a std::collections::BTreeSet<suzatary_google::calendar::CalendarName>>,
    /// Where the data repository pushes: its token is asked for only on GitHub.
    pub pushed: &'a suzatary_connect::settings::remote::Pushed,
    /// Now, UTC.
    pub now: suzatary_health::health::Instant,
    /// The Google sign-in callback, if the app's URL has an origin Google can return to.
    pub redirect: Option<&'a suzatary_google::google::RedirectUri>,
}

/// Everything the rules owe at `seen`, as one set so no rule withdraws another's cards.
///
/// A locked store would make a stored key read as missing, so then the set holds every card and
/// owes only the unlock card; it also holds while any name failed.
#[must_use]
pub fn due(seen: &Seen<'_>, inbox: &inbox::Fold) -> Owed {
    use suzatary_connect::secrets::Answer;
    let snapshot = parts::part("secrets", || {
        suzatary_connect::secrets::Snapshot::read(seen.secrets, seen.settables.iter())
    });
    let Ok(answers) = snapshot.answers() else {
        return super::fixes::unlock(inbox).holding();
    };
    let answered = |wanted: Answer| move |name: &Settable| answers.get(name) == Some(&wanted);
    let resolves = answered(Answer::Set);
    let answering = super::bootstrap::Answering {
        model: seen.model,
        health: seen.answered,
        settables: seen.settables,
        resolves: &resolves,
    };
    let in_use = seen.settables.restricted(|by| by.in_use(seen.pushed));
    let owed = super::fixes::owed(&in_use, answered(Answer::Missing), inbox)
        .join(super::bootstrap::owed(&answering, seen.open))
        .join(seen.declared.map_or_else(
            || Owed::default().holding(),
            |declared| {
                parts::part("connections", || {
                    super::connections::owed(
                        declared,
                        seen.secrets,
                        super::connections::Google {
                            client: SecretState::all(
                                [
                                    suzatary_google::google::CLIENT_ID,
                                    suzatary_google::google::CLIENT_SECRET,
                                ]
                                .map(|name| {
                                    Settable::mint(seen.settables, name)
                                        .ok()
                                        .and_then(|name| answers.get(&name).copied())
                                        .map_or(SecretState::Missing, SecretState::from)
                                }),
                            ),
                            redirect: seen.redirect,
                        },
                        inbox,
                    )
                })
            },
        ));
    // Joined after, so a secret or sign-in already asked for keeps its card.
    let failing = match (seen.declared, seen.health) {
        (Some(declared), Some(health)) => parts::part("failing", || {
            super::failing::owed(&super::failing::Observed {
                declared,
                answering: &answering,
                health,
                heard: seen.heard,
                now: seen.now,
            })
        }),
        (None, _) | (_, None) => Owed::default().holding(),
    };
    let owed = owed
        .join(failing)
        .join(super::fixes::replacing(seen.settables, inbox))
        .retiring();
    if answers.values().any(|answer| *answer == Answer::Failed) {
        owed.holding()
    } else {
        owed
    }
}

/// File what is [`due`] at `at`.
pub fn notice(
    seen: &Seen<'_>,
    conversations: Conversations<'_>,
    inbox_store: &dyn Prodrome<InboxEvent>,
    at: Datetime,
) -> Result<Filed, Refusal> {
    let inbox = parts::part("inbox", || inbox::fold(inbox_store))?;
    parts::part("due", || due(seen, &inbox)).file(conversations, inbox_store, at)
}

impl FromIterator<Noticed> for Owed {
    fn from_iter<I: IntoIterator<Item = Noticed>>(noticed: I) -> Self {
        noticed
            .into_iter()
            .map(|one| Self {
                owed: vec![one],
                stale: Stale::Withdrawn,
            })
            .fold(Self::default(), Self::join)
    }
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/owing.rs"]
pub mod owing;

#[cfg(test)]
mod tests {
    use super::owing::{Store, at, owed};
    use super::*;
    use suzatary_inbox::secretary::acts::Act;

    /// The union is a semilattice with the empty set as its identity, read as the set it holds.
    #[test]
    fn the_union_is_a_semilattice() {
        let set = |owed: &Owed| {
            let mut names: Vec<String> = owed
                .iter()
                .map(|noticed| match &noticed.proposal {
                    Proposal::Act(Act::Secret { name, .. }) => name.clone(),
                    _ => String::new(),
                })
                .collect();
            names.sort();
            names
        };
        let (a, b, c) = (owed(&["a", "b"]), owed(&["b", "c"]), owed(&["c"]));
        assert_eq!(
            set(&a.clone().join(b.clone())),
            set(&b.clone().join(a.clone()))
        );
        assert_eq!(
            set(&a.clone().join(b.clone()).join(c.clone())),
            set(&a.clone().join(b.clone().join(c)))
        );
        assert_eq!(set(&a.clone().join(a.clone())), set(&a));
        assert_eq!(set(&a.clone().join(Owed::default())), set(&a));
    }

    /// Holding is absorbed by the join from either side, and withdrawing is its unit.
    #[test]
    fn one_set_that_holds_makes_the_join_hold() {
        let (a, b) = (owed(&["a"]), owed(&["b"]));
        for (x, y) in [
            (a.clone().holding(), b.clone()),
            (a.clone(), b.clone().holding()),
        ] {
            assert_eq!(x.join(y).stale, Stale::Held);
        }
        assert_eq!(a.join(b).stale, Stale::Withdrawn);
    }

    /// A health register that cannot be read holds every waiting card and costs no other rule
    /// its cards: a missing secret is still asked for.
    #[test]
    fn an_unreadable_health_register_holds_and_asks_the_rest() {
        let settables = suzatary_connect::settings::Settables::derive(
            suzatary_connect::settings::MODEL_TOKEN,
            [suzatary_connect::connect::Channel::Ntfy],
            None,
            &[],
        );
        let seen = Seen {
            settables: &settables,
            secrets: &Store::default(),
            model: "a-cli/large",
            answered: &suzatary_health::health::Healths::default(),
            open: &[],
            declared: Some(&suzatary_connect::connect::Declared::default()),
            health: None,
            heard: Some(&std::collections::BTreeSet::new()),
            pushed: &suzatary_connect::settings::remote::Pushed::Nowhere,
            now: prodrome_core::fpl::instant_of(at(0)),
            redirect: None,
        };
        let owed = due(&seen, &inbox::Fold::default());
        assert_eq!(owed.stale, Stale::Held);
        assert!(owed.iter().any(|noticed| matches!(&noticed.proposal,
            Proposal::Act(suzatary_inbox::secretary::acts::Act::Secret { name, .. }) if name == "NTFY_TOPIC")));
    }
}
