//! Every store's bytes on disk, pinned. A scripted sequence written through each store must
//! leave exactly the files under `tests/stores/`, and reading a copy of them writes nothing.
//!
//! `tests/stores/` is never regenerated: a difference is a change to stored data, which is a
//! migration. The chain writes the Prodrome's `Change`, so its appends are pinned in
//! `tests/stores-changes/`; `tests/stores/` keeps its legacy bytes, which a copy is still read
//! from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use prodrome_core::event::{Actor, Hash, mk_completed, mk_created};
use prodrome_core::fpl::mk_flat;
use prodrome_core::policy::Untrusted;
use suzatary_core::capability::{Datetime, DriveFailure, Turn};
use suzatary_core::cast::CAST;
use suzatary_core::drive::DrivePath;
use suzatary_core::mail::dedup_key::DedupKey;
use suzatary_core::mail::known::Known;
use suzatary_core::mail::local_state::Kept;
use suzatary_core::mail::state::Decision;
use suzatary_core::plugin::PluginName;
use suzatary_core::record::{EventStore, TodoEvent, mk_authored};
use suzatary_core::secretary::digests::Digests;
use suzatary_core::secretary::inbox;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::memories::Memories;
use suzatary_core::secretary::memory::{Fact, Kind, Origin};
use suzatary_core::secretary::store::{ConversationId, Draft, Entry, Proposal, Reasons};
use suzatary_core::secretary::transcript;
use suzatary_core::store::Prodrome;
use suzatary_core::store::Reads as _;
use suzatary_io::attachments::AttachmentStore;
use suzatary_io::blobs::Blobs;
use suzatary_io::objects::Objects;
use suzatary_io::pdftotext::Pdftotext;
use suzatary_io::plugin_store::PluginStore;

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];
const SCAN: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 4, 5, 6];

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stores")
}

/// The prodromes whose appends write `Change`s, pinned apart from their legacy bytes.
const CHANGES: [&str; 1] = ["events/"];

/// The inbox at the proposal schema, pinned in `tests/stores-changes/` only.
const INBOX: &str = "inbox";

/// The conversations' lines at their schema, and the keys of their words, pinned in
/// `tests/stores-changes/` only.
const LINES: &str = "secretary-lines";
const LINE_KEYS: &str = "secretary-lines-keys";

/// The memory's history at its schema, and its texts, pinned in `tests/stores-changes/` only.
const MEMORY: &str = "memory";
const MEMORY_TEXTS: &str = "secretary-memory-texts";
const DIGESTS: &str = "digests";
const DIGEST_TEXTS: &str = "secretary-digest-texts";

/// The calendar plugin's store, a cache-tier store, pinned in `tests/stores-changes/` only.
const CALENDAR: &str = "cache/plugins/ics/store";

/// The recorded decisions and the two topic stores, pinned in `tests/stores-changes/` only.
const DECISIONS: &str = "decisions";
const TOPICS: &str = "secretary-topics";
const TOPIC_ALIASES: &str = "secretary-topic-aliases";

/// Every store pinned in `tests/stores-changes/` only.
const CHANGED: [&str; 10] = [
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
];

/// The triage's bookkeeping in its keyed form, pushed and box-local, pinned in
/// `tests/stores-changes/` only.
const MAIL_STATE: [&str; 2] = ["state.json", "local"];

/// The dedup key every scripted write and read names messages by.
fn dedup_key() -> DedupKey {
    DedupKey::of(&suzatary_core::capability::Secret::new("5a".repeat(32))).expect("a key")
}

fn ledger(root: &Path) -> suzatary_io::state::Ledger {
    suzatary_io::state::Ledger::new(
        root.join("state.json"),
        root.join("local/mail.json"),
        || Ok(dedup_key()),
    )
}

/// One salt for every text, so the texts' names can be pinned; the store's own salts are the
/// system's.
struct Pinned;

impl suzatary_core::capability::Entropy for Pinned {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
        Ok(())
    }
}

/// Each key one count past the last, so the keys, and the lines that name them, can be pinned.
#[derive(Default)]
struct Counted(std::sync::atomic::AtomicU8);

impl suzatary_core::capability::Entropy for Counted {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        Ok(())
    }
}

/// The lines and their keys under `root`.
fn lines_at(root: &Path) -> (Objects<LineEvent>, suzatary_io::text_keys::TextKeys) {
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

/// Every file under `root`, by its path relative to it.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(root, &path, into);
            } else {
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

/// The files that differ, one line each.
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

impl suzatary_core::vault::Vault for Vault {
    fn keep(&self, name: &str, _bytes: &[u8]) -> Result<DrivePath, DriveFailure> {
        Ok(suzatary_core::vault::folder()
            .join(name)
            .expect("a Drive name"))
    }
}

/// The chain: a todo created, authored, completed; a second head adopted from a copy of it, and
/// the merge that joins the two.
fn chain(root: &Path) {
    let todo = &CAST.todos.urgent;
    let store = Objects::chain(EventStore::new(root.join("events"), policy()));
    rooted(&store, suzatary_core::record::GENESIS);
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
                suzatary_core::record::MarkupSource::from_chain(todo.body),
                mk_flat(0.55).expect("a term"),
                vec!["it is due".to_owned()],
                todo.category,
                "",
                suzatary_core::record::MarkupSource::from_chain(todo.detail),
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

/// The inbox's prodrome begun with a nonce of zeros, so its bytes can be pinned; the store's own
/// first write draws a random one.
fn begun(root: &Path) -> Objects<suzatary_core::secretary::inbox_event::InboxEvent> {
    let store = Objects::open(root.join(INBOX));
    rooted(&store, "inbox");
    store
}

/// Begin `store` with a genesis labelled `label` and a nonce of zeros, so its names are pinned.
fn rooted<E: prodrome_core::schema::Schema>(store: &Objects<E>, label: &str) {
    let genesis = prodrome_core::event::Envelope::<E>::Genesis(
        prodrome_core::genesis::mk_genesis(label, &"0".repeat(32)).expect("a genesis"),
    );
    let name = prodrome_core::event::seal_hash(&genesis);
    let print = BTreeMap::from([(
        name.clone(),
        prodrome_core::event::canonical_envelope(&genesis),
    )]);
    store.inner().adopt_objects(&print, &name).expect("begins");
}

/// The inbox: one email asked, one asked and dismissed; the first's id, which the morning
/// conversation names.
fn inbox(root: &Path) -> inbox::ProposalId {
    let store = begun(root);
    let assistant = Actor::new("assistant").expect("an actor");
    let ask = |subject: &str, minute: u32| {
        let asked = inbox::Asked {
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
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
    let store = suzatary_core::secretary::conversations::Conversations {
        lines: &lines,
        keys: &keys,
        entropy: &entropy,
        renders: &suzatary_core::capability::Nothing,
        opened: Box::leak(Box::default()),
    };
    let opened = |title: &str| Entry::Opened {
        at: at(12, 0),
        title: suzatary_core::secretary::words::Words::kept(title.to_owned()),
        model: "scripted/golden".to_owned(),
        kind: suzatary_core::secretary::store::Kind::Topic,
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

/// The memory: a `Genesis` begun with a nonce of zeros, two facts remembered and one forgotten,
/// whose text is deleted.
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
            lines: suzatary_core::secretary::conversations::Conversations {
                lines: &lines,
                keys: &keys,
                entropy: &Pinned,
                renders: &suzatary_core::capability::Nothing,
                opened: Box::leak(Box::default()),
            },
            owners: &std::collections::BTreeSet::new(),
            compacting: &suzatary_core::secretary::compacting::Compacting::Off,
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
}

/// The calendar plugin's store, at its declared schema: a `Genesis` begun with a nonce of zeros,
/// one occurrence learned, learned again unchanged (nothing written), then cancelled.
fn calendar(root: &Path) {
    use suzatary_core::calendar::{CalendarName, Content, Occurrence, OccurrenceId, Status, When};
    use suzatary_core::store::learn;

    let store = PluginStore::at(root.join(CALENDAR))
        .begin(&calendar_schema())
        .expect("begins")
        .store();
    rooted(&store, "plugin");
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
        learn(&store, vec![event]).expect("learns");
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
    impl suzatary_core::capability::Clock for At {
        fn now(&self) -> Datetime {
            self.0
        }
    }
    let store = suzatary_io::devices::Devices::open(root.join("devices"));
    let register = |actor: &str, platform: &str, token: &str, minute: u32| {
        suzatary_core::devices::register_device(
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
    suzatary_core::devices::forget(&store, CAST.owner.actor, "tok-2").expect("forgets");
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

/// The digest: a `Genesis` begun with a nonce of zeros, and one node's summary retracted by
/// the owner.
fn digests(root: &Path) {
    use prodrome_core::event::parse_event;
    use suzatary_core::secretary::digest_event::{DigestEvent, DigestVocabulary};
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

/// The recorded decisions: a `Genesis` begun with a nonce of zeros, one decision decided, a
/// second superseding it, and the first withdrawn.
fn decisions(root: &Path) {
    use suzatary_core::decisions::{DecisionEvent, DecisionId, Statement};
    let store = Objects::<DecisionEvent>::open(root.join(DECISIONS));
    rooted(&store, "decisions");
    let id = |name: &str| DecisionId::new(name).expect("an id");
    let said = |title: &str| {
        Statement::new(
            title,
            suzatary_core::record::MarkupSource::escaped("The *body*."),
            "Because.",
        )
        .expect("a statement")
    };
    let decided = |name: &str, minute, supersedes: &[&str]| {
        DecisionEvent::decided(
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
            DecisionEvent::withdrawn(id("first"), at(15, 2), owner(), "Replaced.")
                .expect("a withdrawal"),
        )
        .expect("appends");
}

/// The topics and their aliases, each begun with a nonce of zeros: a round put on a topic and
/// taken off another, and the first topic renamed.
fn topics(root: &Path) {
    use suzatary_core::secretary::topics::alias_event::AliasEvent;
    use suzatary_core::secretary::topics::membership_event::MembershipEvent;
    use suzatary_core::secretary::topics::topic::Topic;
    let topics = suzatary_io::topics::Topics::open(root.join(TOPICS), root.join(TOPIC_ALIASES));
    rooted(&topics.memberships, "topics");
    rooted(&topics.aliases, "topic-aliases");
    let round = || "round:default/0".parse().expect("a target");
    let topic = |name: &str| Topic::new(name).expect("a topic");
    for event in [
        MembershipEvent::added(round(), topic("travel"), at(16, 0), owner()),
        MembershipEvent::added(round(), topic("taxes"), at(16, 1), owner()),
        MembershipEvent::removed(round(), topic("taxes"), at(16, 2), owner()),
    ] {
        topics.memberships.append(event).expect("appends");
    }
    topics
        .aliases
        .append(
            AliasEvent::aliased(topic("travel"), topic("trips"), at(16, 3), owner())
                .expect("an alias"),
        )
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
    calendar(root);
    devices(root);
    state(root);
    uploads(root);
}

#[test]
fn every_store_writes_the_pinned_bytes() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    write_every_store(dir.path());
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

/// The database's conversations, keys drawn as the pins were.
fn conversations_of<'a>(
    database: &suzatary_core::store::Database<'a>,
) -> suzatary_core::secretary::conversations::Conversations<'a> {
    suzatary_core::secretary::conversations::Conversations {
        lines: database.conversations,
        keys: database.line_keys,
        entropy: &Pinned,
        renders: &suzatary_core::capability::Nothing,
        opened: Box::leak(Box::default()),
    }
}

/// How many conversations the store holds, each of them read whole and found not empty.
fn conversations_read(store: suzatary_core::secretary::conversations::Conversations<'_>) -> usize {
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
    use suzatary_core::attachment::AttachmentId;
    use suzatary_core::chains::ChainName;
    use suzatary_core::store::Database;

    let dir = tempfile::tempdir().expect("a scratch dir");
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
            compacting: &suzatary_core::secretary::compacting::Compacting::Off,
        },
    }
    .read()
    .expect("reads");
    assert_eq!((read.facts.len(), read.forgotten.len()), (1, 1));
    decisions_and_topics_read(&database);
    let phones = suzatary_core::devices::everyone(database.devices).expect("reads");
    assert_eq!(phones.values().map(|them| them.0.len()).sum::<usize>(), 2);
    triage_reads(&database);
    for id in [PNG, SCAN].map(AttachmentId::of) {
        assert!(database.uploads.get(&id).expect("reads").is_some());
    }
    let file = AttachmentId::of(b"%PDF-1.7 a kept file");
    assert!(database.kept.get(&file).expect("reads").is_some());
    let calendar =
        suzatary_core::calendar::Calendar::of(&database.plugins[&ics()].nodes().expect("reads"));
    assert_eq!(calendar.len(), 1);

    let after = tree(root);
    let pinned = pinned_with(&changed);
    assert!(after == pinned, "{}", differences(&after, &pinned));
}

/// Both decisions and the one topic still on its round read back.
fn decisions_and_topics_read(database: &suzatary_core::store::Database<'_>) {
    let ledger = suzatary_core::decisions::Decisions {
        store: database.decisions,
    }
    .ledger()
    .expect("reads");
    assert_eq!(ledger.len(), 2);
    let reading = suzatary_core::secretary::topics::stores::TopicStores {
        memberships: database.topics,
        aliases: database.topic_aliases,
    }
    .reading()
    .expect("reads");
    assert_eq!(reading.memberships().len(), 1);
}

/// The triage's two documents read back through the key as one.
fn triage_reads(database: &suzatary_core::store::Database<'_>) {
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

/// What reading costs at the size a long conversation reaches: 1,500 lines on disk, each read
/// with an empty cache of opened words (every slot opened again, as before the cache) and with a
/// warm one. Run by name, in release: `cargo test --release -p suzatary-io --test all -- --ignored
/// lines_at_scale --nocapture`.
#[test]
#[ignore = "a measurement, run by name"]
fn lines_at_scale() {
    use suzatary_core::secretary::store::{ConversationId, Entry};
    let dir = tempfile::tempdir().expect("a scratch dir");
    let (lines, keys) = lines_at(dir.path());
    let entropy = suzatary_io::texts::SystemEntropy;
    let stores = |opened: &'static suzatary_core::secretary::opened::Opened| {
        suzatary_core::secretary::conversations::Conversations {
            lines: &lines,
            keys: &keys,
            entropy: &entropy,
            renders: &suzatary_core::capability::Nothing,
            opened,
        }
    };
    let warm: &'static suzatary_core::secretary::opened::Opened = Box::leak(Box::default());
    let id = ConversationId::new("2026-10-09-1").expect("a slug");
    let said = |k: usize| Entry::Said {
        at: at(
            9 + u32::try_from(k / 3600).expect("few"),
            u32::try_from(k / 60 % 60).expect("few"),
        ),
        turn: suzatary_core::capability::Turn::Person {
            text: format!("line {k}: {}", "a sentence of what was said ".repeat(6)),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some("ada".into()),
    };
    transcript::start(
        stores(warm),
        &id,
        &Entry::Opened {
            at: at(9, 0),
            title: suzatary_core::secretary::words::Words::kept("Long".to_owned()),
            model: "claude-cli/haiku".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        },
    )
    .expect("started");
    for k in 1..1_500 {
        transcript::push(stores(warm), &id, &said(k)).expect("said");
    }
    let time = |what: &str, f: &dyn Fn()| {
        let started = std::time::Instant::now();
        f();
        println!("{what}: {:?}", started.elapsed());
    };
    let cold =
        || -> &'static suzatary_core::secretary::opened::Opened { Box::leak(Box::default()) };
    time("read, cold", &|| {
        transcript::read(stores(cold()), &id).expect("reads");
    });
    time("read, warm", &|| {
        transcript::read(stores(warm), &id).expect("reads");
    });
    time("push, cold", &|| {
        transcript::push(stores(cold()), &id, &said(1_500)).expect("said");
    });
    time("push, warm", &|| {
        transcript::push(stores(warm), &id, &said(1_501)).expect("said");
    });
    let objects = lines.objects().expect("objects");
    let digest = |opened| {
        let read = stores(opened);
        let entries = transcript::opened(&objects, &read.slots());
        suzatary_core::secretary::digest::digest(
            &objects,
            &entries,
            &[],
            &std::collections::BTreeSet::new(),
            |_| None,
        )
        .expect("no cycle");
    };
    time("digest, cold", &|| digest(cold()));
    time("digest, warm", &|| digest(warm));
}

/// The lines a phone holds at the live size, as a tab's pull hands them: 4,600 lines over four
/// conversations, a person's turn and a two-block answer by turns, through the box's one door,
/// and one answer erased; written as `{objects, keys}` to `$SUZATARY_LINES_FIXTURE`, which the
/// tab's module is timed over (the report of the words plan's tab half). Run by name, in release: `cargo test
/// --release -p suzatary-io --test all -- --ignored lines_for_a_tab --nocapture`.
#[test]
#[ignore = "a fixture for a measurement, run by name"]
fn lines_for_a_tab() {
    use suzatary_core::secretary::store::{ConversationId, Entry};
    let out =
        std::env::var("SUZATARY_LINES_FIXTURE").expect("SUZATARY_LINES_FIXTURE names the file");
    let dir = tempfile::tempdir().expect("a scratch dir");
    let (lines, keys) = lines_at(dir.path());
    let entropy = suzatary_io::texts::SystemEntropy;
    let opened: &'static suzatary_core::secretary::opened::Opened = Box::leak(Box::default());
    let stores = suzatary_core::secretary::conversations::Conversations {
        lines: &lines,
        keys: &keys,
        entropy: &entropy,
        renders: &suzatary_core::capability::Nothing,
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
                title: suzatary_core::secretary::words::Words::kept("Long".to_owned()),
                model: "claude-cli/haiku".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Topic,
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
        suzatary_core::secretary::erasure::Erasure::Asked { by: owner() },
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
    let dir = tempfile::tempdir().expect("a scratch dir");
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
    let store = suzatary_core::secretary::conversations::Conversations {
        lines: &lines,
        keys: &keys,
        entropy: &Pinned,
        renders: &suzatary_core::capability::Nothing,
        opened: Box::leak(Box::default()),
    };
    let names = transcript::keys(store).expect("lists");
    assert!(!names.is_empty());
    for name in &names {
        let stored = suzatary_core::secretary::read(store, name).expect("reads");
        assert!(stored.title.get().is_some(), "{name}'s title opens");
        assert_eq!(
            stored.kind,
            suzatary_core::secretary::line_shape::unkinded(name),
            "{name}"
        );
    }
}
