//! Every store's bytes on disk, pinned: a scripted sequence written through each store leaves
//! exactly the pinned files, and reading a copy writes nothing. `tests/stores/` is never
//! regenerated: a difference is a migration.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use prodrome_core::event::{Actor, Hash, mk_completed, mk_created};
use prodrome_core::fpl::mk_flat;
use prodrome_core::policy::Untrusted;
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::drive_path::DriveFailure;
use suzatary_base::drive_path::DrivePath;
use suzatary_connect::cast::CAST;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Entry;
use suzatary_digest::secretary::digests::Digests;
use suzatary_digest::secretary::memories::Memories;
use suzatary_health::health::plugin::PluginName;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::proposal::{Draft, Proposal, Reasons};
use suzatary_io::attachments::AttachmentStore;
use suzatary_io::blobs::Blobs;
use suzatary_io::objects::Objects;
use suzatary_io::pdftotext::Pdftotext;
use suzatary_io::plugin_store::PluginStore;
use suzatary_lines::secretary::held_lines;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::transcript;
use suzatary_mail::mail::dedup_key::DedupKey;
use suzatary_memory::secretary::memory::{Fact, Kind, Origin};
use suzatary_record::record::{EventStore, TodoEvent, mk_authored};
use suzatary_store::store::Prodrome;
use suzatary_store::store::Reads as _;
use suzatary_triage::mail::known::Known;
use suzatary_triage::mail::local_state::Kept;
use suzatary_triage::mail::state::Decision;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
const SCAN: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 4, 5, 6];

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stores")
}

/// The prodromes whose appends write `Change`s, pinned apart from their legacy bytes.
const CHANGES: [&str; 1] = ["events/"];

pub(crate) const INBOX: &str = "inbox";

pub(crate) const LINES: &str = "secretary-lines";
const LINE_KEYS: &str = "secretary-lines-keys";

pub(crate) const MEMORY: &str = "memory";
const MEMORY_TEXTS: &str = "secretary-memory-texts";
pub(crate) const DIGESTS: &str = "digests";
const DIGEST_TEXTS: &str = "secretary-digest-texts";

const CALENDAR: &str = "cache/plugins/ics/store";

pub(crate) const DECISIONS: &str = "decisions";
pub(crate) const TOPICS: &str = "secretary-topics";
pub(crate) const TOPIC_ALIASES: &str = "secretary-topic-aliases";
/// The box's refusals of what a tab posted.
pub(crate) const ASKS: &str = "secretary-asks";

/// Every store pinned in `tests/stores-changes/` only.
const CHANGED: [&str; 11] = [
    DIGESTS,
    INBOX,
    LINES,
    LINE_KEYS,
    MEMORY,
    MEMORY_TEXTS,
    CALENDAR,
    DECISIONS,
    TOPICS,
    TOPIC_ALIASES,
    ASKS,
];

/// The triage's bookkeeping in its keyed form, pushed and box-local.
const MAIL_STATE: [&str; 2] = ["state.json", "local"];

fn dedup_key() -> DedupKey {
    DedupKey::of(&suzatary_base::capability::Secret::new("5a".repeat(32))).expect("a key")
}

fn ledger(root: &Path) -> suzatary_io::state::Ledger {
    suzatary_io::state::Ledger::new(
        root.join("state.json"),
        root.join("local/mail.json"),
        || Ok(dedup_key()),
    )
}

/// One salt for every text, so the texts' names can be pinned.
struct Pinned;

impl suzatary_base::capability::Entropy for Pinned {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
        Ok(())
    }
}

/// Each key one count past the last, so the keys, and the lines that name them, can be pinned.
#[derive(Default)]
struct Counted(std::sync::atomic::AtomicU8);

impl suzatary_base::capability::Entropy for Counted {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        Ok(())
    }
}

pub(crate) type Lines = Objects<LineEvent>;

pub(crate) fn lines_at(root: &Path) -> (Lines, suzatary_io::text_keys::TextKeys) {
    (
        Objects::open(root.join(LINES)),
        suzatary_io::text_keys::open(root.join(LINE_KEYS)),
    )
}

/// What the scripted writes leave: `tests/stores/`, its prodromes from `tests/stores-changes/`.
fn pinned() -> BTreeMap<String, Vec<u8>> {
    let mut files = tree(&golden());
    files.retain(|name, _| !CHANGES.iter().any(|dir| name.starts_with(dir)));
    files.extend(tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stores-changes"),
    ));
    files
}

fn at(hour: u32, minute: u32) -> Datetime {
    Datetime::new(2026, 9, 20, hour, minute, 0, 0).expect("an instant")
}

fn policy() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("an actor")])
}

fn owner() -> Actor {
    Actor::new(CAST.owner.actor).expect("an actor")
}

/// Every file under `root`, by its path relative to it, but a store's record of its heads and
/// the mark of its last hold: a record is derived from the objects beside it and local to one
/// machine, so it is checked against them ([`records_are_the_derived_heads`]) and never pinned
/// or copied, and a mark is drawn at random at each hold.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(root, &path, into);
            } else if ![".heads", ".writes"]
                .map(std::ffi::OsStr::new)
                .contains(&path.file_name().unwrap_or_default())
            {
                let name = path.strip_prefix(root).expect("under root");
                into.insert(
                    name.to_string_lossy().into_owned(),
                    std::fs::read(&path).expect("a file"),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

fn differences(found: &BTreeMap<String, Vec<u8>>, pinned: &BTreeMap<String, Vec<u8>>) -> String {
    let mut lines = Vec::new();
    for (name, bytes) in found {
        match pinned.get(name) {
            None => lines.push(format!("new: {name}")),
            Some(held) if held != bytes => lines.push(format!("changed: {name}")),
            Some(_) => {}
        }
    }
    for name in pinned.keys().filter(|name| !found.contains_key(*name)) {
        lines.push(format!("missing: {name}"));
    }
    lines.join("\n")
}

fn copy(from: &Path, to: &Path) {
    for (name, bytes) in tree(from) {
        let path = to.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, bytes).expect("a copy");
    }
}

struct Vault;

impl suzatary_held::vault::Vault for Vault {
    fn keep(&self, name: &str, _bytes: &[u8]) -> Result<DrivePath, DriveFailure> {
        Ok(suzatary_held::vault::folder()
            .join(name)
            .expect("a Drive name"))
    }
}

/// The chain: a todo created, authored, completed; a second head adopted from a copy of it, and
/// the merge that joins the two.
fn chain(root: &Path) {
    let todo = &CAST.todos.urgent;
    let store = Objects::chain(EventStore::new(root.join("events"), policy()));
    rooted(&store, suzatary_record::record::GENESIS);
    store
        .append(mk_created(todo.id, at(8, 0), CAST.owner.actor, todo.created, "").expect("valid"))
        .expect("appends");
    store
        .append(
            mk_authored(
                todo.id,
                at(8, 1),
                CAST.owner.actor,
                "todo",
                at(8, 1),
                suzatary_record::record::MarkupSource::from_chain(todo.body),
                mk_flat(0.55).expect("a term"),
                vec!["it is due".to_owned()],
                todo.category,
                "",
                suzatary_record::record::MarkupSource::from_chain(todo.detail),
                None,
                Vec::new(),
                Vec::new(),
                "",
            )
            .expect("valid"),
        )
        .expect("appends");

    let other = Objects::chain(EventStore::new(root.join("elsewhere"), policy()));
    other.accept(&literals(&store)).expect("accepts");
    other
        .append(mk_completed(todo.id, at(9, 0), CAST.owner.actor, "").expect("valid"))
        .expect("appends");
    let flat = &CAST.todos.flat;
    store
        .append(mk_created(flat.id, at(9, 30), CAST.owner.actor, flat.created, "").expect("valid"))
        .expect("appends");
    store.accept(&literals(&other)).expect("accepts");
    assert!(store.tips().expect("tips").len() > 1, "nothing joins them");
    std::fs::remove_dir_all(root.join("elsewhere")).expect("the copy goes");
}

fn literals(store: &Objects<TodoEvent>) -> Vec<(Hash, String)> {
    let objects = store.inner().root().join("objects");
    store
        .objects()
        .expect("reads")
        .into_iter()
        .map(|(name, _)| {
            let text = std::fs::read_to_string(objects.join(format!("{}.py", name.as_str())))
                .expect("an object");
            (name, text)
        })
        .collect()
}

/// The inbox's prodrome begun with a nonce of zeros, so its bytes can be pinned.
fn begun(root: &Path) -> Objects<suzatary_inbox::secretary::inbox_event::InboxEvent> {
    let store = Objects::open(root.join(INBOX));
    rooted(&store, "inbox");
    store
}

/// Begin `store` with a genesis labelled `label` and a nonce of zeros, so its names are pinned.
fn rooted<E: suzatary_stores::stores::Store<Vocabulary: Clone>>(store: &Objects<E>, label: &str) {
    let genesis = prodrome_core::event::Envelope::<E>::Genesis(
        prodrome_core::genesis::mk_genesis(label, &"0".repeat(32)).expect("a genesis"),
    );
    let name = prodrome_core::event::seal_hash(&genesis);
    let print = BTreeMap::from([(
        name.clone(),
        prodrome_core::event::canonical_envelope(&genesis),
    )]);
    prodrome_core::store::EventStore::<E, Untrusted>::at(
        store.inner().root(),
        Untrusted::none(),
        prodrome_core::store::Replica::schema(store.inner()).clone(),
    )
    .adopt_objects(&print, &name, prodrome_core::store::Sender::Stranger)
    .expect("begins");
}

/// The inbox: one email asked, one asked and dismissed; the first's id, which the morning
/// conversation names.
fn inbox(root: &Path) -> inbox::ProposalId {
    let store = begun(root);
    let assistant = Actor::new("assistant").expect("an actor");
    let ask = |subject: &str, minute: u32| {
        let asked = inbox::Asked {
            proposer: suzatary_inbox::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Email(Draft {
                account: CAST.owner.email.to_owned(),
                to: vec![CAST.counsel.email.to_owned()],
                cc: Vec::new(),
                subject: subject.to_owned(),
                body: "Thursday works.".to_owned(),
                attachments: Vec::new(),
            }),
            reasons: Reasons::new(
                "they asked",
                vec!["mail:abcdef012345".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: "proposal:default/0".parse().expect("a reference"),
            price: inbox::Price::unpriced(),
        };
        inbox::ask(&store, asked, &assistant, at(11, minute)).expect("filed")
    };
    let thursday = ask("Thursday", 0);
    let dismissed = ask("Friday", 1);
    inbox::dismiss(&store, &dismissed, &owner(), at(11, 2), "not this week").expect("dismissed");
    thursday
}

/// Two conversations, started and appended to, the first proposing `asked`.
fn conversations(root: &Path, asked: &inbox::ProposalId) {
    let (lines, keys) = lines_at(root);
    rooted(&lines, "lines");
    let entropy = Counted::default();
    let store = suzatary_lines::secretary::conversations::Conversations {
        runs: &*suzatary_lines::nothing::RUNS,
        lines: &lines,
        keys: &keys,
        entropy: &entropy,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened: Box::leak(Box::default()),
    };
    let opened = |title: &str| Entry::Opened {
        at: at(12, 0),
        title: suzatary_memory::secretary::words::Words::kept(title.to_owned()),
        model: "scripted/golden".to_owned(),
        kind: suzatary_conversations::secretary::store::Kind::Topic,
    };
    let morning = ConversationId::new("2026-09-20-1").expect("a slug");
    transcript::start(store, &morning, &opened("Morning")).expect("starts");
    transcript::push(
        store,
        &morning,
        &Entry::Said {
            at: at(12, 1),
            turn: Turn::Person {
                text: "what is due".to_owned(),
                references: vec![
                    format!("todo:life/{}", CAST.todos.urgent.id)
                        .parse()
                        .expect("a reference"),
                ],
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("appends");
    transcript::push(
        store,
        &morning,
        &Entry::Proposed {
            at: at(12, 2),
            object: asked.clone(),
            revises: None,
        },
    )
    .expect("appends");
    transcript::start(
        store,
        &ConversationId::new("default").expect("a slug"),
        &opened("Default"),
    )
    .expect("starts");
}

/// The deployment's writers in these tests: ada, who taps.
fn owners() -> std::collections::BTreeSet<prodrome_core::event::Actor> {
    [prodrome_core::event::Actor::new("ada").expect("an actor")].into()
}

/// The memory: two facts remembered and one forgotten, then two more consolidated into a sixth.
fn memories(root: &Path) {
    let history = Objects::open(root.join(MEMORY));
    rooted(&history, "memory");
    let texts = suzatary_io::texts::Texts::open(root.join(MEMORY_TEXTS));
    let cards = Objects::open(root.join(INBOX));
    let (lines, keys) = lines_at(root);
    let digests = Objects::digests(root.join(DIGESTS), owners());
    let digest_texts = suzatary_io::texts::Texts::open(root.join(DIGEST_TEXTS));
    let memories = Memories {
        history: &history,
        texts: &texts,
        cards: &cards,
        entropy: &Pinned,
        digests: Digests {
            history: &digests,
            texts: &digest_texts,
            lines: suzatary_lines::secretary::conversations::Conversations {
                runs: &*suzatary_lines::nothing::RUNS,
                lines: &lines,
                keys: &keys,
                entropy: &Pinned,
                renders: &suzatary_base::capability::Nothing,
                sessions: &suzatary_base::capability::Nothing,
                opened: Box::leak(Box::default()),
            },
            owners: &std::collections::BTreeSet::new(),
            compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
        },
    };
    let said = Origin::Turn {
        conversation: ConversationId::new("default").expect("a slug"),
        turn: 0,
    };
    memories
        .remember(
            &Fact::new(format!("{} takes the 7:40 train", CAST.owner.name)).expect("a fact"),
            Kind::Owner,
            &said,
            at(13, 0),
        )
        .expect("remembers");
    let gone = memories
        .remember(
            &Fact::new(format!("{} is the accountant", CAST.accountant.name)).expect("a fact"),
            Kind::Person,
            &said,
            at(13, 1),
        )
        .expect("remembers");
    memories
        .forget(gone.id, at(13, 2), &owner())
        .expect("forgets");
    let sources = [("mornings", 3), ("tea", 4)].map(|(what, minute)| {
        memories
            .remember(
                &Fact::new(format!("{} prefers {what}", CAST.owner.name)).expect("a fact"),
                Kind::Preference,
                &said,
                at(13, minute),
            )
            .expect("remembers")
            .id
    });
    memories
        .consolidate(
            &sources.into(),
            &Fact::new(format!("{} prefers tea in the morning", CAST.owner.name)).expect("a fact"),
            Kind::Preference,
            &Origin::Turn {
                conversation: ConversationId::new("default").expect("a slug"),
                turn: 1,
            },
            at(13, 5),
        )
        .expect("consolidates");
}

/// The calendar plugin's store: one occurrence learned, learned again unchanged, then cancelled.
fn calendar(root: &Path) {
    use suzatary_google::calendar::{
        CalendarName, Content, Occurrence, OccurrenceId, Status, When,
    };
    use suzatary_store::store::learn;

    let begun = PluginStore::at(root.join(CALENDAR))
        .begin(&calendar_schema())
        .expect("begins");
    let store = begun.store();
    rooted(store, "plugin");
    let review = |status| Occurrence {
        id: OccurrenceId::new(
            CalendarName::parse("PLUGIN_ICS_WORK").expect("a name"),
            "review@example.test",
            "20260921T130000Z",
        )
        .expect("an id"),
        content: Content::new(
            "Quarterly review".to_owned(),
            When::parse("2026-09-21T13:00:00Z/2026-09-21T14:00:00Z").expect("a span"),
            "Room 4".to_owned(),
            vec![CAST.owner.name.to_owned(), CAST.accountant.name.to_owned()],
            status,
        )
        .expect("content"),
    };
    let ics = Actor::new("ics").expect("an actor");
    let learned = |status, minute| {
        let event = review(status)
            .event(&calendar_schema(), at(14, minute), &ics)
            .expect("an event");
        learn(store, vec![event]).expect("learns");
    };
    learned(Status::Confirmed, 0);
    learned(Status::Confirmed, 15);
    learned(Status::Cancelled, 30);
}

fn calendar_schema() -> prodrome_core::declared::Declaration {
    prodrome_core::declared::Declaration::admit(include_str!("../../plugin-ics/ics.schema"))
        .expect("lawful")
}

/// Two actors' phones: registered, one re-registered, one forgotten.
fn devices(root: &Path) {
    struct At(Datetime);
    impl suzatary_base::capability::Clock for At {
        fn now(&self) -> Datetime {
            self.0
        }
    }
    let store = suzatary_io::devices::Devices::open(root.join("devices"));
    let register = |actor: &str, platform: &str, token: &str, minute: u32| {
        suzatary_signatures::devices::register_device(
            &store,
            &At(at(14, minute)),
            &serde_json::from_value(serde_json::json!({"platform": platform, "token": token}))
                .expect("a registration"),
            &Actor::new(actor).expect("an actor"),
        )
        .expect("registers");
    };
    register(CAST.owner.actor, "android", "tok-1", 0);
    register(CAST.owner.actor, "ios", "tok-2", 1);
    register(CAST.owner.actor, "android", "tok-1", 2);
    register("assistant", "android", "tok-3", 3);
    suzatary_signatures::devices::forget(&store, CAST.owner.actor, "tok-2").expect("forgets");
}

/// The triage's bookkeeping, saved twice.
fn state(root: &Path) {
    let ledger = ledger(root);
    let first = ledger.read().expect("reads");
    ledger
        .write(&Known {
            local: first.local.add_noise(CAST.friend.email),
            ..first
        })
        .expect("saves");
    let second = ledger
        .read()
        .expect("loads")
        .mark("abc123", Decision::Skip, Kept::default());
    ledger.write(&second).expect("saves");
}

/// An upload kept on the box, one kept private, and a file a verb kept for the model.
fn uploads(root: &Path) {
    let uploads = AttachmentStore::new(root.join("secretary/attachments"), Pdftotext::new("."));
    uploads.put(PNG).expect("kept");
    uploads
        .keep_private(SCAN, "scan.png", &Vault)
        .expect("kept private");
    Blobs::open(root.join("secretary/files"))
        .put(b"%PDF-1.7 a kept file")
        .expect("kept");
}

/// The digest: one node's summary retracted by the owner.
fn digests(root: &Path) {
    use prodrome_core::event::parse_event;
    use suzatary_digest::secretary::digest_event::{DigestEvent, DigestVocabulary};
    let store = Objects::digests(root.join(DIGESTS), owners());
    rooted(&store, "digests");
    let retracted = parse_event::<DigestEvent>(
        &DigestVocabulary::default(),
        &format!(
            "Retracted(node='{}', at=datetime(2026, 9, 20, 14, 0, 0), actor='ada')",
            "a".repeat(64)
        ),
    )
    .expect("a retraction");
    store.append(retracted).expect("appends");
}

/// The recorded decisions: one decided, a second superseding it, and the first withdrawn.
fn decisions(root: &Path) {
    use suzatary_todo::decisions::{DecisionEvent, DecisionId, Statement};
    let store = Objects::<DecisionEvent>::open(root.join(DECISIONS));
    rooted(&store, "decisions");
    let id = |name: &str| DecisionId::new(name).expect("an id");
    let said = |title: &str| {
        Statement::new(
            title,
            suzatary_record::record::MarkupSource::escaped("The *body*."),
            "Because.",
        )
        .expect("a statement")
    };
    let decided = |name: &str, minute, supersedes: &[&str]| {
        suzatary_todo::decisions::decided(
            id(name),
            at(15, minute),
            owner(),
            said(name),
            supersedes.iter().map(|link| id(link)).collect(),
        )
        .expect("a decision")
    };
    store.append(decided("first", 0, &[])).expect("appends");
    store
        .append(decided("second", 1, &["first"]))
        .expect("appends");
    store
        .append(
            suzatary_todo::decisions::withdrawn(id("first"), at(15, 2), owner(), "Replaced.")
                .expect("a withdrawal"),
        )
        .expect("appends");
}

/// The topics: a round put on a topic and taken off another, and the first topic renamed.
fn topics(root: &Path) {
    use suzatary_topics::secretary::topics::reading::Topics;
    use suzatary_topics::secretary::topics::topic::Topic;
    let topics = suzatary_io::topics::Topics::open(root.join(TOPICS), root.join(TOPIC_ALIASES));
    rooted(&topics.memberships, "topics");
    rooted(&topics.aliases, "topic-aliases");
    let round = || "round:default/0".parse().expect("a target");
    let topic = |name: &str| Topic::new(name).expect("a topic");
    for event in [
        prodrome_core::tags::membership::added::<Topics>(
            round(),
            topic("travel"),
            at(16, 0),
            owner(),
        ),
        prodrome_core::tags::membership::added::<Topics>(
            round(),
            topic("taxes"),
            at(16, 1),
            owner(),
        ),
        prodrome_core::tags::membership::removed::<Topics>(
            round(),
            topic("taxes"),
            at(16, 2),
            owner(),
        ),
    ] {
        topics.memberships.append(event).expect("appends");
    }
    topics
        .aliases
        .append(
            prodrome_core::tags::alias::aliased::<Topics>(
                topic("travel"),
                topic("trips"),
                at(16, 3),
                owner(),
            )
            .expect("an alias"),
        )
        .expect("appends");
}

/// The asks store: a refusal of a topic object a tab posted.
fn asks(root: &Path) {
    let asks: Objects<suzatary_stores::asks::AskEvent> = Objects::open(root.join(ASKS));
    rooted(&asks, "asks");
    asks.append(suzatary_store::receipt::refused(
        Hash::new("ab".repeat(32)).expect("a name"),
        at(17, 0),
        suzatary_stores::asks::keeper(),
        (
            suzatary_stores::stores::Kind::Topics,
            suzatary_store::receipt::Grounds::Final,
        ),
        "rests on cd…, which this box does not hold".to_owned(),
    ))
    .expect("appends");
}

fn write_every_store(root: &Path) {
    chain(root);
    let asked = inbox(root);
    conversations(root, &asked);
    memories(root);
    digests(root);
    decisions(root);
    topics(root);
    asks(root);
    calendar(root);
    devices(root);
    state(root);
    uploads(root);
}

#[test]
fn every_store_writes_the_pinned_bytes() {
    let dir = suzatary_testkit::scratch("stores");
    write_every_store(dir.path());
    records_are_the_derived_heads(dir.path());
    let found = tree(dir.path());
    let pinned = pinned();
    if found != pinned {
        let kept = dir.keep();
        panic!(
            "the stores wrote other bytes than the pins (left at {}):\n{}",
            kept.display(),
            differences(&found, &pinned)
        );
    }
}

/// Every store under `root` (a directory holding `objects/`) holds a record of its heads that
/// parses, names a present file for each tip and counts as many objects as `objects/` holds,
/// beside a `.gitignore` of `/.heads` and `/.writes` alone.
fn records_are_the_derived_heads(root: &Path) {
    fn stores(dir: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                if path.file_name() == Some(std::ffi::OsStr::new("objects")) {
                    into.push(dir.to_path_buf());
                } else {
                    stores(&path, into);
                }
            }
        }
    }
    let mut found = Vec::new();
    stores(root, &mut found);
    assert!(!found.is_empty(), "stores were written");
    for store in found {
        let recorded = prodrome_core::dag::Heads::parse(
            &std::fs::read_to_string(store.join(".heads"))
                .unwrap_or_else(|_| panic!("{} holds a record", store.display())),
        )
        .expect("the record parses");
        let objects = std::fs::read_dir(store.join("objects"))
            .expect("objects")
            .count();
        assert_eq!(recorded.count(), objects, "{}", store.display());
        for tip in recorded.tips() {
            assert!(
                store
                    .join("objects")
                    .join(format!("{}.py", tip.as_str()))
                    .exists(),
                "{} names a present tip",
                store.display()
            );
        }
        assert_eq!(
            std::fs::read_to_string(store.join(".gitignore")).expect("its gitignore"),
            "/.heads\n/.writes\n",
            "{}",
            store.display()
        );
    }
}

/// The database's conversations, keys drawn as the pins were.
fn conversations_of<'a>(
    database: &suzatary_stores::stores::Database<'a>,
) -> suzatary_lines::secretary::conversations::Conversations<'a> {
    suzatary_lines::secretary::conversations::Conversations {
        runs: &*suzatary_lines::nothing::RUNS,
        lines: database.conversations,
        keys: database.line_keys,
        entropy: &Pinned,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened: Box::leak(Box::default()),
    }
}

/// How many conversations the store holds, each of them read whole and found not empty.
fn conversations_read(store: suzatary_lines::secretary::conversations::Conversations<'_>) -> usize {
    let names = transcript::keys(store).expect("lists");
    for name in &names {
        assert!(!transcript::lines(store, name).expect("reads").is_empty());
    }
    names.len()
}

/// A copy of every pinned store under `root`; answers where the changed stores' pins are.
fn copy_pins(root: &Path) -> PathBuf {
    copy(&golden(), root);
    let changed = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stores-changes");
    for store in CHANGED {
        copy(&changed.join(store), &root.join(store));
    }
    std::fs::copy(changed.join("state.json"), root.join("state.json")).expect("a copy");
    copy(&changed.join("local"), &root.join("local"));
    changed
}

#[test]
fn reading_a_copy_of_every_store_writes_nothing() {
    use suzatary_base::chains::ChainName;
    use suzatary_held::attachment::AttachmentId;
    use suzatary_stores::stores::Database;

    let dir = suzatary_testkit::scratch("stores");
    let root = dir.path();
    let changed = copy_pins(root);

    let chain = Objects::chain(EventStore::new(root.join("events"), policy()));
    let inbox = Objects::open(root.join(INBOX));
    let (lines, keys) = lines_at(root);
    let memory = Objects::open(root.join(MEMORY));
    let memory_texts = suzatary_io::texts::Texts::open(root.join(MEMORY_TEXTS));
    let digests = Objects::digests(root.join(DIGESTS), owners());
    let digest_texts = suzatary_io::texts::Texts::open(root.join(DIGEST_TEXTS));
    let calendar = PluginStore::at(root.join(CALENDAR))
        .read()
        .expect("reads")
        .expect("a store");
    let triage = suzatary_io::state::State::open(root.join("state.json"));
    let triage_local = suzatary_io::state::Local::open(root.join("local/mail.json"));
    let devices = suzatary_io::devices::Devices::open(root.join("devices"));
    let uploads = AttachmentStore::new(root.join("secretary/attachments"), Pdftotext::new("."));
    let kept = Blobs::open(root.join("secretary/files"));
    let topics = suzatary_io::topics::Topics::open(root.join(TOPICS), root.join(TOPIC_ALIASES));
    let asks = Objects::open(root.join(ASKS));
    let database = Database {
        chains: [(
            ChainName::new("life").expect("a chain name"),
            &chain as &dyn Prodrome<_>,
        )]
        .into(),
        inbox: &inbox,
        conversations: &lines,
        line_keys: &keys,
        memory: &memory,
        memory_texts: &memory_texts,
        digests: &digests,
        digest_texts: &digest_texts,
        decisions: &Objects::open(root.join(DECISIONS)),
        topics: &topics.memberships,
        topic_aliases: &topics.aliases,
        asks: &asks,
        plugins: [(ics(), &calendar as &dyn Prodrome<_>)].into(),
        triage: &triage,
        triage_local: &triage_local,
        devices: &devices,
        uploads: &uploads,
        kept: &kept,
    };

    assert!(database.verify().iter().all(|(_, found)| found.is_empty()));
    let life = &database.chains[&ChainName::new("life").expect("a chain name")];
    assert_eq!(life.objects().expect("reads").len(), 5);
    assert_eq!(life.nodes().expect("reads").len(), 5);
    assert_eq!(life.tips().expect("reads").len(), 1);
    assert_eq!(
        inbox::fold(database.inbox)
            .expect("reads")
            .objects()
            .count(),
        2
    );
    let conversations = conversations_of(&database);
    assert_eq!(conversations_read(conversations), 2);
    let read = Memories {
        history: database.memory,
        texts: database.memory_texts,
        cards: database.inbox,
        entropy: &Pinned,
        digests: Digests {
            history: database.digests,
            texts: database.digest_texts,
            lines: conversations,
            owners: &std::collections::BTreeSet::new(),
            compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
        },
    }
    .reads()
    .read()
    .expect("reads");
    assert_eq!(
        (read.facts.len(), read.forgotten.len(), read.merged.len()),
        (2, 1, 2)
    );
    decisions_and_topics_read(&database);
    let phones = suzatary_signatures::devices::everyone(database.devices).expect("reads");
    assert_eq!(phones.values().map(|them| them.0.len()).sum::<usize>(), 2);
    triage_reads(&database);
    for id in [PNG, SCAN].map(AttachmentId::of) {
        assert!(database.uploads.get(&id).expect("reads").is_some());
    }
    let file = AttachmentId::of(b"%PDF-1.7 a kept file");
    assert!(database.kept.get(&file).expect("reads").is_some());
    let calendar =
        suzatary_google::calendar::Calendar::of(&database.plugins[&ics()].nodes().expect("reads"));
    assert_eq!(calendar.len(), 1);

    let after = tree(root);
    let pinned = pinned_with(&changed);
    assert!(after == pinned, "{}", differences(&after, &pinned));
}

/// Both decisions and the one topic still on its round read back.
fn decisions_and_topics_read(database: &suzatary_stores::stores::Database<'_>) {
    let ledger = suzatary_todo::decisions::Decisions {
        store: database.decisions,
    }
    .ledger()
    .expect("reads");
    assert_eq!(ledger.len(), 2);
    let reading = suzatary_topics::secretary::topics::stores::TopicStores {
        memberships: database.topics,
        aliases: database.topic_aliases,
    }
    .reading()
    .expect("reads");
    assert_eq!(reading.memberships().len(), 1);
    let asks = database.asks.objects().expect("reads");
    let refused = suzatary_store::receipt::refusals(
        asks.iter().map(|(name, object)| (name, object)),
        &suzatary_stores::stores::Kind::Topics,
    );
    assert_eq!(refused.len(), 1);
}

/// The triage's two documents read back through the key as one.
fn triage_reads(database: &suzatary_stores::stores::Database<'_>) {
    let known = Known {
        pushed: database
            .triage
            .get(&())
            .expect("loads")
            .expect("a document"),
        local: database
            .triage_local
            .get(&())
            .expect("loads")
            .expect("a document"),
        key: dedup_key(),
    };
    assert!(known.decided("abc123").is_some());
    assert!(known.is_noise(CAST.friend.email));
}

/// The golden tree with each store `changed` pins in its place.
fn pinned_with(changed: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut pinned = tree(&golden());
    for store in CHANGED {
        pinned.extend(
            tree(&changed.join(store))
                .into_iter()
                .map(|(name, bytes)| (format!("{store}/{name}"), bytes)),
        );
    }
    pinned.extend(
        tree(changed)
            .into_iter()
            .filter(|(name, _)| MAIL_STATE.iter().any(|store| name.starts_with(store))),
    );
    pinned
}

fn ics() -> PluginName {
    PluginName::parse("ics").expect("a plugin name")
}

/// Line `k` of a long conversation: a person's turn on odd lines, a long answer on even ones.
pub(crate) fn said(k: usize) -> Entry {
    Entry::Said {
        at: at(
            9 + u32::try_from(k / 60).expect("few"),
            u32::try_from(k % 60).expect("few"),
        ),
        turn: if k % 2 == 1 {
            Turn::Person {
                text: format!("line {k}: {}", "a sentence of what was said ".repeat(6)),
                references: Vec::new(),
                attachments: Vec::new(),
            }
        } else {
            Turn::Model {
                text: format!(
                    "= Answer {k}\n\n{}",
                    "A paragraph of the answer. ".repeat(300)
                ),
                calls: Vec::new(),
            }
        },
        by: Some("ada".into()),
    }
}

/// `conversations` conversations of `per` lines each, from `said`, written into `lines` with
/// their keys in `keys` in one run; answers the ids and how many slots they hold.
pub(crate) fn conversations_at_scale(
    lines: &Lines,
    keys: &suzatary_io::text_keys::TextKeys,
    conversations: usize,
    per: usize,
) -> (Vec<ConversationId>, usize) {
    let entropy = suzatary_io::texts::SystemEntropy;
    let ids: Vec<ConversationId> = (1..=conversations)
        .map(|n| ConversationId::new(format!("2026-10-09-{n}")).expect("a slug"))
        .collect();
    let opening = Entry::Opened {
        at: at(9, 0),
        title: suzatary_memory::secretary::words::Words::kept("Long".to_owned()),
        model: "claude-cli/haiku".to_owned(),
        kind: suzatary_conversations::secretary::store::Kind::Topic,
    };
    let writer = Actor::new(suzatary_base::writer::Writer::Assistant.actor()).expect("an actor");
    let mut written = (0..ids.len() * per).map(|n| (&ids[n / per], n % per));
    let mut slots = 0;
    lines
        .append_each(&mut || {
            let Some((id, k)) = written.next() else {
                return Ok(None);
            };
            let entry = if k == 0 { opening.clone() } else { said(k) };
            let held = u32::try_from(k).expect("few");
            let (line, kept) = LineEvent::write(id.clone(), writer.clone(), held, &entry, &entropy)
                .expect("a line");
            for key in &kept {
                suzatary_store::store::Doc::set(keys, &key.name(), key)?;
            }
            slots += kept.len();
            Ok(Some(line))
        })
        .expect("written");
    (ids, slots)
}

/// What a digest read costs at the live store's size: cold, warm, and after one new line. Run by
/// name, in release, with `--ignored digest_at_scale --nocapture`.
#[test]
#[ignore = "a measurement, run by name"]
fn digest_at_scale() {
    use suzatary_lines::secretary::opened::Opened;
    let dir = suzatary_testkit::scratch("stores");
    let (lines, keys) = lines_at(dir.path());
    let (history, texts) = (
        Objects::<suzatary_digest::secretary::digest_event::DigestEvent>::open(
            dir.path().join("digests"),
        ),
        suzatary_io::texts::Texts::open(dir.path().join("digest-texts")),
    );
    let entropy = suzatary_io::texts::SystemEntropy;
    let stores =
        |opened: &'static Opened| suzatary_lines::secretary::conversations::Conversations {
            runs: &*suzatary_lines::nothing::RUNS,
            lines: &lines,
            keys: &keys,
            entropy: &entropy,
            renders: &suzatary_base::capability::Nothing,
            sessions: &suzatary_base::capability::Nothing,
            opened,
        };
    let owners = std::collections::BTreeSet::new();
    let digests = |opened| Digests {
        history: &history,
        texts: &texts,
        lines: stores(opened),
        owners: &owners,
        compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
    };
    let (ids, slots) = conversations_at_scale(&lines, &keys, 20, 500);
    println!("{slots} slots");
    let time = |what: &str, f: &dyn Fn()| {
        let started = std::time::Instant::now();
        f();
        println!("{what}: {:?}", started.elapsed());
    };
    let warm: &'static Opened = Box::leak(Box::default());
    time("digest, cold", &|| {
        digests(warm).reads().read().expect("reads");
    });
    time("digest, warm", &|| {
        digests(warm).reads().read().expect("reads");
    });
    transcript::push(stores(warm), &ids[0], &said(500)).expect("said");
    time("digest, after one new line", &|| {
        digests(warm).reads().read().expect("reads");
    });
    time("the lines' fold alone, warm", &|| {
        stores(warm)
            .reads()
            .fold(held_lines::HeldLines::gathered)
            .expect("reads");
    });
    time("one conversation, warm", &|| {
        transcript::read(stores(warm), &ids[1]).expect("reads");
    });
}

/// The lines a phone holds at the live size, as a tab's pull hands them, written to
/// `$SUZATARY_LINES_FIXTURE`. Run by name, in release, with `--ignored lines_for_a_tab --nocapture`.
#[test]
#[ignore = "a fixture for a measurement, run by name"]
fn lines_for_a_tab() {
    use suzatary_base::conversation_id::ConversationId;
    use suzatary_conversations::secretary::store::Entry;
    let out =
        std::env::var("SUZATARY_LINES_FIXTURE").expect("SUZATARY_LINES_FIXTURE names the file");
    let dir = suzatary_testkit::scratch("stores");
    let (lines, keys) = lines_at(dir.path());
    let entropy = suzatary_io::texts::SystemEntropy;
    let opened: &'static suzatary_lines::secretary::opened::Opened = Box::leak(Box::default());
    let stores = suzatary_lines::secretary::conversations::Conversations {
        runs: &*suzatary_lines::nothing::RUNS,
        lines: &lines,
        keys: &keys,
        entropy: &entropy,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened,
    };
    let second = |k: usize| {
        at(
            9 + u32::try_from(k / 3600).expect("few"),
            u32::try_from(k / 60 % 60).expect("few"),
        )
    };
    let said = |k: usize| Entry::Said {
        at: second(k),
        turn: if k % 2 == 1 {
            Turn::Person {
                text: format!("line {k}: {}", "a sentence of what was said ".repeat(6)),
                references: Vec::new(),
                attachments: Vec::new(),
            }
        } else {
            Turn::Model {
                text: format!(
                    "= Answer {k}\n\n{}",
                    "A paragraph of the answer. ".repeat(8)
                ),
                calls: Vec::new(),
            }
        },
        by: Some("ada".into()),
    };
    let ids: Vec<ConversationId> = (1..=4)
        .map(|n| ConversationId::new(format!("2026-10-09-{n}")).expect("a slug"))
        .collect();
    for id in &ids {
        transcript::start(
            stores,
            id,
            &Entry::Opened {
                at: at(9, 0),
                title: suzatary_memory::secretary::words::Words::kept("Long".to_owned()),
                model: "claude-cli/haiku".to_owned(),
                kind: suzatary_conversations::secretary::store::Kind::Topic,
            },
        )
        .expect("started");
    }
    for k in 1..1_150 {
        for id in &ids {
            transcript::push(stores, id, &said(k)).expect("said");
        }
    }
    let read = transcript::read(stores, &ids[0]).expect("reads");
    let line = read.line_of(1).expect("an answer");
    transcript::erase(
        stores,
        &ids[0],
        read.lines()[line].line.keys().cloned().collect(),
        suzatary_memory::secretary::erasure::Erasure::Asked { by: owner() },
        second(2_000),
    )
    .expect("erased");
    let objects: Vec<serde_json::Value> = lines
        .objects()
        .expect("objects")
        .into_iter()
        .map(|(hash, object)| {
            serde_json::json!({"hash": hash.as_str(), "text": prodrome_core::event::canonical_envelope(&object)})
        })
        .collect();
    let keys: serde_json::Map<String, serde_json::Value> =
        std::fs::read_dir(dir.path().join(LINE_KEYS))
            .expect("the keys")
            .map(|entry| {
                let entry = entry.expect("a key");
                let bytes = std::fs::read(entry.path()).expect("its bytes");
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    serde_json::Value::String(hex::encode(bytes)),
                )
            })
            .collect();
    println!("{} objects, {} keys", objects.len(), keys.len());
    std::fs::write(
        out,
        serde_json::to_string(&serde_json::json!({"objects": objects, "keys": keys}))
            .expect("prints"),
    )
    .expect("written");
}

/// Lines written before an opening stored its kind, `stores-unkinded/`, read whole by this build:
/// every title opens under the binding it was locked with, and each kind is read off its id.
#[test]
fn lines_written_before_kinds_were_stored_read_whole() {
    let dir = suzatary_testkit::scratch("stores");
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stores-unkinded"),
        dir.path(),
    );
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/stores-changes")
            .join(LINE_KEYS),
        &dir.path().join(LINE_KEYS),
    );
    let (lines, keys) = lines_at(dir.path());
    let store = suzatary_lines::secretary::conversations::Conversations {
        runs: &*suzatary_lines::nothing::RUNS,
        lines: &lines,
        keys: &keys,
        entropy: &Pinned,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened: Box::leak(Box::default()),
    };
    let names = transcript::keys(store).expect("lists");
    assert!(!names.is_empty());
    for name in &names {
        let stored = suzatary_lines::secretary::conversations::read(store, name).expect("reads");
        assert!(stored.title.get().is_some(), "{name}'s title opens");
        assert_eq!(
            stored.kind,
            suzatary_lines::secretary::line_shape::unkinded(name),
            "{name}"
        );
    }
}

/// What a compacted node and a record call cost over a digest of about 2,000 nodes, half stale.
/// Run by name, in release, with `--ignored digest_nodes_at_scale --nocapture`.
#[test]
#[ignore = "a measurement, run by name"]
#[expect(clippy::too_many_lines, reason = "one measurement, read top to bottom")]
fn digest_nodes_at_scale() {
    use suzatary_digest::secretary::compact_ask::Beside;
    use suzatary_digest::secretary::compaction::Reading;
    use suzatary_digest::secretary::digest::Standing;
    use suzatary_digest::secretary::digest_event::{Attempt, DigestEvent, summarized};
    use suzatary_digest::secretary::digest_line::Line;
    use suzatary_digest::secretary::shown_record::OwnersRecord;
    use suzatary_lines::secretary::opened::Opened;
    use suzatary_memory::secretary::salted_text::Text;
    let dir = suzatary_testkit::scratch("stores");
    let (lines, keys) = lines_at(dir.path());
    let owners = std::collections::BTreeSet::new();
    let history = Objects::<DigestEvent>::digests(dir.path().join("digests"), owners.clone());
    let texts = suzatary_io::texts::Texts::open(dir.path().join("digest-texts"));
    let entropy = suzatary_io::texts::SystemEntropy;
    let opened: &'static Opened = Box::leak(Box::default());
    let digests = Digests {
        history: &history,
        texts: &texts,
        lines: suzatary_lines::secretary::conversations::Conversations {
            runs: &*suzatary_lines::nothing::RUNS,
            lines: &lines,
            keys: &keys,
            entropy: &entropy,
            renders: &suzatary_base::capability::Nothing,
            sessions: &suzatary_base::capability::Nothing,
            opened,
        },
        owners: &owners,
        compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
    };
    conversations_at_scale(&lines, &keys, 20, 300);
    let model = suzatary_digest::model_name::ModelName::parse("claude-cli/haiku").expect("a model");
    let compactor = suzatary_digest::secretary::digests::compactor();
    let mut count = 0u32;
    // Summarise `nodes` as `reading` reads them, each in a few words of its own.
    let mut summarise = |nodes: &[Hash], reading: &suzatary_digest::secretary::digest::Digest| {
        let mut nodes = nodes.iter();
        history
            .append_each(&mut || {
                let Some(node) = nodes.next() else {
                    return Ok(None);
                };
                count += 1;
                let label = *reading.node(node).expect("in the tree").measure();
                let text = Text::new(
                    Line::new(&format!("summary {count} of what was said")).expect("a line"),
                    [7; suzatary_base::capability::SALT_BYTES],
                );
                suzatary_store::store::Doc::set(&texts, &text.name(), &text)?;
                let attempt = Attempt {
                    over: reading.over(node).expect("over"),
                    model: model.clone(),
                    taint: label.taint,
                    beside: Beside::of(&label),
                };
                Ok(Some(
                    summarized(node.clone(), at(23, 0), compactor.clone(), &text, attempt)
                        .expect("an event"),
                ))
            })
            .expect("summarised");
    };
    loop {
        let reading = digests.reads().read().expect("reads");
        let pending = reading.pending();
        if pending.is_empty() {
            break;
        }
        summarise(&pending, &reading);
    }
    let reading = digests.reads().read().expect("reads");
    let low = inner(&reading, |node| {
        node.children()
            .iter()
            .all(prodrome_core::digest::Node::is_leaf)
    });
    let again: Vec<Hash> = low.into_iter().step_by(3).collect();
    summarise(&again, &reading);
    let reading = digests.reads().read().expect("reads");
    let stale = inner(&reading, |node| {
        reading.standing(node.name()) != Some(Standing::Current)
    });
    println!(
        "{} nodes, {} stale",
        inner(&reading, |_| true).len(),
        stale.len()
    );
    let time = |what: &str, f: &mut dyn FnMut()| {
        let started = std::time::Instant::now();
        f();
        println!("{what}: {:?}", started.elapsed());
    };
    for round in ["cold", "warm"] {
        time(&format!("record, {round}"), &mut || {
            let _ = OwnersRecord::of(Reading::of(&digests.reads()).expect("reads"));
        });
    }
    for n in 1..=3 {
        let reading = digests.reads().read().expect("reads");
        let next = reading.pending().into_iter().next().expect("a stale node");
        time(
            &format!("one compacted node ({n}): its write and two reads"),
            &mut || {
                summarise(std::slice::from_ref(&next), &reading);
                Reading::of(&digests.reads()).expect("reads");
                digests.reads().read().expect("reads");
            },
        );
        time(&format!("record after it ({n})"), &mut || {
            let _ = OwnersRecord::of(Reading::of(&digests.reads()).expect("reads"));
        });
    }
    let first = ConversationId::new("2026-10-09-1").expect("a slug");
    transcript::push(digests.lines, &first, &said(301)).expect("said");
    time("resumed, right after one line", &mut || {
        transcript::resumed(
            digests.lines,
            at(23, 59),
            suzatary_lines::secretary::transcript::Retry::Allowed,
        )
        .expect("reads");
    });
    time("the conversation list, after it", &mut || {
        transcript::each(digests.lines).expect("reads");
    });
    time("record, after it", &mut || {
        let _ = OwnersRecord::of(Reading::of(&digests.reads()).expect("reads"));
    });
}

/// The name of every inner node of `reading`'s tree that `keep` keeps.
fn inner(
    reading: &suzatary_digest::secretary::digest::Digest,
    keep: impl Fn(&prodrome_core::digest::Node<suzatary_digest::secretary::digest_label::Label>) -> bool,
) -> Vec<Hash> {
    let mut kept = Vec::new();
    let mut stack: Vec<_> = reading
        .tree()
        .map(prodrome_core::digest::Tree::root)
        .into_iter()
        .collect();
    while let Some(node) = stack.pop() {
        if !node.is_leaf() {
            if keep(node) {
                kept.push(node.name().clone());
            }
            stack.extend(node.children());
        }
    }
    kept
}

/// What a deploy of a change of the scrub's rules does to a stored, summarised store, with no
/// migration run: `SUZATARY_RULES_STORE=<dir> SUZATARY_RULES_PHASE=write|read`, by name, in
/// release, with `--ignored a_rules_change_over_a_stored_digest --nocapture`.
#[test]
#[ignore = "a measurement, run by name"]
#[expect(clippy::too_many_lines, reason = "one measurement, read top to bottom")]
fn a_rules_change_over_a_stored_digest() {
    use suzatary_digest::secretary::compact_ask::Beside;
    use suzatary_digest::secretary::digest_event::{Attempt, DigestEvent, summarized};
    use suzatary_digest::secretary::digest_line::Line;
    use suzatary_lines::secretary::opened::Opened;
    use suzatary_memory::secretary::salted_text::Text;
    use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
    let root = PathBuf::from(std::env::var("SUZATARY_RULES_STORE").expect("a directory"));
    let phase = std::env::var("SUZATARY_RULES_PHASE").expect("write or read");
    let (lines, keys) = lines_at(&root);
    let owners = std::collections::BTreeSet::new();
    let history = Objects::<DigestEvent>::digests(root.join("digests"), owners.clone());
    let texts = suzatary_io::texts::Texts::open(root.join("digest-texts"));
    let entropy = suzatary_io::texts::SystemEntropy;
    let stores =
        |opened: &'static Opened| suzatary_lines::secretary::conversations::Conversations {
            runs: &*suzatary_lines::nothing::RUNS,
            lines: &lines,
            keys: &keys,
            entropy: &entropy,
            renders: &suzatary_base::capability::Nothing,
            sessions: &suzatary_base::capability::Nothing,
            opened,
        };
    let digests = |opened: &'static Opened| Digests {
        history: &history,
        texts: &texts,
        lines: stores(opened),
        owners: &owners,
        compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
    };
    let model = suzatary_digest::model_name::ModelName::parse("claude-cli/haiku").expect("a model");
    let rule = suzatary_digest::secretary::compaction::Compaction {
        ladder: suzatary_conversations::nonempty::NonEmpty::new(model.clone(), Vec::new()),
    };
    let codes = ConversationId::new("2026-10-09-codes").expect("a slug");
    let code = |k: usize| format!("7391{k:02}");
    if phase == "write" {
        conversations_at_scale(&lines, &keys, 20, 500);
        let opened: &'static Opened = Box::leak(Box::default());
        let opening = Entry::Opened {
            at: at(9, 59),
            title: suzatary_memory::secretary::words::Words::kept("Codes".to_owned()),
            model: "claude-cli/haiku".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        };
        transcript::start(stores(opened), &codes, &opening).expect("opened");
        for k in 0..12 {
            let entry = Entry::Said {
                at: at(10, u32::try_from(k).expect("few")),
                turn: Turn::Person {
                    text: format!("Your verification code is {}. fee", code(k)),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: None,
            };
            transcript::push(stores(opened), &codes, &entry).expect("said");
        }
        let compactor = suzatary_digest::secretary::digests::compactor();
        let mut count = 0u32;
        loop {
            let reading = digests(opened).reads().read().expect("reads");
            let pending = reading.pending();
            if pending.is_empty() {
                break;
            }
            let mut nodes = pending.iter();
            history
                .append_each(&mut || {
                    let Some(node) = nodes.next() else {
                        return Ok(None);
                    };
                    count += 1;
                    let label = *reading.node(node).expect("in the tree").measure();
                    let text = Text::new(
                        Line::new(&format!("summary {count} of what was said")).expect("a line"),
                        [7; suzatary_base::capability::SALT_BYTES],
                    );
                    suzatary_store::store::Doc::set(&texts, &text.name(), &text)?;
                    let attempt = Attempt {
                        over: reading.over(node).expect("over"),
                        model: model.clone(),
                        taint: label.taint,
                        beside: Beside::of(&label),
                    };
                    Ok(Some(
                        summarized(node.clone(), at(23, 0), compactor.clone(), &text, attempt)
                            .expect("an event"),
                    ))
                })
                .expect("summarised");
        }
        println!("summarised {count} nodes");
    }
    let objects = lines.objects().expect("reads");
    let (mut current, mut stale) = (0usize, 0usize);
    for (_, object) in &objects {
        if let Some(line) = object.event() {
            for under in line.scrubbed_under() {
                if *under == ScrubbedUnder::current(()) {
                    current += 1;
                } else {
                    stale += 1;
                }
            }
        }
    }
    println!(
        "{} lines; slots under the current rules: {current}, stale: {stale}",
        objects.len()
    );
    let time = |what: &str, f: &mut dyn FnMut()| {
        let started = std::time::Instant::now();
        f();
        println!("{what}: {:?}", started.elapsed());
    };
    let warm: &'static Opened = Box::leak(Box::default());
    let scrubs = suzatary_held::redact::scrubs();
    time("the lines' fold, cold", &mut || {
        stores(warm)
            .reads()
            .fold(held_lines::HeldLines::gathered)
            .expect("reads");
    });
    time("the lines' fold, warm", &mut || {
        stores(warm)
            .reads()
            .fold(held_lines::HeldLines::gathered)
            .expect("reads");
    });
    let first = ConversationId::new("2026-10-09-1").expect("a slug");
    time("one conversation of 500 lines, cold", &mut || {
        transcript::read(stores(warm), &first).expect("reads");
    });
    time("one conversation of 500 lines, warm", &mut || {
        transcript::read(stores(warm), &first).expect("reads");
    });
    time("the digest, cold", &mut || {
        digests(warm).reads().read().expect("reads");
    });
    time("the digest, warm", &mut || {
        digests(warm).reads().read().expect("reads");
    });
    println!(
        "values scrubbed while reading: {}",
        suzatary_held::redact::scrubs() - scrubs
    );
    let read = format!(
        "{:?}",
        transcript::read(stores(warm), &codes).expect("reads")
    );
    let shown = (0..12).filter(|&k| read.contains(&code(k))).count();
    println!("of 12 lines holding a code a claiming word follows, {shown} show their code");
    let reading = digests(warm).reads().read().expect("reads");
    print!(
        "{}",
        suzatary_digest::secretary::digest_census::Census::of(&reading, &rule)
    );
}

/// The lines written by one handle into `dir` and read by another, as the box's serve and a
/// command share the store: `(reader, writer, push)`, where `push(k)` says line `k`.
fn two_handles(dir: &Path) -> (Lines, &'static Lines, impl Fn(u32)) {
    let (reader, _) = lines_at(dir);
    let (writer, keys) = lines_at(dir);
    let writer: &'static Lines = Box::leak(Box::new(writer));
    let keys: &'static suzatary_io::text_keys::TextKeys = Box::leak(Box::new(keys));
    let entropy: &'static Counted = Box::leak(Box::default());
    let opened: &'static suzatary_lines::secretary::opened::Opened = Box::leak(Box::default());
    let stores = suzatary_lines::secretary::conversations::Conversations {
        runs: &*suzatary_lines::nothing::RUNS,
        lines: writer,
        keys,
        entropy,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened,
    };
    let id = ConversationId::new("2026-11-15-heads").expect("a slug");
    transcript::start(
        stores,
        &id,
        &Entry::Opened {
            at: at(10, 0),
            title: suzatary_memory::secretary::words::Words::kept("Heads".to_owned()),
            model: "claude-cli/haiku".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        },
    )
    .or_else(|refused| {
        // A copy of a store that holds it already goes on from where it stands.
        format!("{refused:?}")
            .contains("Taken")
            .then_some(())
            .ok_or(refused)
    })
    .expect("started");
    let push = move |k: u32| {
        transcript::push(
            stores,
            &id,
            &Entry::Said {
                at: at(10, k),
                turn: Turn::Person {
                    text: format!("line {k}"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            },
        )
        .expect("said");
    };
    (reader, writer, push)
}

/// Line `k` of the conversation `two_handles` began, written through `lines`, a handle of its own
/// on the same store, with keys drawn at random so they never meet the first handle's.
fn said_through(lines: &'static Lines, dir: &Path) -> impl Fn(u32) {
    let keys: &'static suzatary_io::text_keys::TextKeys = Box::leak(Box::new(lines_at(dir).1));
    let entropy: &'static suzatary_io::texts::SystemEntropy = Box::leak(Box::default());
    let opened: &'static suzatary_lines::secretary::opened::Opened = Box::leak(Box::default());
    let stores = suzatary_lines::secretary::conversations::Conversations {
        runs: &*suzatary_lines::nothing::RUNS,
        lines,
        keys,
        entropy,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened,
    };
    let id = ConversationId::new("2026-11-15-heads").expect("a slug");
    move |k: u32| {
        transcript::push(
            stores,
            &id,
            &Entry::Said {
                at: at(10, k),
                turn: Turn::Person {
                    text: format!("line {k}, from the other handle"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            },
        )
        .expect("said");
    }
}

/// The heads a fresh handle derives from the files, with no record to read.
fn derived(dir: &Path) -> prodrome_core::dag::Heads {
    let fresh = lines_at(dir).0;
    let held = prodrome_core::store::Replica::held(fresh.inner()).expect("held");
    let recorded = std::fs::read(dir.join(LINES).join(".heads")).ok();
    let _ = std::fs::remove_file(dir.join(LINES).join(".heads"));
    let heads = prodrome_core::store::Replica::heads(lines_at(dir).0.inner()).expect("heads");
    if let Some(bytes) = recorded {
        std::fs::write(dir.join(LINES).join(".heads"), bytes).expect("restored");
    }
    assert_eq!(heads.tips(), &held.tips);
    heads
}

/// LAW (the Prodrome's 60, over the lines' store): after lines written by two handles of one store
/// in turn, each taking the lock for its own write, and a husk, the record on disk is the derived
/// heads, so `heads()` serves it; the store's root holds a `.gitignore` of `/.heads` alone.
#[test]
fn the_lines_store_records_the_derived_heads() {
    let dir = suzatary_testkit::scratch("stores");
    let (reader, writer, push) = two_handles(dir.path());
    let other: &'static Lines = Box::leak(Box::new(lines_at(dir.path()).0));
    let push_other = said_through(other, dir.path());
    let root = dir.path().join(LINES);
    let record = || {
        prodrome_core::dag::Heads::parse(
            &std::fs::read_to_string(root.join(".heads")).expect("a record"),
        )
        .expect("it parses")
    };
    for k in 1..4 {
        push(2 * k);
        assert_eq!(record(), derived(dir.path()), "after line {}", 2 * k);
        push_other(2 * k + 1);
        assert_eq!(
            record(),
            derived(dir.path()),
            "after line {}, from the other handle",
            2 * k + 1
        );
    }
    let names = writer
        .inner()
        .dag()
        .expect("reads")
        .objects()
        .keys()
        .cloned()
        .collect();
    assert!(
        writer.husk(&names).expect("husked").objects > 0,
        "a whole was husked"
    );
    assert_eq!(record(), derived(dir.path()), "a husk keeps every name");
    assert_eq!(
        prodrome_core::store::Replica::heads(reader.inner()).expect("heads"),
        record()
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".gitignore")).expect("its gitignore"),
        "/.heads\n/.writes\n"
    );
    assert!(writer.inner().verify().is_empty(), "no finding");
}

/// LAW (the Prodrome's 62 and 63, over the lines' store): an object brought into `objects/` by
/// another door reads as the older heads, and `verify` reports the record, until the heads are
/// recorded again or the record is deleted; a record naming a head whose file is gone is not
/// served.
#[test]
fn another_door_reads_as_the_recorded_heads_until_they_are_recorded_again() {
    let dir = suzatary_testkit::scratch("stores");
    let elsewhere = suzatary_testkit::scratch("stores");
    let (reader, _, push) = two_handles(dir.path());
    push(1);
    let recorded = derived(dir.path());
    // The same conversation, one line further, in a copy of the store; its newest object is
    // then copied in by hand, as a checkout or a copy would bring it.
    let objects = dir.path().join(LINES).join("objects");
    let theirs = elsewhere.path().join(LINES).join("objects");
    std::fs::create_dir_all(&theirs).expect("a copy");
    for entry in std::fs::read_dir(&objects).expect("lists") {
        let entry = entry.expect("an entry");
        std::fs::copy(entry.path(), theirs.join(entry.file_name())).expect("copied");
    }
    let keys_from = dir.path().join(LINE_KEYS);
    copy(&keys_from, &elsewhere.path().join(LINE_KEYS));
    let (_, other, push_there) = two_handles(elsewhere.path());
    push_there(2);
    let mut brought = 0;
    for entry in std::fs::read_dir(&theirs).expect("lists") {
        let entry = entry.expect("an entry");
        if !objects.join(entry.file_name()).exists() {
            std::fs::copy(entry.path(), objects.join(entry.file_name())).expect("brought in");
            brought += 1;
        }
    }
    assert!(brought > 0, "the other door brought objects in");
    let _ = other;
    let heads = || prodrome_core::store::Replica::heads(reader.inner()).expect("heads");
    assert_eq!(heads(), recorded, "the older heads, read with no listing");
    assert!(
        reader
            .inner()
            .verify()
            .iter()
            .any(|finding| matches!(finding, prodrome_core::dag::Finding::StaleHeads { .. })),
        "verify reports the stale record"
    );
    let now = derived(dir.path());
    assert_ne!(now, recorded);
    std::fs::remove_file(dir.path().join(LINES).join(".heads")).expect("deleted");
    assert_eq!(heads(), now, "a deleted record: the heads derived");

    // A record naming a head whose file is gone is not served.
    std::fs::write(dir.path().join(LINES).join(".heads"), now.print()).expect("recorded");
    let tip = now.tips().iter().next().expect("a tip").clone();
    std::fs::remove_file(objects.join(format!("{}.py", tip.as_str()))).expect("removed");
    assert_ne!(heads(), now, "the record names an absent head");
    assert_eq!(heads(), derived(dir.path()));
}

/// LAW (SPEC §3, the sender): a post through the door is a stranger's, so at the lines' schema,
/// which reads husks from their keeper, a print that does not hash to its name is refused and
/// nothing is placed, whether it reads as that name's husk or as another whole; the whole itself
/// is taken.
#[test]
fn a_post_whose_print_does_not_hash_to_its_name_is_refused() {
    let dir = suzatary_testkit::scratch("stores");
    let elsewhere = suzatary_testkit::scratch("stores");
    let (_, store, push) = two_handles(dir.path());
    push(1);
    // The next line, written in a copy, so this store holds what it rests on and not it.
    copy(&dir.path().join(LINES), &elsewhere.path().join(LINES));
    copy(
        &dir.path().join(LINE_KEYS),
        &elsewhere.path().join(LINE_KEYS),
    );
    let (_, copied, push_there) = two_handles(elsewhere.path());
    push_there(2);
    let held = store.inner().dag().expect("reads");
    let there = copied.inner().dag().expect("reads");
    let (name, whole) = there
        .objects()
        .iter()
        .find(|(name, _)| !held.objects().contains_key(*name))
        .map(|(name, object)| (name.clone(), object.clone()))
        .expect("a line this store does not hold");
    let prodrome_core::event::Envelope::Change(change) = &whole else {
        panic!("a line is a change");
    };
    let husk = prodrome_core::store::HuskObject::of(change)
        .expect("a line with words has a husk")
        .print();
    let other = prodrome_core::event::canonical_envelope(
        held.objects().values().next().expect("an object"),
    );
    for (print, reads_as) in [(husk, "its husk"), (other, "another whole")] {
        assert!(
            store.accept(&[(name.clone(), print)]).is_err(),
            "a print that reads as {reads_as} is refused"
        );
        assert!(
            !store
                .inner()
                .dag()
                .expect("reads")
                .objects()
                .contains_key(&name),
            "and nothing is placed under the name"
        );
    }
    store
        .accept(&[(
            name.clone(),
            prodrome_core::event::canonical_envelope(&whole),
        )])
        .expect("the whole is taken");
    assert!(
        store
            .inner()
            .dag()
            .expect("reads")
            .objects()
            .contains_key(&name)
    );
}

/// LAW: the lines' reading is kept by the store's recorded heads, and it is read from the files.
/// A handle that read keeps its reading while the record stands, so objects another door brings in
/// read, through it, as the recorded heads (the Prodrome's law 62); a new handle, as a new process
/// opens the store, reads the files, door and all; and once the record is deleted the first handle
/// reads the files too. So no process that opens a store serves a reading older than its files.
#[test]
fn the_lines_reading_is_kept_by_the_record_and_read_from_the_files() {
    let dir = suzatary_testkit::scratch("stores");
    let elsewhere = suzatary_testkit::scratch("stores");
    let (reader, _, push) = two_handles(dir.path());
    push(1);
    let names = |store: &Lines| -> BTreeSet<Hash> {
        store
            .objects()
            .expect("reads")
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    };
    let before = names(&reader);
    // The same conversation, one line further, in a copy of the store; its new objects are then
    // copied in by hand, as a checkout or a copy would bring them.
    let objects = dir.path().join(LINES).join("objects");
    let theirs = elsewhere.path().join(LINES).join("objects");
    std::fs::create_dir_all(&theirs).expect("a copy");
    for entry in std::fs::read_dir(&objects).expect("lists") {
        let entry = entry.expect("an entry");
        std::fs::copy(entry.path(), theirs.join(entry.file_name())).expect("copied");
    }
    copy(
        &dir.path().join(LINE_KEYS),
        &elsewhere.path().join(LINE_KEYS),
    );
    let (_, _, push_there) = two_handles(elsewhere.path());
    push_there(2);
    for entry in std::fs::read_dir(&theirs).expect("lists") {
        let entry = entry.expect("an entry");
        if !objects.join(entry.file_name()).exists() {
            std::fs::copy(entry.path(), objects.join(entry.file_name())).expect("brought in");
        }
    }
    let files = names(&lines_at(dir.path()).0);
    assert!(
        files.is_superset(&before) && files.len() > before.len(),
        "the door brought objects"
    );
    assert_eq!(
        names(&reader),
        before,
        "the handle that read keeps the recorded heads' reading"
    );
    assert_eq!(
        names(&lines_at(dir.path()).0),
        files,
        "a new handle reads the files"
    );
    std::fs::remove_file(dir.path().join(LINES).join(".heads")).expect("deleted");
    assert_eq!(names(&reader), files, "with the record deleted, the files");
}
