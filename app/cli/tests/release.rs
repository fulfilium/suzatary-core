//! `suzatary release` through the real binary, against a fixture project chain and a scratch
//! checkout: the reading, the cut that writes the release file and the changelog,
//! and the check a land asks before it tags.

mod common;

use std::path::{Path, PathBuf};

use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use suzatary_core::record::{EventStore, GENESIS, MarkupSource, TodoEvent, mk_authored};

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 9, 0, 0, 0).expect("a real instant")
}

/// The items `suzatarytwo` composes here: three done, one cancelled, two open.
const ITEMS: [(&str, &str); 6] = [
    ("prodromemain", "done"),
    ("typedverbs", "done"),
    ("wireonce", "done"),
    ("instancesplit", "cancelled"),
    ("portsuites", "open"),
    ("calendarlinks", "open"),
];

fn authored(id: &str, body: &str, spec: prodrome_core::term::Term) -> TodoEvent {
    mk_authored(
        id,
        at(1),
        "ada",
        "todo",
        at(1),
        MarkupSource::escaped(body),
        spec,
        vec!["the fixture's".to_owned()],
        "project",
        "",
        MarkupSource::empty(),
        None,
        Vec::new(),
        Vec::new(),
        "",
    )
    .expect("a record")
}

/// The project chain under `root`: the release, its items, and the lifecycle above.
fn a_project_chain(root: &Path) {
    let store = EventStore::new(
        root.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    );
    let append = |event| {
        suzatary_core::store::append_to(&store, GENESIS, event).expect("appends");
    };
    let mut terms = vec![fpl::mk_flat(0.6).expect("a term")];
    terms.extend(
        ITEMS
            .iter()
            .map(|(id, _)| fpl::mk_ref((*id).to_owned()).expect("a ref")),
    );
    append(authored(
        "suzatarytwo",
        "Suzatary 2.0",
        fpl::mk_conj(terms, -4.0).expect("a conjunction"),
    ));
    for (id, lot) in ITEMS {
        append(authored(
            id,
            &format!("the {id} item"),
            fpl::mk_flat(0.7).expect("a term"),
        ));
        let settled = match lot {
            "done" => Some(prodrome_core::event::mk_completed(id, at(2), "ada", "")),
            "cancelled" => Some(prodrome_core::event::mk_cancelled(id, at(2), "ada", "")),
            _ => None,
        };
        if let Some(event) = settled {
            append(event.expect("an event"));
        }
    }
}

fn an_instance(root: &Path) -> PathBuf {
    let path = root.join("instance.toml");
    std::fs::write(
        &path,
        format!(
            r#"
repo = "{repo}"

[signatures]
root = "{key}"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["assistant", "ada", "triage"]
owner = "ada"

[[chains]]
name = "suzatary"
events = "events-suzatary"
untrusted = ["triage"]
actors = ["assistant", "ada", "triage"]
owner = "ada"

[web]
viewers = ["someone@example.com"]
app_url = "https://example.invalid"

[web.act_as]
"someone@example.com" = "ada"

[web.serving]
host = "127.0.0.1"
port = 8790
tls_dir = "tls"
dist = "dist"
briefings = "briefings"

[categories]
project = 0
other = 1

[mail]
root = "mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
heartbeat = "triage-heartbeat"

[[mail.accounts]]
email = "someone@example.com"
subdir = "someone"
sync = "gmail"
"#,
            repo = root.display(),
            key = suzatary_core::cast::keys::root().as_str(),
        ),
    )
    .expect("an instance file");
    path
}

/// A checkout at 1.3.0 with one entry unreleased.
fn a_checkout(root: &Path) -> PathBuf {
    let tree = root.join("tree");
    std::fs::create_dir_all(tree.join("app/core")).expect("a tree");
    std::fs::write(tree.join("app/release.toml"), "version = \"1.3.0\"\n").expect("a release");
    std::fs::write(
        tree.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n### Added\n- a calendar link\n\n## [1.3.0] - 2026-09-01\n\n### Added\n- the first\n",
    )
    .expect("a changelog");
    std::fs::write(
        tree.join("app/core/releases.toml"),
        "chain = \"suzatary\"\nbase = 1\n\n[[release]]\ntodo = \"suzatarytwo\"\nmajor = 2\nminor = 0\n",
    )
    .expect("a plan");
    tree
}

fn suzatary(instance: &Path, args: &[&str]) -> std::process::Output {
    crate::binary::keyed()
        .args(["--instance", &instance.to_string_lossy()])
        .args(args)
        .output()
        .expect("the binary runs")
}

fn ok(out: &std::process::Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("JSON on stdout")
}

#[test]
fn a_release_is_cut_from_the_chain_and_checked_against_it() {
    let root = common::scratch("release-cut");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    let tree_arg = tree.to_string_lossy().into_owned();

    let read = ok(&suzatary(&instance, &["release", "read", "--json"]));
    assert_eq!(read["reads"], "1.0", "no release is completed");
    assert_eq!(read["releases"][0]["todo"], "suzatarytwo");
    assert_eq!(read["releases"][0]["version"], "2.0");
    assert_eq!(read["releases"][0]["place"], "next");

    let cut = ok(&suzatary(
        &instance,
        &[
            "release",
            "cut",
            "--completes",
            "suzatarytwo",
            "--completes",
            "calendarlinks",
            "--tree",
            &tree_arg,
        ],
    ));
    assert_eq!(cut["version"], "2.0.0");
    assert_eq!(cut["completes"][1]["title"], "the calendarlinks item");
    let changelog = std::fs::read_to_string(tree.join("CHANGELOG.md")).expect("a changelog");
    assert!(
        changelog.contains(
            "## [Unreleased]\n\n## [2.0.0] - 20"
        ) && changelog.contains(
            "\n- Completes `todo:suzatary/calendarlinks`: the calendarlinks item\n\n### Added\n- a calendar link\n"
        ),
        "{changelog}"
    );
    let released =
        std::fs::read_to_string(tree.join("app/release.toml")).expect("the release file");
    assert!(
        released.contains("version = \"2.0.0\"\n")
            && released.contains("completes = [\"suzatarytwo\", \"calendarlinks\"]")
            && !released.contains("the calendarlinks item"),
        "{released}"
    );

    // Land checks before it records: the chain with the tree's completions done reads 2.0.
    let before = ok(&suzatary(
        &instance,
        &["release", "check", "--tree", &tree_arg],
    ));
    assert_eq!(before["version"], "2.0.0");

    ok_status(&suzatary(
        &instance,
        &[
            "todo",
            "--chain",
            "suzatary",
            "done",
            "suzatarytwo",
            "--actor",
            "assistant",
            "--note",
            "PR #7",
        ],
    ));
    let checked = ok(&suzatary(
        &instance,
        &["release", "check", "--tree", &tree_arg],
    ));
    assert_eq!(checked["version"], "2.0.0");

    // The next train completes no release: the next patch after the last release, which the
    // train reads off origin's newest `Release:` trailer.
    let again = ok(&suzatary(
        &instance,
        &["release", "cut", "--last", "2.0.0", "--tree", &tree_arg],
    ));
    assert_eq!(again["version"], "2.0.1");
}

#[test]
fn a_cut_names_only_todos_the_chain_holds() {
    let root = common::scratch("release-unknown");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    let out = suzatary(
        &instance,
        &[
            "release",
            "cut",
            "--completes",
            "nosuchtodo",
            "--tree",
            &tree.to_string_lossy(),
        ],
    );
    assert!(!out.status.success(), "an unknown todo was cut");
    let released = std::fs::read_to_string(tree.join("app/release.toml")).expect("a release");
    assert!(released.contains("1.3.0"), "a refused cut wrote the tree");
}

/// A checkout ahead of the chain is refused: a version never goes down.
#[test]
fn a_cut_below_the_tree_is_refused() {
    let root = common::scratch("release-behind");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    std::fs::write(tree.join("app/release.toml"), "version = \"1.9.0\"\n").expect("a release");
    let out = suzatary(
        &instance,
        &[
            "release",
            "cut",
            "--last",
            "1.9.0",
            "--tree",
            &tree.to_string_lossy(),
        ],
    );
    assert!(!out.status.success(), "the version went down");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("never goes down"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn ok_status(out: &std::process::Output) {
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `known` names every todo the chain does not hold, and passes those it does: what a train
/// asks of one PR's `Completes:` line.
#[test]
fn known_names_what_the_chain_does_not_hold() {
    let root = common::scratch("release-known");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    let tree = tree.to_string_lossy().into_owned();
    ok_status(&suzatary(
        &instance,
        &[
            "release",
            "known",
            "--tree",
            &tree,
            "calendarlinks",
            "portsuites",
        ],
    ));
    let out = suzatary(
        &instance,
        &[
            "release",
            "known",
            "--tree",
            &tree,
            "calendarlinks",
            "nosuch",
            "Bad Id",
        ],
    );
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("nosuch") && said.contains("Bad Id"), "{said}");
}

/// A checkout whose version no tag names is cut from the tag: the hand-set number does not
/// block the count, and its section joins the release.
#[test]
fn a_cut_from_the_tag_folds_an_untagged_version() {
    let root = common::scratch("release-untagged");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    std::fs::write(tree.join("app/release.toml"), "version = \"1.9.0\"\n").expect("a release");
    std::fs::write(
        tree.join("CHANGELOG.md"),
        "# Changelog\n\n## [Unreleased]\n\n## [1.9.0] - 2026-10-12\n\n- hand-set\n\n## [0.17.0] - 2026-09-29\n\n- old\n",
    )
    .expect("a changelog");
    let cut = ok(&suzatary(
        &instance,
        &[
            "release",
            "cut",
            "--last",
            "0.17.0",
            "--tree",
            &tree.to_string_lossy(),
        ],
    ));
    assert_eq!(cut["version"], "1.0.0");
    let changelog = std::fs::read_to_string(tree.join("CHANGELOG.md")).expect("a changelog");
    assert!(!changelog.contains("## [1.9.0]"), "{changelog}");
    assert!(changelog.contains("## [1.0.0] - "), "{changelog}");
    assert!(changelog.contains("- hand-set"), "{changelog}");
}

/// A release completed by hand since the cut is the next train's: the chain reads above the
/// cut, and the check passes. A tree that claims more than the chain holds is refused.
#[test]
fn a_check_wants_the_chain_at_least_at_the_cut() {
    let root = common::scratch("release-at-least");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    let tree_arg = tree.to_string_lossy().into_owned();
    ok(&suzatary(
        &instance,
        &[
            "release",
            "cut",
            "--completes",
            "calendarlinks",
            "--tree",
            &tree_arg,
        ],
    ));
    ok_status(&suzatary(
        &instance,
        &[
            "todo",
            "--chain",
            "suzatary",
            "done",
            "suzatarytwo",
            "--actor",
            "ada",
        ],
    ));
    let above = ok(&suzatary(
        &instance,
        &["release", "check", "--tree", &tree_arg],
    ));
    assert_eq!(above["version"], "1.0.0", "the tag is the cut");

    std::fs::write(tree.join("app/release.toml"), "version = \"2.1.0\"\n").expect("a release");
    let below = suzatary(&instance, &["release", "check", "--tree", &tree_arg]);
    assert!(!below.status.success(), "a tree above the chain passed");
    assert!(
        String::from_utf8_lossy(&below.stderr).contains("below the tree's 2.1.0"),
        "stderr: {}",
        String::from_utf8_lossy(&below.stderr)
    );
}

/// `release items` attaches and detaches a release's todos in one trusted revision; a detached
/// item reads as dropped.
#[test]
fn items_rescope_a_release() {
    let root = common::scratch("release-items");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let rescoped = ok(&suzatary(
        &instance,
        &[
            "release",
            "items",
            "suzatarytwo",
            "--remove",
            "wireonce",
            "--remove",
            "portsuites",
            "--actor",
            "ada",
        ],
    ));
    let named = |items: &serde_json::Value, todo: &str| {
        items
            .as_array()
            .and_then(|items| items.iter().find(|i| i["todo"] == todo))
            .map(|i| i["named"].clone())
    };
    assert_eq!(
        named(&rescoped["items"], "wireonce"),
        Some(serde_json::json!("dropped"))
    );
    assert_eq!(
        named(&rescoped["items"], "typedverbs"),
        Some(serde_json::json!("current"))
    );

    let back = ok(&suzatary(
        &instance,
        &[
            "release",
            "items",
            "suzatarytwo",
            "--add",
            "portsuites",
            "--actor",
            "ada",
        ],
    ));
    assert_eq!(
        named(&back["items"], "portsuites"),
        Some(serde_json::json!("current"))
    );

    for refused in [
        &["release", "items", "suzatarytwo", "--add", "nosuchtodo"][..],
        &["release", "items", "portsuites", "--add", "calendarlinks"][..],
        &["release", "items", "suzatarytwo", "--remove", "wireonce"][..],
        &["release", "items", "suzatarytwo"][..],
    ] {
        assert!(
            !suzatary(&instance, refused).status.success(),
            "{refused:?}"
        );
    }
}

/// An unknown todo exits 3, so a train skips the PR; any other failure exits 1, so it stops.
#[test]
fn known_tells_an_unknown_todo_from_a_failure() {
    let root = common::scratch("release-known-codes");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    let tree = tree.to_string_lossy().into_owned();
    let unknown = suzatary(
        &instance,
        &["release", "known", "--tree", &tree, "nosuchtodo"],
    );
    assert_eq!(unknown.status.code(), Some(3));
    let broken = suzatary(
        &instance,
        &[
            "release",
            "known",
            "--tree",
            "/nonexistent",
            "calendarlinks",
        ],
    );
    assert_eq!(broken.status.code(), Some(1));
}

/// `release record` writes each of the tree's completed todos the chain lacks, with its note,
/// and nothing the second time.
#[test]
fn record_is_idempotent() {
    let root = common::scratch("release-record");
    a_project_chain(&root);
    let instance = an_instance(&root);
    let tree = a_checkout(&root);
    let tree_arg = tree.to_string_lossy().into_owned();
    ok(&suzatary(
        &instance,
        &[
            "release",
            "cut",
            "--completes",
            "calendarlinks",
            "--completes",
            "wireonce",
            "--tree",
            &tree_arg,
        ],
    ));
    let first = ok(&suzatary(
        &instance,
        &[
            "release",
            "record",
            "--tree",
            &tree_arg,
            "--note",
            "calendarlinks=PR #7",
        ],
    ));
    assert_eq!(
        first["recorded"],
        serde_json::json!(["calendarlinks"]),
        "wireonce was done already"
    );
    let again = ok(&suzatary(
        &instance,
        &["release", "record", "--tree", &tree_arg],
    ));
    assert_eq!(again["recorded"], serde_json::json!([]));
    let events = EventStore::new(
        root.join("events-suzatary"),
        prodrome_core::policy::Untrusted::none(),
    )
    .events()
    .expect("the chain reads");
    let notes: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            TodoEvent::Completed(done) if done.todo.as_str() == "calendarlinks" => {
                Some(done.note.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(notes, vec!["PR #7"]);
}

/// The binary copied into the layout the flake's package installs, `<out>/bin/suzatary` beside
/// `<out>/share/suzatary/release.toml` holding `release` (none when `None`): what `--version`
/// then prints.
fn version_installed(name: &str, release: Option<&str>) -> String {
    let out =
        std::env::temp_dir().join(format!("suzatary-installed-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(out.join("bin")).expect("a bin dir");
    let exe = out.join("bin/suzatary");
    suzatary_testkit::copy_program(Path::new(&crate::binary::path()), &exe);
    if let Some(text) = release {
        let file = out.join(suzatary_cli::stamp::BESIDE);
        std::fs::create_dir_all(file.parent().expect("a share dir")).expect("a share dir");
        std::fs::write(&file, text).expect("the release file");
    }
    let ran = crate::binary::run(&exe)
        .arg("--version")
        .output()
        .expect("the copy runs");
    assert!(ran.status.success(), "{ran:?}");
    String::from_utf8(ran.stdout).expect("utf-8")
}

/// A release is data installed beside the binary, not compiled into it: one binary, copied into
/// two packages with two release files, prints each package's release, and with none prints
/// `dev`. `GET /api/about` asks the same reader (`stamp::release`).
#[test]
fn the_release_is_read_from_the_file_beside_the_binary() {
    assert_eq!(
        version_installed("one", Some("version = \"3.1.4\"\n")),
        "suzatary 3.1.4\n"
    );
    assert_eq!(
        version_installed(
            "two",
            Some("version = \"3.2.0\"\ncompletes = [\"example\"]\n")
        ),
        "suzatary 3.2.0\n"
    );
    assert_eq!(version_installed("none", None), "suzatary dev\n");
    assert!(
        version_installed("bad", Some("version = \"not one\"\n"))
            .starts_with("suzatary unreadable"),
        "a release file that holds no release is named, never read as dev"
    );
}
