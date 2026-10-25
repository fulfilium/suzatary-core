//! Stores written before change identity, read by this build.
//!
//! `tests/fixtures/legacy/` is the e2e fixture's chains and replica (`suzatary-dev
//! fixture-chain --propose --reviewed --settings`) and one store with two content conflicts and
//! a state conflict, all written by the core that sealed every append on the tips. They are never
//! regenerated: `legacy/readings.txt` is what that core read out of them, and every line of it
//! that is not a conflict must read the same forever.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use prodrome_core::event::{Actor, Hash, TodoEvent, mk_completed, mk_reopened};
use prodrome_core::literal::Datetime;
use prodrome_core::payload::Payload;
use prodrome_core::policy::Untrusted;
use prodrome_core::schema::Schema;
use suzatary_core::record::{EventStore, Record};
use suzatary_core::store::Prodrome;
use suzatary_core::store::Reads as _;
use suzatary_io::objects::Objects;

/// The chains and the agent's replica.
const CHAINS: [&str; 4] = ["events", "events-suzatary", "events-triage", "conflicted"];

fn legacy() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy")
}

fn policy() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("an actor")])
}

fn chain(root: &Path) -> Objects<TodoEvent<Record>> {
    Objects::chain(EventStore::new(root.to_path_buf(), policy()))
}

/// A day after the store's last event, so every reading is of the whole store.
fn after<E: Schema>(store: &dyn Prodrome<E>) -> Datetime {
    let last = store
        .events()
        .expect("reads")
        .iter()
        .map(E::at)
        .max()
        .expect("an event");
    let day = prodrome_core::fpl::instant_of(last) + chrono::Duration::days(1);
    prodrome_core::fpl::datetime_of(day).expect("an instant")
}

fn names(names: &[Hash]) -> String {
    names
        .iter()
        .map(|name| &name.as_str()[..12])
        .collect::<Vec<_>>()
        .join(",")
}

/// Everything a reader takes from one store: its shape, its findings, and every entry.
fn reading<P: Payload>(
    label: &str,
    store: &dyn Prodrome<TodoEvent<P>>,
    untrusted: &Untrusted,
    at: Datetime,
) -> String {
    let mut out = String::new();
    let objects = store.objects().expect("reads");
    let order: Vec<Hash> = objects.iter().map(|(name, _)| name.clone()).collect();
    writeln!(out, "## {label}").expect("a string");
    writeln!(out, "order {}", names(&order)).expect("a string");
    writeln!(out, "tips {}", names(&store.tips().expect("tips"))).expect("a string");
    for finding in store.verify() {
        writeln!(out, "finding {finding}").expect("a string");
    }
    let nodes = store.nodes().expect("nodes");
    for entry in prodrome_core::view::entries(&nodes, at, untrusted).expect("entries") {
        let value = match entry.value() {
            Ok(Some(value)) => format!("{value:.9}"),
            Ok(None) => "absent".to_owned(),
            Err(_) => "unlinked".to_owned(),
        };
        let conflicts: Vec<String> = entry
            .conflicts
            .iter()
            .map(|(kind, writes)| format!("{}:{}", kind.as_str(), names(writes)))
            .collect();
        writeln!(
            out,
            "entry {} state={} at={} claim={} value={value} content={} conflicts={} confidence={:?}",
            entry.key.as_str(),
            entry.state(),
            entry.at(),
            entry.claimed(),
            names(entry.content()),
            conflicts.join(";"),
            entry.confidence,
        )
        .expect("a string");
    }
    out
}

/// Every store's reading, read from a copy so nothing touches the fixture.
fn readings(root: &Path) -> String {
    let mut out = String::new();
    for name in CHAINS {
        let store = chain(&root.join(name));
        out += &reading(name, &store, &policy(), after(&store));
    }
    out
}

fn copy(from: &Path, to: &Path) {
    for entry in walkdir(from) {
        let target = to.join(entry.strip_prefix(from).expect("under the root"));
        std::fs::create_dir_all(target.parent().expect("a parent")).expect("a directory");
        std::fs::copy(&entry, &target).expect("a copy");
    }
}

fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            found.extend(walkdir(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
}

/// Every object file under `root`, with its bytes.
fn objects(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    walkdir(root)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "py"))
        .map(|path| {
            let bytes = std::fs::read(&path).expect("an object");
            (path.strip_prefix(root).expect("under").to_path_buf(), bytes)
        })
        .collect()
}

#[test]
fn every_legacy_store_reads_as_it_did() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    copy(&legacy(), dir.path());
    let found = readings(dir.path());
    let pinned = std::fs::read_to_string(legacy().join("readings.txt")).expect("the pin");
    assert!(
        found == pinned,
        "the legacy stores read differently:\n{found}"
    );
}

/// What ordinary writes put on a legacy chain, as bytes, and that nothing already stored moves.
#[test]
fn writes_on_a_legacy_chain() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    copy(&legacy(), dir.path());
    let root = dir.path().join("events");
    let before = objects(&root);
    let store = chain(&root);
    let at = after(&store);
    let instant = Datetime::new(2026, 9, 30, 12, 0, 0, 0).expect("an instant");
    // A todo whose state was never written, then one whose completion is a legacy object. Each
    // change rests on its todo's legacy heads (the first on its `Authored`), and each is a head
    // beside the chain's tip, which nothing joins.
    let done = mk_completed("file-permit", instant, "ada", "").expect("an event");
    let reopened = mk_reopened("renew-card", instant, "ada", "").expect("an event");
    let mut written = Vec::new();
    for event in [done.clone(), reopened] {
        let name = store.append(event).expect("appends");
        assert!(store.tips().expect("tips").contains(&name), "a head");
        written.push(name);
    }
    assert_eq!(
        store.append(done).expect("a replay answers"),
        written[0],
        "a replay is the object already written"
    );

    let after = objects(&root);
    for (path, bytes) in &before {
        assert!(
            after.iter().any(|(held, now)| held == path && now == bytes),
            "{} moved",
            path.display()
        );
    }
    let fresh: Vec<&(PathBuf, Vec<u8>)> = after
        .iter()
        .filter(|(path, _)| !before.iter().any(|(held, _)| held == path))
        .collect();
    assert_eq!(fresh.len(), 2, "two changes, the replay nothing");
    let mut shown = String::new();
    for name in &written {
        let (_, bytes) = fresh
            .iter()
            .find(|(path, _)| path.ends_with(format!("{}.py", name.as_str())))
            .expect("a written object");
        let print = std::str::from_utf8(bytes).expect("text");
        writeln!(shown, "{}\n{print}", name.as_str()).expect("a string");
    }
    let pinned = std::fs::read_to_string(legacy().join("appended.txt")).expect("the pin");
    assert_eq!(shown, pinned, "the objects two writes add, in order");
    assert!(store.verify().is_empty(), "{:?}", store.verify());

    // Every other todo reads as it did.
    let untouched = reading("events", &chain(&root), &policy(), at);
    let pinned = std::fs::read_to_string(legacy().join("readings.txt")).expect("the pin");
    for line in untouched.lines().filter(|line| {
        line.starts_with("entry ")
            && !line.starts_with("entry file-permit ")
            && !line.starts_with("entry renew-card ")
    }) {
        assert!(pinned.contains(line), "moved after the writes: {line}");
    }
}
