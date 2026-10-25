//! `secretary migrate-models` over a fixture: the frozen inbox (`io/tests/fixtures/inbox-proposals`)
//! with four proposals of the old forms added beside it, and a model health holding a provider's
//! readings.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use prodrome_core::event::{Envelope, parse_envelope};
use sha2::{Digest, Sha256};
use suzatary_cli::commands::migrate_models;
use suzatary_core::secretary::inbox_event::{InboxEvent, InboxVocabulary};

const GENESIS: &str = "1c6752deb225bb67153f12e87cd0dd82a3055e68549589200bfcaccd12d1ac29";

fn frozen() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../io/tests/fixtures/inbox-proposals/objects")
}

fn names(objects: &Path) -> BTreeSet<String> {
    std::fs::read_dir(objects)
        .expect("the objects")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// `text` stored under the name its bytes hash to, which is answered.
fn store(objects: &Path, text: &str) -> String {
    let name = hex::encode(Sha256::digest(text.as_bytes()));
    std::fs::write(objects.join(format!("{name}.py")), text).expect("an object");
    name
}

/// An asking of `act` for `proposal`, as the inbox stores one.
fn asked(proposal: &str, act: &str) -> String {
    format!(
        "Change(genesis='{GENESIS}', deps=(), event=Asked(proposal='{proposal}', \
         at=datetime(2026, 10, 14, 9, 0, 0), actor='assistant', act={act}, why='she asked', \
         sources=('mail:abcdef012345',), asked='proposal:models/0', serves=None, spec=None))"
    )
}

/// The owner's no to `proposal`, resting on `on`.
fn dismissed(proposal: &str, on: &str) -> String {
    format!(
        "Change(genesis='{GENESIS}', deps=('{on}',), event=Dismissed(proposal='{proposal}', \
         at=datetime(2026, 10, 14, 10, 0, 0), actor='ada', reason='not now'))"
    )
}

/// Every proposal of a removed form goes whole, children first, and nothing else moves: what is
/// left is the frozen inbox, every object read by this build's reader, and a second run removes
/// nothing.
#[test]
fn the_old_cards_go_whole_and_the_rest_of_the_inbox_stays_as_stored() {
    let dir = tempfile::tempdir().expect("scratch");
    let objects = dir.path().join("objects");
    std::fs::create_dir_all(&objects).expect("the objects directory");
    for name in names(&frozen()) {
        std::fs::copy(frozen().join(&name), objects.join(&name)).expect("copied");
    }
    let before = names(&objects);
    let order = "p0000000000000000000000000000000a";
    let provider = "p0000000000000000000000000000000b";
    let connect = "p0000000000000000000000000000000c";
    let sign_in = "p0000000000000000000000000000000f";
    let order_asked = store(
        &objects,
        &asked(
            order,
            "ConfigureOrder(job='review', from=(), to=('phala/deepseek-v4-flash',))",
        ),
    );
    let order_no = store(&objects, &dismissed(order, &order_asked));
    let provider_asked = store(
        &objects,
        &asked(
            provider,
            "ConfigureProvider(from=DeclaredProvider(name='phala', \
             base_url='https://inference.phala.com/v1', key='PHALA_API_KEY', expect=None), \
             to=None)",
        ),
    );
    let connect_asked = store(
        &objects,
        &asked(
            connect,
            "Connect(connection=Connection(template='anthropic-api', params=()))",
        ),
    );

    let sign_in_asked = store(
        &objects,
        &asked(sign_in, "ClaudeSignIn(since='no model has answered yet')"),
    );

    let dry = migrate_models::inbox(dir.path(), true).expect("a dry run");
    assert_eq!(
        dry.dropped,
        BTreeSet::from([order, provider, connect, sign_in].map(str::to_owned))
    );
    assert_eq!(
        names(&objects).len(),
        before.len() + 5,
        "a dry run removes nothing"
    );

    let plan = migrate_models::inbox(dir.path(), false).expect("a run");
    assert_eq!(plan.dropped, dry.dropped);
    let removed: Vec<String> = plan
        .removed
        .iter()
        .map(|name| name.as_str().to_owned())
        .collect();
    let at = |name: &str| {
        removed
            .iter()
            .position(|held| held == name)
            .expect("removed")
    };
    assert_eq!(removed.len(), 5);
    assert!(at(&order_no) < at(&order_asked), "the response goes first");
    for name in [provider_asked, connect_asked, sign_in_asked] {
        at(&name);
    }
    assert_eq!(names(&objects), before);
    for name in names(&objects) {
        let text = std::fs::read_to_string(objects.join(&name)).expect("an object");
        let read: Result<Envelope<InboxEvent>, _> = parse_envelope(&InboxVocabulary, &text);
        assert!(read.is_ok(), "{name}: {read:?}");
    }
    let again = migrate_models::inbox(dir.path(), false).expect("a second run");
    assert!(again.dropped.is_empty() && again.removed.is_empty());
}

/// A kept object resting on one the run would remove is refused, and nothing is removed.
#[test]
fn a_kept_object_resting_on_a_removed_one_is_refused() {
    let dir = tempfile::tempdir().expect("scratch");
    let objects = dir.path().join("objects");
    std::fs::create_dir_all(&objects).expect("the objects directory");
    let removed = "p0000000000000000000000000000000d";
    let kept = "p0000000000000000000000000000000e";
    let on = store(
        &objects,
        &asked(removed, "ConfigureReserve(from=0.3, to=0.5)"),
    );
    store(&objects, &dismissed(kept, &on));
    let refused = migrate_models::inbox(dir.path(), false).expect_err("refused");
    assert!(format!("{refused:#}").contains("kept"), "{refused:#}");
    assert_eq!(names(&objects).len(), 2);
}

/// The model health keeps the `claude` CLI's readings and drops every other.
#[test]
fn the_model_health_keeps_the_cli_readings_alone() {
    let reading = r#"{"at": "2026-10-07T09:00:00", "outcome": "succeeded", "words": ""}"#;
    let text = format!(
        "{{\"claude-cli/claude-opus-5-5\": {reading}, \"phala/deepseek-v4-flash\": {reading}, \
         \"anthropic/claude-sonnet-5\": {reading}}}"
    );
    let (kept, dropped) = migrate_models::health(&text).expect("a health");
    assert_eq!(
        dropped,
        ["anthropic/claude-sonnet-5", "phala/deepseek-v4-flash"]
    );
    let kept: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&kept).expect("JSON");
    assert_eq!(
        kept.keys().collect::<Vec<_>>(),
        ["claude-cli/claude-opus-5-5"]
    );
}
