//! A DEPLOYMENT THAT MOUNTS A PUBLIC CHAIN TAPS (`publicchaindoor`), a security boundary, over the
//! real binary: the e2e fixture's box, whose chains are `life` and `suzatary`, private, and
//! `roadmap`, public. A key is asked of the private chains alone, so a trusted device's tap is
//! performed, an untrusted key is refused and a revoked one wiped; `device init`, `trust`,
//! `revoke` and `list` succeed; the server starts with no adoption warning; and no step writes a
//! byte to the public store.

use crate::common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;

use prodrome_core::event::seal_hash;
use prodrome_core::sign::Secret;

use crate::binary::{Serving, keyed_fixture};

fn seed(byte: char) -> Secret {
    Secret::new(&byte.to_string().repeat(64)).expect("a seed")
}

/// Every file of the public store, by path, with its bytes.
fn public_store(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("the store reads") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, into);
            } else {
                let bytes = std::fs::read(&path).expect("a file reads");
                into.insert(path, bytes);
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(&root.join("box/events-roadmap"), &mut files);
    files
}

/// `suzatary-dev device <args>` at the box's terminal, which must succeed.
fn device(root: &Path, args: &[&str]) -> String {
    let ran: Output = keyed_fixture(root)
        .arg("--instance")
        .arg(root.join("box/instance.toml"))
        .arg("device")
        .args(args)
        .output()
        .expect("the verb runs");
    assert!(
        ran.status.success(),
        "device {args:?}: {}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    String::from_utf8_lossy(&ran.stdout).into_owned()
}

/// The device's request stamped now, as a tab sends it.
fn act(base: &str, path: &str, body: &serde_json::Value, key: &Secret) -> (u16, serde_json::Value) {
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let mut answer = common::act_signed_at(&format!("{base}{path}"), body, key, &now);
    let status = common::status(&answer);
    (status, common::body_json(&mut answer))
}

/// `key`'s standing for the fixture's owner on each chain, as `device list` prints it.
fn listed(root: &Path, key: &Secret) -> Vec<(String, String)> {
    device(root, &["list"])
        .lines()
        .filter_map(|line| {
            let [chain, _, listed, standing] = line.split('\t').collect::<Vec<_>>()[..] else {
                return None;
            };
            (listed == key.public().as_str()).then(|| (chain.to_owned(), standing.to_owned()))
        })
        .collect()
}

#[test]
fn a_box_that_mounts_a_public_chain_performs_a_trusted_tap() {
    let root = suzatary_testkit::this_test().join("box-root");
    std::fs::create_dir_all(root.join("state")).expect("a directory");
    let port = crate::ports::outside_ephemeral();
    let built = keyed_fixture(&root)
        .args(["fixture-chain", "--root"])
        .arg(root.join("box"))
        .args(["--port", &port.to_string(), "--repo"])
        .arg(&root)
        .env("SUZATARY_FIXTURE_NO_MAIL", "1")
        .output()
        .expect("the fixture runs");
    assert!(
        built.status.success(),
        "fixture-chain: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let public = public_store(&root);
    assert!(public.keys().any(|path| path.ends_with("charter.toml")));
    let untouched = |after: &str| {
        assert_eq!(
            public_store(&root),
            public,
            "the public store moved: {after}"
        );
    };

    device(&root, &["init"]);
    untouched("device init");
    let (phone, new, stranger) = (seed('1'), seed('2'), seed('3'));
    device(&root, &["trust", phone.public().as_str()]);
    untouched("device trust");
    let keyed = |standing: &str| {
        vec![
            ("life".to_owned(), standing.to_owned()),
            ("suzatary".to_owned(), standing.to_owned()),
        ]
    };
    assert_eq!(listed(&root, &phone), keyed("trusted"));
    untouched("device list");

    let serving = Serving::start(&root, &root.join("box/instance.toml"));
    let base = format!("http://127.0.0.1:{port}");

    // A new device asks for its card, and the trusted phone's tap trusts it.
    let (status, asked) = act(
        &base,
        "/api/act/key",
        &serde_json::json!({ "key": new.public().as_str() }),
        &new,
    );
    assert_eq!(status, 200, "{asked}");
    let index = asked["proposed"].as_u64().expect("an index");
    let life = prodrome_core::store::EventStore::<suzatary_record::record::TodoEvent>::new(
        root.join("box/events"),
        prodrome_core::policy::Untrusted::none(),
    )
    .dag()
    .expect("life reads");
    let added = suzatary_signatures::signatures::Enrolment {
        actor: prodrome_core::event::Actor::new(suzatary_connect::cast::CAST.owner.actor)
            .expect("an actor"),
        key: new.public(),
    }
    .added(&life)
    .expect("a prodrome");
    let endorsed = phone.sign(&seal_hash(&added));
    let default = suzatary_base::conversation_id::DEFAULT.as_str().to_owned();
    let (status, tapped) = act(
        &base,
        &format!("/api/secretary/conversations/{default}/proposals/{index}/tap"),
        &serde_json::json!({
            "endorsement": {"key": endorsed.key.as_str(), "signature": endorsed.signature.as_str()}
        }),
        &phone,
    );
    assert_eq!(status, 200, "{tapped}");
    assert_eq!(listed(&root, &new), keyed("trusted"));
    untouched("the trust card's tap");

    // A key no private chain trusts is refused in the standing words.
    let dismiss = "/api/secretary/conversations/nothing/proposals/0/dismiss";
    let reason = serde_json::json!({ "reason": "not now" });
    let (status, refused) = act(&base, dismiss, &reason, &stranger);
    assert_eq!(status, 403, "{refused}");
    let why = refused["error"].as_str().unwrap_or_default();
    assert!(why.contains("which is not trusted"), "{why}");

    // A revoked key is refused and told to wipe.
    device(&root, &["revoke", phone.public().as_str()]);
    untouched("device revoke");
    assert_eq!(listed(&root, &phone), keyed("revoked"));
    let (status, wiped) = act(&base, dismiss, &reason, &phone);
    assert_eq!(status, 410, "{wiped}");
    let why = wiped["error"].as_str().unwrap_or_default();
    assert!(why.contains("revoked") && why.contains("deletes"), "{why}");

    let log = serving.log();
    assert!(!log.contains("not every chain has adopted"), "{log}");
    untouched("the server's run");
}
