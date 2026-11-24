//! The laws every prodrome keeps whatever its schema, checked for every prodrome row of
//! `suzatary_stores::stores::Kind` over the objects its pins hold (`app/io-chains/tests/stores-changes/`):
//!
//! - a store begun holds one `Genesis`, labelled as its row says;
//! - each object reads back by its name as its canonical print, and a name it does not hold
//!   reads as absent;
//! - a fresh store takes a store's prints whole, and taking them again takes nothing;
//! - its fsck finds nothing in what it wrote or took;
//! - its hold is the one its row declares;
//! - `append_each` writes what one `append` per event writes, twins included.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use prodrome_core::declared::Declared;
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use prodrome_core::store::{EventStore, Replica as _};
use suzatary_digest::secretary::digest_event::DigestEvent;
use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::{InboxEvent, Said as Inboxed};
use suzatary_inbox::secretary::responded::{Responded, Tried};
use suzatary_io_chains::objects::Objects;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_memory::secretary::memory::{FactId, Kind as FactKind, Origin};
use suzatary_memory::secretary::memory_event::{MemoryEvent, forgotten, remembered};
use suzatary_memory::secretary::salted_text::TextName;
use suzatary_record::record::{MarkupSource, TodoEvent};
use suzatary_store::store::Reads as _;
use suzatary_store::store::{Hold, Prodrome};
use suzatary_stores::stores::{Kind, Store};
use suzatary_todo::decisions::{DecisionEvent, DecisionId, Statement, decided, revised, withdrawn};
use suzatary_topics::secretary::topics::reading::AliasEvent;
use suzatary_topics::secretary::topics::reading::MembershipEvent;

fn pins(store: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/stores-changes")
        .join(store)
}

fn copy(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("a directory") {
        let path = entry.expect("an entry").path();
        let into = to.join(path.file_name().expect("a name"));
        if path.is_dir() {
            std::fs::create_dir_all(&into).expect("a directory");
            copy(&path, &into);
        } else {
            std::fs::copy(&path, &into).expect("a copy");
        }
    }
}

fn laws<E: Store>(pinned: &str, open: impl Fn(PathBuf) -> Objects<E>) -> Kind {
    let dir = suzatary_testkit::scratch("store-laws");
    let source_at = dir.path().join("source");
    std::fs::create_dir_all(&source_at).expect("a directory");
    copy(&pins(pinned), &source_at);
    let source = open(source_at.clone());
    let objects = source.objects().expect("reads");
    assert!(objects.len() > 1, "{pinned} pins a genesis and its writes");
    let prints: Vec<(Hash, String)> = objects
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();

    let names: Vec<Hash> = prints.iter().map(|(name, _)| name.clone()).collect();
    let absent = Hash::new("0".repeat(64)).expect("a name");
    let read = source
        .literals(&[names.clone(), vec![absent]].concat())
        .expect("reads");
    let (held, unheld) = read.split_at(names.len());
    assert_eq!(
        held.to_vec(),
        prints
            .iter()
            .map(|(_, print)| Some(print.clone()))
            .collect::<Vec<_>>(),
        "{pinned}: each object reads back as its print"
    );
    assert_eq!(unheld, [None], "{pinned}: a name not held reads as absent");

    let copy = open(dir.path().join("copy"));
    let taken: BTreeSet<Hash> = copy.accept(&prints).expect("takes").into_iter().collect();
    assert_eq!(
        taken,
        names.iter().cloned().collect(),
        "{pinned}: takes them whole"
    );
    assert!(
        copy.accept(&prints).expect("takes").is_empty(),
        "{pinned}: taking twice is taking once"
    );
    assert_eq!(copy.tips().expect("reads"), source.tips().expect("reads"));
    assert_eq!(source.verify(), Vec::<String>::new(), "{pinned}");
    assert_eq!(copy.verify(), Vec::<String>::new(), "{pinned}");

    let held = copy.locked().expect("holds");
    match E::HOLD {
        Hold::File(name) => assert!(
            dir.path().join("copy").join(name).is_file(),
            "{pinned}: held by {name}"
        ),
        Hold::Reentrant => assert!(
            copy.locked().is_ok(),
            "{pinned}: one thread takes its hold again"
        ),
    }
    drop(held);

    appends_each_as_one_by_one(pinned, &open, &objects, &prints, dir.path());

    let fresh = open(dir.path().join("fresh"));
    fresh.begin().expect("begins");
    let begun = fresh.objects().expect("reads");
    assert!(
        matches!(begun.as_slice(), [(_, Envelope::Genesis(genesis))] if genesis.label == E::LABEL),
        "{pinned}: begun as {}",
        E::LABEL
    );
    E::KIND
}

/// Replayed twice so the second pass is all twins.
fn appends_each_as_one_by_one<E: Store>(
    pinned: &str,
    open: impl Fn(PathBuf) -> Objects<E>,
    objects: &[(Hash, Envelope<E>)],
    prints: &[(Hash, String)],
    dir: &Path,
) {
    let genesis: Vec<(Hash, String)> = prints
        .iter()
        .filter(|(name, _)| {
            matches!(
                objects.iter().find(|(n, _)| n == name),
                Some((_, Envelope::Genesis(_)))
            )
        })
        .cloned()
        .collect();
    let events: Vec<E> = objects
        .iter()
        .filter_map(|(_, o)| o.event().cloned())
        .collect();
    let (each, single) = (open(dir.join("each")), open(dir.join("single")));
    each.accept(&genesis).expect("takes the genesis");
    single.accept(&genesis).expect("takes the genesis");
    let one_by_one: Vec<Hash> = events
        .iter()
        .chain(&events)
        .map(|event| single.append(event.clone()).expect("appends"))
        .collect();
    let mut twice = events.iter().chain(&events).cloned();
    let together = each.append_each(&mut || Ok(twice.next())).expect("appends");
    assert_eq!(
        together, one_by_one,
        "{pinned}: append_each answers what append answers"
    );
    let on_disk = |at: &str| {
        let reopened = open(dir.join(at));
        let objects: Vec<(Hash, String)> = reopened
            .objects()
            .expect("reads")
            .iter()
            .map(|(name, object)| (name.clone(), canonical_envelope(object)))
            .collect();
        (objects, reopened.tips().expect("reads"))
    };
    assert_eq!(
        on_disk("each"),
        on_disk("single"),
        "{pinned}: append_each writes to disk what append writes"
    );
}

fn untrusted() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("an actor")])
}

#[test]
fn every_prodrome_keeps_the_store_laws() {
    let owners = || [Actor::new("ada").expect("an actor")].into();
    let covered: BTreeSet<Kind> = [
        laws::<TodoEvent>("events", |root| {
            Objects::chain(EventStore::new(root, untrusted()))
        }),
        laws::<InboxEvent>("inbox", Objects::open),
        laws::<LineEvent>("secretary-lines", Objects::open),
        laws::<MemoryEvent>("memory", Objects::open),
        laws::<DigestEvent>("digests", |root| Objects::digests(root, owners())),
        laws::<DecisionEvent>("decisions", Objects::open),
        laws::<MembershipEvent>("secretary-topics", Objects::open),
        laws::<AliasEvent>("secretary-topic-aliases", Objects::open),
        laws::<suzatary_stores::asks::AskEvent>("secretary-asks", Objects::open),
        laws::<Declared>("cache/plugins/ics/store", |root| {
            let pinned =
                suzatary_io_chains::plugin_store::PluginStore::at(pins("cache/plugins/ics/store"))
                    .read()
                    .expect("reads")
                    .expect("a store");
            Objects::plugin(root, pinned.inner().schema().clone())
        }),
    ]
    .into();
    let prodromes: BTreeSet<Kind> = Kind::ALL
        .iter()
        .copied()
        .filter(|kind| kind.holds_objects())
        .collect();
    assert_eq!(covered, prodromes);
}

/// Each inflationary register's reading, by the names of its writes, of every entity: its
/// frontier may move under a received write, and its reading may not.
fn readings<E: Store>(store: &Objects<E>) -> Vec<Vec<Hash>> {
    use prodrome_core::fold::Product as _;
    let nodes = store.nodes().expect("reads");
    prodrome_core::registers::fold(&nodes)
        .entities()
        .flat_map(|(_, stream)| {
            let read = prodrome_core::fold::read(stream, None, &prodrome_core::policy::Everything);
            read.climbing()
                .into_iter()
                .map(|register| {
                    read.reading(register)
                        .iter()
                        .map(|s| s.name.clone())
                        .collect()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// `honest` appended, then `below` built by hand as a change resting on every tip and RECEIVED,
/// as a store takes another's files: the reading does not move, and `verify` names the write.
fn received_below<E: Store<Vocabulary: Default>>(store: &str, honest: Vec<E>, below: E) {
    let dir = suzatary_testkit::scratch("received-below");
    let objects = Objects::<E>::open(dir.path().join(store));
    objects.begin().expect("begins");
    for event in honest {
        objects.append(event).expect("an honest write");
    }
    assert!(
        objects.append(below.clone()).is_err(),
        "{store}: the append refuses the write below its reading"
    );
    let before = readings(&objects);
    let genesis = objects
        .objects()
        .expect("reads")
        .into_iter()
        .find(|(_, object)| matches!(object, Envelope::Genesis(_)))
        .map(|(name, _)| name)
        .expect("a genesis");
    let change = Envelope::Change(
        prodrome_core::change::mk_change(genesis, objects.tips().expect("tips"), below)
            .expect("a change"),
    );
    let name = seal_hash(&change);
    objects
        .accept(&[(name.clone(), canonical_envelope(&change))])
        .expect("taken in");
    assert_eq!(
        readings(&objects),
        before,
        "{store}: the reading did not fall"
    );
    let found = objects.verify();
    assert!(
        matches!(found.as_slice(), [only] if only.contains(name.as_str())),
        "{store}: verify names the write below, and nothing else: {found:?}"
    );
}

/// Law 32 for every inflationary register this application keeps: the inbox's state, the
/// memory's life and the decisions' life read the greatest of all their writes.
#[test]
fn a_write_below_its_reading_received_neither_lowers_it_nor_passes_verify() {
    let at = |minute| Datetime::new(2026, 10, 1, 9, minute, 0, 0).expect("an instant");
    let ada = || Actor::new("ada").expect("an actor");

    let proposal = ProposalId::new("p0").expect("a proposal id");
    let inboxed = |minute, said| InboxEvent {
        proposal: proposal.clone(),
        at: at(minute),
        actor: ada(),
        said,
    };
    received_below(
        "inbox",
        vec![
            inboxed(0, Inboxed::Asked(Box::new(suzatary_inbox::drawn::asked()))),
            inboxed(
                1,
                Inboxed::Responded(Responded::Dismissed {
                    reason: "not this one".to_owned(),
                }),
            ),
        ],
        inboxed(
            2,
            Inboxed::Responded(Responded::Tried {
                attempt: 1,
                tried: Tried::NotSent {
                    reason: "no grant".to_owned(),
                },
            }),
        ),
    );

    let fact = FactId(1);
    let remembering = |minute| {
        remembered(
            fact,
            at(minute),
            ada(),
            TextName::new(format!("{:064x}", 1)).expect("a text name"),
            FactKind::Person,
            Origin::Turn {
                conversation: suzatary_base::conversation_id::ConversationId::new("default")
                    .expect("a conversation"),
                turn: 0,
            },
        )
        .expect("a memory event")
    };
    received_below(
        "memory",
        vec![
            remembering(0),
            forgotten(fact, at(1), ada(), 2).expect("a forgetting"),
        ],
        remembering(2),
    );

    let decision = DecisionId::new("onewriter").expect("a decision id");
    let statement = || {
        Statement::new("One writer", MarkupSource::escaped("What it says."), "Why.")
            .expect("a statement")
    };
    received_below(
        "decisions",
        vec![
            decided(decision.clone(), at(0), ada(), statement(), BTreeSet::new())
                .expect("a decision"),
            withdrawn(decision.clone(), at(1), ada(), "It no longer holds.").expect("a withdrawal"),
        ],
        revised(decision, at(2), ada(), statement(), BTreeSet::new()).expect("a revision"),
    );
}
