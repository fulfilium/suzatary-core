//! The todo verbs through the real binary, since the open list is a printed row the briefing
//! pastes: `todos open` never shows a held batch's `decision` record as one of the owner's
//! todos (the document and the web app keep it out the same way), a staged todo stays off the
//! chain, and each write is signed by the key its actor owns.

use std::path::{Path, PathBuf};

use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use suzatary_core::record::{EventStore, mk_authored};
use suzatary_core::store::Reads as _;

/// A scratch root unique to this process and thread.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "suzatary-todos-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a scratch root");
    root
}

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 9, 0, 0, 0).expect("a real instant")
}

/// One ordinary todo and one held-batch decision, on a chain of its own, the decision in the
/// shape `secretary::review::author_batch_decision` writes.
pub(crate) fn a_chain(root: &Path) {
    let store = EventStore::new(
        root.join("events"),
        prodrome_core::policy::Untrusted::none(),
    );
    suzatary_core::store::append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_authored(
            "parkingpermit",
            at(1),
            "ada",
            "todo",
            at(1),
            suzatary_core::record::MarkupSource::escaped("Renew the parking permit"),
            fpl::mk_flat(0.75).expect("a term"),
            vec!["an errand with a recoverable cost".to_owned()],
            "home",
            "",
            suzatary_core::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("valid"),
    )
    .expect("appends");
    suzatary_core::store::append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_authored(
            "settletriagebatchdb12e3b7onlife",
            at(1),
            "assistant",
            "decision",
            at(1),
            suzatary_core::record::MarkupSource::escaped(
                "Settle the triage batch db12e3b7 on life",
            ),
            fpl::mk_flat(0.55).expect("a term"),
            vec!["The secretary held it for Ada.".to_owned()],
            "",
            "",
            suzatary_core::record::MarkupSource::escaped("batch:life/db12e3b7\n\nheld, standing"),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("valid"),
    )
    .expect("appends");
}

/// An instance file naming that chain, and nothing this box depends on, under the cast's root.
pub(crate) fn an_instance(root: &Path) -> PathBuf {
    a_signing_instance(root, &suzatary_core::cast::keys::root())
}

/// [`an_instance`], declaring `key` its root.
pub(crate) fn a_signing_instance(root: &Path, key: &prodrome_core::sign::PublicKey) -> PathBuf {
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
home = 0
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
            key = key.as_str(),
        ),
    )
    .expect("an instance file");
    path
}

#[test]
fn a_held_batchs_decision_never_shows_on_the_open_list() {
    let root = scratch("decision-off-the-list");
    a_chain(&root);
    let instance = an_instance(&root);

    let output = crate::binary::keyed()
        .args([
            "--instance",
            &instance.to_string_lossy(),
            "todos",
            "open",
            "--json",
        ])
        .output()
        .expect("the binary runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("todos open prints JSON");
    let rows = printed["open"].as_array().expect("an array of rows");

    assert!(
        rows.iter().all(|row| row["kind"] != "decision"),
        "a decision record showed on the open list: {rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row["id"] == "parkingpermit"),
        "the ordinary todo is still on the list: {rows:#?}"
    );
}

/// The binary with exactly these signing seeds in its environment, none inherited.
pub(crate) fn signing(
    instance: &Path,
    args: &[&str],
    seeds: &[(&str, String)],
) -> std::process::Output {
    // The keys come from the seeds given and nowhere else, and `secretspec` finds its provider
    // through the config under this home.
    let isolated = instance
        .parent()
        .expect("an instance in a directory")
        .join("home");
    let mut command = crate::binary::suzatary();
    command
        .args(["--instance", &instance.to_string_lossy()])
        .args(args)
        .env("HOME", &isolated)
        .env("XDG_CONFIG_HOME", isolated.join("config"));
    for (name, seed) in seeds {
        command.env(name, seed);
    }
    command.output().expect("the binary runs")
}

pub(crate) fn objects(
    root: &Path,
) -> Vec<prodrome_core::event::Envelope<suzatary_core::record::TodoEvent>> {
    suzatary_io::objects::Objects::chain(EventStore::new(
        root.join("events"),
        prodrome_core::policy::Untrusted::none(),
    ))
    .objects()
    .expect("the chain reads")
    .into_iter()
    .map(|(_, object)| object)
    .collect()
}

/// A box whose secret store is locked after a reboot still reads; each write refuses, says
/// which key and what to do, and appends nothing.
#[test]
fn a_box_whose_keys_do_not_resolve_reads_and_refuses_each_write_saying_why() {
    let root = scratch("keys-missing");
    a_chain(&root);
    let key = prodrome_core::sign::Secret::new(&"9".repeat(64)).expect("a seed");
    let instance = a_signing_instance(&root, &key.public());
    let before = objects(&root).len();

    let done = signing(&instance, &["todo", "done", "parkingpermit"], &[]);
    assert!(!done.status.success(), "a write with no key landed");
    let said = String::from_utf8_lossy(&done.stderr);
    // This instance names no secretspec.toml, so there is no store to ask: one to repair, said
    // as one, never "not set" and never "unlock".
    assert!(
        said.contains("SUZATARY_BOX_KEY") && said.contains("repair the secret store"),
        "stderr: {said}"
    );
    assert_eq!(objects(&root).len(), before, "nothing appended");

    let open = signing(&instance, &["todos", "open"], &[]);
    assert!(
        open.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&open.stderr)
    );
    assert!(String::from_utf8_lossy(&open.stdout).contains("parking permit"));
}

/// No event is signed by a key its actor does not own: the owner's write is the root's, and a
/// write recorded as the untrusted agent is the agent key's, the only actor it is registered for.
#[test]
fn each_write_is_signed_by_the_key_its_actor_owns() {
    use prodrome_core::event::Envelope;
    let root = scratch("keys-per-actor");
    a_chain(&root);
    let (own, agent) = ("9".repeat(64), "8".repeat(64));
    let public = |seed: &str| {
        prodrome_core::sign::Secret::new(seed)
            .expect("a seed")
            .public()
    };
    let instance = a_signing_instance(&root, &public(&own));
    let seeds = [
        (suzatary_core::signatures::BOX_KEY, own.clone()),
        (suzatary_core::signatures::AGENT_KEY, agent.clone()),
    ];
    let signer_of = |todo: &str| {
        let held = objects(&root);
        let change = held
            .iter()
            .find_map(|object| match object {
                Envelope::Change(change)
                    if matches!(
                        object.event(),
                        Some(event @ suzatary_core::record::TodoEvent::Completed(_))
                            if event.todo().as_str() == todo
                    ) =>
                {
                    Some(prodrome_core::event::seal_hash(&Envelope::Change(
                        change.clone(),
                    )))
                }
                _ => None,
            })
            .expect("the write");
        held.iter()
            .find_map(|object| match object {
                Envelope::Signed(signed) if signed.object == change => Some(signed.key.clone()),
                _ => None,
            })
            .expect("signed")
    };

    for (todo, actor) in [
        ("parkingpermit", "ada"),
        ("settletriagebatchdb12e3b7onlife", "triage"),
    ] {
        let done = signing(&instance, &["todo", "done", todo, "--actor", actor], &seeds);
        assert!(
            done.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&done.stderr)
        );
    }
    assert_eq!(signer_of("parkingpermit"), public(&own));
    assert_eq!(signer_of("settletriagebatchdb12e3b7onlife"), public(&agent));
}
