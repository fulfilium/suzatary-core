//! `suzatary heartbeat` through the real binary: the wrapper records each pass with it, and the
//! brief reads the result through the corpus, so the two meet at the instance's `mail.heartbeat`
//! and nowhere the wrapper spells.

use suzatary_cli::instance::Instance;
use suzatary_core::mail::corpus::Corpus as _;
use suzatary_core::mail::heartbeat::{PassOutcome, Recorded, Stage};

use super::todos::{an_instance, scratch};

fn heartbeat(instance: &std::path::Path, args: &[&str]) -> std::process::Output {
    crate::binary::keyed()
        .args(["--instance", &instance.to_string_lossy(), "heartbeat"])
        .args(args)
        .output()
        .expect("the binary runs")
}

#[test]
fn what_the_verb_records_is_what_the_brief_reads() {
    let root = scratch("heartbeat");
    let path = an_instance(&root);
    let instance = Instance::read(&path, &suzatary_cli::paths::Env::under(&root, &root))
        .expect("the instance reads");
    let corpus = suzatary_cli::commands::mail::operator(&instance);

    let ok = heartbeat(&path, &["ok"]);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let beat = corpus
        .last_triage_pass()
        .expect("an ok moves the heartbeat");
    assert_eq!(
        instance.mail.heartbeat,
        root.join("triage-heartbeat"),
        "the instance names the file, under its data root"
    );

    for stage in ["review", "sweep"] {
        let failed = heartbeat(&path, &["failed", stage]);
        assert!(
            failed.status.success(),
            "{}",
            String::from_utf8_lossy(&failed.stderr)
        );
    }
    match corpus.last_triage_outcome() {
        PassOutcome::Recorded(Recorded::Failed { stage, count, .. }) => {
            assert_eq!(stage, Stage::Sweep);
            assert_eq!(count.get(), 2, "two failures running");
        }
        other => panic!("two failures read as {other:?}"),
    }
    assert_eq!(
        corpus.last_triage_pass(),
        Some(beat),
        "a failed pass leaves the heartbeat where the last good one put it"
    );

    let refused = heartbeat(&path, &["failed", "wobble"]);
    assert!(
        !refused.status.success(),
        "a stage outside the closed list is refused"
    );
    let _ = std::fs::remove_dir_all(&root);
}
