//! The inbox: its own prodrome at its own schema ([`InboxEvent`]), where a proposal is asked
//! once and each response is a write to its state register, a machine that only climbs
//! ([`crate::secretary::phase::Phase`]).
//!
//! Its id is its content ([`ProposalId::of`] hashes the act, and the instant for an act that
//! recurs), so the same draft proposed twice is one object, and [`submit`] runs the effect only
//! while the state register's reading says it still stands: a retried or second submit gets the
//! recorded response. Every reading here ([`fold`]) is monotone in the objects and takes no
//! lock; only the writes ask a negation of it, under the inbox's one lock.
//!
//! The act is a literal of the inbox's own vocabulary
//! ([`crate::secretary::act_vocabulary`]), read by the walker that reads every other field.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash, TodoId, mk_sealed, seal_hash};
use prodrome_core::fpl::{Env, FplError, link_in, mk_absent};
use prodrome_core::literal::{Datetime, ProdromeError, Value};
use prodrome_core::payload::spec_field;
use prodrome_core::policy::Everything;
use prodrome_core::registers::{Node, Stamp};
use prodrome_core::term::{Term, TermF};

use crate::chain_set::ChainSet;
use crate::content_id::ContentId;
use crate::plugin::PluginName;
use crate::reference::{ProposalRef, TodoRef};
use crate::refusal::Refusal;
use crate::secretary::inbox_event::{InboxEvent, Said};
use crate::secretary::inbox_reading::{Answer, Conflict, Reading};
use crate::secretary::inbox_registers::{ContentRegister, StateRegister};
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::{CheckFields, Done, Performed, Responded, Tried, Withdrawal};
use crate::secretary::store::{Proposal, Reasons};
use crate::secretary::tapped::Tapped;
use crate::store::Prodrome;
use prodrome_core::literal::codec::{Constructor, Fields, Literal};

/// Its fulfillment at the set's instant, its price linked in `set`: what it serves, read as that
/// todo's chain reads it, and `∅` for a chain or a todo the set does not hold; `None` where its
/// own spec names something nothing links, or its askings do not price ([`Fold::findings`] says
/// why).
#[must_use]
pub fn value(object: &Object, set: &ChainSet<'_>) -> Option<crate::wire::Fulfillment> {
    let price = object.price.as_ref().ok()?;
    crate::wire::Fulfillment::of(
        link_in(price, &BTreeMap::new(), set.stores())
            .map(|closed| prodrome_core::fpl::fulfillment(&closed, set.instant(), &Env::new())),
    )
}

/// One proposal, as the inbox stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct Asked {
    /// Who asked for it: the secretary's is printed `Asked`, the triage agent's `Triaged`, a
    /// plugin's `Offered` with the plugin's name and the box's own `Noticed`, so no stored `Asked`
    /// moved when others began to propose.
    pub proposer: Proposer,
    /// The act, whole: what the card shows is what a submit performs.
    pub proposal: Proposal,
    /// Why, and what it rests on.
    pub reasons: Reasons,
    /// Where it was first asked.
    pub asked: ProposalRef,
    /// What prices it.
    pub price: Price,
}

/// What prices a proposal: the todo it serves, or a spec of its own where it serves none.
#[derive(Debug, Clone, PartialEq)]
pub enum Price {
    /// It serves this todo, so it is priced as that todo is: stored `serves=<todo>, spec=None`.
    Serves(TodoRef),
    /// It serves no todo and carries its own spec, the empty term while nothing prices it: stored
    /// `serves=None, spec=<term>`, the empty term as `spec=None`.
    Own(Term),
}

impl Price {
    /// No todo and no spec of its own: the empty term, whose value is `∅`.
    #[must_use]
    pub fn unpriced() -> Self {
        Price::Own(mk_absent())
    }

    /// The todo it serves, if it serves one.
    #[must_use]
    pub fn serves(&self) -> Option<&TodoRef> {
        match self {
            Price::Serves(todo) => Some(todo),
            Price::Own(_) => None,
        }
    }

    /// Its term: `RefIn(<chain>, <todo>)` for what it serves, so it moves exactly when that
    /// todo's history does, and its own spec where it serves none.
    #[must_use]
    pub fn term(&self) -> Term {
        match self {
            // Built whole rather than through `mk_ref_in`, whose checks the reference's own types
            // already made: a chain name is a todo id's alphabet, and a todo id holds no `/`.
            Price::Serves(TodoRef { chain, id }) => Term::new(TermF::Ref {
                store: Some(chain.as_str().to_owned()),
                entity: id.as_str().to_owned(),
            }),
            Price::Own(spec) => spec.clone(),
        }
    }

    /// Its two stored fields, `serves` and `spec`; the empty term is stored `spec=None`, so it has
    /// one spelling on disk.
    fn fields(&self) -> [Value; 2] {
        match self {
            Price::Serves(todo) => [todo.to_value(), Value::None],
            Price::Own(spec) if matches!(spec.out(), TermF::Absent) => [Value::None, Value::None],
            Price::Own(spec) => [Value::None, spec.to_value()],
        }
    }

    /// Read back what [`Price::fields`] printed: a todo, or a spec, never both.
    fn of_fields(fields: Fields<'_>) -> Result<Self, ProdromeError> {
        match (
            fields.get::<Option<TodoRef>>("serves")?,
            spec_field(fields.call(), "spec")?,
        ) {
            (Some(todo), None) => Ok(Price::Serves(todo)),
            (None, spec) => Ok(spec.map_or_else(Price::unpriced, Price::Own)),
            (Some(todo), Some(_)) => Err(ProdromeError::invalid(format!(
                "{}: it serves {todo}, which prices it, so it carries no spec of its own",
                fields.name()
            ))),
        }
    }
}

impl Asked {
    /// Whether asking it again later is a new request ([`ProposalId::of`]): an act meant to
    /// recur, asked by anyone but the box, whose proposals are functions of a reading and so of
    /// their content alone, but where a reading's card has episodes
    /// ([`Proposal::episodic`]).
    #[must_use]
    pub fn recurs(&self) -> bool {
        self.proposal.recurs() && (self.proposer != Proposer::Box || self.proposal.episodic())
    }

    /// Its constructor's own fields, after the proposal, the instant and the actor, and their
    /// values.
    pub(crate) fn body(&self) -> (Constructor, Vec<Value>) {
        let [serves, spec] = self.price.fields();
        let asked = [
            self.proposal.to_value(),
            self.reasons.why().to_owned().to_value(),
            self.reasons.sources().to_vec().to_value(),
            self.asked.to_value(),
            serves,
            spec,
        ];
        match &self.proposer {
            Proposer::Secretary => (ASKED, asked.into()),
            Proposer::Box => (NOTICED, asked.into()),
            Proposer::Triage => (TRIAGED, asked.into()),
            Proposer::Plugin(name) => (
                OFFERED,
                std::iter::once(name.as_str().to_owned().to_value())
                    .chain(asked)
                    .collect(),
            ),
        }
    }

    /// Read back what [`Asked::body`] printed.
    ///
    /// # Errors
    /// [`ProdromeError`] naming the field that does not read.
    pub(crate) fn of_fields(fields: Fields<'_>) -> Result<Self, ProdromeError> {
        let invalid =
            |failed: String| ProdromeError::invalid(format!("{}: {failed}", fields.name()));
        let reasons = Reasons::new(fields.get::<String>("why")?, fields.get("sources")?)
            .map_err(|failed| invalid(failed.to_string()))?;
        let proposer = match fields.name() {
            "Asked" => Proposer::Secretary,
            "Noticed" => Proposer::Box,
            "Triaged" => Proposer::Triage,
            "Offered" => Proposer::Plugin(
                PluginName::parse(&fields.get::<String>("plugin")?)
                    .map_err(|failed| invalid(failed.to_string()))?,
            ),
            other => return Err(prodrome_core::literal::codec::no_arm("an asking", other)),
        };
        Ok(Asked {
            proposer,
            proposal: fields.get("act")?,
            reasons,
            asked: fields.get("asked")?,
            price: Price::of_fields(fields)?,
        })
    }
}

/// `Asked(act, why, sources, asked, serves, spec)`: the secretary's asking, after the head.
pub const ASKED: Constructor = (
    "Asked",
    &["act", "why", "sources", "asked", "serves", "spec"],
);
/// `Noticed(..)`: the box's own, the same fields.
pub const NOTICED: Constructor = (
    "Noticed",
    &["act", "why", "sources", "asked", "serves", "spec"],
);
/// `Triaged(..)`: the triage agent's, the same fields.
pub const TRIAGED: Constructor = (
    "Triaged",
    &["act", "why", "sources", "asked", "serves", "spec"],
);
/// `Offered(plugin, ..)`: a plugin's, naming it.
pub const OFFERED: Constructor = (
    "Offered",
    &["plugin", "act", "why", "sources", "asked", "serves", "spec"],
);

/// What a proposal is, as its name is the content of: its act, and the instant it was asked when
/// the act recurs ([`Asked::recurs`]). Never stored: only its print is hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identity {
    /// An act that is the same request whenever it is asked: the act's own literal.
    Once(Proposal),
    /// An act meant to recur, asked at `at`: `Recurring(at, act)`.
    Recurring {
        /// When it was asked.
        at: Datetime,
        /// The act.
        proposal: Proposal,
    },
}

/// `Recurring(at, act)`.
const RECURRING: prodrome_core::literal::codec::Constructor = ("Recurring", &["at", "act"]);

impl Identity {
    /// What `asked`, asked at `at`, is.
    #[must_use]
    pub fn of(asked: &Asked, at: Datetime) -> Self {
        if asked.recurs() {
            Identity::Recurring {
                at,
                proposal: asked.proposal.clone(),
            }
        } else {
            Identity::Once(asked.proposal.clone())
        }
    }
}

impl Literal for Identity {
    fn to_value(&self) -> Value {
        match self {
            Identity::Once(proposal) => proposal.to_value(),
            Identity::Recurring { at, proposal } => prodrome_core::literal::codec::record(
                RECURRING,
                vec![at.to_value(), proposal.to_value()],
            ),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        if prodrome_core::literal::codec::arm(value)? == RECURRING.0 {
            let fields = prodrome_core::literal::codec::Fields::of(value, RECURRING)?;
            Ok(Identity::Recurring {
                at: fields.get("at")?,
                proposal: fields.get("act")?,
            })
        } else {
            Proposal::from_value(value).map(Identity::Once)
        }
    }
}

/// A proposal's name: `p` and the first 32 hex digits of its [`Identity`]'s [`ContentId`],
/// given when the content is first asked. It names, it does not identify: a proposal carried
/// from an earlier print keeps the name that print gave it, and the reading names every
/// proposal by its content as well ([`Fold::holding`]).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct ProposalId(TodoId);

impl TryFrom<String> for ProposalId {
    type Error = ProdromeError;
    fn try_from(text: String) -> Result<Self, ProdromeError> {
        Self::new(text)
    }
}

impl From<ProposalId> for String {
    fn from(id: ProposalId) -> String {
        id.0.as_str().to_owned()
    }
}

impl ProposalId {
    /// The id `asked`, asked at `at`, is stored under.
    ///
    /// # Panics
    /// Never: `p` and hex digits are a todo id.
    #[must_use]
    pub fn of(asked: &Asked, at: Datetime) -> Self {
        Self::named(&ContentId::of(&Identity::of(asked, at)))
    }

    /// The name an identity's content id gives.
    fn named(content: &ContentId<Identity>) -> Self {
        Self(TodoId::new(format!("p{}", &content.as_str()[..32])).expect("hex is a todo id"))
    }

    /// Parse one read back off a conversation line.
    ///
    /// # Errors
    /// [`ProdromeError`] for text that is not a todo id.
    pub fn new(text: impl Into<String>) -> Result<Self, ProdromeError> {
        TodoId::new(text).map(Self)
    }

    /// Its text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// A proposal's literal is its name.
impl prodrome_core::literal::codec::Literal for ProposalId {
    fn to_value(&self) -> Value {
        Value::str(self.as_str())
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        Self::new(String::from_value(value)?)
    }
}

impl AsRef<str> for ProposalId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for ProposalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One proposal and what became of it, as its registers read.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// Its name.
    pub id: ProposalId,
    /// When it was first asked.
    pub at: Datetime,
    /// The proposal.
    pub asked: Asked,
    /// Every response, each after every one it descends from.
    pub responses: Vec<Responded>,
    /// Its state register, read.
    pub reading: Reading,
    /// Its content register's reading priced by the Prodrome's [`prodrome_core::fold::price`]
    /// (`Least` of every asking's), unlinked: what [`value`] reads through the chains' set; why
    /// not, for askings that do not price, which cost only this proposal its number.
    pub price: Result<Term, FplError>,
}

impl Object {
    /// How many times a tap performed it: every response a performer gave, a flow and the response
    /// that ended it counted once.
    #[must_use]
    pub fn tries(&self) -> usize {
        let before = std::iter::once(None).chain(self.responses.iter().map(Some));
        self.responses
            .iter()
            .zip(before)
            .filter(|(response, before)| {
                matches!(response, Responded::Tried { .. } | Responded::Done(_))
                    && !matches!(
                        before,
                        Some(Responded::Tried {
                            tried: Tried::Running { .. },
                            ..
                        })
                    )
            })
            .count()
    }
}

/// What a check holds as the reading finds it: the send that may have gone it checks, by its
/// reason. Its field is this module's, so the fold is the only place one is made, and a
/// `Responded<Check>` exists only where a send that may have gone stands beneath the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    reason: String,
}

impl Check {
    /// What the performer knew of the send it checks.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// A check as the reading holds it prints the reason of the send it checks.
impl CheckFields for Check {
    fn body(&self) -> (Constructor, Vec<Value>) {
        (("Checked", &["reason"]), vec![self.reason.to_value()])
    }
}

/// Why a proposal's state cannot be read: its reading holds a check of a send that may have gone,
/// with no such send beneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("it checks a send that nothing says may have gone")]
pub struct Unchecked;

/// Every proposal, its registers read, and every one answered and never asked, which reads as no
/// proposal: the reading is total over what the store holds (the Prodrome's law 44), so one
/// unreadable proposal costs only itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fold {
    objects: BTreeMap<ProposalId, Object>,
    unasked: BTreeSet<ProposalId>,
    /// Each object's name by the name its content gives it ([`ProposalId::of`]) and by the name
    /// its act alone gives it, the least where two objects hold one.
    named: BTreeMap<ProposalId, ProposalId>,
}

fn responded(stamp: &Stamp<InboxEvent>) -> Option<&Responded> {
    match &stamp.event.said {
        Said::Responded(responded) => Some(responded),
        Said::Asked(_) => None,
    }
}

/// What the send that may have gone `check` checks said, found by ancestry.
fn checked<'s>(stream: &'s [Stamp<InboxEvent>], check: &Stamp<InboxEvent>) -> Option<&'s str> {
    stream
        .iter()
        .rev()
        .filter(|earlier| check.descends(earlier))
        .find_map(|earlier| match responded(earlier) {
            Some(Responded::Done(Done::MayHaveGone { reason })) => Some(reason.as_str()),
            _ => None,
        })
}

/// A state write as the reading holds it, a check holding the send it checks; `None` for a check
/// with no such send beneath it.
fn answer(stream: &[Stamp<InboxEvent>], stamp: &Stamp<InboxEvent>) -> Option<Answer> {
    let response = match responded(stamp)? {
        Responded::Tried { attempt, tried } => Responded::Tried {
            attempt: *attempt,
            tried: tried.clone(),
        },
        Responded::Done(done) => Responded::Done(done.clone()),
        Responded::Checked(()) => Responded::Checked(Check {
            reason: checked(stream, stamp)?.to_owned(),
        }),
        Responded::Dismissed { reason } => Responded::Dismissed {
            reason: reason.clone(),
        },
        Responded::Withdrawn(withdrawal) => Responded::Withdrawn(withdrawal.clone()),
    };
    Some(Answer {
        name: stamp.name.clone(),
        at: stamp.event.at,
        response,
    })
}

/// One proposal's registers read off its stream; `None` for one answered and never asked.
fn object(id: &ProposalId, stream: &[Stamp<InboxEvent>]) -> Option<Object> {
    let registers = prodrome_core::fold::read(stream, None, &Everything);
    let (at, asked) = registers
        .content
        .reading::<ContentRegister>()
        .into_iter()
        .find_map(|stamp| match &stamp.event.said {
            Said::Asked(asked) => Some((stamp.event.at, asked.as_ref().clone())),
            Said::Responded(_) => None,
        })?;
    // `Least` of the askings, and the content register holds one, so none is no price either.
    let price = prodrome_core::fold::price::<InboxEvent>(&registers).and_then(|price| {
        price.ok_or_else(|| FplError("an asked proposal prices as nothing".to_owned()))
    });
    let answers = registers
        .state
        .reading::<StateRegister>()
        .into_iter()
        .map(|stamp| answer(stream, stamp))
        .collect::<Option<Vec<Answer>>>();
    let reading = answers.map_or(Reading::Unreadable(Unchecked), |answers| {
        let mut answers = answers.into_iter();
        match (answers.next(), answers.next()) {
            (None, _) => Reading::Standing,
            (Some(one), None) => Reading::One(one),
            (Some(first), Some(second)) => Reading::Conflict(Conflict {
                first,
                second,
                rest: answers.collect(),
            }),
        }
    });
    Some(Object {
        id: id.clone(),
        at,
        asked,
        responses: stream.iter().filter_map(responded).cloned().collect(),
        reading,
        price,
    })
}

impl Fold {
    /// Read every proposal's registers off the store's objects, leaving out each proposal whose
    /// objects read as none.
    #[must_use]
    pub fn of(nodes: &[Node<InboxEvent>]) -> Self {
        let folded = prodrome_core::registers::fold(nodes);
        let mut fold = Self::default();
        for (id, stream) in folded.entities() {
            match object(id, stream) {
                Some(object) => {
                    // Its own identity, and its act alone: an asking that does not recur is the
                    // same request as any proposal of that act, whoever asked it and when.
                    let act = ContentId::of(&Identity::Once(object.asked.proposal.clone()));
                    for name in [
                        ProposalId::of(&object.asked, object.at),
                        ProposalId::named(&act),
                    ] {
                        fold.named.entry(name).or_insert_with(|| id.clone());
                    }
                    fold.objects.insert(id.clone(), object);
                }
                None => {
                    fold.unasked.insert(id.clone());
                }
            }
        }
        fold
    }

    /// The events as one chain, each sealed on the one before: a history written by one writer
    /// in turn, for a test.
    ///
    /// # Panics
    /// Never: each object rests on the one before it, which is held.
    #[must_use]
    pub fn of_chain(events: &[InboxEvent]) -> Self {
        let mut objects: Vec<(Hash, Envelope<InboxEvent>)> = Vec::new();
        for event in events {
            let object = mk_sealed(objects.last().map(|(name, _)| name.clone()), event.clone());
            objects.push((seal_hash(&object), object));
        }
        let nodes = Dag::from_iter(objects).nodes().expect("a chain");
        Self::of(&nodes)
    }

    /// One proposal.
    #[must_use]
    pub fn get(&self, id: &ProposalId) -> Option<&Object> {
        self.objects.get(id)
    }

    /// Every proposal, by id.
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.values()
    }

    /// Every proposal answered and never asked, left out of the reading.
    #[must_use]
    pub fn unasked(&self) -> &BTreeSet<ProposalId> {
        &self.unasked
    }

    /// Every proposal this reading cannot read, and why: what `verify` reports.
    #[must_use]
    pub fn findings(&self) -> Vec<String> {
        let unasked = self
            .unasked
            .iter()
            .map(|id| format!("proposal {id} is answered and was never asked"));
        let unreadable = self.objects().filter_map(|object| match &object.reading {
            Reading::Unreadable(why) => {
                Some(format!("proposal {} cannot be read: {why}", object.id))
            }
            Reading::Standing | Reading::One(_) | Reading::Conflict(_) => None,
        });
        let unpriced = self.objects().filter_map(|object| {
            object
                .price
                .as_ref()
                .err()
                .map(|why| format!("proposal {} does not price: {why}", object.id))
        });
        unasked.chain(unreadable).chain(unpriced).collect()
    }

    /// The texts the remember cards name: each card's, live while the owner may still tap it.
    #[must_use]
    pub fn naming(&self) -> crate::secretary::texts::Naming<crate::secretary::memory::Fact> {
        use crate::secretary::texts::Liveness;
        self.objects()
            .filter_map(|object| match &object.asked.proposal {
                Proposal::Remember { text, .. } => {
                    let liveness = if object.reading.open() {
                        Liveness::Live
                    } else {
                        Liveness::Past
                    };
                    Some((text.clone(), liveness))
                }
                Proposal::Email(_)
                | Proposal::Forward(_)
                | Proposal::Changes(_)
                | Proposal::Act(_)
                | Proposal::Decide(_) => None,
            })
            .collect()
    }

    /// The proposal holding `asked`'s content, whatever print named it: the same [`Identity`], so
    /// an act that recurs at the same instant, and one that does not by its act alone, whoever
    /// asked it. A proposal is its content; its id is only the name [`ProposalId::of`] gave that
    /// content when it was first asked, so for every proposal one proposer filed through this
    /// build this is `get(&ProposalId::of(asked, at))`. The box's asking is held as
    /// [`Self::held`] says, its episode begun at the asking.
    #[must_use]
    pub fn holding(&self, asked: &Asked, at: Datetime) -> Option<&Object> {
        match asked.proposer {
            Proposer::Box => self.held(&asked.proposal, Some(at)),
            Proposer::Secretary | Proposer::Triage | Proposer::Plugin(_) => {
                self.get(self.named.get(&ProposalId::of(asked, at))?)
            }
        }
    }

    /// The card that holds the box's asking of `proposal`, whoever asked it: an open card asking
    /// for the same [`Need`](crate::secretary::store::Need), in whatever words; else, for an act
    /// with episodes ([`Proposal::episodic`]), a card of its act asked at or after `since`, when
    /// its episode began (`None`: any card); for any other act, any card of its act. The one
    /// rule for whether the box has asked, so one need has one card.
    #[must_use]
    pub fn held(&self, proposal: &Proposal, since: Option<Datetime>) -> Option<&Object> {
        let open = self.objects.values().find(|object| {
            object.reading.open() && object.asked.proposal.need() == proposal.need()
        });
        open.or_else(|| {
            self.objects.values().find(|object| {
                object.asked.proposal == *proposal
                    && (!proposal.episodic() || since.is_none_or(|since| object.at >= since))
            })
        })
    }
}

/// What a transition answered: a response it recorded now, or the reading as it stood, recording
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    /// Recorded by this call.
    Ran(Responded),
    /// What stood before it: returned, and nothing performed or recorded.
    Recorded(Reading),
}

impl Fold {
    /// The proposal a write acts on: one the inbox holds, whose reading is no conflict.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for a proposal the inbox does not hold, and [`Refusal::conflict`]
    /// for one whose reading is a conflict.
    pub fn one(&self, id: &ProposalId) -> Result<&Object, Refusal> {
        if self.unasked.contains(id) {
            return Err(Refusal::conflict(format!(
                "proposal {id} is answered and was never asked; nothing acts on it"
            )));
        }
        let object = self
            .get(id)
            .ok_or_else(|| Refusal::not_found(format!("the inbox holds no proposal {id}")))?;
        match &object.reading {
            Reading::Conflict(conflict) => Err(Refusal::conflict(format!(
                "proposal {id} reads {} responses at once, none descending from the others; \
                 nothing acts on it until one response settles them",
                conflict.sides()
            ))),
            Reading::Unreadable(why) => Err(Refusal::conflict(format!(
                "proposal {id} cannot be read: {why}; nothing acts on it"
            ))),
            Reading::Standing | Reading::One(_) => Ok(object),
        }
    }
}

/// What a transition does with one proposal, decided from its object alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Record this response.
    Record(Responded),
    /// Record nothing, and answer the reading as it stands.
    Answer,
}

/// The one transition of one proposal: under the inbox's hold, read it, let `step` decide from
/// its object alone, and record what `step` says before the hold is let go. Every negation a
/// write asks of the history ("is it still standing?", "is it a send that may have gone?") is a
/// `step`, so each is asked under the same hold, and each is a pure function of a value but
/// [`submit`]'s, which performs inside its step.
///
/// # Errors
/// [`Refusal::not_found`] for a proposal the inbox does not hold, [`Refusal::conflict`] for one in
/// conflict, whatever `step` refuses, and the store's own failures.
pub fn transition<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    step: impl FnOnce(&Object) -> Result<Step, Refusal>,
) -> Result<Response, Refusal> {
    let _held = caps.locked()?;
    transition_held(caps, id, by, at, step)
}

/// [`transition`], under the inbox's one hold, which the caller has taken.
///
/// # Errors
/// As [`transition`].
pub fn transition_held<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    step: impl FnOnce(&Object) -> Result<Step, Refusal>,
) -> Result<Response, Refusal> {
    let fold = fold(caps)?;
    let object = fold.one(id)?;
    match step(object)? {
        Step::Answer => Ok(Response::Recorded(object.reading.clone())),
        Step::Record(responded) => {
            caps.append(InboxEvent {
                proposal: object.id.clone(),
                at,
                actor: by.clone(),
                said: Said::Responded(responded.clone()),
            })?;
            Ok(Response::Ran(responded))
        }
    }
}

/// Read the inbox.
///
/// # Errors
/// [`Refusal::internal`] for a store that cannot be read.
pub fn fold<C: Prodrome<InboxEvent> + ?Sized>(caps: &C) -> Result<Fold, Refusal> {
    Ok(Fold::of(&caps.nodes()?))
}

/// What `verify` reports of the inbox: the store's own findings, then every proposal its reading
/// cannot read, and why, or why the inbox does not read at all.
#[must_use]
pub fn findings(store: &dyn Prodrome<InboxEvent>) -> Vec<String> {
    let mut found = store.verify();
    match fold(store) {
        Ok(fold) => found.extend(fold.findings()),
        Err(refused) => found.push(format!("the inbox does not read: {}", refused.message())),
    }
    found
}

/// File a proposal and answer its id; the same content asked again ([`Fold::holding`]) is the
/// same proposal, whatever id it is filed under, and writes nothing.
///
/// # Errors
/// The store's own failures.
pub fn ask<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    asked: Asked,
    by: &Actor,
    at: Datetime,
) -> Result<ProposalId, Refusal> {
    let _held = caps.locked()?;
    ask_held(caps, asked, by, at)
}

/// [`ask`], under the inbox's one hold, which the caller has taken: a filing that must decide
/// more than the ask (the card's index, a plugin's room) decides it under the same hold.
///
/// # Errors
/// The store's own failures.
pub fn ask_held<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    asked: Asked,
    by: &Actor,
    at: Datetime,
) -> Result<ProposalId, Refusal> {
    if let Some(held) = fold(caps)?.holding(&asked, at) {
        return Ok(held.id.clone());
    }
    let id = ProposalId::of(&asked, at);
    caps.append(InboxEvent {
        proposal: id.clone(),
        at,
        actor: by.clone(),
        said: Said::Asked(Box::new(asked)),
    })?;
    Ok(id)
}

/// `submit(id) -> response`: perform the proposal while the tap's rule ([`Tapped`]) holds on its
/// reading, recording what performing answered as the reading's next try where it is one; the
/// NON-MONOTONE question that guards an effect ("is it still standing, so I may perform?"). A
/// `perform` that refuses records nothing.
///
/// # Errors
/// As [`transition`], and whatever `perform` refuses.
pub fn submit<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    perform: impl FnOnce(&Asked) -> Result<Performed, Refusal>,
) -> Result<Response, Refusal> {
    use crate::rule::Rule as _;
    transition(caps, id, by, at, |object| {
        Ok(if (Tapped { now: at }).holds(object) {
            Step::Record(Responded::performed(
                perform(&object.asked)?,
                object.reading.next_attempt(),
            ))
        } else {
            Step::Answer
        })
    })
}

/// Dismiss a proposal nothing completed, with `reason`; dismissing one already dismissed answers
/// the recorded dismissal.
///
/// # Errors
/// As [`transition`], and [`Refusal::conflict`] for one completed or running: what happened
/// cannot be answered no.
pub fn dismiss<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    reason: &str,
) -> Result<Response, Refusal> {
    transition(caps, id, by, at, |object| dismissing(object, reason))
}

/// Withdraw a proposal nothing completed, saying why: the box's or the secretary's act, never
/// the owner's no. Withdrawing one already withdrawn or dismissed answers what was recorded.
///
/// # Errors
/// As [`transition`], and [`Refusal::conflict`] for one completed or running.
pub fn withdraw<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    withdrawal: Withdrawal,
) -> Result<Response, Refusal> {
    transition(caps, id, by, at, |object| withdrawing(object, withdrawal))
}

/// [`withdraw`] a proposal nobody has answered yet: once the owner has tapped it, even a tap that
/// failed, the card is his to settle, and this refuses.
///
/// # Errors
/// As [`transition`], and [`Refusal::conflict`] for a proposal with any response.
pub fn withdraw_unanswered<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    withdrawal: Withdrawal,
) -> Result<Response, Refusal> {
    transition(caps, id, by, at, |object| match object.reading {
        Reading::Standing => withdrawing(object, withdrawal),
        Reading::One(_) | Reading::Conflict(_) | Reading::Unreadable(_) => {
            Err(Refusal::conflict(format!(
                "proposal {} has been answered, so it is the owner's to settle",
                object.id
            )))
        }
    })
}

/// [`withdraw`], under the inbox's one hold, which the caller has taken.
///
/// # Errors
/// As [`withdraw`].
pub fn withdraw_held<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    withdrawal: Withdrawal,
) -> Result<Response, Refusal> {
    transition_held(caps, id, by, at, |object| withdrawing(object, withdrawal))
}

/// The owner has checked a send that may have gone: records his check and performs nothing;
/// acknowledging one already acknowledged answers the recorded check.
///
/// # Errors
/// As [`transition`], and [`Refusal::conflict`] for one that is no send that may have gone.
pub fn acknowledge<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
) -> Result<Response, Refusal> {
    transition(caps, id, by, at, checking)
}

/// End a running act with what its flow came to, at `at` (the flow's thread and a restart hold
/// no clock). An act no longer running records nothing and answers `false`.
///
/// # Errors
/// As [`transition`].
pub fn end<C: Prodrome<InboxEvent> + ?Sized>(
    caps: &C,
    id: &ProposalId,
    by: &Actor,
    at: Datetime,
    ended: Performed,
) -> Result<bool, Refusal> {
    transition(caps, id, by, at, |object| Ok(ending(object, ended)))
        .map(|response| matches!(response, Response::Ran(_)))
}

/// What the response a reading shows is, `None` while nothing answered it.
fn shown(object: &Object) -> Option<&Responded<Check>> {
    object.reading.shown().map(|answer| &answer.response)
}

/// [`dismiss`]'s step: record the no while nothing completed it.
fn dismissing(object: &Object, reason: &str) -> Result<Step, Refusal> {
    closing(
        object,
        Responded::Dismissed {
            reason: reason.to_owned(),
        },
    )
}

/// [`withdraw`]'s step: record the withdrawal while nothing completed it.
fn withdrawing(object: &Object, withdrawal: Withdrawal) -> Result<Step, Refusal> {
    closing(object, Responded::Withdrawn(withdrawal))
}

/// Record `closed`, a no or a withdrawal, while nothing completed the proposal; one already
/// dismissed or withdrawn answers what was recorded.
fn closing(object: &Object, closed: Responded) -> Result<Step, Refusal> {
    match shown(object) {
        None
        | Some(Responded::Tried {
            tried: Tried::NotSent { .. },
            ..
        }) => Ok(Step::Record(closed)),
        Some(Responded::Dismissed { .. } | Responded::Withdrawn(_)) => Ok(Step::Answer),
        Some(Responded::Tried {
            tried: Tried::Running { .. },
            ..
        }) => Err(Refusal::conflict(format!(
            "proposal {} is running and ends by itself",
            object.id
        ))),
        Some(Responded::Done(_) | Responded::Checked(_)) => Err(Refusal::conflict(format!(
            "proposal {} was already performed and cannot be answered no",
            object.id
        ))),
    }
}

/// [`acknowledge`]'s step: record the check of a send that may have gone.
fn checking(object: &Object) -> Result<Step, Refusal> {
    match shown(object) {
        Some(Responded::Done(Done::MayHaveGone { .. })) => Ok(Step::Record(Responded::Checked(()))),
        Some(Responded::Checked(_)) => Ok(Step::Answer),
        None
        | Some(
            Responded::Tried { .. }
            | Responded::Done(Done::Sent { .. } | Done::Applied { .. } | Done::Acted { .. })
            | Responded::Dismissed { .. }
            | Responded::Withdrawn(_),
        ) => Err(Refusal::conflict(format!(
            "proposal {} is no send that may have gone; there is nothing to acknowledge",
            object.id
        ))),
    }
}

/// [`end`]'s step: record what a running flow came to.
fn ending(object: &Object, ended: Performed) -> Step {
    if object.reading.running() {
        Step::Record(Responded::performed(ended, object.reading.next_attempt()))
    } else {
        Step::Answer
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::Actor;

    use super::*;
    use crate::secretary::store::Draft;
    use crate::store::Reads as _;

    fn draft(subject: &str) -> Proposal {
        Proposal::Email(Draft {
            account: "ada@example.com".to_owned(),
            to: vec!["mira@example.com".to_owned()],
            cc: Vec::new(),
            subject: subject.to_owned(),
            body: "Thursday works.".to_owned(),
            attachments: Vec::new(),
        })
    }

    fn asked(subject: &str) -> Asked {
        Asked {
            proposer: Proposer::Secretary,
            proposal: draft(subject),
            reasons: Reasons::new(
                "she asked for a day",
                vec!["mail:abcdef012345".parse().expect("a ref")],
            )
            .expect("reasons"),
            asked: "proposal:default/0".parse().expect("a ref"),
            price: Price::Serves("todo:life/reply".parse().expect("a ref")),
        }
    }

    fn at(minute: u32) -> Datetime {
        Datetime::new(2026, 9, 28, 9, minute, 0, 0).expect("an instant")
    }

    fn signing_in(asking: &str) -> Asked {
        Asked {
            proposer: Proposer::Secretary,
            proposal: Proposal::Act(crate::secretary::acts::Act::Auth {
                account: crate::mail::address::EmailAddress::new("ada@example.com")
                    .expect("a shaped address"),
            }),
            reasons: Reasons::new(asking, vec!["mail:abcdef012345".parse().expect("a ref")])
                .expect("reasons"),
            asked: "proposal:default/1".parse().expect("a ref"),
            price: Price::unpriced(),
        }
    }

    /// An act is stored as its literal, every field printed, and its id is hashed from that.
    #[test]
    fn an_act_prints_as_its_literal_and_is_named_by_it() {
        let proposal = Proposal::Email(Draft {
            account: "her@example.test".to_owned(),
            to: vec!["counsel@example.test".to_owned()],
            cc: Vec::new(),
            subject: "Sent at once".to_owned(),
            body: "Sending the documents today.".to_owned(),
            attachments: Vec::new(),
        });
        assert_eq!(
            prodrome_core::literal::print_literal(&proposal.to_value()),
            "Email(account='her@example.test', to=('counsel@example.test',), cc=(), \
             subject='Sent at once', body='Sending the documents today.', attachments=())"
        );
        let asked = Asked {
            proposer: Proposer::Secretary,
            proposal,
            reasons: Reasons::new(
                "Counsel asked for the exhibits and the last scan is done.",
                vec!["mail:a1b2c3d4e5f6".parse().expect("a ref")],
            )
            .expect("reasons"),
            asked: "proposal:2026-09-20-1/0".parse().expect("a ref"),
            price: Price::unpriced(),
        };
        assert_eq!(
            ProposalId::of(&asked, at(0)).as_str(),
            "pcdc930db4292e3df4e89762218f0ef3f"
        );
    }

    /// A proposal filed under a name an earlier print gave it is found by its content: asking
    /// the same act again answers that name and writes nothing, and a recurring act asked at
    /// another instant is another proposal.
    #[test]
    fn the_same_content_is_the_same_proposal_whatever_named_it() {
        let store = crate::store::InMemory::<InboxEvent>::default();
        let ada = Actor::new("ada").expect("an actor");
        let carried = ProposalId::new("p0123456789abcdef0123456789abcdef").expect("an id");
        store
            .append(InboxEvent {
                proposal: carried.clone(),
                at: at(0),
                actor: ada.clone(),
                said: Said::Asked(Box::new(asked("a"))),
            })
            .expect("files under the old name");
        let before = store.objects().expect("reads").len();
        assert_eq!(ask(&store, asked("a"), &ada, at(5)), Ok(carried.clone()));
        assert_eq!(
            store.objects().expect("reads").len(),
            before,
            "nothing written"
        );
        dismiss(&store, &carried, &ada, at(6), "not this week").expect("dismissed");
        assert_eq!(
            ask(&store, asked("a"), &ada, at(7)),
            Ok(carried.clone()),
            "a settled proposal is the answer too"
        );
        let first = ask(&store, signing_in("sign in"), &ada, at(8)).expect("asks");
        assert_eq!(
            ask(&store, signing_in("sign in"), &ada, at(8)),
            Ok(first.clone())
        );
        assert_ne!(ask(&store, signing_in("sign in"), &ada, at(9)), Ok(first));
    }

    proptest::proptest! {
        /// Over any history the inbox's own `ask` wrote, the reading names every proposal by
        /// its content: `holding` is `get` of the content's id, every asking is held, and a
        /// content never asked is held by nothing.
        #[test]
        fn holding_is_get_of_the_content_id(
            asks in proptest::collection::vec((0u8..3, proptest::bool::ANY, 0u32..3), 1..10),
        ) {
            let store = crate::store::InMemory::<InboxEvent>::default();
            let ada = Actor::new("ada").expect("an actor");
            let make = |subject: u8, recurring: bool| {
                if recurring {
                    let mut asked = signing_in("sign in again");
                    asked.asked = format!("proposal:default/{subject}").parse().expect("a ref");
                    asked
                } else {
                    asked(&format!("subject {subject}"))
                }
            };
            for (subject, recurring, minute) in &asks {
                ask(&store, make(*subject, *recurring), &ada, at(*minute)).expect("asks");
            }
            let fold = fold(&store).expect("reads");
            for subject in 0..4 {
                for recurring in [false, true] {
                    for minute in 0..4 {
                        let asked = make(subject, recurring);
                        let held = fold.holding(&asked, at(minute));
                        proptest::prop_assert_eq!(
                            held,
                            fold.get(&ProposalId::of(&asked, at(minute)))
                        );
                        let was_asked = asks.iter().any(|(s, r, m)| {
                            *r == recurring
                                && (if recurring { *m == minute } else { *s == subject })
                        });
                        proptest::prop_assert_eq!(held.is_some(), was_asked);
                    }
                }
            }
        }
    }

    /// The box's asking of a sign-in, an episode, is held by the secretary's earlier recurring
    /// asking of the same act while that card still waits: one request, one card.
    #[test]
    fn the_box_asking_an_act_the_secretary_asked_is_the_same_proposal() {
        let store = crate::store::InMemory::<InboxEvent>::default();
        let ada = Actor::new("ada").expect("an actor");
        let first = ask(&store, signing_in("sign in"), &ada, at(1)).expect("asks");
        let boxs = Asked {
            proposer: Proposer::Box,
            ..signing_in("the box noticed")
        };
        assert_eq!(ask(&store, boxs, &ada, at(5)), Ok(first));
    }

    /// A card of the act answered no holds nothing, so the box's next episode is a new card;
    /// one still waiting holds it.
    #[test]
    fn a_dismissed_card_does_not_hold_the_box_s_next_episode() {
        let store = crate::store::InMemory::<InboxEvent>::default();
        let ada = Actor::new("ada").expect("an actor");
        let boxs = || Asked {
            proposer: Proposer::Box,
            ..signing_in("the box noticed")
        };
        let first = ask(&store, signing_in("sign in"), &ada, at(1)).expect("asks");
        dismiss(&store, &first, &ada, at(2), "not now").expect("dismisses");
        let again = ask(&store, boxs(), &ada, at(5)).expect("asks again");
        assert_ne!(again, first);
        assert_eq!(ask(&store, boxs(), &ada, at(7)), Ok(again));
    }

    /// A draft with an attachment is a different act, so a different id.
    #[test]
    fn an_attachment_is_part_of_the_act() {
        let plain = asked("a");
        let Proposal::Email(mut draft) = plain.proposal.clone() else {
            unreachable!("asked builds an email");
        };
        draft.attachments = vec![
            crate::secretary::attached::Attached::of(
                crate::secretary::attachment_source::AttachmentSource::Mail {
                    message: "m1@example.test".to_owned(),
                    name: "notice.pdf".to_owned(),
                },
                "notice.pdf",
                "application/pdf",
                b"%PDF-1.7",
            )
            .expect("records"),
        ];
        let text =
            prodrome_core::literal::print_literal(&Proposal::Email(draft.clone()).to_value());
        assert!(
            text.contains("attachments=(Attached(source=FromMail("),
            "{text}"
        );
        let attaching = Asked {
            proposal: Proposal::Email(draft),
            ..plain.clone()
        };
        assert_ne!(
            ProposalId::of(&attaching, at(0)),
            ProposalId::of(&plain, at(0))
        );
    }

    #[test]
    fn an_effect_that_must_not_repeat_is_its_content_alone() {
        let id = ProposalId::of(&asked("a"), at(0));
        assert_eq!(
            id,
            ProposalId::of(&asked("a"), at(1)),
            "a later round, the same email"
        );
        let mut elsewhere = asked("a");
        elsewhere.asked = "proposal:other/4".parse().expect("a ref");
        elsewhere.reasons =
            Reasons::new("reworded", vec!["thread:abc".parse().expect("a ref")]).expect("reasons");
        elsewhere.price = Price::unpriced();
        assert_eq!(
            id,
            ProposalId::of(&elsewhere, at(2)),
            "asked anywhere, for any reason"
        );
        assert_ne!(
            id,
            ProposalId::of(&asked("b"), at(0)),
            "another subject is another email"
        );
    }

    #[test]
    fn an_act_meant_to_recur_is_its_content_and_its_instant() {
        let id = ProposalId::of(&signing_in("the grant lapsed"), at(0));
        assert_eq!(id, ProposalId::of(&signing_in("reworded"), at(0)));
        assert_ne!(id, ProposalId::of(&signing_in("the grant lapsed"), at(1)));
    }

    fn asked_event(asked: Asked) -> InboxEvent {
        InboxEvent {
            proposal: ProposalId::of(&asked, at(0)),
            at: at(0),
            actor: Actor::new("assistant").expect("an actor"),
            said: Said::Asked(Box::new(asked)),
        }
    }

    #[test]
    fn an_asked_event_prints_and_parses_back() {
        let event = asked_event(asked("a"));
        let printed = prodrome_core::event::canonical(&event);
        let back = prodrome_core::event::parse_event::<InboxEvent>(
            &crate::secretary::inbox_event::InboxVocabulary,
            &printed,
        )
        .expect("it parses");
        assert_eq!(back, event);
    }

    /// A plugin's proposal is a kind of its own, `Offered`, naming the plugin in a field; the
    /// secretary's still prints `Asked`, so no stored proposal moved. Who asked is not the act,
    /// so the same act is the same proposal whoever asked for it.
    #[test]
    fn a_plugins_proposal_is_offered_naming_it_and_reads_back() {
        let offered = Asked {
            proposer: Proposer::Plugin(PluginName::parse("dueday").expect("a name")),
            ..asked("a")
        };
        let event = asked_event(offered.clone());
        let printed = prodrome_core::event::canonical(&event);
        assert!(
            printed.starts_with("Offered(proposal=")
                && printed.contains("actor='assistant', plugin='dueday', act=Email("),
            "{printed}"
        );
        let back = prodrome_core::event::parse_event::<InboxEvent>(
            &crate::secretary::inbox_event::InboxVocabulary,
            &printed,
        )
        .expect("it parses");
        assert_eq!(back, event);
        assert!(
            prodrome_core::event::canonical(&asked_event(asked("a"))).starts_with("Asked("),
            "the secretary's print is unchanged"
        );
        assert_eq!(
            ProposalId::of(&offered, at(0)),
            ProposalId::of(&asked("a"), at(0))
        );
    }

    /// The box's own proposal prints `Noticed` and reads back; asked of the store's unlocking, an
    /// act that recurs when the secretary asks it and has no episodes, its name is its content
    /// alone, so the box asking again at another instant is the same proposal.
    #[test]
    fn the_boxs_proposal_is_noticed_and_named_by_its_content() {
        let act = Proposal::Act(crate::secretary::acts::Act::UnlockStore { after: None });
        let noticed = Asked {
            proposer: Proposer::Box,
            proposal: act.clone(),
            ..asked("a")
        };
        let event = asked_event(noticed.clone());
        let printed = prodrome_core::event::canonical(&event);
        assert!(printed.starts_with("Noticed(proposal="), "{printed}");
        let back = prodrome_core::event::parse_event::<InboxEvent>(
            &crate::secretary::inbox_event::InboxVocabulary,
            &printed,
        )
        .expect("it parses");
        assert_eq!(back, event);
        assert_eq!(
            ProposalId::of(&noticed, at(0)),
            ProposalId::of(&noticed, at(9))
        );
        let asked_by_the_secretary = Asked {
            proposal: act,
            ..asked("a")
        };
        assert_ne!(
            ProposalId::of(&asked_by_the_secretary, at(0)),
            ProposalId::of(&asked_by_the_secretary, at(9)),
            "the secretary's ask still recurs"
        );
    }

    /// Unpriced is `Absent` in memory and `spec=None` on disk, so the empty term has one spelling
    /// there; it reads back `Absent`, valued `∅`.
    #[test]
    fn an_unpriced_proposal_is_stored_as_none_and_reads_absent() {
        let unpriced = Asked {
            price: Price::unpriced(),
            ..asked("a")
        };
        let printed = prodrome_core::event::canonical(&asked_event(unpriced));
        assert!(printed.contains("serves=None, spec=None"), "{printed}");
        assert!(!printed.contains("Absent"), "{printed}");
        let InboxEvent {
            said: Said::Asked(back),
            ..
        } = prodrome_core::event::parse_event::<InboxEvent>(
            &crate::secretary::inbox_event::InboxVocabulary,
            &printed,
        )
        .expect("it parses")
        else {
            panic!("an asked event")
        };
        assert!(matches!(&back.price, Price::Own(spec) if matches!(spec.out(), TermF::Absent)));
    }

    /// A check reads only above the send it checks; one over nothing is no inbox at all.
    #[test]
    fn a_check_reads_only_above_a_send_that_may_have_gone() {
        let id = ProposalId::of(&asked("a"), at(0));
        let response = |minute, said| InboxEvent {
            proposal: id.clone(),
            at: at(minute),
            actor: Actor::new("ada").expect("an actor"),
            said: Said::Responded(said),
        };
        let gone = response(
            1,
            Responded::Done(Done::MayHaveGone {
                reason: "no answer".to_owned(),
            }),
        );
        let check = response(2, Responded::Checked(()));
        let unchecked = Fold::of_chain(&[asked_event(asked("a")), check.clone()]);
        assert_eq!(
            unchecked.get(&id).map(|object| &object.reading),
            Some(&Reading::Unreadable(Unchecked))
        );
        let checked = Fold::of_chain(&[asked_event(asked("a")), gone, check]);
        assert!(checked.unasked().is_empty() && checked.get(&id).is_some());
    }

    /// Each transition's step is a function of the object alone, read here off folds and never a
    /// store: a no while nothing completed it, a check over a send that may have gone, and a
    /// flow's end over a running one.
    #[test]
    fn every_step_is_a_function_of_the_object() {
        let id = ProposalId::of(&asked("a"), at(0));
        let object = |responses: &[Responded]| -> Object {
            let events: Vec<InboxEvent> = std::iter::once(asked_event(asked("a")))
                .chain(
                    responses
                        .iter()
                        .zip(1..)
                        .map(|(response, minute)| InboxEvent {
                            proposal: id.clone(),
                            at: at(minute),
                            actor: Actor::new("ada").expect("an actor"),
                            said: Said::Responded(response.clone()),
                        }),
                )
                .collect();
            Fold::of_chain(&events).get(&id).expect("held").clone()
        };
        let not_sent = Responded::Tried {
            attempt: 1,
            tried: Tried::NotSent {
                reason: "refused".to_owned(),
            },
        };
        let running = Responded::Tried {
            attempt: 1,
            tried: Tried::Running {
                began: "2026-09-28T09:01:00".to_owned(),
                expires_at: "2026-09-28T09:15:00".to_owned(),
            },
        };
        let gone = Responded::Done(Done::MayHaveGone {
            reason: "no answer".to_owned(),
        });
        let sent = Responded::Done(Done::Sent {
            recipients: Vec::new(),
            message_id: "<1@box>".to_owned(),
        });
        let no = Responded::Dismissed {
            reason: "not now".to_owned(),
        };
        for answered in [&[][..], &[not_sent.clone()][..]] {
            assert!(matches!(
                dismissing(&object(answered), "not now"),
                Ok(Step::Record(Responded::Dismissed { .. }))
            ));
        }
        assert!(matches!(
            dismissing(&object(std::slice::from_ref(&no)), "again"),
            Ok(Step::Answer)
        ));
        assert!(dismissing(&object(std::slice::from_ref(&running)), "no").is_err());
        assert!(dismissing(&object(std::slice::from_ref(&sent)), "no").is_err());
        let replaced = || Withdrawal::Superseded;
        for answered in [&[][..], &[not_sent.clone()][..]] {
            assert_eq!(
                withdrawing(&object(answered), replaced()),
                Ok(Step::Record(Responded::Withdrawn(replaced())))
            );
        }
        let withdrawn = Responded::Withdrawn(Withdrawal::NoLongerNeeded {
            why: "she replied first".to_owned(),
        });
        for closed in [&withdrawn, &no] {
            assert_eq!(
                withdrawing(&object(std::slice::from_ref(closed)), replaced()),
                Ok(Step::Answer),
                "{closed}"
            );
            assert_eq!(
                dismissing(&object(std::slice::from_ref(closed)), "again"),
                Ok(Step::Answer)
            );
        }
        assert!(withdrawing(&object(std::slice::from_ref(&running)), replaced()).is_err());
        assert!(withdrawing(&object(std::slice::from_ref(&sent)), replaced()).is_err());
        assert!(matches!(
            checking(&object(std::slice::from_ref(&gone))),
            Ok(Step::Record(Responded::Checked(())))
        ));
        assert!(matches!(
            checking(&object(&[gone, Responded::Checked(())])),
            Ok(Step::Answer)
        ));
        assert!(checking(&object(&[sent])).is_err());
        let acted = Performed::Done(Done::Acted {
            what: "signed in".to_owned(),
        });
        assert!(matches!(
            ending(&object(&[running]), acted.clone()),
            Step::Record(Responded::Done(Done::Acted { .. }))
        ));
        assert_eq!(ending(&object(&[not_sent]), acted), Step::Answer);
    }

    /// Sent on one device and dismissed on another reads as a conflict of exactly those two
    /// answers, least named first: it performs nothing, its card offers no answer, and a write is
    /// refused until one response settles it.
    #[test]
    fn two_answers_apart_are_a_conflict_of_two() {
        let ada = Actor::new("ada").expect("an actor");
        let (one, two) = (
            crate::store::InMemory::<InboxEvent>::default(),
            crate::store::InMemory::<InboxEvent>::default(),
        );
        let id = ask(&one, asked("a"), &ada, at(0)).expect("asks");
        let asking: Vec<(Hash, String)> = one
            .objects()
            .expect("reads")
            .iter()
            .map(|(name, object)| {
                (
                    name.clone(),
                    prodrome_core::event::canonical_envelope(object),
                )
            })
            .collect();
        two.accept(&asking).expect("takes the asking");
        submit(&one, &id, &ada, at(1), |_| {
            Ok(Done::Sent {
                recipients: vec!["mira@example.com".to_owned()],
                message_id: "<1@box>".to_owned(),
            }
            .into())
        })
        .expect("sent here");
        dismiss(&two, &id, &ada, at(1), "not this one").expect("dismissed there");
        let union: Vec<_> = one
            .objects()
            .expect("reads")
            .into_iter()
            .chain(two.objects().expect("reads"))
            .collect();
        let fold = Fold::of(&Dag::from_iter(union).nodes().expect("one history"));
        let object = fold.get(&id).expect("asked");
        let Reading::Conflict(conflict) = &object.reading else {
            panic!("not a conflict: {}", object.reading);
        };
        assert_eq!(conflict.sides(), 2);
        assert!(
            conflict.first.name < conflict.second.name,
            "least named first"
        );
        assert_eq!(object.reading.shown(), Some(&conflict.first));
        assert!(!object.reading.performs(), "a conflict performs nothing");
        let card = crate::secretary::card::of(&crate::wire::ProposedAct::Auth {
            account: "ada@example.com".to_owned(),
            sentence: "Sign in".to_owned(),
        });
        assert!(!card.answers.is_empty());
        assert!(
            crate::secretary::card::at(card, &object.reading)
                .answers
                .is_empty()
        );
        assert!(fold.one(&id).is_err());
    }

    /// Withdrawn on one replica and dismissed on another settles as the owner's no, with his
    /// words, whichever write names the lesser object: a withdrawal is below a no.
    #[test]
    fn a_withdrawal_and_a_no_apart_settle_as_the_no() {
        let ada = Actor::new("ada").expect("an actor");
        let assistant = Actor::new("assistant").expect("an actor");
        let (one, two) = (
            crate::store::InMemory::<InboxEvent>::default(),
            crate::store::InMemory::<InboxEvent>::default(),
        );
        let id = ask(&one, asked("a"), &assistant, at(0)).expect("asks");
        let asking: Vec<(Hash, String)> = one
            .objects()
            .expect("reads")
            .iter()
            .map(|(name, object)| {
                (
                    name.clone(),
                    prodrome_core::event::canonical_envelope(object),
                )
            })
            .collect();
        two.accept(&asking).expect("takes the asking");
        withdraw(&one, &id, &assistant, at(1), Withdrawal::Superseded).expect("withdrawn here");
        dismiss(&two, &id, &ada, at(1), "not this one").expect("dismissed there");
        let union: Vec<_> = one
            .objects()
            .expect("reads")
            .into_iter()
            .chain(two.objects().expect("reads"))
            .collect();
        let fold = Fold::of(&Dag::from_iter(union).nodes().expect("one history"));
        let object = fold.get(&id).expect("asked");
        let Reading::One(answer) = &object.reading else {
            panic!("not settled: {}", object.reading);
        };
        assert_eq!(
            answer.response,
            Responded::Dismissed {
                reason: "not this one".to_owned()
            }
        );
        assert!(fold.one(&id).is_ok(), "nothing is left to settle");
    }

    /// What prices a proposal reads back as one of its two cases: a todo it serves, or a spec
    /// of its own, never both, and a `serves` that names no todo does not read.
    #[test]
    fn a_price_serves_a_todo_or_is_its_own() {
        use crate::secretary::inbox_event::InboxVocabulary;
        use prodrome_core::event::{canonical, parse_event};
        let printed = canonical(&asked_event(asked("a")));
        assert!(
            printed.contains("serves='todo:life/reply', spec=None"),
            "{printed}"
        );
        let both = printed.replace("spec=None", "spec=Flat(value=0.4)");
        assert!(parse_event::<InboxEvent>(&InboxVocabulary, &both).is_err());
        let mail = printed.replace("todo:life/reply", "mail:abcdef012345");
        assert!(parse_event::<InboxEvent>(&InboxVocabulary, &mail).is_err());
        let own = Asked {
            price: Price::Own(prodrome_core::fpl::mk_flat(0.4).expect("a flat")),
            ..asked("a")
        };
        let event = asked_event(own);
        let printed = canonical(&event);
        assert!(
            printed.contains("serves=None, spec=Flat(value=0.4)"),
            "{printed}"
        );
        assert_eq!(parse_event(&InboxVocabulary, &printed), Ok(event));
    }
}
