//! A deployment adopting signatures through the real binary: a write adopts nothing, `device
//! init` adopts each chain once and nothing again, and adopting again refuses while a write since
//! the epoch carries no signature, until `device sign-unsigned` repairs it.

use std::path::Path;

use prodrome_core::event::Envelope;
use prodrome_core::sign::{PublicKey, Secret};
use suzatary_core::record::{EventStore, TodoEvent};
use suzatary_core::signatures::{AGENT_KEY, BOX_KEY};

use super::todos::{a_chain, a_signing_instance, objects, scratch, signing};

const OWN: &str = "9999999999999999999999999999999999999999999999999999999999999999";
const AGENT: &str = "8888888888888888888888888888888888888888888888888888888888888888";

fn public(seed: &str) -> PublicKey {
    Secret::new(seed).expect("a seed").public()
}

/// The box's seeds and its dedup key, as `device init` keeps or mints all three.
fn seeds() -> [(&'static str, String); 3] {
    [
        (BOX_KEY, OWN.to_owned()),
        (AGENT_KEY, AGENT.to_owned()),
        (suzatary_core::mail::dedup_key::DEDUP_KEY, "7".repeat(64)),
    ]
}

fn count(root: &Path, kind: fn(&Envelope<TodoEvent>) -> bool) -> usize {
    objects(root).iter().filter(|object| kind(object)).count()
}

fn ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn done(todo: &str, by: &str) -> TodoEvent {
    let at = prodrome_core::literal::Datetime::new(2026, 9, 3, 9, 0, 0, 0).expect("an instant");
    prodrome_core::event::mk_completed(todo, at, by, "").expect("an event")
}

#[test]
fn a_write_adopts_nothing_and_device_init_adopts_once() {
    let root = scratch("adopt-once");
    a_chain(&root);
    let instance = a_signing_instance(&root, &public(OWN));
    let snapshots = |root: &Path| count(root, |o| matches!(o, Envelope::Snapshot(_)));
    let keys = |root: &Path| count(root, |o| matches!(o, Envelope::KeyAdded(_)));
    ok(&signing(
        &instance,
        &["todo", "done", "parkingpermit"],
        &seeds(),
    ));
    assert_eq!(snapshots(&root), 0, "a write takes no epoch");
    assert_eq!(keys(&root), 0, "and registers no key");

    ok(&signing(&instance, &["device", "init"], &seeds()));
    assert_eq!(snapshots(&root), 1, "the epoch");
    assert_eq!(keys(&root), 3, "a key for each actor on the roster");
    ok(&signing(
        &instance,
        &["todo", "reopen", "parkingpermit"],
        &seeds(),
    ));
    ok(&signing(&instance, &["device", "init"], &seeds()));
    assert_eq!(snapshots(&root), 1, "adopted once");
    assert_eq!(keys(&root), 3);
}

#[test]
fn adopting_again_refuses_an_unsigned_write_since_the_epoch_until_it_is_repaired() {
    let root = scratch("adopt-repair");
    a_chain(&root);
    let instance = a_signing_instance(&root, &public(OWN));
    ok(&signing(&instance, &["device", "init"], &seeds()));
    // What a crash between the append and its signature leaves.
    let unsigned = suzatary_core::store::append_to(
        &EventStore::new(
            root.join("events"),
            prodrome_core::policy::Untrusted::none(),
        ),
        suzatary_core::record::GENESIS,
        done("parkingpermit", "ada"),
    )
    .expect("an unsigned write");

    let again = signing(&instance, &["device", "init"], &seeds());
    assert!(
        !again.status.success(),
        "a re-run absorbed an unsigned write"
    );
    let said = String::from_utf8_lossy(&again.stderr);
    assert!(said.contains("device sign-unsigned"), "stderr: {said}");

    // Listed first, with its chain, actor and instant, and nothing signed without --yes.
    let listed = signing(&instance, &["device", "sign-unsigned"], &seeds());
    assert!(!listed.status.success(), "it signed without being told to");
    let shown = String::from_utf8_lossy(&listed.stdout);
    assert!(
        shown.contains(&format!("{}\tlife\tada\t", unsigned.as_str())),
        "stdout: {shown}"
    );
    assert!(String::from_utf8_lossy(&listed.stderr).contains("--yes"));
    let repair = signing(&instance, &["device", "sign-unsigned", "--yes"], &seeds());
    ok(&repair);
    assert!(String::from_utf8_lossy(&repair.stdout).contains("life: signed 1 write(s)"));
    ok(&signing(&instance, &["device", "init"], &seeds()));
    assert_eq!(
        count(&root, |o| matches!(o, Envelope::Snapshot(_))),
        1,
        "still the one epoch"
    );
}

/// What `device list` says of `key` for `actor` on the chain `life`.
fn listed(instance: &Path, actor: &str, key: &PublicKey) -> String {
    let list = signing(instance, &["device", "list"], &seeds());
    ok(&list);
    String::from_utf8_lossy(&list.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix(&format!("life\t{actor}\t{}\t", key.as_str()))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "absent".to_owned())
}

/// `device revoke <key> --actor <actor>` revokes that key for that actor alone; without
/// `--actor` it is revoked for every actor it speaks for, as for a stolen device.
#[test]
fn a_revoke_names_its_actor_or_takes_the_key_from_everyone() {
    let root = scratch("revoke-actor");
    a_chain(&root);
    let instance = a_signing_instance(&root, &public(OWN));
    ok(&signing(&instance, &["device", "init"], &seeds()));
    // What adoption before custody left: the box's key speaking for the agent's actor.
    ok(&signing(
        &instance,
        &["device", "trust", public(OWN).as_str(), "--actor", "triage"],
        &seeds(),
    ));
    assert_eq!(listed(&instance, "triage", &public(OWN)), "trusted");

    let revoked = signing(
        &instance,
        &[
            "device",
            "revoke",
            public(OWN).as_str(),
            "--actor",
            "triage",
        ],
        &seeds(),
    );
    ok(&revoked);
    assert_eq!(listed(&instance, "triage", &public(OWN)), "revoked");
    assert_eq!(listed(&instance, "ada", &public(OWN)), "trusted");
    assert_eq!(listed(&instance, "assistant", &public(OWN)), "trusted");
    assert_eq!(listed(&instance, "triage", &public(AGENT)), "trusted");
    ok(&signing(
        &instance,
        &["todo", "done", "parkingpermit"],
        &seeds(),
    ));
    ok(&signing(&instance, &["verify"], &seeds()));

    let phone = public(&"1".repeat(64));
    for actor in ["ada", "assistant"] {
        ok(&signing(
            &instance,
            &["device", "trust", phone.as_str(), "--actor", actor],
            &seeds(),
        ));
    }
    ok(&signing(
        &instance,
        &["device", "revoke", phone.as_str()],
        &seeds(),
    ));
    assert_eq!(listed(&instance, "ada", &phone), "revoked");
    assert_eq!(listed(&instance, "assistant", &phone), "revoked");
    assert_eq!(listed(&instance, "ada", &public(OWN)), "trusted");
}

/// The instance as the sample and the box write it: a chain whose fold trusts every actor.
fn trusting_everyone(instance: &Path) -> &Path {
    let text = std::fs::read_to_string(instance).expect("the instance reads");
    assert!(text.contains("untrusted = [\"triage\"]"));
    std::fs::write(
        instance,
        text.replace("untrusted = [\"triage\"]", "untrusted = []"),
    )
    .expect("the instance writes");
    instance
}

/// A box adopted before custody, as it was on 2026-10-07: the box's key registered for every
/// actor of the roster and the epoch taken. `device init` repairs it, saying each act, and run
/// again does nothing.
#[test]
fn device_init_repairs_a_box_adopted_before_custody() {
    let root = scratch("adopt-repair-custody");
    a_chain(&root);
    let instance = a_signing_instance(&root, &public(OWN));
    let instance = trusting_everyone(&instance);
    let own = Secret::new(OWN).expect("a seed");
    {
        let chain = suzatary_io::objects::Objects::chain(EventStore::new(
            root.join("events"),
            prodrome_core::policy::Untrusted::none(),
        ));
        for actor in ["ada", "assistant", "triage"] {
            let actor = prodrome_core::event::Actor::new(actor).expect("an actor");
            suzatary_core::signatures::trust(&chain, &own, &actor, &own.public())
                .expect("registered");
        }
        suzatary_core::signatures::epoch(&chain, &own).expect("the epoch");
    }
    assert_eq!(listed(instance, "triage", &public(OWN)), "trusted");
    assert_eq!(listed(instance, "triage", &public(AGENT)), "absent");

    let repaired = signing(instance, &["device", "init"], &seeds());
    ok(&repaired);
    let said = String::from_utf8_lossy(&repaired.stdout);
    for act in [
        format!("life: registered {} for triage", public(AGENT).as_str()),
        format!("life: revoked {} for triage", public(OWN).as_str()),
    ] {
        assert!(said.contains(&act), "{act} in {said}");
    }
    assert_eq!(listed(instance, "triage", &public(OWN)), "revoked");
    assert_eq!(listed(instance, "triage", &public(AGENT)), "trusted");
    assert_eq!(listed(instance, "ada", &public(OWN)), "trusted");
    assert_eq!(listed(instance, "assistant", &public(OWN)), "trusted");
    ok(&signing(instance, &["verify"], &seeds()));

    let again = signing(instance, &["device", "init"], &seeds());
    ok(&again);
    let said = String::from_utf8_lossy(&again.stdout);
    assert!(
        !said.lines().any(|line| line.starts_with("life: ")),
        "a second run acted: {said}"
    );
}
