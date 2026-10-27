//! `suzatary decision` against a scratch store: a decision recorded, superseded, withdrawn,
//! found by a word of its body and viewed, and each refusal a writer meets (markup the door
//! refuses, a link that would close a cycle, a restatement of a withdrawn decision), with the
//! store reading the same afterwards.

use std::path::Path;
use std::process::Output;

fn decision(store: &Path, args: &[&str]) -> Output {
    crate::binary::keyed()
        .arg("decision")
        .arg("--store")
        .arg(store)
        .args(args)
        .output()
        .expect("the binary runs")
}

fn ok(store: &Path, args: &[&str]) -> String {
    let out = decision(store, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

fn refused(store: &Path, args: &[&str]) -> String {
    let out = decision(store, args);
    assert!(!out.status.success(), "{args:?} was not refused");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn record<'a>(id: &'a str, title: &'a str, supersedes: &'a str) -> Vec<&'a str> {
    vec![
        "record",
        id,
        "--title",
        title,
        "--body",
        "We keep *one* store, and `decisions/` is it.",
        "--rationale",
        "One place to look.",
        "--supersedes",
        supersedes,
    ]
}

/// Markup the door refuses, a link that would close a cycle, an id decided twice.
fn the_writers_refusals(store: &Path) {
    let markup = refused(
        store,
        &[
            "record",
            "evil",
            "--title",
            "Evil",
            "--body",
            "#html.elem(\"script\")",
            "--rationale",
            "No.",
        ],
    );
    assert!(markup.contains("body is typst markup"), "{markup}");
    let cycle = refused(
        store,
        &[
            "revise",
            "datasplit",
            "--title",
            "Split",
            "--body",
            "Split.",
            "--rationale",
            "Again.",
            "--supersedes",
            "userrepo",
        ],
    );
    assert!(cycle.contains("already stands above"), "{cycle}");
    let twice = refused(store, &record("datasplit", "Split again", ""));
    assert!(twice.contains("already decided"), "{twice}");
}

#[test]
fn a_decision_is_recorded_superseded_withdrawn_and_viewed() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    let store = dir.path().join("decisions");

    ok(
        &store,
        &record("datasplit", "Split the data from the code", ""),
    );
    ok(
        &store,
        &record("livebranch", "Deploy from a live branch", ""),
    );
    ok(
        &store,
        &record(
            "userrepo",
            "The data repository pins the app",
            "datasplit,livebranch",
        ),
    );

    let listed = ok(&store, &["list"]);
    let at = |id: &str| listed.find(id).unwrap_or_else(|| panic!("{id}: {listed}"));
    assert!(
        at("userrepo") < at("datasplit") && at("userrepo") < at("livebranch"),
        "{listed}"
    );
    assert!(listed.contains("superseded by userrepo"), "{listed}");
    assert!(listed.contains("in force"), "{listed}");
    let bodies = ok(&store, &["list", "--grep", "STORE is"]);
    assert!(
        ["datasplit", "livebranch", "userrepo"]
            .iter()
            .all(|id| bodies.contains(id)),
        "words only a body holds are found: {bodies}"
    );
    assert_eq!(ok(&store, &["list", "--grep", "store nowhere"]), "");

    the_writers_refusals(&store);

    ok(
        &store,
        &[
            "withdraw",
            "livebranch",
            "--reason",
            "The lock is the deploy now.",
        ],
    );
    let again = refused(
        &store,
        &[
            "revise",
            "livebranch",
            "--title",
            "Live",
            "--body",
            "Live.",
            "--rationale",
            "Back.",
        ],
    );
    assert!(again.contains("withdrawn"), "{again}");

    let shown = ok(&store, &["show", "livebranch"]);
    assert!(
        shown.contains("withdrawn: The lock is the deploy now."),
        "{shown}"
    );

    let html = dir.path().join("decisions.html");
    ok(&store, &["view", "--html", html.to_str().expect("a path")]);
    let page = std::fs::read_to_string(&html).expect("written");
    for words in [
        "The data repository pins the app",
        "Split the data from the code",
        "Superseded",
        "Withdrawn",
    ] {
        assert!(page.contains(words), "{words}: {page}");
    }
    let objects = std::fs::read_dir(store.join("objects"))
        .expect("objects")
        .count();
    assert_eq!(
        objects, 5,
        "a genesis, three decisions and a withdrawal; no refusal wrote"
    );
}
