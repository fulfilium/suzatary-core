//! `suzatary verify` and `suzatary sources migrate-health` through the real binary: a deploy's
//! verify refuses an instance whose old health registers hold a failure the sources' register
//! lacks, and passes once the move carried it, so the new binary never reads a failing source
//! as a new episode.

use std::path::Path;

use super::todos::{a_chain, an_instance, scratch};

fn run(instance: &Path, args: &[&str]) -> std::process::Output {
    crate::binary::keyed()
        .args(["--instance", &instance.to_string_lossy()])
        .args(args)
        .output()
        .expect("the binary runs")
}

#[test]
fn verify_refuses_an_unmoved_failure_until_migrate_health_carries_it() {
    let root = scratch("migrate-health");
    a_chain(&root);
    let instance = an_instance(&root);
    assert!(
        run(&instance, &["verify"]).status.success(),
        "nothing to move"
    );

    let failure = serde_json::json!({
        "at": "2026-10-01T09:00:00", "class": {"kind": "needs_login"}, "words": "lapsed"
    });
    let old = serde_json::json!({
        "someone@example.com": {
            "ok": "2026-09-30T09:00:00", "failed": failure, "classified": failure,
            "first": "2026-09-01T09:00:00"
        }
    });
    std::fs::write(root.join("mail-health.json"), old.to_string()).expect("an old register");
    let refused = run(&instance, &["verify"]);
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "{said}");
    assert!(said.contains("mail:someone@example.com"), "{said}");

    let moved = run(&instance, &["sources", "migrate-health"]);
    assert!(
        moved.status.success(),
        "{}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let verified = run(&instance, &["verify"]);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
}
