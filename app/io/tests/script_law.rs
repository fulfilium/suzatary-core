//! Law 68, the Prodrome's script law (`prodrome_core::laws::script`), over every store this
//! application keeps whose schema is not the conversations' lines: the todo chain, the
//! decisions, the inbox, the memory, the topics and their aliases, the digests and the asks.
//!
//! Each store's strategy of writes draws what the application writes, through the schema's own
//! constructors: a todo authored, revised and settled; a decision decided, revised and
//! withdrawn; a proposal asked and every response to it; a fact remembered, consolidated and
//! forgotten; a topic put on a target, taken off and aliased; a node summarized, found unfit,
//! asked about and retracted; a refusal of a post. Each actor writes under its own name at one
//! of four minutes, over three entities, so writes collide in one tick and arrive after readings
//! at later minutes.
//!
//! The normal run draws [`CASES`] scripts of at most [`STEPS`] steps for each store, with faults
//! and without. The nightly run, `cargo test -p suzatary-secretary --test all
//! script_law::a_long_run -- --ignored --nocapture`, draws 1024 scripts of twice as many steps
//! for each store and prints scripts and steps per second.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use prodrome_core::event::{
    Hash, mk_cancelled, mk_completed, mk_reopened, mk_spec_revised, mk_tended,
};
use prodrome_core::fpl;
use prodrome_core::laws::script::{self, Script, Who, a_script, an_honest_script};
use prodrome_core::literal::Datetime;
use prodrome_core::schema::Schema;
use prodrome_core::tags::{alias, membership};
use prodrome_core::term::Term;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestCaseError, TestRunner};

use suzatary_base::conversation_id::ConversationId;
use suzatary_base::topic_target::Target;
use suzatary_conversations::secretary::taint::Taint;
use suzatary_digest::model_name::ModelName;
use suzatary_digest::secretary::compact_ask::Beside;
use suzatary_digest::secretary::digest_event::{self, Attempt, DigestEvent, DigestVocabulary};
use suzatary_digest::secretary::digest_line::Line;
use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::{InboxEvent, InboxVocabulary, Said as Inboxed};
use suzatary_memory::secretary::memory::{FactId, Kind as FactKind, Origin};
use suzatary_memory::secretary::memory_event::{self, MemoryEvent, MemoryVocabulary};
use suzatary_memory::secretary::salted_text::{Text, TextName};
use suzatary_record::record::{MarkupSource, Record, TodoEvent, TodoVocabulary, mk_authored};
use suzatary_store::receipt::{Grounds, refused};
use suzatary_stores::asks::{AskEvent, AskVocabulary};
use suzatary_stores::stores::Kind;
use suzatary_todo::decisions::{
    DecisionEvent, DecisionId, DecisionVocabulary, Statement, decided, revised, withdrawn,
};
use suzatary_topics::secretary::topics::reading::{
    AliasEvent, AliasVocabulary, MembershipEvent, MembershipVocabulary, Topics,
};
use suzatary_topics::secretary::topics::topic::Topic;

use suzatary_inbox::drawn::{a_response, asked};

/// The longest script the normal run draws.
pub(crate) const STEPS: usize = 24;

/// The scripts the normal run draws for each store, with faults and without.
const CASES: u32 = 16;

/// The minute `minute` of one morning: four of them, so writes collide in one tick.
fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 1, 9, minute, 0, 0).expect("an instant")
}

/// One of three entities, by position.
fn one_of<T: Clone + std::fmt::Debug + 'static>(of: &[T]) -> impl Strategy<Value = T> {
    prop::sample::select(of.to_vec())
}

/// The minute a write is dated: one of four.
fn a_minute() -> impl Strategy<Value = Datetime> {
    (0..4u32).prop_map(at)
}

// --- the todo chain ----------------------------------------------------------------------------

const TODOS: [&str; 3] = ["alpha", "beta", "gamma"];

/// A spec: a flat price, a reference to another todo, or none.
fn a_spec() -> impl Strategy<Value = Term> {
    prop_oneof![
        (1..10u32).prop_map(|tenth| fpl::mk_flat(f64::from(tenth) / 10.0).expect("a flat")),
        one_of(&TODOS).prop_map(|todo| fpl::mk_ref(todo.to_owned()).expect("a ref")),
        Just(fpl::mk_absent()),
    ]
}

/// A todo authored (as a todo or as a decision awaiting a tap), its spec revised, or a lifecycle
/// event: what the box, the phone and the triage write to a chain.
pub(crate) fn a_todo_write(who: Who) -> impl Strategy<Value = TodoEvent> {
    let actor = who.name();
    prop_oneof![
        3 => (one_of(&TODOS), a_minute(), prop::sample::select(vec!["todo", "decision"]), a_spec())
            .prop_map(move |(todo, at, kind, spec)| {
                mk_authored(
                    todo,
                    at,
                    actor,
                    kind,
                    at,
                    MarkupSource::escaped(&format!("Do {todo}")),
                    spec,
                    Vec::new(),
                    "",
                    "",
                    MarkupSource::empty(),
                    None,
                    Vec::new(),
                    Vec::new(),
                    "",
                )
                .expect("a record")
            }),
        1 => (one_of(&TODOS), a_minute(), a_spec()).prop_map(move |(todo, at, spec)| {
            mk_spec_revised::<Record>(todo, at, actor, spec, "").expect("a revision")
        }),
        2 => (one_of(&TODOS), a_minute(), 0..4u8).prop_map(move |(todo, at, which)| {
            match which {
                0 => mk_completed::<Record>(todo, at, actor, ""),
                1 => mk_cancelled::<Record>(todo, at, actor, ""),
                2 => mk_reopened::<Record>(todo, at, actor, ""),
                _ => mk_tended::<Record>(todo, at, actor, ""),
            }
            .expect("a lifecycle event")
        }),
    ]
}

// --- the decisions -----------------------------------------------------------------------------

const DECISIONS: [&str; 3] = ["onewriter", "twochains", "nopdf"];

fn decision(name: &str) -> DecisionId {
    DecisionId::new(name).expect("a decision id")
}

/// A decision decided or revised, superseding some of the others, or withdrawn.
pub(crate) fn a_decision_write(who: Who) -> impl Strategy<Value = DecisionEvent> {
    (
        0..DECISIONS.len(),
        a_minute(),
        0..3u8,
        prop::collection::btree_set(0..DECISIONS.len(), 0..3),
    )
        .prop_map(move |(n, at, which, over)| {
            let id = decision(DECISIONS[n]);
            let supersedes: BTreeSet<DecisionId> = over
                .into_iter()
                .filter(|other| *other != n)
                .map(|other| decision(DECISIONS[other]))
                .collect();
            let statement = Statement::new(
                &format!("Decision {} at {at:?}", DECISIONS[n]),
                MarkupSource::escaped("What it says."),
                "Why it holds.",
            )
            .expect("a statement");
            match which {
                0 => decided(id, at, who.actor(), statement, supersedes),
                1 => revised(id, at, who.actor(), statement, supersedes),
                _ => withdrawn(id, at, who.actor(), "It no longer holds."),
            }
            .expect("a decision event")
        })
}

// --- the inbox ---------------------------------------------------------------------------------

const PROPOSALS: [&str; 3] = ["p0", "p1", "p2"];

/// A proposal asked, or a response to it: the responses are the inbox schema's own strategy.
pub(crate) fn an_inbox_write(who: Who) -> impl Strategy<Value = InboxEvent> {
    let said = prop_oneof![
        1 => Just(Inboxed::Asked(Box::new(asked()))),
        3 => a_response().prop_map(Inboxed::Responded),
    ];
    (one_of(&PROPOSALS), a_minute(), said).prop_map(move |(proposal, at, said)| InboxEvent {
        proposal: ProposalId::new(proposal).expect("a proposal id"),
        at,
        actor: who.actor(),
        said,
    })
}

// --- the memory --------------------------------------------------------------------------------

fn words(n: u64) -> TextName {
    TextName::new(format!("{n:064x}")).expect("a text name")
}

fn origin(turn: usize) -> Origin {
    Origin::Turn {
        conversation: ConversationId::new("default").expect("a conversation"),
        turn,
    }
}

/// Fact 1, 2 or 3 remembered, forgotten at a later version, or fact 3 consolidated from the
/// facts below it.
pub(crate) fn a_memory_write(who: Who) -> impl Strategy<Value = MemoryEvent> {
    (1..=3u64, a_minute(), 0..3u8, 1..4u64, 0..4usize).prop_map(
        move |(fact, at, which, later, turn)| {
            match which {
                0 => memory_event::remembered(
                    FactId(fact),
                    at,
                    who.actor(),
                    words(fact),
                    FactKind::ALL[turn % FactKind::ALL.len()],
                    origin(turn),
                ),
                1 => memory_event::forgotten(FactId(fact), at, who.actor(), fact + later),
                _ => memory_event::consolidated(
                    FactId(3),
                    at,
                    who.actor(),
                    (1..=fact.min(2)).map(FactId).collect(),
                    words(3 + later),
                    FactKind::Person,
                    origin(turn),
                ),
            }
            .expect("a memory event")
        },
    )
}

// --- the topics and their aliases ------------------------------------------------------------

const TOPICS: [&str; 3] = ["tax", "travel", "garden"];

fn topic(n: usize) -> Topic {
    Topic::new(TOPICS[n % TOPICS.len()]).expect("a topic")
}

fn target(turn: usize) -> Target {
    let conversation = ConversationId::new("default").expect("a conversation");
    if turn.is_multiple_of(2) {
        Target::Round { conversation, turn }
    } else {
        Target::Note { conversation, turn }
    }
}

/// A topic put on or taken off one of three targets.
pub(crate) fn a_membership_write(who: Who) -> impl Strategy<Value = MembershipEvent> {
    (0..3usize, 0..TOPICS.len(), any::<bool>(), a_minute()).prop_map(move |(turn, tag, on, at)| {
        if on {
            membership::added::<Topics>(target(turn), topic(tag), at, who.actor())
        } else {
            membership::removed::<Topics>(target(turn), topic(tag), at, who.actor())
        }
    })
}

/// A topic made an alias of another, or a name of its own again.
pub(crate) fn an_alias_write(who: Who) -> impl Strategy<Value = AliasEvent> {
    (0..TOPICS.len(), 1..TOPICS.len(), any::<bool>(), a_minute()).prop_map(
        move |(tag, step, to, at)| {
            if to {
                alias::aliased::<Topics>(topic(tag), topic(tag + step), at, who.actor())
            } else {
                alias::unaliased::<Topics>(topic(tag), at, who.actor())
            }
            .expect("never an alias of itself")
        },
    )
}

// --- the digests -------------------------------------------------------------------------------

fn node(n: u8) -> Hash {
    Hash::new(format!("{n:064x}")).expect("a name")
}

/// A node summarized from the others, found unfit, asked about, or its summary retracted.
pub(crate) fn a_digest_write(who: Who) -> impl Strategy<Value = DigestEvent> {
    (0..3u8, a_minute(), 0..4u8, any::<bool>(), 1..9u32).prop_map(
        move |(n, at, which, tainted, size)| {
            let model = ModelName::parse("a-model").expect("a model");
            match which {
                0 => digest_event::summarized(
                    node(n),
                    at,
                    who.actor(),
                    &Text::new(
                        Line::new(&format!("a line of {size}")).expect("a line"),
                        [0; 16],
                    ),
                    Attempt {
                        over: vec![node(n + 3)],
                        model,
                        taint: if tainted {
                            Taint::Tainted
                        } else {
                            Taint::Clean
                        },
                        beside: Beside::CleanWords,
                    },
                ),
                1 => digest_event::unfit(node(n), at, who.actor(), model, size),
                2 => digest_event::asked(node(n), at, who.actor(), u64::from(size)),
                _ => DigestEvent::new(node(n), at, who.actor(), digest_event::Said::Retracted),
            }
            .expect("a digest event")
        },
    )
}

// --- the asks ----------------------------------------------------------------------------------

/// The box's refusal of an object posted to a store a tab writes, final or outdated.
pub(crate) fn an_ask_write(who: Who) -> impl Strategy<Value = AskEvent> {
    (
        0..3u8,
        a_minute(),
        prop::sample::select(vec![Kind::Topics, Kind::TopicAliases, Kind::Lines]),
        any::<bool>(),
    )
        .prop_map(move |(n, at, store, last)| {
            let grounds = if last {
                Grounds::Final
            } else {
                Grounds::Outdated
            };
            refused(
                node(n),
                at,
                who.actor(),
                (store, grounds),
                format!("refused at {at:?}"),
            )
        })
}

// --- the runs ----------------------------------------------------------------------------------

fn runs<E: Schema>(
    vocabulary: &E::Vocabulary,
    script: &Script<E>,
) -> Result<script::Ran, TestCaseError> {
    script::run(vocabulary, script).map_err(|broken| TestCaseError::fail(broken.to_string()))
}

/// Each store's two laws: every invariant after every step of a script with faults, and of an
/// honest one, which also verifies clean.
macro_rules! script_laws {
    ($($store:ident: $vocabulary:expr, $writes:expr;)*) => {
        $(mod $store {
            use super::*;

            proptest! {
                #![proptest_config(Config::with_cases(CASES))]

                #[test]
                fn every_step_of_a_script_keeps_every_invariant(
                    script in a_script($writes, STEPS),
                ) {
                    runs(&$vocabulary, &script)?;
                }

                #[test]
                fn every_step_of_an_honest_script_verifies_clean(
                    script in an_honest_script($writes, STEPS),
                ) {
                    runs(&$vocabulary, &script)?;
                }
            }
        })*

        /// Law 68 over many more scripts for each store, for the nightly run; prints each
        /// store's scripts and steps per second.
        #[test]
        #[ignore = "the nightly run calls it"]
        fn a_long_run_of_every_store_keeps_every_invariant() {
            $(long_run(stringify!($store), &$vocabulary, $writes);)*
        }
    };
}

script_laws! {
    chain: TodoVocabulary::default(), a_todo_write;
    decisions: DecisionVocabulary::default(), a_decision_write;
    inbox: InboxVocabulary, an_inbox_write;
    memory: MemoryVocabulary::default(), a_memory_write;
    topics: MembershipVocabulary::default(), a_membership_write;
    topic_aliases: AliasVocabulary::default(), an_alias_write;
    digests: DigestVocabulary::default(), a_digest_write;
    asks: AskVocabulary::default(), an_ask_write;
}

/// 1024 scripts of `2 * STEPS` steps over `vocabulary`, with faults; panics at the first broken
/// invariant with its shrunk script.
fn long_run<E, S>(
    store: &str,
    vocabulary: &E::Vocabulary,
    writes: impl Fn(Who) -> S + Clone + 'static,
) where
    E: Schema,
    S: Strategy<Value = E> + 'static,
{
    let mut runner = TestRunner::new(Config::with_cases(1024));
    let started = Instant::now();
    let (scripts, steps) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let result = runner.run(&a_script(writes, 2 * STEPS), |script| {
        steps.fetch_add(runs(vocabulary, &script)?.steps, Ordering::Relaxed);
        scripts.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let seconds = started.elapsed().as_secs_f64();
    let (scripts, steps) = (
        scripts.load(Ordering::Relaxed),
        steps.load(Ordering::Relaxed),
    );
    #[allow(clippy::cast_precision_loss)]
    let rate = |n: usize| n as f64 / seconds;
    println!(
        "{store}: {scripts} scripts, {steps} steps in {seconds:.1} s: {:.1} scripts/s, {:.0} steps/s",
        rate(scripts),
        rate(steps),
    );
    if let Err(failure) = result {
        panic!("{store}: {failure}");
    }
}
