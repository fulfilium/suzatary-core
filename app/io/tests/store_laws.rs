//! The laws every prodrome keeps whatever its schema, checked for every prodrome row of
//! `suzatary_core::stores::Kind` over the objects its pins hold (`tests/stores-changes/`):
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
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope};
use prodrome_core::policy::Untrusted;
use prodrome_core::store::{EventStore, Replica as _};
use suzatary_core::decisions::DecisionEvent;
use suzatary_core::secretary::digest_event::DigestEvent;
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::topics::reading::AliasEvent;
use suzatary_core::secretary::topics::reading::MembershipEvent;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{Hold, Prodrome};
use suzatary_core::stores::{Files, Kind, Store};
use suzatary_io::objects::Objects;
use suzatary_record::record::TodoEvent;

fn pins(store: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/stores-changes")
        .join(store)
}

/// Every file under `from`, copied under `to`.
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

/// The laws, for the store `pinned` names, each copy of it opened by `open`; answers its row.
fn laws<E: Store>(pinned: &str, open: impl Fn(PathBuf) -> Objects<E>) -> Kind {
    let dir = tempfile::tempdir().expect("a scratch dir");
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

/// `append_each` over the pinned events, replayed twice so the second pass is all twins, answers
/// and writes to disk what one `append` per event does, over the same genesis.
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
        laws::<Declared>("cache/plugins/ics/store", |root| {
            let pinned =
                suzatary_io::plugin_store::PluginStore::at(pins("cache/plugins/ics/store"))
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
        .filter(|kind| matches!(kind.files(), Files::Chain | Files::Objects))
        .collect();
    assert_eq!(covered, prodromes);
}
