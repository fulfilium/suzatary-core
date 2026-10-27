//! The laws every prodrome keeps whatever its schema, checked for every prodrome row of
//! `suzatary_core::store::Kind` over the objects its pins hold (`tests/stores-changes/`):
//!
//! - a store begun holds one `Genesis`, labelled as its row says;
//! - each object reads back by its name as its canonical print, and a name it does not hold
//!   reads as absent;
//! - a fresh store takes a store's prints whole, and taking them again takes nothing;
//! - its fsck finds nothing in what it wrote or took;
//! - its hold is the one its row declares.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use prodrome_core::declared::Declared;
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope};
use prodrome_core::policy::Untrusted;
use prodrome_core::store::{EventStore, Replica as _};
use suzatary_core::decisions::DecisionEvent;
use suzatary_core::record::TodoEvent;
use suzatary_core::secretary::digest_event::DigestEvent;
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::topics::reading::AliasEvent;
use suzatary_core::secretary::topics::reading::MembershipEvent;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{Files, Hold, Kind, Prodrome, Store};
use suzatary_io::objects::Objects;

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
