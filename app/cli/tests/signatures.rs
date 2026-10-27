//! Who wrote an object, over a real server (SPEC law 42): a device's objects are refused until a
//! tap from a device already trusted trusts its key, a revoked key's are refused after, and what
//! it signed before still reads.
//!
//! `v4` is the writer `ada@example.com`; both "devices" post from it, as a new phone on the same
//! login does, which is exactly why the login cannot be what trusts a key.

mod common;

use std::sync::Arc;

use common::Fakes;
use prodrome_core::dag::Dag;
use prodrome_core::event::{Envelope, Hash, canonical_envelope, mk_completed, seal_hash};
use prodrome_core::sign::{Registrar, Secret};
use suzatary_core::record::TodoEvent;
use suzatary_core::signatures::{BoxKeys, Enrolment, KEPT_CHECKS, Trust, Verified};
use suzatary_core::store::Prodrome as _;
use suzatary_core::store::Reads as _;

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn seed(byte: char) -> Secret {
    Secret::new(&byte.to_string().repeat(64)).expect("a seed")
}

fn keys() -> BoxKeys {
    BoxKeys::of(
        &seed('9').public(),
        &suzatary_core::capability::Secret::new("9".repeat(64)),
        &suzatary_core::capability::Secret::new("8".repeat(64)),
    )
    .expect("the deployment's keys")
}

/// A box declaring `keys`' root, `phone` trusted for ada at the terminal.
fn signing_box(tag: &str, phone: &Secret) -> (String, Arc<Fakes>) {
    let (running, fakes) = signing_server(tag, phone);
    (running.base, fakes)
}

fn signing_server(tag: &str, phone: &Secret) -> (common::Running, Arc<Fakes>) {
    signing_over(
        Fakes::new(tag),
        common::deployment([ada()], [(ada(), common::actor("ada"))]),
        None,
        phone,
    )
}

/// A box over `fakes` and `deployment`, its one chain under `keys`' root, `phone` trusted for ada
/// at the terminal, and the agent's key for triage.
fn signing_over(
    fakes: Fakes,
    mut deployment: suzatary_core::deployment::Deployment,
    settings: Option<common::SettingsFixture>,
    phone: &Secret,
) -> (common::Running, Arc<Fakes>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(fakes.whois(whois));
    let keys = keys();
    for (actor, key) in [("ada", phone.public()), ("triage", seed('8').public())] {
        suzatary_core::signatures::trust(&fakes.chain, keys.root(), &common::actor(actor), &key)
            .expect("trusted at the terminal");
    }
    deployment.chains =
        suzatary_core::chains::Chains::of(vec![suzatary_core::chains::ChainPolicy {
            root: keys.root().public(),
            ..common::chain()
        }])
        .expect("one chain");
    let server = common::Server::new(Arc::clone(&fakes), deployment, Vec::new()).keys(keys);
    let server = match settings {
        Some(settings) => server.settings(settings),
        None => server,
    };
    (server.start(), fakes)
}

/// A tab's act, signed by `key`: the two prints a tab posts.
fn act(fakes: &Fakes, key: &Secret, todo: &str) -> (Hash, Vec<String>) {
    let at = prodrome_core::literal::Datetime::new(2026, 9, 2, 11, 0, 0, 0).expect("an instant");
    let change = common::written(
        &fakes.chain,
        mk_completed(todo, at, "ada", "").expect("an event"),
    );
    let name = seal_hash(&change);
    let signed: Envelope<TodoEvent> = Envelope::Signed(key.sign(&name));
    (
        name,
        vec![canonical_envelope(&change), canonical_envelope(&signed)],
    )
}

fn post(base: &str, objects: &[String]) -> serde_json::Value {
    let mut response = common::act(
        &format!("{base}/api/objects"),
        &serde_json::json!({ "objects": objects }),
    );
    assert_eq!(common::status(&response), 200);
    common::body_json(&mut response)
}

fn adopted(receipt: &serde_json::Value) -> usize {
    receipt["adopted"].as_array().map_or(0, Vec::len)
}

/// The tap on card `index` of the default conversation, carrying `key`'s endorsement.
fn tap(base: &str, fakes: &Fakes, index: u64, key: &Secret, new: &Secret) -> (u16, Hash) {
    let history: Dag<TodoEvent> = fakes.chain.objects().expect("reads").into_iter().collect();
    let added = Enrolment {
        actor: common::actor("ada"),
        key: new.public(),
    }
    .added(&history)
    .expect("a prodrome");
    let signed = key.sign(&seal_hash(&added));
    let default = suzatary_core::secretary::store::DEFAULT.as_str().to_owned();
    let url = format!("{base}/api/secretary/conversations/{default}/proposals/{index}/tap");
    let body = serde_json::json!({
        "endorsement": {"key": signed.key.as_str(), "signature": signed.signature.as_str()}
    });
    (
        common::status(&common::act_signed(&url, &body, key)),
        common::request_named(&url, &body, common::STAMPED, &key.public()),
    )
}

#[test]
fn a_new_device_is_refused_until_a_trusted_device_taps_its_card() {
    let (phone, new) = (seed('1'), seed('2'));
    let (running, fakes) = signing_server("signatures-enrol", &phone);
    let base = running.base.clone();

    let (_, mine) = act(&fakes, &phone, "alpha");
    assert_eq!(
        adopted(&post(&base, &mine)),
        2,
        "the trusted phone's act and its signature"
    );

    let (_, theirs) = act(&fakes, &new, "beta");
    let refused = post(&base, &theirs);
    assert_eq!(adopted(&refused), 0, "{refused}");
    assert!(
        refused["refused"][0]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("not trusted")),
        "{refused}"
    );

    // A card for a key is asked for by the device holding it, and only by it.
    let body = serde_json::json!({ "key": new.public().as_str() });
    assert_eq!(
        common::status(&common::act(&format!("{base}/api/act/key"), &body)),
        403,
        "an unsigned ask"
    );
    assert_eq!(
        common::status(&common::act_signed(
            &format!("{base}/api/act/key"),
            &body,
            &phone
        )),
        403,
        "an ask for a key the asker does not hold"
    );
    let mut asked = common::act_signed(&format!("{base}/api/act/key"), &body, &new);
    assert_eq!(common::status(&asked), 200);
    let index = common::body_json(&mut asked)["proposed"]
        .as_u64()
        .expect("an index");

    assert_eq!(
        tap(&base, &fakes, index, &new, &new).0,
        403,
        "a device cannot vouch for itself"
    );
    assert_eq!(adopted(&post(&base, &theirs)), 0);
    let (tapped, request) = tap(&base, &fakes, index, &phone, &new);
    assert_eq!(tapped, 200);
    // The record names the device that asked and what the box wrote for it: the key, signed.
    let record = running
        .requests
        .get(&request)
        .expect("the record reads")
        .expect("kept");
    assert_eq!(record.key, phone.public().as_str());
    assert!(record.verifies());
    let written: Vec<Envelope<TodoEvent>> = fakes
        .chain
        .objects()
        .expect("reads")
        .into_iter()
        .filter(|(name, _)| record.wrote.contains(&name.as_str().to_owned()))
        .map(|(_, object)| object)
        .collect();
    assert!(
        written
            .iter()
            .any(|object| matches!(object, Envelope::KeyAdded(added) if added.key == new.public())),
        "the record names the key object the tap wrote: {:?}",
        record.wrote
    );
    let receipt = post(&base, &theirs);
    assert_eq!(adopted(&receipt), 2, "{receipt}");
}

#[test]
fn a_revoked_key_is_refused_and_its_earlier_objects_still_read() {
    let phone = seed('1');
    let (base, fakes) = signing_box("signatures-revoke", &phone);
    let (before, mine) = act(&fakes, &phone, "alpha");
    assert_eq!(adopted(&post(&base, &mine)), 2);

    suzatary_core::signatures::revoke(
        &fakes.chain,
        keys().root(),
        &common::actor("ada"),
        &phone.public(),
    )
    .expect("revoked at the terminal");
    let (_, after) = act(&fakes, &phone, "beta");
    let refused = post(&base, &after);
    assert_eq!(adopted(&refused), 0, "{refused}");
    assert!(
        refused["refused"][0]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("revoked")),
        "{refused}"
    );
    let history: Dag<TodoEvent> = fakes.chain.objects().expect("reads").into_iter().collect();
    let trust = Trust::of(
        &history,
        &Registrar::Roots([keys().root().public()].into_iter().collect()),
    );
    assert!(trust.proves(&before), "what it signed before still reads");
}

/// A route the box performs for a device is held to the same door: a revoked phone, an unsigned
/// request, a body changed after it was signed and a key nobody trusted are all refused before
/// any work, so no write reaches a chain through the box that would not reach it as an object.
#[test]
fn a_box_route_is_held_to_the_key_that_asked() {
    let phone = seed('1');
    let (running, fakes) = signing_server("signatures-requests", &phone);
    let base = running.base.clone();
    let accept = format!("{base}/api/secretary/conversations/nothing/proposals/0/dismiss");
    let body = serde_json::json!({ "reason": "not now" });
    let said = |mut response: ureq::http::Response<ureq::Body>| {
        (
            common::status(&response),
            common::body_json(&mut response)["error"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        )
    };

    let (status, why) = said(common::act_signed(&accept, &body, &phone));
    assert_ne!(status, 403, "a trusted phone passes the door: {why}");
    // Kept once, as the phone's, and checkable from the record.
    let recorded = running
        .requests
        .get(&common::request_named(
            &accept,
            &body,
            common::STAMPED,
            &phone.public(),
        ))
        .expect("the record reads")
        .expect("kept");
    assert_eq!(recorded.key, phone.public().as_str());
    assert_eq!(
        recorded.target,
        "/api/secretary/conversations/nothing/proposals/0/dismiss"
    );
    assert_eq!(recorded.actor, "ada");
    assert!(recorded.verifies());

    let (status, why) = said(common::act_unsigned(&accept, &body));
    assert_eq!((status, why.contains("no signature")), (403, true), "{why}");
    let (status, why) = said(common::act_signed(&accept, &body, &seed('3')));
    assert_eq!((status, why.contains("not trusted")), (403, true), "{why}");

    suzatary_core::signatures::revoke(
        &fakes.chain,
        keys().root(),
        &common::actor("ada"),
        &phone.public(),
    )
    .expect("revoked at the terminal");
    let (status, why) = said(common::act_signed(&accept, &body, &phone));
    assert_eq!((status, why.contains("revoked")), (410, true), "{why}");
}

/// A read signed by a revoked key is answered with the wipe, and nothing else is: a trusted
/// key's signed read, an unsigned read and a read whose signature does not verify are read as
/// before.
#[test]
fn a_read_signed_by_a_revoked_key_is_answered_with_the_wipe() {
    let phone = seed('1');
    let (base, fakes) = signing_box("signatures-revoked-read", &phone);
    let fold = format!("{base}/api/fold");
    assert_eq!(common::status(&common::get_signed(&fold, &phone)), 200);
    suzatary_core::signatures::revoke(
        &fakes.chain,
        keys().root(),
        &common::actor("ada"),
        &phone.public(),
    )
    .expect("revoked at the terminal");
    let mut wiped = common::get_signed(&fold, &phone);
    assert_eq!(common::status(&wiped), 410);
    assert_eq!(
        wiped
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store"),
        "a cached wipe would answer the new device the tab comes back as"
    );
    assert_eq!(
        wiped
            .headers()
            .get("clear-site-data")
            .and_then(|v| v.to_str().ok()),
        Some("\"cache\""),
        "the browser clears the HTTP cache the tab cannot reach"
    );
    let why = common::body_json(&mut wiped)["error"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(why.contains("revoked") && why.contains("deletes"), "{why}");
    assert_eq!(common::status(&common::get(&fold)), 200, "an unsigned read");
    assert_eq!(
        common::status(&common::get_signed(&fold, &seed('3'))),
        200,
        "a key nobody revoked"
    );
}

/// The browser suite's servers resolve the fixture's seeds from the environment, which
/// `scripts/web-e2e.sh` exports: the two spellings are one pair, or every fixture server would
/// refuse to start for a root it cannot sign as.
#[cfg(feature = "fixture")]
#[test]
fn the_e2e_script_exports_the_fixtures_own_seeds() {
    use suzatary_cli::commands::fixture::{AGENT_SEED, BOX_SEED};
    let script = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/web-e2e.sh"),
    )
    .expect("the e2e script reads");
    for (name, seed) in [
        (suzatary_core::signatures::BOX_KEY, BOX_SEED),
        (suzatary_core::signatures::AGENT_KEY, AGENT_SEED),
    ] {
        assert!(
            script.contains(&format!("export {name}=\"{seed}\"")),
            "web-e2e.sh exports {name} as the fixture's own seed"
        );
    }
}

/// What `suzatary device pair` keeps for `code`, minted at the server's clock for ada.
fn paired(running: &common::Running, code: &str) {
    use suzatary_core::signatures::{Code, Pairing, Pairings as _, Salt};
    let at = chrono::DateTime::parse_from_rfc3339(common::STAMPED)
        .expect("an instant")
        .with_timezone(&chrono::Utc);
    let minted = Pairing::minted(
        &Code::typed(code).expect("a code"),
        Salt([1; 32]),
        &common::actor("ada"),
        at,
    );
    running
        .pairings
        .rewrite(&mut |_| Some(minted.clone()))
        .expect("kept");
}

/// A code typed by `key`: its status and the box's words.
fn typed(base: &str, key: &Secret, code: &str) -> (u16, serde_json::Value) {
    let mut response = common::act_signed(
        &format!("{base}/api/act/pair"),
        &serde_json::json!({ "code": code }),
        key,
    );
    (common::status(&response), common::body_json(&mut response))
}

/// THE FIRST DEVICE'S CODE: the device whose signed request carries the code the box printed is
/// trusted, its waiting card leaves the Inbox, and the code then trusts no other device; a wrong
/// try is counted and named, and nothing the box keeps of the request holds the code.
#[test]
fn a_pairing_code_trusts_the_one_device_that_typed_it() {
    let (phone, new, other) = (seed('1'), seed('2'), seed('3'));
    let (running, fakes) = signing_server("signatures-pair", &phone);
    let base = running.base.clone();
    let mut asked = common::act_signed(
        &format!("{base}/api/act/key"),
        &serde_json::json!({ "key": new.public().as_str() }),
        &new,
    );
    assert_eq!(common::status(&asked), 200);
    common::body_json(&mut asked);

    let (status, said) = typed(&base, &new, "BCDF-GHJK");
    assert_eq!(status, 403, "no code waits yet: {said}");
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("No pairing code is waiting"))
    );

    paired(&running, "BCDF-GHJK");
    let (status, said) = typed(&base, &new, "ZZZZ-ZZZZ");
    assert_eq!(status, 403);
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("4 tries left")),
        "{said}"
    );
    let (status, said) = typed(&base, &new, "not a code");
    assert_eq!(status, 400, "malformed, and not counted: {said}");
    assert_eq!(
        common::status(&common::act_unsigned(
            &format!("{base}/api/act/pair"),
            &serde_json::json!({ "code": "BCDF-GHJK" })
        )),
        403,
        "an unsigned request trusts no key"
    );

    let (status, said) = typed(&base, &new, "bcdf ghjk");
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["trusted"], new.public().as_str());
    let (_, theirs) = act(&fakes, &new, "beta");
    assert_eq!(
        adopted(&post(&base, &theirs)),
        2,
        "the paired device writes"
    );

    let cards = suzatary_core::secretary::inbox::fold(&fakes.inbox).expect("reads");
    let waiting: Vec<_> = cards.objects().filter(|o| o.reading.performs()).collect();
    assert!(waiting.is_empty(), "its card left the Inbox: {waiting:?}");

    let (status, said) = typed(&base, &other, "BCDF-GHJK");
    assert_eq!(status, 403);
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("already used")),
        "{said}"
    );
    let (_, others) = act(&fakes, &other, "gamma");
    assert_eq!(
        adopted(&post(&base, &others)),
        0,
        "a used code trusts nobody else"
    );

    let kept = std::fs::read_dir(common::requests_dir(running.port()))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect::<String>();
    assert!(
        !kept.to_lowercase().contains("bcdf"),
        "no record holds the code"
    );
}

/// A revoked key that types a waiting code is refused in plain words, and the try spends
/// nothing: the code still takes its five tries, and another device then pairs with it.
#[test]
fn a_revoked_key_is_refused_a_code_and_spends_no_try() {
    let phone = seed('1');
    let (running, fakes) = signing_server("signatures-pair-revoked", &phone);
    let base = running.base.clone();
    suzatary_core::signatures::revoke(
        &fakes.chain,
        keys().root(),
        &common::actor("ada"),
        &phone.public(),
    )
    .expect("revoked at the terminal");
    paired(&running, "BCDF-GHJK");
    let (status, said) = typed(&base, &phone, "BCDF-GHJK");
    assert_eq!(status, 403, "{said}");
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("was revoked")),
        "{said}"
    );
    let new = seed('2');
    let (_, said) = typed(&base, &new, "ZZZZ-ZZZZ");
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("4 tries left")),
        "the revoked key's try was not counted: {said}"
    );
    let (status, said) = typed(&base, &new, "BCDF-GHJK");
    assert_eq!(status, 200, "the code was not spent: {said}");
}

/// A key trusted at the terminal leaves no request behind: its waiting card is withdrawn, and
/// withdrawing again withdraws nothing.
#[test]
fn a_key_trusted_at_the_terminal_withdraws_its_card() {
    let new = seed('2');
    let (running, fakes) = signing_server("signatures-withdraw", &seed('1'));
    let mut asked = common::act_signed(
        &format!("{}/api/act/key", running.base),
        &serde_json::json!({ "key": new.public().as_str() }),
        &new,
    );
    assert_eq!(common::status(&asked), 200);
    common::body_json(&mut asked);
    let waiting = || {
        suzatary_core::secretary::inbox::fold(&fakes.inbox)
            .expect("reads")
            .objects()
            .filter(|object| object.reading.performs())
            .count()
    };
    assert_eq!(waiting(), 1);
    let withdraw = || {
        suzatary_core::secretary::routes::withdraw_trusted(
            &fakes.inbox,
            &fakes.chain,
            &keys().root().public(),
            prodrome_core::literal::Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("an instant"),
        )
        .expect("withdrawn")
        .len()
    };
    assert_eq!(withdraw(), 0, "an untrusted key's card stays");
    suzatary_core::signatures::trust(
        &fakes.chain,
        keys().root(),
        &common::actor("ada"),
        &new.public(),
    )
    .expect("trusted at the terminal");
    assert_eq!(withdraw(), 1);
    assert_eq!(waiting(), 0, "its card left the Inbox");
    assert_eq!(withdraw(), 0, "run again, it withdraws nothing");
}

/// Five wrong tries end a code, and the right one after them is refused in plain words.
#[test]
fn five_wrong_tries_end_a_pairing_code() {
    let (running, _fakes) = signing_server("signatures-pair-tries", &seed('1'));
    let base = running.base.clone();
    paired(&running, "BCDF-GHJK");
    let new = seed('2');
    // Each try differs: the same signed request again is a replay, answered from its record.
    for (wrong, left) in [
        ("ZZZZ-ZZZB", "4 tries left"),
        ("ZZZZ-ZZZC", "3 tries left"),
        ("ZZZZ-ZZZD", "2 tries left"),
        ("ZZZZ-ZZZF", "1 try left"),
    ] {
        let (status, said) = typed(&base, &new, wrong);
        assert_eq!(status, 403);
        assert!(
            said["error"].as_str().is_some_and(|w| w.contains(left)),
            "{said}"
        );
    }
    let (_, said) = typed(&base, &new, "ZZZZ-ZZZG");
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("last try")),
        "{said}"
    );
    let (status, said) = typed(&base, &new, "BCDF-GHJK");
    assert_eq!(status, 403);
    assert!(
        said["error"]
            .as_str()
            .is_some_and(|w| w.contains("no longer works")),
        "{said}"
    );
}

/// A key nobody trusts may ask, so asking is bounded: past three waiting device cards a new key
/// is refused until the owner answers them, while a key already waiting is answered its card.
#[test]
fn a_stray_key_cannot_flood_the_inbox_with_device_cards() {
    let (base, _fakes) = signing_box("signatures-flood", &seed('1'));
    let ask = |key: &Secret| {
        common::status(&common::act_signed(
            &format!("{base}/api/act/key"),
            &serde_json::json!({ "key": key.public().as_str() }),
            key,
        ))
    };
    for byte in ['2', '3', '4'] {
        assert_eq!(ask(&seed(byte)), 200, "key {byte} asks");
    }
    assert_eq!(ask(&seed('5')), 429, "a fourth waits for the owner");
    assert_eq!(
        ask(&seed('2')),
        200,
        "a key already waiting is answered its card"
    );
}

/// A model that proposes the ntfy topic, then answers three rounds of another conversation.
fn replaying_model() -> common::FakeModel {
    common::FakeModel::saying([
        r#"{"say": "I will ask for the key.",
            "calls": [{"verb": "propose_act", "args": {
              "act": "secret",
              "name": "NTFY_TOPIC",
              "why": "Notifications have been failing for want of a topic.",
              "sources": "todo:life/permitfiling"
            }}]}"#,
        r#"{"say": "Proposed. Tap it and type the key."}"#,
        r#"{"say": "one"}"#,
        r#"{"say": "two"}"#,
        r#"{"say": "three"}"#,
    ])
}

/// The secret names a deployment with an ntfy channel may set.
fn ntfy() -> suzatary_core::settings::Settables {
    suzatary_core::settings::Settables::derive(
        suzatary_io::model::TOKEN,
        [suzatary_core::connect::Channel::Ntfy],
        None,
        &[],
    )
}

/// Settings declaring an ntfy channel, its topic settable.
fn ntfy_settings(settings: common::SettingsFixture) -> common::SettingsFixture {
    common::SettingsFixture {
        live: suzatary_cli::serve::Live {
            settables: ntfy(),
            ..settings.live
        },
        ..settings
    }
}

/// A box whose model proposes a secret, whose secret writes are recorded, and whose phone is
/// trusted: every effect a replay could repeat can be counted.
struct Replaying {
    running: common::Running,
    fakes: Arc<Fakes>,
    secrets: Arc<common::SecretRecorder>,
    phone: Secret,
}

fn replaying(tag: &str) -> Replaying {
    replaying_with(tag, replaying_model())
}

fn replaying_with(tag: &str, model: common::FakeModel) -> Replaying {
    let phone = seed('1');
    let secrets = Arc::new(common::SecretRecorder::new());
    let settings = ntfy_settings(common::SettingsFixture::with_writer(
        Arc::clone(&secrets) as _
    ));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let (running, fakes) = signing_over(
        Fakes::new(tag).model(model),
        deployment,
        Some(settings),
        &phone,
    );
    Replaying {
        running,
        fakes,
        secrets,
        phone,
    }
}

impl Replaying {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.running.base)
    }

    /// `body` posted to `path` by the phone, as a fresh request.
    fn ask(&self, path: &str, body: &serde_json::Value) -> (u16, serde_json::Value) {
        let mut answer = common::act_signed(&self.url(path), body, &self.phone);
        assert!(
            answer.headers().get("x-suzatary-replayed").is_none(),
            "{path}: a fresh request is performed"
        );
        (common::status(&answer), common::body_json(&mut answer))
    }

    /// The same request sent again, which must be answered from its record as `first` was.
    fn replayed(&self, path: &str, body: &serde_json::Value, first: &(u16, serde_json::Value)) {
        let mut again = common::act_signed(&self.url(path), body, &self.phone);
        assert_eq!(
            again
                .headers()
                .get("x-suzatary-replayed")
                .and_then(|value| value.to_str().ok()),
            Some("1"),
            "{path}: the second is answered from the record"
        );
        assert_eq!(
            (common::status(&again), common::body_json(&mut again)),
            first.clone(),
            "{path}: the same answer"
        );
    }

    /// `body` posted to `path` and then replayed: the first answers `ok`, the replay the same.
    fn twice(&self, path: &str, body: &serde_json::Value, ok: u16) -> serde_json::Value {
        let first = self.ask(path, body);
        assert_eq!(first.0, ok, "{path}: {}", first.1);
        self.replayed(path, body, &first);
        first.1
    }

    /// Open a conversation titled `title`, its id.
    fn open(&self, title: &str) -> String {
        let (status, opened) = self.ask(
            "/api/secretary/conversations",
            &serde_json::json!({ "title": title }),
        );
        assert_eq!(status, 200, "{opened}");
        opened["id"].as_str().expect("an id").to_owned()
    }

    fn settled(&self, id: &str) -> serde_json::Value {
        for _ in 0..400 {
            let payload = common::reading(&self.running.base, id, &[]);
            if payload["state"] == "idle" {
                return payload;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("the round never finished");
    }
}

/// A REQUEST IS PERFORMED ONCE: each route the box performs for a device, sent again with the
/// same name (a capture replayed), is answered from its record, marked as a replay, with the same
/// status and body, and its effect is not repeated. Every first call here succeeds, so each
/// replay shows a performed effect not done twice: here the secretary's routes, the secret a
/// card's tap sets, the turn, the round asked again and the edit each once.
#[test]
fn a_replayed_request_is_answered_from_its_record_and_performs_nothing() {
    let r = replaying("signatures-replay");
    // A card the secretary proposed, tapped with the key's value.
    let carded = r.open("A key");
    r.twice(
        &format!("/api/secretary/conversations/{carded}/say"),
        &serde_json::json!({ "say": "the notifications topic is missing" }),
        202,
    );
    assert_eq!(r.settled(&carded)["proposals"][0]["act"]["kind"], "secret");
    r.twice(
        &format!("/api/secretary/conversations/{carded}/proposals/0/tap"),
        &serde_json::json!({ "value": "sk-tapped-once" }),
        200,
    );
    assert_eq!(
        r.secrets.calls(),
        vec![("NTFY_TOPIC".to_owned(), "sk-tapped-once".to_owned())],
        "the tapped secret was set exactly once"
    );

    // A turn said, a round asked again, an edit: each once.
    let asked = r.open("A morning");
    let path = |verb: &str| format!("/api/secretary/conversations/{asked}/{verb}");
    r.twice(
        &path("say"),
        &serde_json::json!({ "say": "what changed" }),
        202,
    );
    r.settled(&asked);
    let sessions = r.fakes.model.opened().len();
    r.twice(&path("regenerate"), &serde_json::json!({}), 202);
    r.settled(&asked);
    r.twice(
        &path("edit"),
        &serde_json::json!({ "say": "what changed today" }),
        202,
    );
    let reading = r.settled(&asked);
    let turns: Vec<_> = reading["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .map(|turn| turn["text"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        turns,
        ["what changed", "one", "two", "what changed today", "three"],
        "each said, asked again and edited once: {reading}"
    );
    assert_eq!(
        r.fakes.model.opened().len(),
        sessions + 2,
        "one fresh session for the regenerate and one for the edit"
    );

    // A device card dismissed once.
    let mut card = common::act_signed(
        &r.url("/api/act/key"),
        &serde_json::json!({ "key": seed('5').public().as_str() }),
        &seed('5'),
    );
    assert_eq!(common::status(&card), 200);
    let index = common::body_json(&mut card)["proposed"]
        .as_u64()
        .expect("an index");
    let default = suzatary_core::secretary::store::DEFAULT.as_str().to_owned();
    r.twice(
        &format!("/api/secretary/conversations/{default}/proposals/{index}/dismiss"),
        &serde_json::json!({ "reason": "not mine" }),
        200,
    );
}

/// The same request sent many times AT ONCE is performed once: each copy is answered by the
/// performance, from its record, or refused while it runs, and the turn is said once.
#[test]
fn a_request_sent_many_times_at_once_is_performed_once() {
    let r = replaying("signatures-race");
    let id = r.open("At once");
    let url = r.url(&format!("/api/secretary/conversations/{id}/say"));
    let body = serde_json::json!({ "say": "only once" });
    let statuses: Vec<u16> = std::thread::scope(|scope| {
        let copies: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| common::status(&common::act_signed(&url, &body, &r.phone))))
            .collect();
        copies
            .into_iter()
            .map(|copy| copy.join().expect("a copy"))
            .collect()
    });
    assert!(
        statuses.iter().all(|status| [202, 409].contains(status)),
        "{statuses:?}"
    );
    let reading = r.settled(&id);
    let said = reading["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["text"] == "only once")
        .count();
    assert_eq!(said, 1, "said once: {statuses:?} {reading}");
}

const SECRET: &str = "sk-never-in-git-4d1f";
const MARKER: &str = "marker-unpushed-9c2e";
const KEPT: &str = "The car is on level four of the Elm garage";
const GONE: &str = "Ada moved to Oslo in September";

/// Every request a test sent: each body's digest, and each request's path, status and name.
#[derive(Default)]
struct Sent {
    digests: Vec<String>,
    requests: Vec<(String, u16, Hash)>,
}

impl Sent {
    /// `body` posted to `path`, signed by `key`, its status and answer.
    fn send(
        &mut self,
        r: &Replaying,
        path: &str,
        body: &serde_json::Value,
        key: &Secret,
    ) -> (u16, String) {
        let url = r.url(path);
        self.digests.push(suzatary_core::signatures::body_digest(
            &serde_json::to_vec(body).expect("JSON prints"),
        ));
        let mut answer = common::act_signed(&url, body, key);
        let status = common::status(&answer);
        self.requests.push((
            path.to_owned(),
            status,
            common::request_named(&url, body, common::STAMPED, &key.public()),
        ));
        (
            status,
            answer.body_mut().read_to_string().unwrap_or_default(),
        )
    }
}

/// A body `route` reads and performs, with the secret or the marker wherever it takes words, and
/// the key that signs it.
fn performed_body(route: &str) -> (serde_json::Value, Secret) {
    use base64::Engine as _;
    let phone = seed('1');
    let body = match route
        .trim_start_matches("/api/")
        .trim_start_matches("{chain}/")
    {
        "act/device" => serde_json::json!({ "platform": "android", "token": SECRET }),
        // A card for a key is asked by the device holding it.
        "act/key" => {
            return (
                serde_json::json!({ "key": seed('7').public().as_str() }),
                seed('7'),
            );
        }
        // A code typed by a device nobody trusts yet; none waits, so the box refuses it.
        "act/pair" => {
            return (serde_json::json!({ "code": "BCDF-GHJK" }), seed('7'));
        }
        "held/reveal" => serde_json::json!({
            "handle": suzatary_core::held_value::Handle::new(MARKER, 0).expect("a handle").print(),
        }),
        "author" => serde_json::json!({ "add": {
            "body": MARKER, "category": "home",
            "priority": { "how": "flat", "percent": 75 }, "rationale": SECRET,
        }}),
        "secretary/attachments" => {
            let jpeg = [&[0xFF, 0xD8, 0xFF, 0xE0][..], SECRET.as_bytes()].concat();
            serde_json::json!({
                "file": base64::engine::general_purpose::STANDARD.encode(jpeg),
                "name": MARKER,
            })
        }
        "secretary/conversations" | "secretary/conversations/{conversation}/title" => {
            serde_json::json!({ "title": MARKER })
        }
        "secretary/conversations/{conversation}/erase" => serde_json::json!({ "turns": [0] }),
        // A topic whose name is the marker, over a prodrome the box does not hold: refused by name,
        // and still a request performed.
        "stores/{store}/objects" => {
            let added = suzatary_core::secretary::topics::membership_event::added(
                "round:default/0".parse().expect("a target"),
                suzatary_core::secretary::topics::topic::Topic::new(MARKER)
                    .expect("the marker is a topic"),
                prodrome_core::literal::Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("an instant"),
                common::actor("ada"),
            );
            let change = prodrome_core::change::mk_change(
                prodrome_core::event::Hash::new("a".repeat(64)).expect("a name"),
                Vec::new(),
                added,
            )
            .expect("a change");
            serde_json::json!({ "objects": [prodrome_core::event::canonical_envelope(
                &prodrome_core::event::Envelope::Change(change)
            )] })
        }
        "secretary/conversations/{conversation}/say"
        | "secretary/conversations/{conversation}/edit" => serde_json::json!({ "say": MARKER }),
        "secretary/conversations/{conversation}/proposals/{index}/tap" => {
            serde_json::json!({ "value": SECRET })
        }
        "secretary/conversations/{conversation}/proposals/{index}/dismiss"
        | "secretary/conversations/{conversation}/proposals/{index}/drop" => {
            serde_json::json!({ "reason": MARKER })
        }
        // The rest read no body, and each request still signs the bytes it carries.
        _ => serde_json::json!({ "said": MARKER }),
    };
    (body, phone)
}

/// No file under `dir` holds any of `words` or `digests`, and each record holds exactly the
/// provenance fields.
fn provenance_only(dir: &std::path::Path, words: &[&str], digests: &[String]) {
    for entry in std::fs::read_dir(dir).expect("the records") {
        let path = entry.expect("an entry").path();
        let text = std::fs::read_to_string(&path).expect("a record");
        for said in words {
            assert!(!text.contains(said), "{} holds {said:?}", path.display());
        }
        for digest in digests {
            assert!(
                !text.contains(digest.as_str()),
                "{} holds a body's digest",
                path.display()
            );
        }
        if path.extension().is_some_and(|ext| ext == "json") {
            let fields: Vec<String> = serde_json::from_str::<serde_json::Value>(&text)
                .expect("JSON")
                .as_object()
                .expect("an object")
                .keys()
                .cloned()
                .collect();
            assert_eq!(
                fields,
                [
                    "actor",
                    "at",
                    "key",
                    "method",
                    "name",
                    "signature",
                    "status",
                    "target",
                    "wrote"
                ],
                "{}",
                path.display()
            );
        }
    }
}

/// A model that remembers two facts in one round, then proposes the ntfy topic and answers once
/// more.
fn private_model() -> common::FakeModel {
    common::FakeModel::saying([
        r#"{"say": "", "calls": [
                {"verb": "remember", "args": {"fact": "The car is on level four of the Elm garage", "kind": "owner"}},
                {"verb": "remember", "args": {"fact": "Ada moved to Oslo in September", "kind": "owner"}}]}"#,
        r#"{"say": "Noted."}"#,
        r#"{"say": "I will ask for the key.",
                "calls": [{"verb": "propose_act", "args": {
                  "act": "secret",
                  "name": "NTFY_TOPIC",
                  "why": "Notifications have been failing for want of a topic.",
                  "sources": "todo:life/permitfiling"
                }}]}"#,
        r#"{"say": "Proposed."}"#,
        r#"{"say": "Fine."}"#,
    ])
}

/// THE PUSHED RECORD IS PROVENANCE ONLY. Every route that asks a signature is sent a body it
/// performs, carrying a secret's value or a marker wherever it takes words; a fact is remembered
/// and another forgotten (the answer lists the standing fact's words), and a secret card tapped
/// with the value. Then no file the push would carry holds a byte of any body or answer, any
/// body's digest or the requests' nonce, no store the box keeps holds the value, no record and body sent recompute the request's name,
/// and each record holds exactly the provenance fields.
#[test]
fn a_pushed_request_record_holds_nothing_said_either_way() {
    let r = replaying_with("signatures-private", private_model());
    let mut sent = Sent::default();
    let remembered = r.open("Remember");
    sent.send(
        &r,
        &format!("/api/secretary/conversations/{remembered}/say"),
        &serde_json::json!({ "say": "remember both" }),
        &r.phone,
    );
    r.settled(&remembered);
    let carded = r.open("A key");
    sent.send(
        &r,
        &format!("/api/secretary/conversations/{carded}/say"),
        &serde_json::json!({ "say": "the notifications topic is missing" }),
        &r.phone,
    );
    r.settled(&carded);
    let (status, forgot) = sent.send(
        &r,
        "/api/secretary/memory/2/forget",
        &serde_json::json!({}),
        &r.phone,
    );
    assert_eq!(status, 200, "{forgot}");
    assert!(
        forgot.contains(KEPT),
        "the answer lists the standing fact: {forgot}"
    );
    let (status, card) = sent.send(
        &r,
        "/api/act/key",
        &serde_json::json!({ "key": seed('6').public().as_str() }),
        &seed('6'),
    );
    assert_eq!(status, 200, "{card}");
    let device_card = serde_json::from_str::<serde_json::Value>(&card).expect("JSON")["proposed"]
        .as_u64()
        .expect("an index");
    let default = suzatary_core::secretary::store::DEFAULT.as_str().to_owned();

    // Every route the server lists that asks a signature, sent a body it performs.
    let mut listed = 0;
    for line in include_str!("surface.golden").lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if line.starts_with('#') || words.len() != 4 || words[0] != "POST" || words[3] == "-" {
            continue;
        }
        listed += 1;
        let route = words[1];
        let (conversation, index) = if route.ends_with("/dismiss") {
            (default.as_str(), device_card)
        } else if route.ends_with("/tap") {
            (carded.as_str(), 0)
        } else {
            (remembered.as_str(), 0)
        };
        let path = route
            .replace("{chain}", "life")
            .replace("{conversation}", conversation)
            .replace("{index}", &index.to_string())
            .replace("{id}", id_in(route))
            .replace("{store}", "topics");
        let (body, key) = performed_body(route);
        let (status, said) = sent.send(&r, &path, &body, &key);
        // A body the route cannot read is refused before it performs, and leaves no record; every
        // body here is one it reads, so each route performs (its own 404 or 409 included).
        assert_ne!(status, 400, "{path}: {said}");
    }

    assert!(listed > 20, "the route list was read: {listed}");
    for ((path, status, name), digest) in sent.requests.iter().zip(&sent.digests) {
        let record = r
            .running
            .requests
            .get(name)
            .expect("reads")
            .unwrap_or_else(|| panic!("{path} was performed and not recorded"));
        assert_eq!(record.status, *status, "{path} is recorded with its status");
        // The record and the body sent do not name the request: only the nonce, which no record
        // holds, does.
        let named = |nonce: &str| {
            suzatary_core::signatures::request_name(
                &record.method,
                &record.target,
                &record.at,
                nonce,
                digest,
                &record.key,
            )
        };
        assert_eq!(&named(common::NONCE), name, "{path}");
        assert_ne!(&named(""), name, "{path}: named without its nonce");
    }
    nothing_kept(&r, &sent);
}

/// The `{id}` in `route`'s path: a watch is named by 16 hex digits, and a path naming none is
/// refused before the route performs.
fn id_in(route: &str) -> &'static str {
    if route.starts_with("/api/mail/watches/") {
        "0123456789abcdef"
    } else {
        "1"
    }
}

/// No pushed record holds a byte of anything sent or answered, and no store the box keeps holds
/// the value: every route was sent it wherever it takes one, the secret card's tap included.
fn nothing_kept(r: &Replaying, sent: &Sent) {
    provenance_only(
        &common::requests_dir(r.running.port()),
        &[SECRET, MARKER, KEPT, GONE, common::NONCE],
        &sent.digests,
    );
    common::never_stored(&r.fakes.root, SECRET);
}

/// A request is stamped by its device, and one stamped far from the box's clock is refused, so a
/// capture kept past the window cannot be sent at all; one with no stamp or no nonce is unsigned.
#[test]
fn a_request_outside_the_window_or_unstamped_is_refused() {
    let phone = seed('1');
    let (base, _fakes) = signing_box("signatures-window", &phone);
    let url = format!("{base}/api/act/device");
    let body = serde_json::json!({ "platform": "android", "token": "a-push-token" });
    // Whole seconds: a minute and a half past ten minutes is outside, as is one second past.
    for at in [
        "2026-09-02T12:11:00.000Z",
        "2026-09-02T11:49:00.000Z",
        "2026-09-02T12:10:01.000Z",
        "2026-09-02T14:10:30.000+02:00",
    ] {
        let mut late = common::act_signed_at(&url, &body, &phone, at);
        assert_eq!(common::status(&late), 403, "{at}");
        assert!(
            common::body_json(&mut late)["error"]
                .as_str()
                .is_some_and(|why| why.contains("minutes from the box's clock")),
            "{at}"
        );
    }
    assert_eq!(
        common::status(&common::act_signed_at(
            &url,
            &body,
            &phone,
            "2026-09-02T14:10:00.000+02:00"
        )),
        200,
        "inside the window, read in UTC whatever the offset"
    );

    // With no stamp or no nonce it is unsigned; with a nonce that is not one, malformed.
    let bytes = serde_json::to_vec(&body).expect("JSON prints");
    let name = common::request_named(&url, &body, common::STAMPED, &phone.public());
    let signed = phone.sign(&name);
    let send = |at: Option<&str>, nonce: Option<&str>| {
        let mut request = common::agent()
            .post(&url)
            .header("x-suzatary-act", "1")
            .header(
                suzatary_core::gate::CLIENT_HEADER,
                suzatary_core::gate::CLIENT_MIN.to_string(),
            )
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .header("x-suzatary-key", signed.key.as_str())
            .header("x-suzatary-signature", signed.signature.as_str());
        if let Some(at) = at {
            request = request.header("x-suzatary-at", at);
        }
        if let Some(nonce) = nonce {
            request = request.header("x-suzatary-nonce", nonce);
        }
        common::status(&request.send(&bytes[..]).expect("the request completes"))
    };
    assert_eq!(send(None, Some(common::NONCE)), 403, "unstamped");
    assert_eq!(send(Some(common::STAMPED), None), 403, "no nonce");
    assert_eq!(
        send(Some(common::STAMPED), Some("0123")),
        400,
        "not a nonce"
    );
}

/// How long the door's reading takes as a chain grows: read cold (the objects read, the history
/// built, every signature checked), then as the door reads it after one new write (the chain
/// moved by one signed change), and again unmoved. A measurement, run by name with `--ignored
/// --nocapture`.
#[test]
#[ignore = "a measurement, not a law"]
fn the_door_s_reading_by_chain_size() {
    let phone = seed('1');
    let (_running, fakes) = signing_server("signatures-cost", &phone);
    let roots = || Registrar::Roots([keys().root().public()].into_iter().collect());
    let write = |n: u32| {
        let todo = common::CAST.todos.all()[n as usize % 6].id;
        let at = prodrome_core::literal::Datetime::new(2026, 9, 2, 11, n / 60 % 60, n % 60, 0)
            .expect("an instant");
        let change = common::written(
            &fakes.chain,
            mk_completed(todo, at, "ada", "").expect("an event"),
        );
        let name = seal_hash(&change);
        fakes
            .chain
            .accept(&[(name.clone(), canonical_envelope(&change))])
            .expect("kept");
        suzatary_core::store::sign(&fakes.chain, &phone, &name).expect("signed");
    };
    let mut written: u32 = 0;
    for size in [100, 400, 1600] {
        while written < size {
            write(written);
            written += 1;
        }
        let timed = std::time::Instant::now();
        let objects = fakes.chain.objects().expect("reads");
        let read = timed.elapsed();
        let count = objects.len();
        let timed = std::time::Instant::now();
        let history: Dag<TodoEvent> = objects.into_iter().collect();
        let built = timed.elapsed();
        let timed = std::time::Instant::now();
        let _cold = Trust::through(&history, &roots(), &Verified::new(KEPT_CHECKS));
        let cold = timed.elapsed();
        Trust::held(&fakes.chain, &roots()).expect("reads");
        write(written);
        written += 1;
        // The store keeps its read by its heads, so whichever reader asks first after a write
        // pays it and the route the door admits reads the same: timed apart from the door.
        let timed = std::time::Instant::now();
        fakes.chain.objects().expect("reads");
        let reread = timed.elapsed();
        let timed = std::time::Instant::now();
        Trust::held(&fakes.chain, &roots()).expect("reads");
        let moved = timed.elapsed();
        let timed = std::time::Instant::now();
        Trust::held(&fakes.chain, &roots()).expect("reads");
        let unmoved = timed.elapsed();
        eprintln!(
            "{count} objects: cold read {read:?}, history {built:?}, trust {cold:?}; after one \
             new write the store's read {reread:?}, then the door {moved:?}; unmoved {unmoved:?}"
        );
    }
}
