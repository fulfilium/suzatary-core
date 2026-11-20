//! A followed store through the real binary: `follow fetch` copies it, the one list reads it and
//! a wait from a private chain raises its task, and every command that walks the chains (adoption,
//! an append aimed at it, the release plan, the public push's listing) leaves its copy
//! byte-identical; and what `todos open` answers for a store it could not read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use suzatary_record::record::{EventStore, MarkupSource, mk_authored};

use super::todos::{a_chain, a_signing_instance, scratch, signing};

const OWN: &str = "9999999999999999999999999999999999999999999999999999999999999999";
const AGENT: &str = "8888888888888888888888888888888888888888888888888888888888888888";

fn seeds() -> [(&'static str, String); 3] {
    [
        (suzatary_signatures::signatures::BOX_KEY, OWN.to_owned()),
        (suzatary_signatures::signatures::AGENT_KEY, AGENT.to_owned()),
        (suzatary_mail::mail::dedup_key::DEDUP_KEY, "7".repeat(64)),
    ]
}

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 9, 0, 0, 0).expect("a real instant")
}

fn task(id: &str, price: f64, waiting_on: &str) -> suzatary_record::record::TodoEvent {
    mk_authored(
        id,
        at(1),
        "ada",
        "todo",
        at(1),
        MarkupSource::escaped(&format!("Do {id}")),
        fpl::mk_flat(price).expect("a flat"),
        vec![],
        "",
        waiting_on,
        MarkupSource::empty(),
        None,
        Vec::new(),
        Vec::new(),
        "",
    )
    .expect("a record")
}

fn git(dir: &Path, args: &[&str]) {
    let ran = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.test"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        ran.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// A repository whose `main` holds, in `roadmap/` beside two documents, a public store of one
/// task with a plan and a charter that a walk of the chains would read.
fn an_upstream(root: &Path) -> PathBuf {
    let upstream = root.join("upstream");
    let store = upstream.join("roadmap");
    let events = EventStore::new(store.clone(), prodrome_core::policy::Untrusted::none());
    suzatary_store::store::append_to(
        &events,
        suzatary_record::record::GENESIS,
        task("libtask", 0.9, ""),
    )
    .expect("appends");
    std::fs::write(store.join("charter.toml"), "public = true\n").expect("a charter");
    std::fs::write(store.join("releases.toml"), "not a plan\n").expect("a plan");
    std::fs::write(upstream.join("README.md"), "A library.\n").expect("a document");
    std::fs::write(upstream.join("PLAN.md"), "Its plan.\n").expect("a document");
    git(&upstream, &["init", "--quiet", "-b", "main"]);
    git(&upstream, &["add", "."]);
    git(&upstream, &["commit", "--quiet", "-m", "a store"]);
    upstream
}

fn json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("JSON")
}

/// Every file under `dir`, by its path, with its bytes.
fn bytes(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut walking = vec![dir.to_path_buf()];
    while let Some(here) = walking.pop() {
        for entry in std::fs::read_dir(&here).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walking.push(path);
            } else {
                found.insert(path.clone(), std::fs::read(&path).expect("a file"));
            }
        }
    }
    found
}

/// A box whose private chain `life` holds `ship`, waiting on the followed store's `libtask`, and
/// whose instance follows `library`, the `roadmap/` of the branch `main` of a repository beside
/// it named by a relative remote, which resolves against the instance file's directory.
fn a_box(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = scratch(name);
    a_chain(&root);
    let life = EventStore::new(
        root.join("events"),
        prodrome_core::policy::Untrusted::none(),
    );
    suzatary_store::store::append_to(
        &life,
        suzatary_record::record::GENESIS,
        task("ship", 0.2, "Ada, and todo:library/libtask"),
    )
    .expect("appends");
    let upstream = an_upstream(&root);
    let instance = a_signing_instance(
        &root,
        &prodrome_core::sign::Secret::new(OWN)
            .expect("a seed")
            .public(),
    );
    let text = format!(
        "{}\n[[follows]]\nname = \"library\"\nremote = \"upstream\"\nbranch = \"main\"\n\
         path = \"roadmap\"\n",
        std::fs::read_to_string(&instance).expect("the instance"),
    );
    std::fs::write(&instance, text).expect("the instance");
    (root, instance, upstream)
}

fn ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_followed_store_is_read_and_never_written() {
    let (root, instance, _) = a_box("follows");
    let run = |args: &[&str]| signing(&instance, args, &seeds());
    ok(&run(&["follow", "fetch"]));
    let copy = root.join("cache/follows/library");
    let before = bytes(&copy);

    let open = run(&["todos", "open", "--json"]);
    ok(&open);
    let printed = json(&open);
    assert_eq!(printed["follows"][0]["copy"]["tree"]["type"], "Confirmed");
    let library = printed["open"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["chain"] == "library")
        .expect("the followed store's task is listed");
    assert_eq!(library["id"], "libtask");
    assert_eq!(library["reading"]["type"], "Raised", "{library}");
    assert_eq!(library["reading"]["by"], "todo:life/ship");
    assert_eq!(printed["open"][0]["id"], "ship", "the most urgent first");
    ok(&run(&["todo", "--chain", "library", "show", "libtask"]));

    ok(&run(&["device", "init"]));
    let aimed = run(&[
        "todo",
        "--chain",
        "library",
        "add",
        "xyz",
        "--body",
        "Do x",
        "--rationale",
        "r",
        "--priority",
        "50",
    ]);
    assert!(
        String::from_utf8_lossy(&aimed.stderr).contains("no chain named \"library\""),
        "an append aimed at it is refused: {}",
        String::from_utf8_lossy(&aimed.stderr)
    );
    let plan = run(&["release", "read"]);
    assert!(
        String::from_utf8_lossy(&plan.stderr).contains("no chain of"),
        "the release plan does not read it: {}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let public = run(&["instance", "public"]);
    ok(&public);
    assert!(
        String::from_utf8_lossy(&public.stdout).trim().is_empty(),
        "the push lists it not"
    );
    ok(&run(&["todos", "open"]));
    assert!(bytes(&copy) == before, "the copy is byte-identical");
}

/// `todos open`'s answer: a followed store no fetch confirmed is reported and the list succeeds,
/// `--chain` naming it fails, and a store that is there and does not read fails the list. A
/// fetch that the upstream's rewritten history refuses leaves the copy as it was, and the copy
/// reads as stale.
#[test]
fn the_list_fails_for_a_store_it_could_not_read_and_says_a_refused_fetch() {
    let (root, instance, upstream) = a_box("follows-answer");
    let run = |args: &[&str]| signing(&instance, args, &seeds());
    let open = run(&["todos", "open", "--json"]);
    ok(&open);
    assert_eq!(json(&open)["follows"][0]["copy"]["tree"]["type"], "Never");
    assert_eq!(json(&open)["failed"], serde_json::json!([]));
    let absent = run(&["todos", "open", "--chain", "library", "--json"]);
    assert!(!absent.status.success());
    assert_eq!(json(&absent)["failed"][0]["chain"], "library");

    ok(&run(&["follow", "fetch"]));
    // What a fetch killed in its checkout leaves: git's lock, and a file that is not the commit's.
    let copy = root.join("cache/follows/library");
    let fetched = bytes(&copy.join("roadmap"));
    std::fs::write(copy.join(".git/index.lock"), "").expect("a lock");
    std::fs::write(copy.join("roadmap/charter.toml"), "half").expect("a half-written file");
    ok(&run(&["follow", "fetch"]));
    assert!(
        bytes(&copy.join("roadmap")) == fetched,
        "the next fetch puts the tree back"
    );
    git(
        &upstream,
        &["commit", "--quiet", "--amend", "-m", "rewritten"],
    );
    let store = root.join("cache/follows/library/roadmap");
    let before = bytes(&store);
    assert!(
        !run(&["follow", "fetch"]).status.success(),
        "a rewrite is refused"
    );
    assert!(bytes(&store) == before, "the copy stays as it was");
    let open = run(&["todos", "open", "--json"]);
    ok(&open);
    assert_eq!(
        json(&open)["follows"][0]["copy"]["tree"]["type"],
        "Confirmed"
    );
    assert_eq!(
        json(&open)["follows"][0]["copy"]["since"]["type"],
        "Refused"
    );

    let objects = root.join("events/objects");
    std::fs::rename(&objects, root.join("events/objects-moved")).expect("moved");
    std::fs::write(&objects, "not a directory").expect("a store that cannot be read");
    assert!(
        !run(&["todos", "open"]).status.success(),
        "a store that does not read"
    );
    let library = run(&["todos", "open", "--chain", "library"]);
    ok(&library);
    assert!(
        String::from_utf8_lossy(&library.stderr).contains("not read: the chain life"),
        "{}",
        String::from_utf8_lossy(&library.stderr)
    );
}

/// An upstream whose store's `objects` is a link to a directory of this machine: the copy holds
/// a plain file there, and the reading refuses the store rather than read through it.
#[test]
fn a_link_in_a_followed_store_reads_nothing_through_it() {
    let (root, instance, _) = a_box("follows-link");
    let upstream = root.join("upstream-link");
    std::fs::create_dir_all(upstream.join("roadmap")).expect("a directory");
    std::os::unix::fs::symlink(
        root.join("events/objects"),
        upstream.join("roadmap/objects"),
    )
    .expect("a link to this box's own chain");
    std::fs::write(upstream.join("README.md"), "A library.\n").expect("a document");
    git(&upstream, &["init", "--quiet", "-b", "main"]);
    git(&upstream, &["add", "."]);
    git(&upstream, &["commit", "--quiet", "-m", "a link"]);
    let text = format!(
        "{}\n[[follows]]\nname = \"linked\"\nremote = \"upstream-link\"\nbranch = \"main\"\n\
         path = \"roadmap\"\n",
        std::fs::read_to_string(&instance).expect("the instance"),
    );
    std::fs::write(&instance, text).expect("the instance");
    let run = |args: &[&str]| signing(&instance, args, &seeds());

    ok(&run(&["follow", "fetch", "linked"]));
    let objects = root.join("cache/follows/linked/roadmap/objects");
    assert!(
        !std::fs::symlink_metadata(&objects)
            .expect("checked out")
            .file_type()
            .is_symlink(),
        "a link checks out as a plain file"
    );
    let open = run(&["todos", "open", "--chain", "linked", "--json"]);
    assert!(
        !open.status.success(),
        "a store that is no directory does not read"
    );
    assert_eq!(json(&open)["open"], serde_json::json!([]));
    assert!(
        json(&open)["failed"][0]["why"]
            .as_str()
            .is_some_and(|why| why.contains("is not a directory inside the copy")),
        "{}",
        json(&open)
    );

    // A link inside a copy's `objects`, which no fetch of this build writes, is refused too.
    ok(&run(&["follow", "fetch", "library"]));
    let objects = root.join("cache/follows/library/roadmap/objects");
    std::os::unix::fs::symlink(
        root.join("events/.heads"),
        objects.join(format!("{}.py", "0".repeat(64))),
    )
    .expect("a link among the objects");
    let open = run(&["todos", "open", "--chain", "library", "--json"]);
    assert!(!open.status.success());
    assert!(
        json(&open)["failed"][0]["why"]
            .as_str()
            .is_some_and(|why| why.contains("is not a regular file inside the copy")),
        "{}",
        json(&open)
    );
}
