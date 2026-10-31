//! The vectors the Rust writes and `web/test/vectors.test.ts` must agree with.
//!
//! It lives in `suzatary-core` because every capability and route it needs is this crate's, so
//! the six-todo chain is a byte-for-byte port of `app/cli/tests/common::six_todo_chain` rather
//! than a dependency in the wrong direction.
//!
//! A disagreement is a finding, never a file to refresh: vectors are written only under
//! `SUZATARY_WRITE_VECTORS=1`, and otherwise recomputed and compared.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Duration;
use serde::Serialize;

use prodrome_core::event::{Actor, Hash, mk_completed, mk_created};
use prodrome_core::fpl::{self, mk_decay, mk_flat};
use prodrome_core::policy::Untrusted;

use suzatary_core::capability::Entropy;
use suzatary_core::capability::{
    AnswerError, Answers, Clock, Datetime, Html, Model, ModelError, Nothing, Render, RenderError,
    Server, Turn,
};
use suzatary_core::cast::{self, CAST};
use suzatary_core::chain_set::Read;
use suzatary_core::chains::{ChainName, ChainPolicy};
use suzatary_core::deployment::Deployment;
use suzatary_core::escape::Escaped;
use suzatary_core::record::TodoEvent;
use suzatary_core::record::{
    Envelope, EventStore, Node, SubTodo, mk_authored, mk_source, mk_subtodo,
};
use suzatary_core::reference::{ProposalRef, Reference};
use suzatary_core::routes;
use suzatary_core::secretary::about;
use suzatary_core::secretary::inbox::{self, ProposalId};
use suzatary_core::secretary::memories::Memories;
use suzatary_core::secretary::memory::{Fact, Kind};
use suzatary_core::secretary::readings;
use suzatary_core::secretary::responded::{Done, Performed, Tried};
use suzatary_core::secretary::routes::{self as secretary_routes, Cards};
use suzatary_core::secretary::salted_text::{Text, TextName};
use suzatary_core::secretary::store::{
    ConversationId, Draft, Entry as ConversationEntry, Proposal, Reasons, Stored, stamp,
};
use suzatary_core::secretary::transcript;
use suzatary_core::store::{
    InMemory, InMemoryDoc, Prodrome, Reads, StoreError, WriteLock, append_to, linearised,
};
use suzatary_core::wire;

fn vector(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("web")
        .join(file)
}

/// One comparison for every vector, pretty-printed with a trailing newline so a diff reads.
fn write_or_check<T: Serialize>(value: &T, file: &str) {
    let printed = format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("a vector serialises")
    );
    let path = vector(file);
    if std::env::var_os("SUZATARY_WRITE_VECTORS").is_some() {
        std::fs::write(&path, &printed).expect("the vector directory is writable");
        return;
    }
    let stored = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is committed: {e}", path.display()));
    assert_eq!(
        stored,
        printed,
        "{} disagrees with what suzatary-core now computes. That is a finding, \
         not staleness (docs/WEBALGEBRA-PLAN-2026-09-16.md §1 item 5) — decide \
         whether the Rust or the TypeScript is right, fix the one that is \
         wrong, and only then rerun with SUZATARY_WRITE_VECTORS=1 to update \
         this file in the same commit.",
        path.display()
    );
}

/// The instant the six-todo chain is authored around, as `app/cli/tests/common::now`.
fn now() -> Datetime {
    Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("a real instant")
}

/// `at` shifted by whole days, as `app/cli/tests/common::shift`.
fn shift(at: Datetime, days: i64) -> Datetime {
    fpl::datetime_of(fpl::instant_of(at) + Duration::days(days)).expect("in range")
}

/// `at` shifted by whole seconds, for the scripted conversation's turns.
fn later(at: Datetime, seconds: i64) -> Datetime {
    fpl::datetime_of(fpl::instant_of(at) + Duration::seconds(seconds)).expect("in range")
}

fn actor(name: &str) -> Actor {
    Actor::new(name).expect("a roster name")
}

/// A scratch directory unique to this process and test, since one process's tests run in
/// parallel.
fn scratch(test: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "suzatary-core-web-vectors-{test}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// The six-todo chain, ported from `app/cli/tests/common::six_todo_chain` with [`CAST`]'s prose.
#[expect(
    clippy::too_many_lines,
    reason = "ported from app/cli/tests/common::six_todo_chain, which carries \
              the identical allow for the identical reason: six todos, each \
              priced in full"
)]
fn six_todo_chain(root: &Path) -> EventStore {
    let untrusted: Untrusted = Untrusted::of([actor("triage")]);
    let events = EventStore::new(root.join("events"), untrusted);
    // Begun with a nonce of zeros, so the vectors' names are a function of this file.
    let genesis: suzatary_core::record::Envelope = prodrome_core::event::Envelope::Genesis(
        prodrome_core::genesis::mk_genesis(suzatary_core::record::GENESIS, &"0".repeat(32))
            .expect("a genesis"),
    );
    let name = prodrome_core::event::seal_hash(&genesis);
    events
        .adopt_objects(
            &std::collections::BTreeMap::from([(
                name.clone(),
                prodrome_core::event::canonical_envelope(&genesis),
            )]),
            &name,
        )
        .expect("begins");
    let now = now();
    let born = shift(now, -30);
    let made = shift(now, -20);
    let owner = CAST.owner.actor;
    let todos = &CAST.todos;

    let append = |event| {
        append_to(&events, suzatary_core::record::GENESIS, event).expect("appends");
    };
    let authored = |todo: &cast::Todo, at, actor, spec, source| {
        let subtodos: Vec<SubTodo> = todo
            .subtodos
            .iter()
            .map(|sub| mk_subtodo(sub.text, sub.done).expect("valid"))
            .collect();
        mk_authored(
            todo.id,
            at,
            actor,
            "todo",
            made,
            suzatary_core::record::MarkupSource::from_chain(todo.body),
            spec,
            todo.rationale.iter().map(|&line| line.to_owned()).collect(),
            todo.category,
            "",
            suzatary_core::record::MarkupSource::from_chain(todo.detail),
            source,
            subtodos,
            Vec::new(),
            "",
        )
        .expect("valid")
    };
    for todo in todos.all() {
        append(mk_created(todo.id, born, owner, todo.created, "").expect("valid"));
        match todo.id {
            id if id == todos.urgent.id => append(authored(
                todo,
                made,
                owner,
                mk_decay(
                    0.6,
                    0.05,
                    fpl::instant_of(shift(now, 2)),
                    Duration::days(14),
                    Some(fpl::instant_of(shift(now, -12))),
                )
                .expect("valid"),
                None,
            )),
            id if id == todos.flat.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.55).expect("valid"),
                    None,
                ));
            }
            id if id == todos.checklist.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.72).expect("valid"),
                    None,
                ));
            }
            id if id == todos.resolved.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.4).expect("valid"),
                    None,
                ));
                append(mk_completed(todo.id, shift(made, 1), owner, "").expect("valid"));
            }
            id if id == todos.high.id => {
                append(authored(
                    todo,
                    made,
                    owner,
                    mk_flat(0.88).expect("valid"),
                    None,
                ));
            }
            _ => append(authored(
                todo,
                shift(made, 2),
                "triage",
                mk_flat(0.66).expect("valid"),
                Some(mk_source(
                    CAST.mail[0].from.email,
                    CAST.mail[0].subject,
                    made,
                    AGENTS_TODO_MESSAGE_HASH,
                    "",
                    "",
                )),
            )),
        }
    }
    events
}

/// The source hash the agent's todo was authored from, as in `app/cli/tests/common`.
const AGENTS_TODO_MESSAGE_HASH: &str =
    "e2e0000message0000hash0000000000000000000000000000000000000000";

/// The fixture's one chain, as in `app/cli/tests/common`.
/// The bundle's one chain, read once: the set every row and proposal is priced through.
fn chains_of(bundle: &Bundle) -> Read {
    let policy = chain_policy();
    Read::of_stores([(&policy, bundle as &dyn Prodrome<TodoEvent>)])
}

fn chain_policy() -> ChainPolicy {
    ChainPolicy {
        name: ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor(CAST.owner.actor), actor("triage")]
            .into_iter()
            .collect(),
        root: suzatary_core::cast::keys::root(),
    }
}

/// A deployment over the fixture's roster; [`Cards`] needs one only for a changeset's categories.
fn deployment() -> Deployment {
    Deployment {
        viewers: std::collections::BTreeSet::new(),
        actors: BTreeMap::new(),
        chains: suzatary_core::chains::Chains::of(vec![chain_policy()]).expect("one chain"),
        categories: [
            ("work".to_owned(), 1),
            ("home".to_owned(), 2),
            ("other".to_owned(), 3),
            ("study".to_owned(), 4),
        ]
        .into_iter()
        .collect(),
        app_url: "https://example.test".to_owned(),
        coding: None,
        zone: suzatary_core::zone::Zone::UTC,
    }
}

/// An exclusive hold that holds nothing, since nothing here writes concurrently.
struct NoLock;
impl WriteLock for NoLock {}

/// Every capability the vectors need, over one real chain and an in-memory conversation table.
///
/// `Render` and `Answers` echo, so no vector needs typst, and `Model` is never called because
/// every turn is scripted.
struct Bundle {
    events: EventStore,
    conversations: suzatary_core::secretary::conversations::InMemory,
    inbox: InMemory<suzatary_core::secretary::inbox_event::InboxEvent>,
    memory: InMemory<suzatary_core::secretary::memory_event::MemoryEvent>,
    texts: InMemoryDoc<TextName, Text<Fact>>,
    digests: InMemory<suzatary_core::secretary::digest_event::DigestEvent>,
    digest_texts: InMemoryDoc<TextName, Text<suzatary_core::secretary::digest_line::Line>>,
    owners: std::collections::BTreeSet<prodrome_core::event::Actor>,
    entropy: Counting,
    at: Datetime,
}

/// Each salt the next byte, so a card's text name is the same on every run.
#[derive(Default)]
struct Counting(std::sync::atomic::AtomicU8);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        Ok(())
    }
}

impl Bundle {
    fn over(events: EventStore, at: Datetime) -> Self {
        Self {
            events,
            conversations: suzatary_core::secretary::conversations::InMemory::default(),
            inbox: InMemory::default(),
            memory: InMemory::default(),
            texts: InMemoryDoc::default(),
            digests: InMemory::default(),
            digest_texts: InMemoryDoc::default(),
            owners: std::collections::BTreeSet::new(),
            entropy: Counting::default(),
            at,
        }
    }

    /// The memory, whose text store keeps a remember card's words.
    fn memories(&self) -> Memories<'_> {
        Memories {
            history: &self.memory,
            texts: &self.texts,
            cards: &self.inbox,
            entropy: &self.entropy,
            digests: suzatary_core::secretary::digests::Digests {
                history: &self.digests,
                texts: &self.digest_texts,
                lines: self.conversations.conversations(),
                owners: &self.owners,
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        }
    }

    /// File a remember card about `fact`, its words kept in the memory's text store, as the
    /// `remember` verb does in a round that read something not the owner's own.
    fn card(
        &self,
        id: &ConversationId,
        index: usize,
        fact: &str,
        at: Datetime,
    ) -> ConversationEntry {
        let held = self.memories().hold().expect("held");
        let kept = held
            .keep_card(&Fact::new(fact).expect("a fact"))
            .expect("kept");
        self.file(
            id,
            index,
            Proposal::Remember {
                text: kept.name().clone(),
                kind: Kind::Person,
            },
            Reasons::new(
                "her signature says so",
                vec!["mail:a1b2c3d4e5f6".parse().expect("a reference")],
            )
            .expect("a why and a source"),
            None,
            at,
        )
    }

    /// File one proposal through the real `inbox::ask` and answer the line that names it.
    fn file(
        &self,
        id: &ConversationId,
        index: usize,
        proposal: Proposal,
        reasons: Reasons,
        serves: Option<suzatary_core::reference::TodoRef>,
        at: Datetime,
    ) -> ConversationEntry {
        let asked = inbox::Asked {
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
            proposal,
            reasons,
            asked: format!("proposal:{id}/{index}")
                .parse()
                .expect("a reference"),
            price: serves.map_or_else(inbox::Price::unpriced, inbox::Price::Serves),
        };
        let assistant = Actor::new("assistant").expect("an actor");
        ConversationEntry::Proposed {
            at,
            object: inbox::ask(&self.inbox, asked, &assistant, at).expect("filed"),
            revises: None,
        }
    }

    /// Seed one conversation's whole transcript at once, which `Stored::of` folds the same as
    /// appends.
    fn seed(&self, id: &ConversationId, entries: &[ConversationEntry]) {
        transcript::start(self.conversations.conversations(), id, &entries[0]).expect("opens");
        for entry in &entries[1..] {
            transcript::push(self.conversations.conversations(), id, entry).expect("pushes");
        }
    }
}

/// The server's record, with this bundle behind every effect a route reads.
fn server_of(bundle: &Bundle) -> Server<'_> {
    Server {
        secretary: suzatary_core::capability::Secretary {
            chain: bundle,
            render: bundle,
            clock: bundle,
            model: bundle,
            inbox: &bundle.inbox,
            conversations: bundle.conversations.conversations(),
            ..Nothing::secretary()
        },
        answers: bundle,
        ..Nothing::server()
    }
}

impl Reads<TodoEvent> for Bundle {
    fn objects(&self) -> Result<Vec<(Hash, Envelope)>, StoreError> {
        linearised(&self.events).map_err(|e| StoreError::unreadable("chain", e.to_string()))
    }

    fn nodes(&self) -> Result<Vec<Node>, StoreError> {
        prodrome_core::dag::Dag::from_iter(self.objects()?)
            .nodes()
            .map_err(|e| StoreError::unreadable("chain", e.to_string()))
    }

    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        Ok(std::collections::BTreeSet::new())
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.events
            .tips()
            .map(|tips| tips.into_iter().collect())
            .map_err(|e| StoreError::unreadable("chain", e.to_string()))
    }

    fn verify(&self) -> Vec<String> {
        self.events
            .verify()
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

impl Prodrome<TodoEvent> for Bundle {
    fn append(&self, event: suzatary_core::record::TodoEvent) -> Result<Hash, StoreError> {
        append_to(&self.events, suzatary_core::record::GENESIS, event)
            .map_err(|e| StoreError::not_appended("chain", e.to_string()))
    }

    fn accept(&self, _objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        Err(StoreError::not_appended(
            "chain",
            "the vectors fixture accepts nothing".to_owned(),
        ))
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        Ok(Box::new(NoLock))
    }
}

impl suzatary_core::capability::Typeset for Bundle {
    fn closed(&self, source: &str) -> Result<String, suzatary_core::capability::Rejected> {
        suzatary_core::capability::parsed(source)
            .map(|()| "<!DOCTYPE html><html><body></body></html>".to_owned())
    }
}

impl Render for Bundle {
    fn html(&self, markup: &Escaped) -> Result<Html, RenderError> {
        Ok(Html::from_render(markup.as_str()))
    }

    fn cached(&self, markup: &Escaped) -> Option<Html> {
        Some(Html::from_render(markup.as_str()))
    }

    fn warm(&self, markup: &[Escaped]) -> usize {
        markup.len()
    }

    fn page(&self, page: &suzatary_core::reading::Page) -> Result<Html, RenderError> {
        Ok(Html::from_render(page.as_str()))
    }
}

impl Clock for Bundle {
    fn now(&self) -> Datetime {
        self.at
    }
}

impl Model for Bundle {
    fn name(&self) -> String {
        "vectors-fixture".to_owned()
    }

    fn turn(
        &self,
        _conversation: &suzatary_core::capability::Conversation,
        _tools: &serde_json::Value,
    ) -> Result<suzatary_core::capability::Reply, ModelError> {
        Err(ModelError::Unreachable(
            "the vectors fixture scripts every turn; nothing asks the model".to_owned(),
        ))
    }
}

impl Answers for Bundle {
    fn block(
        &self,
        _block: &suzatary_core::secretary::answer::Renderable,
    ) -> Result<suzatary_core::capability::Html, AnswerError> {
        Err(AnswerError(
            "the vectors fixture renders nothing; every block is its source".to_owned(),
        ))
    }
}

/// Two rounds and one proposal about [`CAST`]'s agent todo, so the vectors describe one scene.
///
/// The proposal follows round one's two `Said` lines, which makes it round one's own.
/// A conversation holding every kind of line a round reads or skips: a rename, a stop, a rewind
/// and, once `resealed` runs, a turn sealed under older rules and sealed again.
fn every_line() -> Vec<ConversationEntry> {
    use suzatary_core::secretary::store::{Namer, Title};
    use suzatary_core::secretary::words::Words;
    let opened = later(now(), 200);
    let owner = CAST.owner.actor.to_owned();
    let person = |at: u32, text: &str| ConversationEntry::Said {
        at: later(opened, at.into()),
        turn: Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(CAST.owner.actor.into()),
    };
    let model = |at: u32, text: &str| ConversationEntry::Said {
        at: later(opened, at.into()),
        turn: Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        },
        by: None,
    };
    vec![
        ConversationEntry::Opened {
            at: opened,
            title: Words::kept("Every line".to_owned()),
            model: "vectors-fixture".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        },
        person(1, "what is planted first?"),
        model(2, "The beans, in the second week."),
        ConversationEntry::Titled {
            at: later(opened, 3),
            title: Words::kept(Title::new("The garden, by week").expect("a title")),
            by: Namer::Person {
                actor: owner.clone(),
            },
        },
        person(4, "and the peas?"),
        ConversationEntry::Stopped {
            at: later(opened, 5),
            by: owner.clone(),
        },
        person(6, "the peas, then?"),
        model(7, "An answer set aside."),
        ConversationEntry::Rewound {
            to: 5,
            at: later(opened, 8),
            by: owner,
        },
        model(9, "The peas, in the third week."),
    ]
}

/// One turn of conversation `id`, written under the rules from before lines recorded them, then
/// every such slot sealed again as `migrate-scrub` does.
fn resealed(bundle: &Bundle, id: &ConversationId) {
    use suzatary_core::secretary::line_event::LineEvent;
    use suzatary_core::secretary::scrubbed_under::ScrubbedUnder;
    use suzatary_core::store::Doc as _;
    let held = transcript::read(bundle.conversations.conversations(), id)
        .expect("reads")
        .lines()
        .len();
    let at = later(now(), 220);
    let (line, keys) = LineEvent::written_under(
        &ScrubbedUnder::Unversioned,
        id.clone(),
        Actor::new("assistant").expect("an actor"),
        u32::try_from(held).expect("a count"),
        &ConversationEntry::Said {
            at,
            turn: Turn::Person {
                text: "and the beans after that?".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some(CAST.owner.actor.into()),
        },
        &bundle.conversations.entropy,
    )
    .expect("a line");
    for key in &keys {
        bundle
            .conversations
            .keys
            .set(&key.name(), key)
            .expect("kept");
    }
    bundle.conversations.lines.append(line).expect("appended");
    let resealed = suzatary_core::secretary::rescrub::reseal(
        bundle.conversations.conversations(),
        later(at, 1),
    )
    .expect("reseals");
    assert!(resealed > 0, "the older turn is sealed again");
}

fn scripted_conversation(bundle: &Bundle, id: &ConversationId) -> Vec<ConversationEntry> {
    let opened = now();
    let asked = later(opened, 1);
    let answered = later(opened, 20);
    let proposed = later(opened, 21);
    let followed_up = later(opened, 90);
    let closed = later(opened, 95);

    let reference_todo: suzatary_core::reference::TodoRef =
        format!("todo:life/{}", CAST.todos.agents.id)
            .parse()
            .expect("a todo reference");
    // A reference's hash is validated hex, so this only matches `AGENTS_TODO_MESSAGE_HASH`'s
    // length.
    let reference_mail: Reference =
        "mail:e2e0000000000000000000000000000000000000000000000000000000000000"
            .parse()
            .expect("a mail reference");

    vec![
        ConversationEntry::Opened {
            at: opened,
            title: suzatary_core::secretary::words::Words::kept("About the deed".to_owned()),
            model: "vectors-fixture".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        },
        ConversationEntry::Said {
            at: asked,
            turn: Turn::Person {
                text: format!("email {} about the deed", CAST.counsel.name),
                references: vec![reference_todo.clone().into()],
                attachments: Vec::new(),
            },
            by: None,
        },
        ConversationEntry::Said {
            at: answered,
            turn: Turn::Model {
                text: "I will draft that now.".to_owned(),
                calls: Vec::new(),
            },
            by: None,
        },
        bundle.file(
            id,
            0,
            Proposal::Email(Draft {
                account: CAST.owner.email.to_owned(),
                to: vec![CAST.counsel.email.to_owned()],
                cc: Vec::new(),
                subject: "Your deed".to_owned(),
                body: "Following up on the deed — draft attached.".to_owned(),
                attachments: Vec::new(),
            }),
            Reasons::new(
                "the deed is due and nothing has gone out yet",
                vec![reference_mail],
            )
            .expect("a why and a source"),
            Some(reference_todo.clone()),
            proposed,
        ),
        ConversationEntry::Said {
            at: followed_up,
            turn: Turn::Person {
                text: "thanks — anything else on it?".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
        ConversationEntry::Said {
            at: closed,
            turn: Turn::Model {
                text: "Nothing else today.".to_owned(),
                calls: Vec::new(),
            },
            by: None,
        },
    ]
}

#[derive(Serialize)]
struct FoldAnswer {
    /// The open todos, ascending by fulfillment with ties by id, as `web/src/todo/todos.ts`'s
    /// `byUrgency`.
    open_ascending: Vec<String>,
    /// How many open todos there are.
    count: usize,
}

fn fold_answer(payload: &wire::FoldPayload) -> FoldAnswer {
    let mut open: Vec<&wire::Entry> = payload
        .entries
        .iter()
        .filter(|entry| entry.state == wire::EntryState::Open)
        .collect();
    open.sort_by(|a, b| {
        let number = |entry: &wire::Entry| entry.value.and_then(wire::Fulfillment::number);
        let left = number(a).unwrap_or(f64::INFINITY);
        let right = number(b).unwrap_or(f64::INFINITY);
        left.partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.todo.cmp(&b.todo))
    });
    FoldAnswer {
        count: open.len(),
        open_ascending: open.into_iter().map(|entry| entry.todo.clone()).collect(),
    }
}

#[derive(Serialize)]
struct ChainAnswer {
    /// `verify`'s verdict, empty when every object hashes to its name and every head reaches
    /// genesis.
    verify: Vec<String>,
    /// The linearisation's order, parents before children; not compared with `verify.ts`'s
    /// heads-first walk, which may pick another valid order.
    order: Vec<String>,
}

fn chain_answer(payload: &wire::ChainPayload) -> ChainAnswer {
    ChainAnswer {
        verify: payload.verify.clone(),
        order: payload.links.iter().map(|link| link.hash.clone()).collect(),
    }
}

#[derive(Serialize)]
struct ConversationAnswer {
    /// One inclusive `[start, end]` of absolute turn indices per round, from
    /// `about::round_spans`.
    rounds: Vec<(usize, usize)>,
    /// Which round each proposal belongs to, in proposal order.
    proposal_rounds: Vec<usize>,
}

fn conversation_answer(stored: &Stored) -> ConversationAnswer {
    let spans = about::round_spans(&stored.turns);
    let proposal_rounds = stored
        .proposals
        .iter()
        .map(|standing| about::owning_span(&spans, standing.after_turn.saturating_sub(1)))
        .collect();
    ConversationAnswer {
        rounds: spans,
        proposal_rounds,
    }
}

/// Builds the three payloads and their answers over the six-todo chain at a fixed instant, once,
/// and checks them against `tests/web/`.
/// The oldest write contract the box takes, which the bundle's own contract must reach.
#[test]
fn the_client_vector_holds_the_box_s_minimum() {
    write_or_check(
        &serde_json::json!({ "min": suzatary_core::gate::CLIENT_MIN }),
        "client.json",
    );
}

#[test]
fn the_vectors_match_what_the_pure_ts_modules_compute() {
    // The cast, for the TypeScript tests' people, todos and mail.
    write_or_check(&CAST, "cast.json");

    let root = scratch("payloads");
    let bundle = Bundle::over(six_todo_chain(&root), now());

    let conversation_id = ConversationId::new("about-the-deed").expect("a slug");
    let script = scripted_conversation(&bundle, &conversation_id);
    bundle.seed(&conversation_id, &script);

    let policy = chain_policy();
    let fold_payload = routes::fold(
        &bundle,
        &policy.name,
        &policy.untrusted,
        &chains_of(&bundle).set(now()),
        None,
    )
    .expect("the fold builds over a closed six-todo chain");
    assert_eq!(
        fold_payload.heads,
        routes::tip(&bundle).expect("the tip reads").heads,
        "a fold names the heads it read as the stream names them, so a tab compares the two"
    );
    write_or_check(&fold_payload, "fold.json");
    write_or_check(&fold_answer(&fold_payload), "fold.answer.json");

    let chain_payload = routes::chain(&bundle, &chain_policy()).expect("the chain payload builds");
    write_or_check(&chain_payload, "chain.json");
    write_or_check(&chain_answer(&chain_payload), "chain.answer.json");

    let deployment = deployment();
    let cards: Cards<'_> = Cards {
        deployment: &deployment,
        chains: &[],
        plugins: None,
        reach: &suzatary_core::connect::PLAIN,
        settables: &suzatary_core::settings::Settables::default(),
    };
    let conversation_payload =
        secretary_routes::newest(&server_of(&bundle), &cards, &conversation_id)
            .expect("the scripted conversation folds");
    write_or_check(&conversation_payload, "conversation.json");

    // Folded directly, since the wire payload may one day be windowed to less than the
    // transcript.
    let stored = Stored::of(&conversation_id, &script).expect("the scripted transcript folds");
    write_or_check(&conversation_answer(&stored), "conversation.answer.json");
}

/// The standings vector: every standing a line can take, each failing one read from a register
/// holding a failure of its class, as the overview reads one. A line's words in the tab are a
/// function of its standing alone, and `web/test/overview.test.ts` reads every one of these.
#[test]
fn the_standings_vector_is_every_standing_a_line_can_take() {
    use suzatary_core::connect::{Failure, Group, Standing};
    use suzatary_core::health::Outcome;
    type Healths = suzatary_core::health::Healths<String>;
    let at = fpl::instant_of(now());
    let failed = |class: Failure| {
        Standing::of_health(
            Healths::one("source", at - Duration::hours(1), Outcome::Succeeded, "")
                .join(&Healths::one(
                    "source",
                    at,
                    Outcome::Failed(class),
                    "what the source said",
                ))
                .reading("source"),
        )
    };
    let standings: Vec<Standing> = [
        Standing::Kept,
        Standing::of_health(Healths::one("source", at, Outcome::Succeeded, "").reading("source")),
        Standing::of_calls(Healths::one("source", at, Outcome::Succeeded, "").reading("source")),
        Standing::of_health(None),
        Standing::Locked,
        Standing::StoreFailed,
        Standing::Unreadable {
            words: "its ledger could not be read".to_owned(),
        },
        Standing::missing("your Work calendar's secret link"),
        Standing::ByHand {
            step: 0,
            command: "example-cli auth login".to_owned(),
            why: "sign the CLI in".to_owned(),
        },
    ]
    .into_iter()
    .chain(Failure::ALL.map(failed))
    .collect();
    write_or_check(
        &wire::Overview {
            groups: Group::ALL
                .into_iter()
                .map(|group| wire::OverviewGroup {
                    group,
                    lines: if group == Group::Plugins {
                        standings
                            .iter()
                            .enumerate()
                            .map(|(n, standing)| wire::OverviewLine {
                                key: format!("plugin:p{n}"),
                                label: format!("p{n}"),
                                detail: None,
                                standing: standing.clone(),
                                cards: Vec::new(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                })
                .collect(),
            ask: "setup".to_owned(),
        },
        "standings.json",
    );
}

/// The blocks vector: answers the splitter cut, each block named as the stream names it.
#[test]
fn the_blocks_vector_is_what_the_splitter_computes() {
    #[derive(Serialize)]
    struct Cut {
        answer: &'static str,
        blocks: Vec<Named>,
    }
    #[derive(Serialize)]
    struct Named {
        #[serde(flatten)]
        block: suzatary_core::secretary::answer::Block,
        sha: String,
    }
    let answers = [
        "",
        "Just one line.",
        "= The deed\n\nIt is due *Friday*; I drafted the cover letter.\n\n- scan the old deed\n- sign page 3\n",
        "Before:\n\n```rust\nfn a() {}\n\nfn b() {}\n```\n\nAfter.",
        "$ sum_(i=1)^n i = (n(n+1))/2 $\n\n#panel[Two\n\nparagraphs]\n\n\n\nTrailing.\n\n",
        "- loose\n\n- list\n\nthen prose",
    ];
    let cuts: Vec<Cut> = answers
        .into_iter()
        .map(|answer| Cut {
            answer,
            blocks: suzatary_core::secretary::answer::blocks(answer)
                .into_iter()
                .map(|block| Named {
                    sha: suzatary_core::secretary::answer::sha(&block.source),
                    block,
                })
                .collect(),
        })
        .collect();
    write_or_check(&cuts, "blocks.json");
}

/// One proposal of [`every_stage`]'s: whether it is open, and what it waits on.
#[derive(Serialize)]
struct StageAnswer {
    open: bool,
}

/// A conversation with one proposal at every stage, each answered through the `inbox` code a tap
/// runs; each is a different subject so each is its own object.
fn every_stage(bundle: &Bundle, id: &ConversationId) -> Vec<ConversationEntry> {
    let opened = now();
    let uncertain = Performed::Done(Done::MayHaveGone {
        reason: "the message was handed over and no answer came: reset".to_owned(),
    });
    let given = [
        Some(Performed::Done(Done::Sent {
            recipients: vec![CAST.counsel.email.to_owned()],
            message_id: "<vectors@box.example>".to_owned(),
        })),
        Some(Performed::Done(Done::Applied {
            objects: vec!["a1b2c3".to_owned()],
            chain: "life".to_owned(),
        })),
        Some(Performed::Tried(Tried::NotSent {
            reason: "auth: 535 5.7.8 Authentication failed".to_owned(),
        })),
        Some(uncertain.clone()),
        Some(Performed::Tried(Tried::Running {
            began: stamp(later(opened, 30)),
            expires_at: stamp(later(opened, 930)),
        })),
        Some(Performed::Done(Done::Acted {
            what: "the grant is stored".to_owned(),
        })),
        None,
    ];
    let reasons = || {
        Reasons::new(
            "the deed is due",
            vec!["briefing:2026-09-27".parse().expect("a reference")],
        )
        .expect("a why and a source")
    };
    let email = |index: usize| {
        Proposal::Email(Draft {
            account: CAST.owner.email.to_owned(),
            to: vec![CAST.counsel.email.to_owned()],
            cc: Vec::new(),
            subject: format!("Your deed ({index})"),
            body: "Following up on the deed.".to_owned(),
            attachments: Vec::new(),
        })
    };
    let mut entries = vec![ConversationEntry::Opened {
        at: opened,
        title: suzatary_core::secretary::words::Words::kept("Every stage".to_owned()),
        model: "vectors-fixture".to_owned(),
        kind: suzatary_core::secretary::store::Kind::Topic,
    }];
    let ada = Actor::new("ada").expect("an actor");
    let checked = given.len() + 1;
    for index in 0..=checked {
        let at = later(opened, i64::try_from(index).expect("a few"));
        let line = bundle.file(id, index, email(index), reasons(), None, at);
        let ConversationEntry::Proposed { object, .. } = &line else {
            unreachable!("file answers a proposed line");
        };
        let object: ProposalId = object.clone();
        entries.push(line);
        if index == checked {
            inbox::submit(&bundle.inbox, &object, &ada, now(), |_| {
                Ok(uncertain.clone())
            })
            .expect("submitted");
            inbox::acknowledge(&bundle.inbox, &object, &ada, now()).expect("checked");
            continue;
        }
        match index.checked_sub(1).map(|at| given[at].clone()) {
            None => {}
            Some(None) => {
                inbox::dismiss(&bundle.inbox, &object, &ada, now(), "wrong person")
                    .expect("dismissed");
            }
            Some(Some(performed)) => {
                inbox::submit(&bundle.inbox, &object, &ada, now(), |_| Ok(performed))
                    .expect("submitted");
            }
        }
    }
    entries
}

/// Two proposals filed in conversation `id`, the first given a check over nothing past the door
/// that would refuse it: the unreadable one's id, then the standing one's.
fn with_an_unreadable_proposal(bundle: &Bundle, id: &ConversationId) -> (ProposalId, ProposalId) {
    use prodrome_core::event::{Envelope, canonical_envelope, seal_hash};
    use suzatary_core::secretary::inbox_event::{InboxEvent, Said};
    use suzatary_core::secretary::responded::Responded;
    use suzatary_core::store::Prodrome as _;

    let opened = now();
    let reasons = || {
        Reasons::new(
            "the deed is due",
            vec!["briefing:2026-09-27".parse().expect("a reference")],
        )
        .expect("a why and a source")
    };
    let email = |subject: &str| {
        Proposal::Email(Draft {
            account: CAST.owner.email.to_owned(),
            to: vec![CAST.counsel.email.to_owned()],
            cc: Vec::new(),
            subject: subject.to_owned(),
            body: "Following up on the deed.".to_owned(),
            attachments: Vec::new(),
        })
    };
    let bad = bundle.file(id, 0, email("Your deed"), reasons(), None, opened);
    let good = bundle.file(id, 1, email("Your lease"), reasons(), None, opened);
    let object = |line: &ConversationEntry| match line {
        ConversationEntry::Proposed { object, .. } => object.clone(),
        _ => unreachable!("file answers a proposed line"),
    };
    let (bad_id, good_id) = (object(&bad), object(&good));
    bundle.seed(
        id,
        &[
            ConversationEntry::Opened {
                at: opened,
                title: suzatary_core::secretary::words::Words::kept("Two proposals".to_owned()),
                model: "vectors-fixture".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Topic,
            },
            bad,
            good,
        ],
    );

    // A check written over the first proposal's asking, past the door that would refuse it.
    let objects = bundle.inbox.objects().expect("reads");
    let genesis = objects
        .iter()
        .find(|(_, object)| matches!(object, Envelope::Genesis(_)))
        .map(|(name, _)| name.clone())
        .expect("a genesis");
    let asked = objects
        .iter()
        .find(|(_, object)| object.event().is_some_and(|event| event.proposal == bad_id))
        .map(|(name, _)| name.clone())
        .expect("the first asking");
    let check = Envelope::Change(
        prodrome_core::change::mk_change(
            genesis,
            vec![asked],
            InboxEvent {
                proposal: bad_id.clone(),
                at: later(opened, 1),
                actor: Actor::new("ada").expect("an actor"),
                said: Said::Responded(Responded::Checked(())),
            },
        )
        .expect("a change"),
    );
    bundle
        .inbox
        .accept(&[(seal_hash(&check), canonical_envelope(&check))])
        .expect("taken in");

    (bad_id, good_id)
}

/// Seed `id` with three proposal lines against `bundle`'s inbox: the first filed in another
/// replica's inbox, so this box holds its line and not its record; the second answered here (a
/// dismissal) with its asking never taken in; the third filed here and placed.
fn with_unplaced_lines(bundle: &Bundle, id: &ConversationId) {
    use prodrome_core::event::{Envelope, canonical_envelope, seal_hash};
    use suzatary_core::secretary::inbox_event::{InboxEvent, Said};
    use suzatary_core::secretary::responded::Responded;
    use suzatary_core::store::Prodrome as _;

    let elsewhere = Bundle::over(
        six_todo_chain(&scratch(&format!("{id}-elsewhere"))),
        bundle.at,
    );
    let reasons = || {
        Reasons::new(
            "the deed is due",
            vec!["briefing:2026-09-27".parse().expect("a reference")],
        )
        .expect("a why and a source")
    };
    let email = |subject: &str| {
        Proposal::Email(Draft {
            account: CAST.owner.email.to_owned(),
            to: vec![CAST.counsel.email.to_owned()],
            cc: Vec::new(),
            subject: subject.to_owned(),
            body: "Following up on the deed.".to_owned(),
            attachments: Vec::new(),
        })
    };
    let at = now();
    let unheld = elsewhere.file(id, 0, email("Your deed"), reasons(), None, at);
    let unasked = elsewhere.file(id, 1, email("Your keys"), reasons(), None, at);
    let held = bundle.file(id, 2, email("Your lease"), reasons(), None, at);
    let ConversationEntry::Proposed { object, .. } = &unasked else {
        unreachable!("file answers a proposed line")
    };
    // A dismissal of the second, taken in here without the asking it answers.
    let genesis = bundle
        .inbox
        .objects()
        .expect("reads")
        .into_iter()
        .find(|(_, object)| matches!(object, Envelope::Genesis(_)))
        .map(|(name, _)| name)
        .expect("a genesis");
    let dismissal = Envelope::Change(
        prodrome_core::change::mk_change(
            genesis,
            Vec::new(),
            InboxEvent {
                proposal: object.clone(),
                at: later(at, 1),
                actor: Actor::new(CAST.owner.actor).expect("an actor"),
                said: Said::Responded(Responded::Dismissed {
                    reason: "not this one".to_owned(),
                }),
            },
        )
        .expect("a change"),
    );
    bundle
        .inbox
        .accept(&[(seal_hash(&dismissal), canonical_envelope(&dismissal))])
        .expect("taken in");
    bundle.seed(
        id,
        &[
            ConversationEntry::Opened {
                at,
                title: suzatary_core::secretary::words::Words::kept("Three proposals".to_owned()),
                model: "vectors-fixture".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Topic,
            },
            unheld,
            unasked,
            held,
        ],
    );
}

/// A proposal line the inbox's reading cannot place is drawn, saying why, and never skipped:
/// its conversation renders with the line in `unplaced`, in order beside the placed one, which
/// keeps its card; `Stored::lines` holds each with its reason; the list counts them; `verify`
/// names each; and acting on one is refused by its reason.
#[test]
fn a_proposal_line_the_inbox_does_not_hold_is_reported_not_skipped() {
    use suzatary_core::secretary::store::{Line, Unheld};

    let bundle = Bundle::over(six_todo_chain(&scratch("unplaced")), now());
    let id = suzatary_core::secretary::store::DEFAULT.clone();
    with_unplaced_lines(&bundle, &id);

    let fold = inbox::fold(&bundle.inbox).expect("the inbox reads");
    let stored =
        suzatary_core::secretary::read(bundle.conversations.conversations(), &id).expect("reads");
    let lines: Vec<(usize, Option<Unheld>)> = stored
        .lines(&fold)
        .iter()
        .map(|line| match line {
            Line::Placed(standing) => (standing.asking.index, None),
            Line::Unplaced(unplaced) => (unplaced.asking.index, Some(unplaced.why)),
        })
        .collect();
    assert_eq!(
        lines,
        vec![
            (0, Some(Unheld::NotHeld)),
            (1, Some(Unheld::Unasked)),
            (2, None)
        ]
    );
    let refused = stored.standing(0, &fold).expect_err("nothing to act on");
    assert!(
        refused
            .message()
            .contains("the inbox holds no record of it"),
        "{}",
        refused.message()
    );
    let refused = stored.standing(1, &fold).expect_err("nothing to act on");
    assert!(
        refused.message().contains("answered and was never asked"),
        "{}",
        refused.message()
    );

    let deployment = deployment();
    let cards: Cards<'_> = Cards {
        deployment: &deployment,
        chains: &[],
        plugins: None,
        reach: &suzatary_core::connect::PLAIN,
        settables: &suzatary_core::settings::Settables::default(),
    };
    let payload = secretary_routes::newest(&server_of(&bundle), &cards, &id)
        .expect("the conversation renders");
    assert_eq!(
        payload
            .proposals
            .iter()
            .map(|row| (row.index, row.state))
            .collect::<Vec<_>>(),
        vec![(2, wire::ProposalState::Standing)]
    );
    assert_eq!(
        payload.unplaced,
        vec![
            wire::UnplacedProposal {
                index: 0,
                why: Unheld::NotHeld,
            },
            wire::UnplacedProposal {
                index: 1,
                why: Unheld::Unasked,
            },
        ]
    );
    let listed = readings::conversations(&listed(&bundle), &fold);
    let row = listed
        .conversations
        .iter()
        .find(|row| row.id == id.as_str())
        .expect("listed");
    assert_eq!(row.standing, 1);
    assert_eq!(
        row.unplaced, payload.unplaced,
        "the list carries the lines the page draws"
    );
    let found =
        suzatary_core::secretary::transcript::findings(&bundle.conversations.lines, &bundle.inbox);
    // The unasked line is the inbox's own finding, so the conversations name only the other.
    assert_eq!(
        found,
        vec![format!(
            "proposal:{id}/0 has no card: the inbox holds no record of it"
        )]
    );
    let unasked = suzatary_core::secretary::inbox::findings(&bundle.inbox);
    assert_eq!(unasked.len(), 1, "{unasked:?}");
    assert!(
        unasked[0].contains("is answered and was never asked"),
        "{unasked:?}"
    );
}

/// One unreadable proposal costs only itself, on every page that reads a conversation: a
/// conversation holding a check over nothing beside a proposal that stands renders, the
/// unreadable card saying why and offering nothing, the other stays tappable, and a device may
/// still ask to be trusted in the conversation that holds both.
#[test]
fn a_conversation_with_an_unreadable_proposal_renders_and_the_rest_stay_tappable() {
    let root = scratch("unreadable");
    let bundle = Bundle::over(six_todo_chain(&root), now());
    let id = suzatary_core::secretary::store::DEFAULT.clone();
    let (bad_id, good_id) = with_an_unreadable_proposal(&bundle, &id);
    let deployment = deployment();
    let cards: Cards<'_> = Cards {
        deployment: &deployment,
        chains: &[],
        plugins: None,
        reach: &suzatary_core::connect::PLAIN,
        settables: &suzatary_core::settings::Settables::default(),
    };
    let server = server_of(&bundle);
    let payload = secretary_routes::newest(&server, &cards, &id).expect("the conversation renders");
    let states: Vec<wire::ProposalState> = payload.proposals.iter().map(|row| row.state).collect();
    assert_eq!(
        states,
        vec![
            wire::ProposalState::Unreadable,
            wire::ProposalState::Standing
        ]
    );
    assert!(payload.proposals[0].card.answers.is_empty());
    assert!(
        payload.proposals[0].outcome.contains("checks a send"),
        "{}",
        payload.proposals[0].outcome
    );
    let fold = inbox::fold(&bundle.inbox).expect("the inbox reads");
    let listed = readings::inbox(&fold, &suzatary_core::chain_set::ChainSet::new(now()), None);
    let row = listed
        .proposals
        .iter()
        .find(|row| row.id == bad_id.as_str())
        .expect("the unreadable proposal is listed");
    assert!(
        matches!(&row.state, wire::InboxState::Unreadable { reason } if reason.contains("checks a send")),
        "it is listed as unreadable, saying why: {:?}",
        row.state
    );
    let set = suzatary_core::chain_set::ChainSet::new(now());
    assert!(
        readings::waiting(&fold, &set)
            .iter()
            .all(|row| row.id != bad_id.as_str()),
        "nothing waits on a proposal nothing can act on"
    );
    let ada = Actor::new("ada").expect("an actor");
    assert!(
        inbox::submit(&bundle.inbox, &bad_id, &ada, now(), |_| unreachable!(
            "never performed"
        ))
        .is_err()
    );
    assert!(matches!(
        inbox::submit(&bundle.inbox, &good_id, &ada, now(), |_| {
            Ok(Performed::Done(Done::Sent {
                recipients: vec![CAST.counsel.email.to_owned()],
                message_id: "<lease@box.example>".to_owned(),
            }))
        }),
        Ok(inbox::Response::Ran(_))
    ));
    let key = prodrome_core::sign::Secret::new(&"1".repeat(64))
        .expect("a seed")
        .public();
    secretary_routes::ask_trust(&server, &ada, &key).expect("a device may ask");
}

/// The proposal card at every stage and whether it is open, which the page's answer controls
/// follow.
#[test]
fn the_proposals_vector_is_every_stage_and_its_decision() {
    let root = scratch("stages");
    let bundle = Bundle::over(six_todo_chain(&root), now());
    let id = ConversationId::new("every-stage").expect("a slug");
    let script = every_stage(&bundle, &id);
    bundle.seed(&id, &script);
    let deployment = deployment();
    let cards: Cards<'_> = Cards {
        deployment: &deployment,
        chains: &[],
        plugins: None,
        reach: &suzatary_core::connect::PLAIN,
        settables: &suzatary_core::settings::Settables::default(),
    };
    let payload =
        secretary_routes::newest(&server_of(&bundle), &cards, &id).expect("every stage folds");
    write_or_check(&payload.proposals, "proposals.json");
    let stored = Stored::of(&id, &script).expect("folds");
    let answers: Vec<StageAnswer> = stored
        .lines(&inbox::fold(&bundle.inbox).expect("the inbox reads"))
        .standings()
        .map(|standing| StageAnswer {
            open: standing.object.reading.open(),
        })
        .collect();
    write_or_check(&answers, "proposals.answer.json");
}

/// Every object of one store, as a replica is handed them: its name and its canonical print.
#[derive(Serialize)]
struct Shipped {
    hash: String,
    text: String,
}

fn shipped<E: suzatary_core::store::Store>(store: &InMemory<E>) -> Vec<Shipped> {
    store
        .objects()
        .expect("an in-memory store reads")
        .into_iter()
        .map(|(hash, object)| Shipped {
            hash: hash.as_str().to_owned(),
            text: prodrome_core::event::canonical_envelope(&object),
        })
        .collect()
}

/// Every key the conversations hold, by name, as the pull spells it.
fn keys_of(
    conversations: &suzatary_core::secretary::conversations::InMemory,
) -> BTreeMap<String, String> {
    use suzatary_core::store::Gets as _;
    conversations
        .keys
        .names()
        .into_iter()
        .map(|name| {
            let key = conversations.keys.get(&name).expect("reads").expect("held");
            (
                name.as_str().to_owned(),
                suzatary_core::secretary::key_ring::spelled(&key),
            )
        })
        .collect()
}

/// The owner erases `id`'s first answer, turn 1.
fn erase_the_first_answer(bundle: &Bundle, id: &ConversationId) {
    let read = suzatary_core::secretary::transcript::read(bundle.conversations.conversations(), id)
        .expect("reads");
    let line = read.line_of(1).expect("the first answer");
    suzatary_core::secretary::transcript::erase(
        bundle.conversations.conversations(),
        id,
        read.lines()[line].line.keys().cloned().collect(),
        suzatary_core::secretary::erasure::Erasure::Asked {
            by: actor(CAST.owner.actor),
        },
        later(now(), 300),
    )
    .expect("erased");
}

/// What a replica holding `keys` forgets of the conversations: every key an erasure names, with
/// the answer keys of the blocks its words were rendered to, by the box's own sweep's reading.
fn forgotten(
    conversations: &suzatary_core::secretary::conversations::InMemory,
    keys: &BTreeMap<String, String>,
) -> BTreeMap<String, Vec<String>> {
    use suzatary_core::secretary::locked_text::KeyName;
    let ring = suzatary_core::secretary::key_ring::KeyRing::of(
        &keys
            .iter()
            .map(|(name, hex)| (KeyName::new(name.clone()).expect("a key name"), hex.clone()))
            .collect(),
    );
    let objects = conversations.lines.objects().expect("reads");
    let mut dead: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (line, slot, name) in suzatary_core::secretary::transcript::dead(
        objects
            .iter()
            .filter_map(|(hash, object)| object.event().map(|line| (hash, line))),
        &|name: &KeyName| ring.key(name),
    ) {
        let fed = ring.key(name).get().map_or_else(Vec::new, |key| {
            suzatary_core::secretary::transcript::fed(line, slot, key)
                .iter()
                .map(|block| {
                    suzatary_core::rendering::key(&suzatary_core::rendering::ANSWER, block.as_str())
                })
                .collect()
        });
        dead.entry(name.as_str().to_owned())
            .or_default()
            .extend(fed);
    }
    assert!(
        dead.values().any(|fed| !fed.is_empty()),
        "the vector forgets a rendered block"
    );
    dead
}

/// One window the tab asks for, as its query spells it, and the box's answer.
#[derive(Serialize)]
struct Asked {
    id: String,
    before: Option<String>,
    limit: Option<String>,
    about: Option<String>,
    memory: bool,
    answer: String,
}

/// A todo chain as a tab receives it: what prices the proposals that serve its todos.
#[derive(Serialize)]
struct ShippedChain {
    name: String,
    untrusted: Vec<String>,
    objects: Vec<Shipped>,
}

/// The bundle's chain, `life`, as the set the inbox prices through, and as a tab receives it.
fn life(bundle: &Bundle) -> (Read, ShippedChain) {
    let policy = chain_policy();
    let set = chains_of(bundle);
    let name = policy.name;
    let objects = linearised(&bundle.events)
        .expect("the chain reads")
        .into_iter()
        .map(|(hash, object)| Shipped {
            hash: hash.as_str().to_owned(),
            text: prodrome_core::event::canonical_envelope(&object),
        })
        .collect();
    let shipped = ShippedChain {
        name: name.as_str().to_owned(),
        untrusted: vec!["triage".to_owned()],
        objects,
    };
    (set, shipped)
}

/// One search, and the box's answer.
#[derive(Serialize)]
struct Searched {
    q: String,
    answer: String,
}

/// The secretary's two stores as a tab receives them, and what the box answers of them.
#[derive(Serialize)]
struct Replicas {
    at: String,
    chains: Vec<ShippedChain>,
    inbox: Vec<Shipped>,
    conversations: Vec<Shipped>,
    /// The conversations' skeleton, whole, as its route answers a replica holding nothing.
    skeleton: wire::SkeletonPayload,
    /// The key of every slot of the conversations' words, as the pull carries them.
    keys: BTreeMap<String, String>,
    /// The keys a tab held before one of the deed's turns was erased.
    stale: BTreeMap<String, String>,
    /// What a tab holding `stale` forgets: every key an erasure names, with the answer keys of the
    /// blocks its words were rendered to.
    erased: BTreeMap<String, Vec<String>>,
    memory: wire::MemoryPayload,
    list: String,
    proposals: String,
    windows: Vec<Asked>,
    searches: Vec<Searched>,
}

/// A second conversation: a remembered fact, an answer in two blocks, a proposal answered no, and
/// two remember cards, one standing and one whose dismissal deletes its words.
fn remembering(bundle: &Bundle, id: &ConversationId) -> Vec<ConversationEntry> {
    let opened = later(now(), 200);
    vec![
        ConversationEntry::Opened {
            at: opened,
            title: suzatary_core::secretary::words::Words::kept("Trains".to_owned()),
            model: "vectors-fixture".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        },
        ConversationEntry::Said {
            at: later(opened, 1),
            turn: Turn::Person {
                text: "remember that I take the early train".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
        ConversationEntry::Said {
            at: later(opened, 2),
            turn: Turn::Tool {
                verb: "remember".into(),
                answer: r#"{"remembered": 1, "kind": "owner"}"#.to_owned(),
                outcome: suzatary_core::secretary::outcome::Outcome::Remembered {
                    fact: suzatary_core::secretary::memory::FactId(1),
                },
                files: Vec::new(),
            },
            by: None,
        },
        ConversationEntry::Said {
            at: later(opened, 3),
            turn: Turn::Model {
                text: "= Noted\n\nThe early train, from now on.".to_owned(),
                calls: Vec::new(),
            },
            by: None,
        },
        bundle.file(
            id,
            0,
            Proposal::Email(Draft {
                account: CAST.owner.email.to_owned(),
                to: vec![CAST.counsel.email.to_owned()],
                cc: Vec::new(),
                subject: "Trains".to_owned(),
                body: "I take the early train now.".to_owned(),
                attachments: Vec::new(),
            }),
            Reasons::new(
                "the meeting moved",
                vec!["mail:a1b2c3d4e5f6".parse().expect("a reference")],
            )
            .expect("a why and a source"),
            None,
            later(opened, 4),
        ),
        bundle.card(
            id,
            1,
            &format!("{} keeps the books", CAST.accountant.name),
            later(opened, 5),
        ),
        bundle.card(id, 2, "Always copy the registry office", later(opened, 6)),
    ]
}

/// A digest history whose read left an object out: every sweep over it refuses.
struct LeftOut;

impl suzatary_core::store::Reads<suzatary_core::secretary::digest_event::DigestEvent> for LeftOut {
    fn objects(
        &self,
    ) -> Result<
        Vec<(
            Hash,
            prodrome_core::event::Envelope<suzatary_core::secretary::digest_event::DigestEvent>,
        )>,
        suzatary_core::store::StoreError,
    > {
        Ok(Vec::new())
    }
    fn excluded(
        &self,
    ) -> Result<std::collections::BTreeSet<Hash>, suzatary_core::store::StoreError> {
        Ok([Hash::new("0".repeat(64)).expect("a name")].into())
    }
    fn tips(&self) -> Result<Vec<Hash>, suzatary_core::store::StoreError> {
        Ok(Vec::new())
    }
    fn verify(&self) -> Vec<String> {
        Vec::new()
    }
}

impl suzatary_core::store::Prodrome<suzatary_core::secretary::digest_event::DigestEvent>
    for LeftOut
{
    fn append(
        &self,
        _: suzatary_core::secretary::digest_event::DigestEvent,
    ) -> Result<Hash, suzatary_core::store::StoreError> {
        Err(suzatary_core::store::StoreError::not_appended(
            "left out",
            "never written",
        ))
    }
    fn accept(&self, _: &[(Hash, String)]) -> Result<Vec<Hash>, suzatary_core::store::StoreError> {
        Ok(Vec::new())
    }
    fn locked(
        &self,
    ) -> Result<Box<dyn suzatary_core::store::WriteLock + '_>, suzatary_core::store::StoreError>
    {
        Err(suzatary_core::store::StoreError::not_appended(
            "left out",
            "never written",
        ))
    }
}

/// A sweep after a write never fails the write: with the digest's read leaving an object out,
/// a dismissal still stands, and its line is in the conversation for the secretary to read.
#[test]
fn a_dismissal_stands_with_its_line_whatever_its_sweep() {
    let root = scratch("dismiss-left-out");
    let bundle = Bundle::over(six_todo_chain(&root), later(now(), 400));
    let trains = ConversationId::new("trains").expect("a slug");
    bundle.seed(&trains, &remembering(&bundle, &trains));
    let memories = Memories {
        digests: suzatary_core::secretary::digests::Digests {
            history: &LeftOut,
            ..bundle.memories().digests
        },
        ..bundle.memories()
    };
    assert!(memories.sweep().is_err(), "this sweep refuses");
    let stored = suzatary_core::secretary::dismiss(
        bundle.conversations.conversations(),
        memories,
        &bundle,
        &actor(CAST.owner.actor),
        &ProposalRef {
            conversation: trains.clone(),
            index: 2,
        },
        "not from a newsletter",
    )
    .expect("the dismissal stands");
    let last = stored.turns.last().expect("a turn");
    assert!(
        matches!(&last.turn, Turn::Act { text, .. } if text.contains("not from a newsletter")),
        "{last:?}"
    );
}

/// The tab's replicas of the inbox and the conversations read as the box reads the same objects:
/// `web/test/replicas.test.ts` hands these objects to the module in every order and compares its
/// answers with these, byte for byte.
#[test]
fn the_replicas_vector_is_what_the_box_answers() {
    let root = scratch("replicas");
    let bundle = Bundle::over(six_todo_chain(&root), later(now(), 400));
    let deed = ConversationId::new("about-the-deed").expect("a slug");
    bundle.seed(&deed, &scripted_conversation(&bundle, &deed));
    let trains = ConversationId::new("trains").expect("a slug");
    bundle.seed(&trains, &remembering(&bundle, &trains));
    // A conversation whose inbox lags it, so the module must draw lines it cannot place.
    let lagging = ConversationId::new("lagging").expect("a slug");
    with_unplaced_lines(&bundle, &lagging);
    for (index, reason) in [(0, "not yet"), (2, "not from a newsletter")] {
        suzatary_core::secretary::dismiss(
            bundle.conversations.conversations(),
            bundle.memories(),
            &bundle,
            &actor(CAST.owner.actor),
            &ProposalRef {
                conversation: trains.clone(),
                index,
            },
            reason,
        )
        .expect("dismissed");
    }
    let stale = keys_of(&bundle.conversations);
    erase_the_first_answer(&bundle, &deed);

    let printed = |value: &dyn erased::Printed| value.printed();
    let mut windows = windows_of(&bundle, &deed, &trains);
    let memory = memory_of(&bundle, &trains);
    let stored = suzatary_core::secretary::read(bundle.conversations.conversations(), &trains)
        .expect("reads");
    let remembered = readings::conversation(
        &stored,
        &inbox::fold(&bundle.inbox).expect("the inbox reads"),
        &readings::Window::newest(),
        &readings::Dress {
            memory: Some(&memory),
            ..readings::Dress::BARE
        },
    );
    windows.push(Asked {
        id: trains.to_string(),
        before: None,
        limit: None,
        about: None,
        memory: true,
        answer: printed(&remembered),
    });
    let lags = readings::conversation(
        &suzatary_core::secretary::read(bundle.conversations.conversations(), &lagging)
            .expect("reads"),
        &inbox::fold(&bundle.inbox).expect("the inbox reads"),
        &readings::Window::newest(),
        &readings::Dress::BARE,
    );
    assert_eq!(
        lags.unplaced.len(),
        2,
        "the vector holds lines the module cannot place"
    );
    windows.push(Asked {
        id: lagging.to_string(),
        before: None,
        limit: None,
        about: None,
        memory: false,
        answer: printed(&lags),
    });
    let searches = ["deed", "EARLY", "nothing like it"]
        .into_iter()
        .map(|q| Searched {
            q: q.to_owned(),
            answer: printed(&readings::search(&listed(&bundle), q).expect("a search")),
        })
        .collect();
    let (set, chain) = life(&bundle);
    let replicas = Replicas {
        at: fpl::iso(fpl::instant_of(bundle.at)),
        chains: vec![chain],
        inbox: shipped(&bundle.inbox),
        conversations: shipped(&bundle.conversations.lines),
        skeleton: skeleton_of(&bundle),
        keys: keys_of(&bundle.conversations),
        erased: forgotten(&bundle.conversations, &stale),
        stale,
        list: printed(&readings::conversations(
            &listed(&bundle),
            &inbox::fold(&bundle.inbox).expect("the inbox reads"),
        )),
        proposals: printed(&readings::inbox(
            &inbox::fold(&bundle.inbox).expect("the inbox reads"),
            &set.set(bundle.at),
            Some(&memory),
        )),
        memory,
        windows,
        searches,
    };
    write_or_check(&replicas, "replicas.json");
}

/// The lines' whole skeleton, as its route answers a replica holding nothing.
fn skeleton_of(bundle: &Bundle) -> wire::SkeletonPayload {
    suzatary_core::routes::lines_skeleton(
        bundle.conversations.conversations(),
        &prodrome_core::store::frontier::Since::Nothing,
        suzatary_core::routes::Books {
            epochs: &suzatary_core::epoch::Epochs::remembered(),
            frontiers: &prodrome_core::store::frontier::Frontiers::remembered(
                prodrome_core::store::frontier::KEPT,
            ),
        },
        bundle.at,
        suzatary_core::routes::Cut::Whole,
    )
    .expect("the skeleton reads")
}

/// The memory as its route answers it, for the receipt's words, with the words its text store
/// keeps for the remember cards as the route reads them.
fn memory_of(bundle: &Bundle, trains: &ConversationId) -> wire::MemoryPayload {
    wire::MemoryPayload {
        facts: vec![wire::FactRow {
            id: suzatary_core::secretary::memory::FactId(1),
            at: stamp(later(now(), 202)),
            fact: "Takes the early train".to_owned(),
            kind: suzatary_core::secretary::memory::Kind::Owner,
            said: wire::FactSaid::Said {
                on: wire::SaidOn::Turn {
                    conversation: trains.to_string(),
                    turn: 0,
                },
            },
        }],
        cap: 200,
        forgotten: Vec::new(),
        merged: Vec::new(),
        cards: secretary_routes::memory(bundle.memories())
            .expect("the memory reads")
            .cards,
    }
}

/// The windows a tab asks of both conversations, and the box's reading of each.
fn windows_of(bundle: &Bundle, deed: &ConversationId, trains: &ConversationId) -> Vec<Asked> {
    let mut windows = Vec::new();
    for (id, before, limit, about) in [
        (deed, None, None, None),
        (deed, Some(3), Some(2), None),
        (
            deed,
            None,
            None,
            Some(format!("todo:life/{}", CAST.todos.agents.id)),
        ),
        (trains, None, None, None),
        (trains, None, Some(1), None),
    ] {
        let window = readings::conversation(
            &suzatary_core::secretary::read(bundle.conversations.conversations(), id)
                .expect("reads"),
            &inbox::fold(&bundle.inbox).expect("the inbox reads"),
            &readings::Window {
                before,
                limit: limit.unwrap_or(readings::DEFAULT_TURN_WINDOW),
                about: about
                    .as_deref()
                    .map(|about| about.parse().expect("a reference")),
            },
            &readings::Dress {
                answers: bundle,
                ..readings::Dress::BARE
            },
        );
        windows.push(Asked {
            id: id.to_string(),
            before: before.map(|n| n.to_string()),
            limit: limit.map(|n| n.to_string()),
            about,
            memory: false,
            answer: erased::Printed::printed(&window),
        });
    }
    windows
}

/// Every conversation, newest first, as the box reads its own store.
fn listed(bundle: &Bundle) -> Vec<Stored> {
    suzatary_core::secretary::list(bundle.conversations.conversations())
        .expect("the conversations read")
}

/// One sample of an answer: its JSON, and where it holds a map, whose keys are data.
#[derive(Serialize)]
struct Sample {
    value: serde_json::Value,
    maps: Vec<Vec<serde_json::Value>>,
}

/// Every answer a route gives, by its TypeScript name, as the samples below serialise.
#[derive(Default)]
struct Samples(BTreeMap<String, Vec<Sample>>);

impl Samples {
    fn add<T: Serialize + ts_rs::TS>(&mut self, values: impl IntoIterator<Item = T>) {
        for value in values {
            let value = serde_json::to_value(&value).expect("an answer serialises");
            let maps = wire::maps(&T::name(), &value).expect("an answer has a shape");
            self.0
                .entry(T::name())
                .or_default()
                .push(Sample { value, maps });
        }
    }
}

fn row(n: u8) -> wire::AttachmentRow {
    wire::AttachmentRow {
        id: format!("{n}").repeat(64),
        media_type: "application/pdf".to_owned(),
        name: format!("{}.pdf", CAST.todos.agents.id),
        private: String::new(),
    }
}

fn mail_row() -> suzatary_core::mail::MailRow {
    suzatary_core::mail::MailRow {
        hash: "a".repeat(16),
        id: "m1@example.test".to_owned(),
        sender: CAST.counsel.email.to_owned(),
        subject: "The deed".to_owned(),
        date: Some(now()),
        container: "inbox".to_owned(),
        account: CAST.owner.email.to_owned(),
        disposition: suzatary_core::mail::Disposed::Todo {
            todo: CAST.todos.agents.id.to_owned(),
        },
    }
}

/// `web/test/payload.test.ts`'s laws run over every answer the box gives: each type a route
/// answers has samples here, every arm of its sums among them, and the samples say where they
/// hold a map, so a law about fields leaves a map's keys alone.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one sample per answer type, each arm of each sum written out"
)]
fn the_answers_vector_holds_every_answer() {
    use suzatary_core::secretary::store::ConversationState;
    use suzatary_core::usage::{self, Allowance, Window};

    let root = scratch("answers");
    let bundle = Bundle::over(six_todo_chain(&root), later(now(), 400));
    let deed = ConversationId::new("about-the-deed").expect("a slug");
    bundle.seed(&deed, &scripted_conversation(&bundle, &deed));
    let stages = ConversationId::new("every-stage").expect("a slug");
    bundle.seed(&stages, &every_stage(&bundle, &stages));
    let lines = ConversationId::new("every-line").expect("a slug");
    bundle.seed(&lines, &every_line());
    resealed(&bundle, &lines);
    let deployment = deployment();
    let cards: Cards<'_> = Cards {
        deployment: &deployment,
        chains: &[],
        plugins: None,
        reach: &suzatary_core::connect::PLAIN,
        settables: &suzatary_core::settings::Settables::default(),
    };
    let policy = chain_policy();
    let at = now();
    let mut samples = Samples::default();

    let chain = routes::chain(&bundle, &policy).expect("the chain");
    let chains = chains_of(&bundle);
    let set = chains.set(at);
    let mut fold =
        routes::fold(&bundle, &policy.name, &policy.untrusted, &set, None).expect("the fold");
    let todo = routes::todo(
        &bundle,
        &policy,
        suzatary_core::zone::Zone::UTC,
        &set,
        CAST.todos.agents.id,
        None,
    )
    .expect("a todo");
    let heads = chain.heads.clone();
    samples.add([wire::FieldChange {
        field: "body".to_owned(),
        before: String::new(),
        after: "'Reply'".to_owned(),
    }]);
    samples.add([todo]);
    samples.add([chain.clone()]);
    samples.add([fold.clone()]);
    if let Some(entry) = fold.entries.first_mut() {
        entry.conflicts = [("state".to_owned(), heads.clone())].into_iter().collect();
    }
    fold.failed = vec![failed_chain()];
    samples.add([fold]);
    samples.add(
        [
            wire::TreeBy::Category,
            wire::TreeBy::Urgency,
            wire::TreeBy::Flat,
        ]
        .map(|by| routes::tree(&policy, &set, by).expect("a tree")),
    );
    samples.add([routes::tip(&bundle).expect("the tip")]);
    samples.add([wire::TipFrame {
        heads: heads.clone(),
    }]);
    let (epochs, frontiers) = (
        suzatary_core::epoch::Epochs::remembered(),
        prodrome_core::store::frontier::Frontiers::remembered(prodrome_core::store::frontier::KEPT),
    );
    let books = routes::Books {
        epochs: &epochs,
        frontiers: &frontiers,
    };
    let nothing = prodrome_core::store::frontier::Since::Nothing;
    samples
        .add([routes::objects_since(&bundle, &policy, &nothing, books, at).expect("the objects")]);
    // The lines' pull: a conversation's objects and the key of every slot they name.
    let lines = suzatary_core::secretary::conversations::InMemory::default();
    suzatary_core::secretary::transcript::start(
        lines.conversations(),
        &ConversationId::new("pulled").expect("a slug"),
        &ConversationEntry::Opened {
            at,
            title: suzatary_core::secretary::words::Words::kept("Pulled".to_owned()),
            model: "vectors-fixture".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        },
    )
    .expect("opens");
    samples.add([routes::lines_since(
        lines.conversations(),
        &nothing,
        books,
        at,
        routes::Cut::PIECE,
    )
    .expect("the lines")]);
    // Its skeleton: the genesis, a structure, and the opening line.
    let skeleton = routes::lines_skeleton(
        lines.conversations(),
        &nothing,
        books,
        at,
        routes::Cut::PIECE,
    )
    .expect("the skeleton");
    let mut names: Vec<&str> = skeleton
        .nodes
        .iter()
        .map(|node| node.hash.as_str())
        .collect();
    names.sort_unstable();
    let hashes =
        suzatary_core::query::Hashes::parse(Some(&names.join(","))).expect("the lines' names");
    samples.add([routes::content(&lines.lines, &hashes).expect("the lines' content")]);
    samples.add([skeleton]);
    samples.add([wire::WaitingPayload { count: 2 }]);
    samples.add([wire::ReceiptPayload {
        heads: heads.clone(),
        adopted: heads.clone(),
        already: Vec::new(),
        since: wire::SinceRead::Unknown,
        frontier: wire::Frontier {
            digest: "f".repeat(64),
            count: heads.len(),
        },
        refused: vec![
            wire::RefusedObject {
                hash: String::new(),
                reason: "not an object".to_owned(),
                kind: wire::RefusalKind::Final,
            },
            wire::RefusedObject {
                hash: heads.first().cloned().unwrap_or_default(),
                reason: "its signatures do not prove it: unsigned".to_owned(),
                kind: wire::RefusalKind::Trust,
            },
        ],
    }]);
    samples.add([wire::MePayload {
        login: CAST.owner.email.to_owned(),
        actor: Some(CAST.owner.actor.to_owned()),
        chains: vec!["life".to_owned()],
        categories: vec!["work".to_owned()],
        zone: "America/New_York".to_owned(),
    }]);
    let held = |todo: &str, title: &str, state, value: Option<f64>| {
        Some(wire::HeldTodo {
            row: wire::Neighbour {
                todo: todo.to_owned(),
                state,
                value: Some(wire::Fulfillment::of_value(value)),
                unconfirmed: false,
            },
            title: title.to_owned(),
        })
    };
    // A fixed version, so a release does not move the vector; the route's own is the build's.
    // A release file names ids alone, so a todo the box's chain does not hold has no title.
    samples.add([wire::AboutPayload {
        version: "1.15.0".to_owned(),
        build: "dev".to_owned(),
        completes: vec![
            wire::Shipped {
                todo: "calendarlinks".to_owned(),
                held: held(
                    "calendarlinks",
                    "calendar links",
                    wire::EntryState::Completed,
                    Some(1.0),
                ),
            },
            wire::Shipped {
                todo: "elsewhere".to_owned(),
                held: None,
            },
        ],
    }]);
    let item = |todo: &str, title: &str, state, value, lot, named| wire::ReleaseItem {
        todo: todo.to_owned(),
        held: held(todo, title, state, value),
        lot,
        named,
    };
    samples.add([wire::ReleasesPayload {
        chain: "suzatary".to_owned(),
        reads: "1.0".to_owned(),
        releases: vec![
            wire::ReleaseRow {
                todo: "suzatarytwo".to_owned(),
                held: held(
                    "suzatarytwo",
                    "Suzatary 2.0",
                    wire::EntryState::Open,
                    Some(0.42),
                ),
                version: "2.0".to_owned(),
                lot: suzatary_core::release::Lot::Open,
                place: suzatary_core::release::Place::Next,
                items: vec![
                    item(
                        "portsuites",
                        "ports as suites",
                        wire::EntryState::Open,
                        Some(0.55),
                        suzatary_core::release::Lot::Open,
                        suzatary_core::release::Named::Current,
                    ),
                    item(
                        "calendarlinks",
                        "calendar links",
                        wire::EntryState::Completed,
                        Some(1.0),
                        suzatary_core::release::Lot::Completed,
                        suzatary_core::release::Named::Current,
                    ),
                    item(
                        "wireonce",
                        "the wire once",
                        wire::EntryState::Open,
                        Some(0.6),
                        suzatary_core::release::Lot::Completed,
                        suzatary_core::release::Named::Current,
                    ),
                    item(
                        "instancesplit",
                        "split the instance",
                        wire::EntryState::Cancelled,
                        Some(1.0),
                        suzatary_core::release::Lot::Cancelled,
                        suzatary_core::release::Named::Current,
                    ),
                    item(
                        "onboarding",
                        "onboarding",
                        wire::EntryState::Open,
                        Some(0.5),
                        suzatary_core::release::Lot::Open,
                        suzatary_core::release::Named::Dropped,
                    ),
                ],
            },
            wire::ReleaseRow {
                todo: "suzatarytwoone".to_owned(),
                held: None,
                version: "2.1".to_owned(),
                lot: suzatary_core::release::Lot::Open,
                place: suzatary_core::release::Place::Later,
                items: Vec::new(),
            },
        ],
    }]);
    samples.add([wire::AuthoredPayload {
        todo: CAST.todos.agents.id.to_owned(),
        event: "Created(...)".to_owned(),
        revision: None,
    }]);
    samples.add([wire::DevicePayload {
        ok: true,
        devices: 1,
    }]);
    samples.add([
        wire::PortalPayload {
            push: wire::PortalPush::None,
        },
        wire::PortalPayload {
            push: wire::PortalPush::Fcm {
                sender: "100000000000".to_owned(),
            },
        },
    ]);
    samples.add([wire::ErrorPayload {
        error: "refused".to_owned(),
    }]);

    let todo = |pending_todo| wire::ProposedTodo {
        todo: CAST.todos.agents.id.to_owned(),
        body: "Reply".to_owned(),
        at: stamp(at),
        actor: "triage".to_owned(),
        pending_todo,
    };
    let object = wire::ProposedObject {
        position: 1,
        act: wire::ObjectAct::Added {
            todo: todo(false),
            changes: vec![wire::ProposedChange {
                field: "body".to_owned(),
                before: String::new(),
                after: "Reply".to_owned(),
            }],
        },
    };
    // Every other act an object can be, so the tab decodes each.
    let others = [
        wire::ObjectAct::Edited {
            todo: todo(false),
            changes: Vec::new(),
        },
        wire::ObjectAct::Repriced {
            todo: todo(false),
            changes: Vec::new(),
        },
        wire::ObjectAct::Created {
            todo: todo(true),
            note: "registered".to_owned(),
        },
        wire::ObjectAct::Completed {
            todo: todo(true),
            note: "done".to_owned(),
        },
        wire::ObjectAct::Cancelled {
            todo: todo(false),
            note: String::new(),
        },
        wire::ObjectAct::Reopened {
            todo: todo(false),
            note: String::new(),
        },
        wire::ObjectAct::Tended {
            todo: todo(false),
            note: String::new(),
        },
        wire::ObjectAct::Respecified {
            todo: todo(false),
            note: String::new(),
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(n, act)| wire::ProposedObject {
        position: n + 2,
        act,
    });
    let rows: Vec<wire::ProposedObject> = std::iter::once(object).chain(others).collect();
    // A triage card's changeset, every row kind drawn, so the tab decodes each.
    samples.add([wire::DrawnPayload {
        proposals: vec![wire::ProposalRow {
            index: 0,
            act: wire::ProposedAct::Changes {
                chain: "life".to_owned(),
                changes: rows.clone(),
                problem: String::new(),
            },
            card: wire::Card {
                title: "Change the chain".to_owned(),
                by: wire::Asker::Triage {},
                sentence: String::new(),
                fields: Vec::new(),
                body: String::new(),
                rows,
                files: Vec::new(),
                problem: String::new(),
                answers: Vec::new(),
                uncertain: String::new(),
                readonly: String::new(),
                link: None,
            },
            why: "a message asks for it".to_owned(),
            sources: vec!["briefing:2026-09-02".parse().expect("a reference")],
            at: stamp(at),
            state: wire::ProposalState::Standing,
            outcome: String::new(),
            tries: 0,
            turn: 0,
            coding: None,
            announced: None,
            revises: None,
        }],
    }]);

    let inbox_fold = inbox::fold(&bundle.inbox).expect("the inbox reads");
    let listed = listed(&bundle);
    let mut proposals = readings::inbox(&inbox_fold, &chains.set(bundle.at), None);
    samples.add([proposals.clone()]);
    proposals.failed = vec![failed_chain()];
    samples.add([proposals]);
    samples.add([readings::conversations(&listed, &inbox_fold)]);
    samples.add([readings::search(&listed, "deed").expect("a search")]);
    let objects = bundle
        .conversations
        .lines
        .objects()
        .expect("the lines read");
    let opened = suzatary_core::secretary::transcript::opened(
        &objects,
        &bundle.conversations.conversations().reads().slots(),
    );
    let mut skeleton = suzatary_core::secretary::outlines::Outlines::default();
    skeleton.hold(
        objects
            .iter()
            .map(|(name, object)| {
                suzatary_core::secretary::outlines::Parsed::of(
                    suzatary_core::routes::skeleton_node(name, object),
                )
                .expect("a node")
            })
            .collect(),
        &[],
    );
    let stream = suzatary_core::secretary::stream::stream(
        skeleton.outlines(),
        suzatary_core::secretary::stream::every(&opened),
        &suzatary_core::secretary::topics::reading::Reading::default(),
        &std::collections::BTreeSet::new(),
        1,
        &readings::Dress::BARE,
    )
    .expect("the stream reads");
    // The bundle holds no round of the box's own, so one is made of its first round, for the tab
    // to decode the other way a round is asked.
    let mut asked_by_the_box = stream.clone();
    asked_by_the_box
        .rounds
        .first_mut()
        .expect("the bundle's stream holds a round")
        .asked = wire::RoundAsked::Box {
        title: "Sweep of later mail".to_owned(),
    };
    samples.add([stream, asked_by_the_box]);
    samples.add([wire::TopicsPayload {
        topics: vec![
            wire::TopicInUse {
                topic: "garden".to_owned(),
                rounds: 2,
                disputed: Vec::new(),
            },
            wire::TopicInUse {
                topic: "trip".to_owned(),
                rounds: 1,
                disputed: vec!["journey".to_owned()],
            },
        ],
    }]);
    let server = server_of(&bundle);
    let conversations = [&deed, &stages]
        .map(|id| secretary_routes::newest(&server, &cards, id).expect("a conversation"));
    // A conversation's proposals drawn in one read, as the route answers them.
    let indices: Vec<usize> = conversations[1].proposals.iter().map(|p| p.index).collect();
    samples.add([secretary_routes::drawn(&server, &cards, &stages, &indices).expect("drawn")]);
    // A message attaching a file from every place one comes from, the repository's with its hash.
    let mut attaching = conversations[1].clone();
    let files: Vec<wire::CardFile> = [
        wire::FileFrom::Mail {
            message: "a".repeat(16),
        },
        wire::FileFrom::Drive {
            path: "Documents/lease.pdf".to_owned(),
        },
        wire::FileFrom::Upload { id: "1".repeat(64) },
        wire::FileFrom::Kept {},
        wire::FileFrom::Repo {
            repo: "bmabsout/resume_typst".to_owned(),
            path: "cv.typ".to_owned(),
            at: "a".repeat(40),
        },
    ]
    .into_iter()
    .map(|from| wire::CardFile {
        name: "lease.pdf".to_owned(),
        media: "application/pdf".to_owned(),
        size: "12 KB".to_owned(),
        sha256: "e".repeat(64),
        from,
    })
    .collect();
    if let Some(proposal) = attaching.proposals.first_mut() {
        proposal.act = wire::ProposedAct::Email {
            account: CAST.owner.email.to_owned(),
            to: vec![CAST.counsel.email.to_owned()],
            cc: Vec::new(),
            subject: "The deed".to_owned(),
            body: "Attached.".to_owned(),
            attachments: files.clone(),
            held: wire::HeldIn::Read { values: Vec::new() },
        };
        proposal.card.files = files;
    }
    samples.add(
        conversations
            .into_iter()
            .chain([attaching])
            .map(|conversation| wire::SayPayload { conversation }),
    );
    samples.add([wire::MemoryPayload {
        facts: vec![
            wire::FactRow {
                id: suzatary_core::secretary::memory::FactId(1),
                at: stamp(at),
                fact: "Takes the early train".to_owned(),
                kind: suzatary_core::secretary::memory::Kind::Person,
                said: wire::FactSaid::Said {
                    on: wire::SaidOn::Turn {
                        conversation: deed.to_string(),
                        turn: 0,
                    },
                },
            },
            wire::FactRow {
                id: suzatary_core::secretary::memory::FactId(2),
                at: stamp(at),
                fact: format!("{} keeps the books", CAST.counsel.name),
                kind: suzatary_core::secretary::memory::Kind::Person,
                said: wire::FactSaid::Said {
                    on: wire::SaidOn::Tapped {
                        conversation: deed.to_string(),
                        proposal: 1,
                    },
                },
            },
            wire::FactRow {
                id: suzatary_core::secretary::memory::FactId(6),
                at: stamp(at),
                fact: "Takes the early train on weekdays".to_owned(),
                kind: suzatary_core::secretary::memory::Kind::Owner,
                said: wire::FactSaid::Merged {
                    from: vec![
                        suzatary_core::secretary::memory::FactId(4),
                        suzatary_core::secretary::memory::FactId(5),
                    ],
                    origins: vec![
                        wire::SaidOn::Turn {
                            conversation: deed.to_string(),
                            turn: 2,
                        },
                        wire::SaidOn::Tapped {
                            conversation: deed.to_string(),
                            proposal: 3,
                        },
                    ],
                    round: wire::SaidOn::Turn {
                        conversation: deed.to_string(),
                        turn: 4,
                    },
                },
            },
        ],
        cap: 200,
        forgotten: vec![suzatary_core::secretary::memory::FactId(3)],
        merged: vec![
            wire::MergedFact {
                fact: suzatary_core::secretary::memory::FactId(4),
                into: suzatary_core::secretary::memory::FactId(6),
            },
            wire::MergedFact {
                fact: suzatary_core::secretary::memory::FactId(5),
                into: suzatary_core::secretary::memory::FactId(6),
            },
        ],
        cards: vec![wire::CardWords {
            text: "c".repeat(64),
            fact: "Prefers mornings".to_owned(),
        }],
    }]);
    samples.add([
        wire::Frame::State {
            state: ConversationState::Thinking,
        },
        wire::Frame::Delta {
            text: "Hel".to_owned(),
        },
        wire::Frame::Turn,
        wire::Frame::Working {
            verb: "context".to_owned(),
        },
        wire::Frame::Block {
            n: 0,
            sha: "d".repeat(64),
            source: "= Plan".to_owned(),
        },
    ]);
    samples.add([wire::BlockRender {
        html: "<h2>Plan</h2>".to_owned(),
        key: suzatary_core::rendering::key(&suzatary_core::rendering::ANSWER, "= Plan"),
    }]);
    samples.add([row(1)]);
    samples.add([
        wire::RevealPayload::Shown {
            value: "000-12-3456".to_owned(),
        },
        wire::RevealPayload::Gone,
        wire::RevealPayload::NotKept,
    ]);
    samples.add([
        wire::Uploaded::Kept { file: row(2) },
        wire::Uploaded::Sensitive {
            why: "it names a tax number".to_owned(),
            folder: "Private".to_owned(),
        },
    ]);

    let block = wire::BlockRow {
        source: "= Plan".to_owned(),
        plain: "Plan".to_owned(),
        kind: suzatary_core::secretary::answer::Kind::Heading,
        html: Some("<h2>Plan</h2>".to_owned()),
    };
    samples.add([wire::BriefingsPayload {
        briefings: vec![wire::BriefingRow {
            date: "2026-09-02".to_owned(),
            title: "Tuesday".to_owned(),
            words: 120,
        }],
        briefs: vec![wire::BriefRow {
            date: "2026-09-02".to_owned(),
            conversation: deed.to_string(),
            turn: 1,
            title: "Plan".to_owned(),
            words: 40,
            text: "= Plan".to_owned(),
        }],
    }]);
    samples.add([
        wire::BriefingPayload {
            date: "2026-09-02".to_owned(),
            day: wire::BriefingDay::Brief {
                conversation: deed.to_string(),
                turn: 1,
                text: "= Plan".to_owned(),
                blocks: vec![block],
            },
        },
        wire::BriefingPayload {
            date: "2026-09-01".to_owned(),
            day: wire::BriefingDay::Saved {
                source: "= Monday".to_owned(),
                rich: None,
            },
        },
    ]);
    samples.add([wire::CalendarPayload {
        from: "2026-09-02".to_owned(),
        days: 7,
        text: "Nothing booked".to_owned(),
        rich: None,
    }]);
    samples.add([wire::InboxPayload {
        accounts: vec![CAST.owner.email.to_owned()],
        messages: [
            suzatary_core::mail::Disposed::Undecided,
            suzatary_core::mail::Disposed::Noise,
            suzatary_core::mail::Disposed::Skip,
        ]
        .into_iter()
        .map(|disposition| suzatary_core::mail::MailRow {
            disposition,
            ..mail_row()
        })
        .chain([mail_row()])
        .collect(),
    }]);
    samples.add([wire::MailSearchPayload {
        q: "from:counsel".to_owned(),
        accounts: Vec::new(),
        messages: vec![mail_row()],
    }]);
    samples.add([wire::WatchesPayload {
        watches: [wire::WatchOn::Sender, wire::WatchOn::Subject]
            .into_iter()
            .map(|on| wire::MailWatch {
                id: "0123456789abcdef".to_owned(),
                on,
                text: "counsel".to_owned(),
                set: "2026-09-02T12:00:00".to_owned(),
                until: "2026-09-16T12:00:00".to_owned(),
            })
            .collect(),
    }]);
    samples.add([wire::ThreadPayload {
        thread: "000000000000c3fb".to_owned(),
        messages: vec![wire::MailPage {
            message: suzatary_core::mail::MailMessage {
                row: mail_row(),
                thread: "000000000000c3fb".to_owned(),
                headers: vec![suzatary_core::mail::MailHeader {
                    name: "From".to_owned(),
                    value: CAST.counsel.email.to_owned(),
                }],
                body: "The signature is due.".to_owned(),
                truncated: false,
                links: vec![suzatary_core::mail::MailLink {
                    url: "https://example.com/deed".to_owned(),
                    label: "Upload".to_owned(),
                }],
            },
            todos: Vec::new(),
        }],
    }]);

    let utc = fpl::instant_of(at).and_utc();
    let reported = usage::Reported {
        utilization: 0.4,
        resets_at: Some(utc),
        as_of: utc,
    };
    samples.add([
        usage::report(
            utc,
            &[],
            &[],
            &[
                (Window::FiveHour, Allowance::Reported(reported.clone())),
                (
                    Window::SevenDay,
                    Allowance::RateLimited {
                        since: utc,
                        retry_after_seconds: 60,
                        last_reported: Some(reported),
                    },
                ),
            ],
            None,
            usage::Scope::default(),
        ),
        usage::report(
            utc,
            &[],
            &[],
            &[(
                Window::FiveHour,
                Allowance::Refused {
                    at: utc,
                    status: 403,
                    words: "no profile scope".to_owned(),
                },
            )],
            None,
            usage::Scope {
                counted: vec!["this box".to_owned()],
                not_counted: vec!["claude.ai".to_owned()],
            },
        ),
    ]);

    {
        use suzatary_core::connect::{Group, Standing};
        let line =
            |key: &str, label: &str, standing: Standing, cards: &[&str]| wire::OverviewLine {
                key: key.to_owned(),
                label: label.to_owned(),
                detail: Some(format!("{label}, in a sentence.")),
                standing,
                cards: cards.iter().map(|card| (*card).to_owned()).collect(),
            };
        let lines = |group: Group| match group {
            Group::Storage => vec![line("repository", "Data repository", Standing::Kept, &[])],
            Group::Mail => vec![line(
                "account:ada@example.test",
                CAST.owner.email,
                Standing::Failing {
                    since: stamp(at),
                    class: suzatary_core::connect::Failure::NeedsLogin,
                    words: "token expired".to_owned(),
                    at: stamp(at),
                },
                &["proposal:setup/2"],
            )],
            Group::Calendar => vec![
                line(
                    "credential:ics/home",
                    "Home",
                    Standing::Synced { at: stamp(at) },
                    &[],
                ),
                line(
                    "credential:ics/work",
                    "Work",
                    Standing::Missing {
                        what: vec!["your Work calendar's secret link".to_owned()],
                    },
                    &["proposal:setup/1"],
                ),
            ],
            Group::Models => vec![line(
                "job:Chat, reviews and titles",
                "Chat, reviews and titles",
                Standing::Answered { at: stamp(at) },
                &[],
            )],
            Group::Notifications => vec![
                line("channel:ntfy", "ntfy", Standing::Locked, &[]),
                line("channel:fcm", "fcm", Standing::StoreFailed, &[]),
            ],
            Group::Plugins => vec![
                line("plugin:due", "due", Standing::Unobserved, &[]),
                line(
                    "plugin:rota",
                    "rota",
                    Standing::Unreadable {
                        words: "its store could not be read".to_owned(),
                    },
                    &[],
                ),
            ],
            Group::Drive | Group::Coding | Group::Box => Vec::new(),
        };
        samples.add([wire::Overview {
            groups: Group::ALL
                .into_iter()
                .map(|group| wire::OverviewGroup {
                    group,
                    lines: lines(group),
                })
                .collect(),
            ask: "setup".to_owned(),
        }]);
    }
    {
        use suzatary_core::secretary::phase::Phase;
        samples.add([
            Phase::Trying {
                attempt: 1,
                running: false,
            },
            Phase::Sent,
            Phase::Applied,
            Phase::Acted,
            Phase::MayHaveGone,
            Phase::Checked,
            Phase::Dismissed,
        ]);
    }

    // The browser's module's own answers: the verifier's, with every arm of its walk and of
    // what it found, and who wrote each object, with every arm of its proof.
    let verdict = |hash: &str, walk: wire::Walk, condition: wire::Condition| wire::ObjectVerdict {
        hash: hash.to_owned(),
        computed: hash.to_owned(),
        hash_ok: true,
        walk,
        condition,
        holds: if hash.starts_with('a') {
            wire::Verified::Event {
                kind: "Created".to_owned(),
                todo: CAST.todos.agents.id.to_owned(),
                actor: "owner".to_owned(),
                at: "2026-09-02T12:00:00".to_owned(),
            }
        } else {
            wire::Verified::Envelope {
                kind: "Woven".to_owned(),
            }
        },
    };
    let tip = "a".repeat(64);
    let stray = "b".repeat(64);
    samples.add([
        wire::VerifiedPayload {
            tips: vec![tip.clone()],
            objects: vec![verdict(
                &tip,
                wire::Walk::Reached { depth: 0 },
                wire::Condition::Clean,
            )],
            walked: 1,
            verified: 1,
            genesis: vec![tip.clone()],
            linearisation: vec![tip.clone()],
            found: wire::Found::of(Vec::new()),
        },
        wire::VerifiedPayload {
            tips: vec![tip.clone()],
            objects: vec![
                verdict(
                    &tip,
                    wire::Walk::Reached { depth: 0 },
                    wire::Condition::Clean,
                ),
                verdict(
                    &stray,
                    wire::Walk::Unreached,
                    wire::Condition::Flawed {
                        problem: "not reachable from any head by its parents".to_owned(),
                    },
                ),
            ],
            walked: 1,
            verified: 2,
            genesis: vec![tip.clone()],
            linearisation: vec![tip.clone(), stray.clone()],
            found: wire::Found::of(vec![format!(
                "object {stray} is not reachable from any head"
            )]),
        },
    ]);
    samples.add([
        wire::ProvenanceRow {
            hash: tip.clone(),
            proof: wire::Proof::Proven {
                key: "c".repeat(64),
            },
            said: "signed by the key cccccccc".to_owned(),
        },
        wire::ProvenanceRow {
            hash: tip.clone(),
            proof: wire::Proof::Predates,
            said: "written before signatures".to_owned(),
        },
        wire::ProvenanceRow {
            hash: stray.clone(),
            proof: wire::Proof::Unproven,
            said: "not proven: no key signs it".to_owned(),
        },
    ]);
    samples.add([wire::SignedObject {
        hash: tip.clone(),
        text: format!(
            "Signed(object='{stray}', key='{}', signature='')",
            "c".repeat(64)
        ),
    }]);
    samples.add([
        wire::StoreFrame {
            store: wire::StoreName::Lines,
            heads: vec![tip.clone()],
        },
        wire::StoreFrame {
            store: wire::StoreName::TopicAliases,
            heads: Vec::new(),
        },
    ]);

    samples.add([wire::RecordPayload {
        summaries: vec![wire::RecordSummary {
            node: "a".repeat(64),
            handle: "0+16".to_owned(),
            lines: 16,
            text: "Asked for the early train on weekdays".to_owned(),
        }],
    }]);

    let names: std::collections::BTreeSet<String> = wire::answer_names().into_iter().collect();
    let covered: std::collections::BTreeSet<String> = samples.0.keys().cloned().collect();
    assert_eq!(
        covered, names,
        "every answer has a sample, and nothing else does"
    );
    write_or_check(&samples.0, "answers.json");
}

/// A route's answer as axum's `Json` prints it.
mod erased {
    pub trait Printed {
        fn printed(&self) -> String;
    }

    impl<T: serde::Serialize> Printed for T {
        fn printed(&self) -> String {
            serde_json::to_string(self).expect("a payload prints")
        }
    }
}

/// A member the set holds as a failure, as the fold and the Inbox name it.
fn failed_chain() -> wire::FailedChain {
    wire::FailedChain {
        chain: "suzatary".to_owned(),
        why: "two geneses: the chain is two prodromes".to_owned(),
    }
}

/// The slots the web vectors store, sealed under #257's own `Rules`, still read as sealed under
/// the current rules now that `Rules` is a `rule_version::Version`: the hex a slot's associated
/// data binds is the hex this build names.
#[test]
fn a_stored_slot_is_under_the_rules_this_build_names() {
    use suzatary_core::redact::Rules;
    use suzatary_core::secretary::line_event::{LineEvent, LineVocabulary};
    use suzatary_core::secretary::scrubbed_under::ScrubbedUnder;

    let text = std::fs::read_to_string(vector("replicas.json")).expect("the replicas vector");
    let vectors: serde_json::Value = serde_json::from_str(&text).expect("json");
    let stored: Vec<&str> = vectors["conversations"]
        .as_array()
        .expect("conversations")
        .iter()
        .filter_map(|object| object["text"].as_str())
        .filter(|text| text.contains("Scrubbed("))
        .collect();
    assert!(!stored.is_empty());
    for text in stored {
        let envelope = prodrome_core::event::parse_envelope::<LineEvent>(&LineVocabulary, text)
            .expect("a stored line reads");
        let line = envelope.event().expect("a line");
        assert!(line.scrubbed_under().count() > 0, "{text}");
        for under in line.scrubbed_under() {
            assert_eq!(under, &ScrubbedUnder::current(()), "{text}");
        }
    }
    assert_eq!(
        Rules::current(()).as_str(),
        "a0d5cffe58839d79401ab650f475384a5eb7fbc064b46cfb3b7a3b25ac91f143"
    );
}

/// What a device held at the end of one scenario, and what the box keeps: the nodes of every
/// skeleton answer it took, the name of every key those answers gave it, and the name of every
/// key the box still holds. `linesync.retiredBy` over the nodes, given the keys the device holds,
/// must leave the device the box's keys and no other (`web/test/retired.test.ts`).
#[derive(Serialize)]
struct Retirement {
    name: &'static str,
    nodes: Vec<wire::SkeletonNode>,
    device: Vec<String>,
    #[serde(rename = "box")]
    kept: Vec<String>,
}

/// A device that pulls the lines' skeleton whole from the frontier its last answer named, and
/// holds every node and key it was given.
struct Device {
    since: prodrome_core::store::frontier::Since,
    nodes: BTreeMap<String, wire::SkeletonNode>,
    keys: BTreeMap<String, String>,
}

impl Device {
    fn pull(
        &mut self,
        lines: &suzatary_core::secretary::conversations::InMemory,
        books: routes::Books<'_>,
    ) {
        let answer = routes::lines_skeleton(
            lines.conversations(),
            &self.since,
            books,
            now(),
            routes::Cut::Whole,
        )
        .expect("the skeleton reads");
        self.since = prodrome_core::store::frontier::Since::Frontier(
            prodrome_core::store::frontier::Token::parse(&answer.frontier.digest).expect("a token"),
        );
        self.nodes.extend(
            answer
                .nodes
                .into_iter()
                .map(|node| (node.hash.clone(), node)),
        );
        self.keys.extend(answer.keys);
    }
}

/// The conversation every retirement scenario writes to.
fn keys_conversation() -> ConversationId {
    ConversationId::new("2026-09-02-keys".to_owned()).expect("a slug")
}

/// The conversation opened and three turns, each sealed under `under`.
fn three_turns(
    under: &suzatary_core::secretary::scrubbed_under::ScrubbedUnder,
) -> suzatary_core::secretary::conversations::InMemory {
    use suzatary_core::secretary::line_event::LineEvent;
    use suzatary_core::store::Doc as _;
    let lines = suzatary_core::secretary::conversations::InMemory::default();
    let opened = ConversationEntry::Opened {
        at: now(),
        title: suzatary_core::secretary::words::Words::kept("Keys".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: suzatary_core::secretary::store::Kind::Topic,
    };
    let said = |n: i64| ConversationEntry::Said {
        at: later(now(), n),
        turn: Turn::Person {
            text: format!("Question {n}"),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: None,
    };
    for (held, entry) in [opened, said(1), said(2), said(3)].iter().enumerate() {
        let held = u32::try_from(held).expect("a count");
        let (line, keys) = LineEvent::written_under(
            under,
            keys_conversation(),
            actor("assistant"),
            held,
            entry,
            &lines.entropy,
        )
        .expect("a line");
        for key in &keys {
            lines.keys.set(&key.name(), key).expect("kept");
        }
        lines.lines.append(line).expect("appended");
    }
    lines
}

/// The words of the conversation's line `at` erased, and swept.
fn erase_line(lines: &suzatary_core::secretary::conversations::InMemory, at: usize) {
    let keys = transcript::read(lines.conversations(), &keys_conversation())
        .expect("reads")
        .lines()[at]
        .line
        .keys()
        .cloned()
        .collect();
    transcript::erase(
        lines.conversations(),
        &keys_conversation(),
        keys,
        suzatary_core::secretary::erasure::Erasure::Asked {
            by: actor(CAST.owner.actor),
        },
        later(now(), 60),
    )
    .expect("erased");
    transcript::sweep(lines.conversations()).expect("swept");
}

/// `migrate-scrub` run on the box: every stale slot sealed again, and swept.
fn migrate(lines: &suzatary_core::secretary::conversations::InMemory) {
    let done = suzatary_core::secretary::rescrub::run(lines.conversations(), later(now(), 30))
        .expect("migrated");
    assert!(done.resealed > 0, "the stale slots are sealed again");
}

/// Every reseal sealed again from its own seal, as a change to the rules would, then swept.
fn migrate_again(lines: &suzatary_core::secretary::conversations::InMemory) {
    reseal_again(lines);
    transcript::sweep(lines.conversations()).expect("swept");
}

/// Every reseal sealed again from its own seal, with no sweep after: both reseals' keys held.
fn reseal_again(lines: &suzatary_core::secretary::conversations::InMemory) {
    use suzatary_core::secretary::line_event::LineEvent;
    use suzatary_core::secretary::line_shape::LineShape;
    use suzatary_core::secretary::line_words::LineWords;
    use suzatary_core::secretary::locked_text::KeyName;
    use suzatary_core::store::Doc as _;
    let read = transcript::read(lines.conversations(), &keys_conversation()).expect("reads");
    // Each slot's words as a reader reads them, by the key of the slot as first written.
    let words: BTreeMap<KeyName, LineWords> = read
        .lines()
        .iter()
        .filter(|written| !matches!(written.line.shape(), LineShape::Resealed { .. }))
        .flat_map(|written| {
            let words = suzatary_core::secretary::line_shape::split(&written.entry)
                .1
                .expect("words");
            written.line.keys().cloned().zip(words).collect::<Vec<_>>()
        })
        .collect();
    let objects = lines.lines.objects().expect("reads");
    let mut held = u32::try_from(objects.len()).expect("a count");
    for (_, object) in &objects {
        let Some(line) = object.event() else { continue };
        let LineShape::Resealed { of, .. } = line.shape() else {
            continue;
        };
        let own = line.keys().next().expect("a reseal's key").clone();
        let (reseal, key) = LineEvent::reseal(
            keys_conversation(),
            later(now(), 90),
            actor("assistant"),
            held,
            of.clone(),
            own,
            &words[of],
            &lines.entropy,
        )
        .expect("a reseal");
        lines.keys.set(&key.name(), &key).expect("kept");
        lines.lines.append(reseal).expect("appended");
        held += 1;
    }
}

/// What a device does after a step of the box: pulls from where it stands, or starts over, as
/// one does when a later epoch replaces its row, and pulls from nothing.
#[derive(Clone, Copy)]
enum Then {
    Pull,
    StartOver,
}

/// One step of a scenario, and what the device does after it.
type Step<'a> = (
    &'a dyn Fn(&suzatary_core::secretary::conversations::InMemory),
    Then,
);

/// A device holding nothing of the lines.
fn fresh() -> Device {
    Device {
        since: prodrome_core::store::frontier::Since::Nothing,
        nodes: BTreeMap::new(),
        keys: BTreeMap::new(),
    }
}

/// The box restored from a copy taken before the migration: the lines keep every reseal, and the
/// keys are the copy's, so each first seal is back and no reseal's own key is held.
fn restore_keys_from_before(
    lines: &suzatary_core::secretary::conversations::InMemory,
    before: &[(
        suzatary_core::secretary::locked_text::KeyName,
        suzatary_core::secretary::locked_text::TextKey,
    )],
) {
    use suzatary_core::store::Doc as _;
    for name in lines.keys.names() {
        lines.keys.remove(&name).expect("removed");
    }
    for (name, key) in before {
        lines.keys.set(name, key).expect("kept");
    }
}

/// Every key the box holds, with its bytes.
fn held_keys(
    lines: &suzatary_core::secretary::conversations::InMemory,
) -> Vec<(
    suzatary_core::secretary::locked_text::KeyName,
    suzatary_core::secretary::locked_text::TextKey,
)> {
    use suzatary_core::store::Gets as _;
    lines
        .keys
        .names()
        .into_iter()
        .map(|name| {
            let key = lines.keys.get(&name).expect("reads").expect("held");
            (name, key)
        })
        .collect()
}

/// An erasure that names a reseal's own key and not its slot's first key, which `erase` refuses:
/// it would leave the first seal open on a device that never held the reseal's key.
fn erase_a_reseal_s_key(lines: &suzatary_core::secretary::conversations::InMemory) {
    use suzatary_core::secretary::line_shape::LineShape;
    let objects = lines.lines.objects().expect("reads");
    let own = objects
        .iter()
        .filter_map(|(_, object)| object.event())
        .find(|line| matches!(line.shape(), LineShape::Resealed { .. }))
        .and_then(|line| line.keys().next().cloned())
        .expect("a reseal");
    let refused = transcript::erase(
        lines.conversations(),
        &keys_conversation(),
        [own].into(),
        suzatary_core::secretary::erasure::Erasure::Asked {
            by: actor(CAST.owner.actor),
        },
        later(now(), 60),
    );
    assert!(
        refused.is_err(),
        "an erasure of a reseal's own key is refused"
    );
}

/// What a device that pulls once before `steps`, and after each as it says, holds, and what the
/// box keeps; checked first by the box's own rule, given the keys the device holds.
fn converged(
    name: &'static str,
    lines: &suzatary_core::secretary::conversations::InMemory,
    steps: &[Step<'_>],
) -> Retirement {
    use suzatary_core::secretary::locked_text::KeyName;
    let (epochs, frontiers) = (
        suzatary_core::epoch::Epochs::remembered(),
        prodrome_core::store::frontier::Frontiers::remembered(prodrome_core::store::frontier::KEPT),
    );
    let books = routes::Books {
        epochs: &epochs,
        frontiers: &frontiers,
    };
    let mut device = fresh();
    device.pull(lines, books);
    for (step, then) in steps {
        step(lines);
        match then {
            Then::Pull => {}
            Then::StartOver => device = fresh(),
        }
        device.pull(lines, books);
    }
    let kept: Vec<String> = lines
        .keys
        .names()
        .iter()
        .map(|name| name.as_str().to_owned())
        .collect();
    let ring = suzatary_core::secretary::key_ring::KeyRing::of(
        &device
            .keys
            .iter()
            .map(|(name, hex)| (KeyName::new(name.clone()).expect("a key name"), hex.clone()))
            .collect(),
    );
    let objects = lines.lines.objects().expect("reads");
    let dead: std::collections::BTreeSet<String> = transcript::dead(
        objects
            .iter()
            .filter_map(|(name, object)| Some((name, object.event()?))),
        &|name: &KeyName| ring.key(name),
    )
    .into_iter()
    .map(|(_, _, key)| key.as_str().to_owned())
    .collect();
    let left: Vec<String> = device
        .keys
        .keys()
        .filter(|name| !dead.contains(*name))
        .cloned()
        .collect();
    assert_eq!(left, kept, "{name}: the device converges on the box's keys");
    Retirement {
        name,
        nodes: device.nodes.into_values().collect(),
        device: device.keys.into_keys().collect(),
        kept,
    }
}

/// Every scenario the device's key hygiene must converge through: an erasure, a migration while
/// the device was away, two migrations whose first reseals the device took in an earlier pull,
/// two while it was away, so it never held the first reseals' keys and finds the first seals
/// dead only through the second reseals, an erasure of a slot sealed again, the box restored
/// with its reseals and without their keys while the device starts over, so each first seal is
/// live again, and an erasure of a reseal's own key, which the box refuses.
#[test]
fn the_retired_vector_is_every_way_a_key_dies() {
    use suzatary_core::secretary::scrubbed_under::ScrubbedUnder;
    let (current, unversioned) = (ScrubbedUnder::current(()), ScrubbedUnder::Unversioned);
    let migrate_twice = |lines: &suzatary_core::secretary::conversations::InMemory| {
        migrate(lines);
        migrate_again(lines);
    };
    let restored = three_turns(&unversioned);
    let before = held_keys(&restored);
    let restore = |lines: &suzatary_core::secretary::conversations::InMemory| {
        restore_keys_from_before(lines, &before);
    };
    let vector = vec![
        converged(
            "an erasure",
            &three_turns(&current),
            &[(&|l| erase_line(l, 2), Then::Pull)],
        ),
        converged(
            "a migration while the device was away",
            &three_turns(&unversioned),
            &[(&migrate, Then::Pull)],
        ),
        converged(
            "two migrations, the first one's reseals taken in an earlier pull",
            &three_turns(&unversioned),
            &[(&migrate, Then::Pull), (&migrate_again, Then::Pull)],
        ),
        converged(
            "two migrations while the device was away",
            &three_turns(&unversioned),
            &[(&migrate_twice, Then::Pull)],
        ),
        converged(
            "an erasure of a slot sealed again",
            &three_turns(&unversioned),
            &[(&migrate, Then::Pull), (&|l| erase_line(l, 1), Then::Pull)],
        ),
        converged(
            "the box restored without the reseals' keys, the device starting over",
            &restored,
            &[(&migrate, Then::Pull), (&restore, Then::StartOver)],
        ),
        converged(
            "an erasure naming a reseal's own key, refused",
            &three_turns(&unversioned),
            &[(&migrate, Then::Pull), (&erase_a_reseal_s_key, Then::Pull)],
        ),
    ];
    write_or_check(&vector, "retired.json");
}

/// A slot sealed again twice, before the sweep, while both reseals' keys are held: it reads from
/// the second reseal, the newest, and the reseal it was sealed again from comes after it.
#[test]
fn a_slot_resealed_twice_reads_its_newest_reseal() {
    use suzatary_core::secretary::line_shape::LineShape;
    use suzatary_core::secretary::scrubbed_under::ScrubbedUnder;
    use suzatary_core::store::Gets as _;
    let lines = three_turns(&ScrubbedUnder::Unversioned);
    migrate(&lines);
    reseal_again(&lines);
    let objects = lines.lines.objects().expect("reads");
    let all: Vec<_> = objects
        .iter()
        .filter_map(|(name, object)| Some((name, object.event()?)))
        .collect();
    let ordered = transcript::reseals(all.iter().copied());
    assert!(!ordered.is_empty());
    for seals in ordered.values() {
        let [newest, older] = seals.as_slice() else {
            panic!("two reseals of each slot, got {}", seals.len());
        };
        let LineShape::Resealed { from, .. } = newest.shape() else {
            panic!("a reseal");
        };
        assert_eq!(
            Some(from),
            older.keys().next(),
            "the newest is sealed again from the older"
        );
        for key in [newest, older].iter().filter_map(|seal| seal.keys().next()) {
            assert!(
                lines.keys.get(key).expect("reads").is_some(),
                "both reseals' keys are held"
            );
        }
    }
}
