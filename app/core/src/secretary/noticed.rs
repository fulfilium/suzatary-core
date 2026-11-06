//! The cards the box files itself, with no model asked, kept equal to the set a reading owes.
//!
//! Filing is idempotent: an open card for the same need holds it. A waiting card of a kind the
//! set `owns` that it no longer holds is withdrawn, unless the set is [holding](Owed::holding).

use std::sync::LazyLock;

use crate::capability::Datetime;
use crate::reference::{ProposalRef, Reference};
use crate::refusal::Refusal;
use crate::secretary::filed::{self, Filing};
use crate::secretary::inbox::{self, ProposalId};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::Withdrawal;
use crate::secretary::store::{ConversationId, Entry, Proposal, Reasons};
use crate::store::Prodrome;

/// The box's own conversation.
pub static CONVERSATION: LazyLock<ConversationId> =
    LazyLock::new(|| ConversationId::new("setup").expect("\"setup\" is a conversation id"));

/// Why a stale card is withdrawn.
pub const WITHDRAWN: &str = "the box no longer asks for this: what it was needed for is in \
                             place, or what declared it is gone";

/// One proposal a reading owes, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Noticed {
    /// The act.
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
    /// The cards withdrawn.
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
    pub fn retiring(mut self) -> Self {
        use crate::secretary::acts::Act;
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

    /// File every owed proposal in [`CONVERSATION`] and withdraw the stale ones.
    pub fn file(
        &self,
        conversations: crate::secretary::conversations::Conversations<'_>,
        inbox: &dyn Prodrome<InboxEvent>,
        at: Datetime,
    ) -> Result<Filed, Refusal> {
        let conversation = &*CONVERSATION;
        let mut done = Filed::default();
        // Read once: this filing adds only other proposals, so it changes no answer here.
        let before = inbox::fold(inbox)?;
        for noticed in &self.owed {
            let held = before
                .held(&noticed.proposal, noticed.since)
                .map(|object| object.id.clone());
            if let Some(held) = held {
                done.owed.push(held);
                continue;
            }
            // Opened by the first card, so a box that owes nothing has no conversation.
            filed::opened(conversations, conversation, "Setup", &Proposer::Box, at)?;
            let card = Reference::Proposal(ProposalRef {
                conversation: conversation.clone(),
                index: super::read(conversations, conversation)?.proposals.len(),
            });
            let why = said(&noticed.why);
            super::append(
                conversations,
                conversation,
                &Entry::noted(at, why.clone(), vec![card.clone()]),
            )?;
            let reasons = Reasons::new(
                why,
                [card].into_iter().chain(noticed.about.clone()).collect(),
            )
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
            let (_, object) = filed::file(
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
            )?;
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
            inbox::withdraw(
                inbox,
                &id,
                &super::writes_as(),
                at,
                Withdrawal::NoLongerNeeded {
                    why: WITHDRAWN.to_owned(),
                },
            )?;
            done.withdrawn.push(id);
        }
        Ok(done)
    }
}

/// Whether a box card asking for `proposal` is this set's to file and withdraw.
pub(crate) fn owns(proposal: &Proposal) -> bool {
    use crate::secretary::acts::Act;
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
            | Act::CalendarEvent { .. },
        )
        | Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Changes(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => false,
    }
}

/// A rule's why as one sentence, capitalised and ended with one full stop.
#[must_use]
pub fn said(why: &str) -> String {
    let trimmed = why.trim().trim_end_matches('.');
    let mut chars = trimmed.chars();
    chars.next().map_or_else(String::new, |first| {
        format!("{}.", first.to_uppercase().chain(chars).collect::<String>())
    })
}

/// Each rule's input, one record.
#[derive(Clone, Copy)]
pub struct Seen<'a> {
    /// Every declared secret name.
    pub settables: &'a crate::settings::Settables,
    /// Where each stands.
    pub secrets: &'a dyn crate::capability::Secrets,
    /// The model's name, as [`crate::capability::Model::name`] spells it.
    pub model: &'a str,
    /// The model's observed calls.
    pub answered: &'a crate::health::Healths<String>,
    /// The sign-ins the box holds open.
    pub open: &'a [super::sign_in::SignInUrl],
    /// The connection tables now; `None` (unreadable) holds every card.
    pub declared: Option<&'a crate::connect::Declared>,
    /// Every source's results; `None` (unreadable) holds every card.
    pub health: Option<&'a crate::health::Healths<crate::source_key::SourceKey>>,
    /// The calendars with an occurrence within [`super::failing::quiet`] of now.
    pub heard: Option<&'a std::collections::BTreeSet<crate::calendar::CalendarName>>,
    /// Where the data repository pushes: its token is asked for only on GitHub.
    pub pushed: &'a crate::settings::remote::Pushed,
    /// Now, UTC.
    pub now: crate::health::Instant,
    /// The Google sign-in callback, if the app's URL has an origin Google can return to.
    pub redirect: Option<&'a crate::google::RedirectUri>,
}

/// Everything the rules owe at `seen`, as one set so no rule withdraws another's cards.
///
/// A locked store would make a stored key read as missing, so then the set holds every card and
/// owes only the unlock card; it also holds while any name failed.
#[must_use]
pub fn due(seen: &Seen<'_>, inbox: &inbox::Fold) -> Owed {
    use crate::capability::Answer;
    let snapshot = crate::capability::Snapshot::read(seen.secrets, seen.settables.iter());
    let Ok(answers) = snapshot.answers() else {
        return super::fixes::unlock(inbox).holding();
    };
    let answered =
        |wanted: Answer| move |name: &crate::settings::Settable| answers.get(name) == Some(&wanted);
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
                super::connections::owed(
                    declared,
                    seen.secrets,
                    super::connections::Google {
                        client: crate::capability::SecretState::all(
                            [crate::google::CLIENT_ID, crate::google::CLIENT_SECRET].map(|name| {
                                crate::settings::Settable::mint(seen.settables, name)
                                    .ok()
                                    .and_then(|name| answers.get(&name).copied())
                                    .map_or(
                                        crate::capability::SecretState::Missing,
                                        crate::capability::SecretState::from,
                                    )
                            }),
                        ),
                        redirect: seen.redirect,
                    },
                    inbox,
                )
            },
        ));
    // Joined after, so a secret or sign-in already asked for keeps its card.
    let failing = match (seen.declared, seen.health) {
        (Some(declared), Some(health)) => super::failing::owed(&super::failing::Observed {
            declared,
            answering: &answering,
            health,
            heard: seen.heard,
            now: seen.now,
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
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox_store: &dyn Prodrome<InboxEvent>,
    at: Datetime,
) -> Result<Filed, Refusal> {
    due(seen, &inbox::fold(inbox_store)?).file(conversations, inbox_store, at)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::acts::Act;
    use crate::store::InMemory;
    use crate::store::Reads as _;

    fn at(minute: u32) -> Datetime {
        Datetime::new(2026, 10, 7, 9, minute, 0, 0).expect("an instant")
    }

    fn secret(name: &str) -> Proposal {
        Proposal::Act(Act::Secret {
            name: name.to_owned(),
            why: format!("{name} is declared"),
        })
    }

    fn owed(names: &[&str]) -> Owed {
        names
            .iter()
            .map(|name| Noticed {
                proposal: secret(name),
                why: format!("{name} is owed"),
                since: None,
                about: Vec::new(),
            })
            .collect()
    }

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

    /// Filing a set twice writes nothing the second time; a card the next set does not hold is
    /// withdrawn while it waits, and one answered stays as it was answered.
    #[test]
    fn filing_is_idempotent_and_withdraws_what_is_no_longer_owed() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let first = owed(&["a", "b"])
            .file(conversations.conversations(), &inbox_store, at(0))
            .expect("filed");
        assert_eq!(first.owed.len(), 2);
        let objects = inbox_store.objects().expect("reads").len();
        let lines = conversations.lines.objects().expect("reads").len();
        let again = owed(&["a", "b"])
            .file(conversations.conversations(), &inbox_store, at(5))
            .expect("filed");
        assert_eq!(again, first, "the same set is the same cards");
        assert_eq!(inbox_store.objects().expect("reads").len(), objects);
        assert_eq!(conversations.lines.objects().expect("reads").len(), lines);

        let later = owed(&["a"])
            .file(conversations.conversations(), &inbox_store, at(9))
            .expect("filed");
        assert_eq!(later.withdrawn, vec![first.owed[1].clone()]);
        let fold = inbox::fold(&inbox_store).expect("reads");
        assert!(matches!(
            fold.get(&first.owed[1])
                .expect("held")
                .reading
                .shown()
                .map(|answer| &answer.response),
            Some(crate::secretary::responded::Responded::Withdrawn(
                Withdrawal::NoLongerNeeded { why }
            )) if why == WITHDRAWN
        ));
        assert!(fold.get(&first.owed[0]).expect("held").reading.open());
        let quiet = owed(&["a"])
            .file(conversations.conversations(), &inbox_store, at(10))
            .expect("filed");
        assert!(
            quiet.withdrawn.is_empty(),
            "a withdrawn card is withdrawn once"
        );
    }

    /// Only the box's own cards are its to withdraw.
    #[test]
    fn a_card_someone_else_asked_for_is_never_withdrawn() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let asked = inbox::Asked {
            proposer: Proposer::Secretary,
            proposal: secret("THEIRS"),
            reasons: Reasons::new(
                "asked in a round",
                vec!["briefing:2026-10-07".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: ProposalRef {
                conversation: ConversationId::new("default").expect("a slug"),
                index: 0,
            },
            price: inbox::Price::unpriced(),
        };
        let theirs =
            inbox::ask(&inbox_store, asked, &super::super::writes_as(), at(0)).expect("filed");
        let filed = Owed::default()
            .file(conversations.conversations(), &inbox_store, at(1))
            .expect("filed");
        assert!(filed.withdrawn.is_empty());
        let fold = inbox::fold(&inbox_store).expect("reads");
        assert!(fold.get(&theirs).expect("held").reading.open());
    }

    /// The box files cards outside the setup conversation, a build's and an erasure's, and the
    /// setup set withdraws none of them.
    #[test]
    fn the_box_s_cards_elsewhere_are_not_the_setup_set_s() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let boxs = |conversation: &str, proposal: Proposal| {
            let asked = inbox::Asked {
                proposer: Proposer::Box,
                proposal,
                reasons: Reasons::new(
                    "the box asks",
                    vec!["briefing:2026-10-07".parse().expect("a reference")],
                )
                .expect("reasons"),
                asked: ProposalRef {
                    conversation: ConversationId::new(conversation).expect("a slug"),
                    index: 0,
                },
                price: inbox::Price::unpriced(),
            };
            inbox::ask(&inbox_store, asked, &super::super::writes_as(), at(0)).expect("filed")
        };
        let build = boxs(
            "build",
            Proposal::Act(crate::secretary::acts::Act::CodingSession {
                task: crate::coding::Task::try_from("Build todo:suzatary/x.".to_owned())
                    .expect("a task"),
            }),
        );
        let erase = boxs(
            "default",
            Proposal::Act(crate::secretary::acts::Act::Erase {
                fact: crate::secretary::memory::FactId(2),
                keys: [crate::secretary::locked_text::KeyName::new("a".repeat(64))
                    .expect("a key name")]
                .into(),
            }),
        );
        let filed = Owed::default()
            .file(conversations.conversations(), &inbox_store, at(1))
            .expect("filed");
        assert!(filed.withdrawn.is_empty(), "{:?}", filed.withdrawn);
        let fold = inbox::fold(&inbox_store).expect("reads");
        for card in [build, erase] {
            assert!(fold.get(&card).expect("held").reading.open());
        }
    }

    fn signing_in() -> Proposal {
        Proposal::Act(crate::secretary::acts::Act::Auth {
            account: crate::mail::address::EmailAddress::new("ada@example.com")
                .expect("a shaped address"),
        })
    }

    /// The secretary's own card for `proposal`, asked at `minute`.
    fn secretarys(
        inbox_store: &InMemory<InboxEvent>,
        proposal: Proposal,
        minute: u32,
    ) -> ProposalId {
        let asked = inbox::Asked {
            proposer: Proposer::Secretary,
            proposal,
            reasons: Reasons::new(
                "asked in a round",
                vec!["briefing:2026-10-07".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: ProposalRef {
                conversation: ConversationId::new("default").expect("a slug"),
                index: 0,
            },
            price: inbox::Price::unpriced(),
        };
        inbox::ask(inbox_store, asked, &super::super::writes_as(), at(minute)).expect("filed")
    }

    /// The Setup conversation's turns, none before it is opened.
    fn said(conversations: &crate::secretary::conversations::InMemory) -> usize {
        super::super::read(conversations.conversations(), &CONVERSATION)
            .map_or(0, |stored| stored.live().len())
    }

    /// While the secretary's sign-in card waits, the box's owed sign-in is that card: filing the
    /// same set again says nothing in Setup and files nothing.
    #[test]
    fn a_waiting_card_of_the_secretary_s_holds_the_box_s_and_says_nothing() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let theirs = secretarys(&inbox_store, signing_in(), 1);
        let owed = Owed::one(signing_in(), "the account was connected").since(at(0));
        let before = said(&conversations);
        for minute in [2, 3] {
            let filed = owed
                .file(conversations.conversations(), &inbox_store, at(minute))
                .expect("filed");
            assert_eq!(filed.owed, vec![theirs.clone()]);
        }
        assert_eq!(said(&conversations), before);
        assert_eq!(
            inbox::fold(&inbox_store).expect("reads").objects().count(),
            1
        );
    }

    /// The secretary's card dismissed within the episode answers the box's asking for it; once
    /// a new episode begins, the box asks again.
    #[test]
    fn a_card_dismissed_within_the_episode_holds_it() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let theirs = secretarys(&inbox_store, signing_in(), 3);
        inbox::dismiss(
            &inbox_store,
            &theirs,
            &super::super::writes_as(),
            at(4),
            "not now",
        )
        .expect("dismissed");
        let within = Owed::one(signing_in(), "the account was connected")
            .since(at(1))
            .file(conversations.conversations(), &inbox_store, at(5))
            .expect("filed");
        assert_eq!(within.owed, vec![theirs.clone()]);
        let next = Owed::one(signing_in(), "the account was connected again")
            .since(at(6))
            .file(conversations.conversations(), &inbox_store, at(7))
            .expect("filed");
        assert_ne!(next.owed, vec![theirs]);
        assert_eq!(next.owed.len(), 1);
    }

    /// A store that answers `Missing` for everything but the names in `locked`, `failed` and
    /// `set`.
    #[derive(Default)]
    struct Store {
        locked: Vec<&'static str>,
        failed: Vec<&'static str>,
        set: Vec<String>,
        asked: std::sync::atomic::AtomicUsize,
        /// The parts every Google account's grant holds, or `None` for no grant.
        granted: Option<crate::google::Scopes>,
    }

    impl crate::capability::Secrets for Store {
        fn secret_state(
            &self,
            settable: &crate::settings::Settable,
        ) -> crate::capability::SecretState {
            self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.locked.contains(&settable.name()) {
                crate::capability::SecretState::Locked
            } else if self.failed.contains(&settable.name()) {
                crate::capability::SecretState::Failed
            } else if self.set.iter().any(|name| name == settable.name()) {
                crate::capability::SecretState::Set
            } else {
                crate::capability::SecretState::Missing
            }
        }

        fn mail_grants(
            &self,
            _account: &crate::mail::account::MailAccount,
        ) -> crate::capability::MailGrants {
            crate::capability::MailGrants::default()
        }

        fn google_grant(&self, _: &crate::mail::address::EmailAddress) -> crate::google::Held {
            self.granted.clone().map_or(
                crate::google::Held::NotSignedIn,
                crate::google::Held::Signed,
            )
        }
    }

    /// The box's open cards, as the acts they ask for.
    fn open(inbox_store: &InMemory<InboxEvent>) -> Vec<crate::secretary::acts::Act> {
        inbox::fold(inbox_store)
            .expect("reads")
            .objects()
            .filter(|object| object.asked.proposer == Proposer::Box && object.reading.open())
            .filter_map(|object| match &object.asked.proposal {
                Proposal::Act(act) => Some(act.clone()),
                _ => None,
            })
            .collect()
    }

    fn noticed(
        store: &Store,
        conversations: &crate::secretary::conversations::InMemory,
        inbox_store: &InMemory<InboxEvent>,
        minute: u32,
    ) -> Filed {
        let settables = crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            crate::connect::Channel::ALL,
            None,
            &[],
        );
        let seen = Seen {
            settables: &settables,
            secrets: store,
            model: "a-cli/large",
            answered: &crate::health::Healths::default(),
            open: &[],
            declared: Some(&crate::connect::Declared::default()),
            health: Some(&crate::health::Healths::default()),
            heard: Some(&std::collections::BTreeSet::new()),
            pushed: &crate::settings::remote::Pushed::Nowhere,
            now: prodrome_core::fpl::instant_of(at(minute)),
            redirect: None,
        };
        notice(
            &seen,
            conversations.conversations(),
            inbox_store,
            at(minute),
        )
        .expect("filed")
    }

    /// A health register that cannot be read holds every waiting card and costs no other rule
    /// its cards: a missing secret is still asked for.
    #[test]
    fn an_unreadable_health_register_holds_and_asks_the_rest() {
        let settables = crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            [crate::connect::Channel::Ntfy],
            None,
            &[],
        );
        let seen = Seen {
            settables: &settables,
            secrets: &Store::default(),
            model: "a-cli/large",
            answered: &crate::health::Healths::default(),
            open: &[],
            declared: Some(&crate::connect::Declared::default()),
            health: None,
            heard: Some(&std::collections::BTreeSet::new()),
            pushed: &crate::settings::remote::Pushed::Nowhere,
            now: prodrome_core::fpl::instant_of(at(0)),
            redirect: None,
        };
        let owed = due(
            &seen,
            &inbox::fold(&InMemory::<InboxEvent>::default()).expect("reads"),
        );
        assert_eq!(owed.stale, Stale::Held);
        assert!(owed.iter().any(|noticed| matches!(&noticed.proposal,
            Proposal::Act(crate::secretary::acts::Act::Secret { name, .. }) if name == "NTFY_TOPIC")));
    }

    /// A locked store neither asks for a key it may hold nor withdraws a card still waiting: the
    /// card stays, and the one card the box adds asks to unlock the store; once it answers, that
    /// card is withdrawn and the waiting one is as it was.
    #[test]
    fn a_locked_store_holds_every_card_and_asks_once_to_be_unlocked() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let answering = Store {
            locked: Vec::new(),
            failed: Vec::new(),
            ..Store::default()
        };
        noticed(&answering, &conversations, &inbox_store, 0);
        let waiting = open(&inbox_store);
        assert!(
            waiting.iter().any(|act| matches!(act,
                crate::secretary::acts::Act::Secret { name, .. } if name == "NTFY_TOPIC")),
            "{waiting:?}"
        );

        let locked = Store {
            locked: vec!["NTFY_TOPIC", crate::settings::MODEL_TOKEN],
            failed: Vec::new(),
            ..Store::default()
        };
        let filed = noticed(&locked, &conversations, &inbox_store, 1);
        assert!(filed.withdrawn.is_empty(), "{filed:?}");
        let held = open(&inbox_store);
        for act in &waiting {
            assert!(
                held.contains(act),
                "{act:?} stays while the store is locked"
            );
        }
        let unlocks: Vec<_> = held
            .iter()
            .filter(|act| matches!(act, crate::secretary::acts::Act::UnlockStore { .. }))
            .collect();
        assert_eq!(unlocks.len(), 1, "{held:?}");
        assert_eq!(
            held.len(),
            waiting.len() + 1,
            "nothing else is filed: {held:?}"
        );
        let again = noticed(&locked, &conversations, &inbox_store, 2);
        assert!(again.withdrawn.is_empty());
        assert_eq!(open(&inbox_store), held, "filing again files nothing");

        let unlocked = noticed(&answering, &conversations, &inbox_store, 3);
        assert_eq!(unlocked.withdrawn.len(), 1, "the unlock card, and only it");
        assert_eq!(open(&inbox_store), waiting);
    }

    /// A name the store failed on is that name's: it is asked for by no card and holds the
    /// waiting ones, while every missing name is still asked for, and no unlock is asked that
    /// would mend nothing.
    #[test]
    fn a_name_the_store_failed_on_costs_only_itself() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let broken = Store {
            locked: Vec::new(),
            failed: vec!["NTFY_TOPIC"],
            ..Store::default()
        };
        let filed = noticed(&broken, &conversations, &inbox_store, 0);
        assert!(filed.withdrawn.is_empty(), "{filed:?}");
        let cards = open(&inbox_store);
        let asks = |wanted: &str| {
            cards.iter().any(|act| {
                matches!(act,
                crate::secretary::acts::Act::Secret { name, .. } if name == wanted)
            })
        };
        assert!(!asks("NTFY_TOPIC"), "{cards:?}");
        assert!(
            asks("FCM_SERVICE_ACCOUNT_B64"),
            "a missing name is still asked for: {cards:?}"
        );
        assert!(
            !cards
                .iter()
                .any(|act| matches!(act, crate::secretary::acts::Act::UnlockStore { .. })),
            "{cards:?}"
        );
        let answering = Store::default();
        noticed(&answering, &conversations, &inbox_store, 1);
        let waiting = open(&inbox_store);
        let refiled = noticed(&broken, &conversations, &inbox_store, 2);
        assert!(
            refiled.withdrawn.is_empty(),
            "a failed name holds its card: {refiled:?}"
        );
        assert_eq!(open(&inbox_store), waiting);
    }

    /// A calendar whose link fails is asked for its link once, whichever rule asks: by the
    /// failing-source rule while its secret is stored, and by the secrets rule once it is lost.
    /// The secret's loss is the same need, so no notice after the first appends anything, and
    /// one card for the link stands.
    #[test]
    fn a_failing_link_whose_secret_is_lost_is_asked_for_once() {
        let work =
            crate::connect::Connection::parse("ics", &["Work".to_owned()]).expect("a calendar");
        let declared = crate::connect::Declared::default()
            .with(&work)
            .expect("declared");
        let settables = crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            [],
            None,
            &declared.plugins,
        );
        let link = work.secrets().into_iter().next().expect("a link's secret");
        let health = crate::health::Healths::one(
            work.sources().into_iter().next().expect("a link's source"),
            prodrome_core::fpl::instant_of(at(0)),
            crate::health::Outcome::Failed(crate::connect::Failure::of_status(401)),
            "it answered 401",
        );
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let notice_at = |store: &Store, minute: u32| {
            let seen = Seen {
                settables: &settables,
                secrets: store,
                model: "a-cli/large",
                answered: &crate::health::Healths::default(),
                open: &[],
                declared: Some(&declared),
                health: Some(&health),
                heard: Some(&std::collections::BTreeSet::new()),
                pushed: &crate::settings::remote::Pushed::Nowhere,
                now: prodrome_core::fpl::instant_of(at(minute)),
                redirect: None,
            };
            notice(
                &seen,
                conversations.conversations(),
                &inbox_store,
                at(minute),
            )
            .expect("filed")
        };
        let written = || {
            (
                inbox_store.objects().expect("reads").len(),
                conversations.lines.objects().expect("reads").len(),
            )
        };
        let asking = || {
            open(&inbox_store)
                .into_iter()
                .filter(|act| {
                    matches!(act, crate::secretary::acts::Act::Secret { name, .. } if *name == link)
                })
                .count()
        };
        let stored = Store {
            set: vec![link.clone()],
            ..Store::default()
        };
        notice_at(&stored, 1);
        assert_eq!(asking(), 1, "the failing link is asked for its link");
        let settled = written();
        let lost = Store::default();
        for minute in 2..6 {
            let filed = notice_at(&lost, minute);
            assert!(filed.withdrawn.is_empty(), "{minute}: {filed:?}");
            assert_eq!(
                written(),
                settled,
                "a notice over the same state appends nothing"
            );
        }
        assert_eq!(asking(), 1, "one card asks for the link");
    }

    /// A link a Google account's calendar was pasted as is asked for until that account reads its
    /// calendars; then its removal is asked for instead, and its link no longer is, while a link
    /// nothing tied to the account is still asked for.
    #[test]
    fn a_link_pasted_as_a_google_calendar_is_retired_once_the_account_reads_its_calendars() {
        use crate::secretary::acts::Act;
        let calendar = |label: &str| {
            crate::connect::Connection::parse("ics", &[label.to_owned()]).expect("a calendar")
        };
        let (work, home) = (calendar("Work"), calendar("Home"));
        let ada = crate::connect::Connection::parse("google", &["ada@example.test".to_owned()])
            .expect("a Google account");
        let declared = [&work, &home, &ada]
            .into_iter()
            .try_fold(
                crate::connect::Declared::default(),
                |declared, connection| declared.with(connection),
            )
            .expect("declared");
        let settables = crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            [],
            None,
            &declared.plugins,
        );
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let notice_at = |store: &Store, minute: u32| {
            let seen = Seen {
                settables: &settables,
                secrets: store,
                model: "a-cli/large",
                answered: &crate::health::Healths::default(),
                open: &[],
                declared: Some(&declared),
                health: Some(&crate::health::Healths::default()),
                heard: Some(&std::collections::BTreeSet::new()),
                pushed: &crate::settings::remote::Pushed::Nowhere,
                now: prodrome_core::fpl::instant_of(at(minute)),
                redirect: None,
            };
            notice(
                &seen,
                conversations.conversations(),
                &inbox_store,
                at(minute),
            )
            .expect("filed")
        };
        let link = |connection: &crate::connect::Connection| connection.secrets().remove(0);
        let asks = |wanted: &str| {
            open(&inbox_store)
                .iter()
                .any(|act| matches!(act, Act::Secret { name, .. } if name == wanted))
        };
        let removes = |link: &crate::connect::Connection| {
            open(&inbox_store)
                .iter()
                .any(|act| matches!(act, Act::Disconnect { connection } if connection == link))
        };
        let unsigned = Store::default();
        notice_at(&unsigned, 0);
        assert!(asks(&link(&work)) && asks(&link(&home)));
        let pasted = inbox::fold(&inbox_store)
            .expect("reads")
            .objects()
            .find(|object| {
                matches!(&object.asked.proposal, Proposal::Act(Act::Secret { name, .. }) if *name == link(&work))
            })
            .map(|object| Reference::Proposal(object.asked.asked.clone()))
            .expect("the Work calendar's card");
        // What a tap of that card with a link to one of ada's calendars files.
        Owed::one(Proposal::Act(Act::Connect { connection: ada }), "pasted")
            .about(&[pasted])
            .since(at(1))
            .holding()
            .file(conversations.conversations(), &inbox_store, at(1))
            .expect("filed");
        notice_at(&unsigned, 2);
        assert!(
            asks(&link(&work)),
            "the account does not read its calendars yet"
        );
        assert!(!removes(&work));

        let signed = Store {
            granted: Some(crate::google::Scopes::all()),
            ..Store::default()
        };
        notice_at(&signed, 3);
        assert!(!asks(&link(&work)), "its link is no longer asked for");
        assert!(removes(&work), "its removal is asked for");
        assert!(
            asks(&link(&home)) && !removes(&home),
            "the other calendar is still asked for"
        );
        let settled = open(&inbox_store);
        notice_at(&signed, 4);
        assert_eq!(open(&inbox_store), settled, "filing again changes nothing");
    }

    /// A locked answer is the store's: the box asks once and files the unlock card, and never
    /// waits out the store's timeout for each of the other names; a store that answers is asked
    /// once per name.
    #[test]
    fn a_locked_store_is_asked_once_and_an_answering_one_once_per_name() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox_store = InMemory::<InboxEvent>::default();
        let every = [
            crate::settings::MODEL_TOKEN,
            "FCM_SERVICE_ACCOUNT_B64",
            "NTFY_TOPIC",
        ];
        let locked = Store {
            locked: every.to_vec(),
            ..Store::default()
        };
        noticed(&locked, &conversations, &inbox_store, 0);
        assert_eq!(locked.asked.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            open(&inbox_store)
                .iter()
                .any(|act| matches!(act, crate::secretary::acts::Act::UnlockStore { .. }))
        );
        let answering = Store::default();
        noticed(&answering, &conversations, &inbox_store, 1);
        assert_eq!(
            answering.asked.load(std::sync::atomic::Ordering::SeqCst),
            every.len()
        );
    }
}
