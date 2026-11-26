//! A line erased by another process stops being served by a server that has already read it.
//! Nothing moves the server but the store: no route, no restart. Two processes share the lines'
//! stores here, a server that has read the conversation and a writer that erases one of its lines.

use crate::common;
use crate::secretary_cli::fresh_instance;

use std::sync::Arc;

use suzatary_base::conversation_id::ConversationId;
use suzatary_cli::instance::Instance;
use suzatary_lines::secretary::transcript;
use suzatary_memory::secretary::erasure::Erasure;

const ERASED: &str = "the spare key is in the blue folder";
#[cfg(feature = "fixture")]
const KEPT: &str = "the garage code is in the red folder";
#[cfg(feature = "fixture")]
const FACT: &str = "Ada keeps the ladder in the shed";

fn ada() -> suzatary_base::capability::Login {
    common::login("ada@example.test")
}

fn at(minute: u32) -> prodrome_data::literal::Datetime {
    prodrome_data::literal::Datetime::new(2026, 10, 25, 9, minute, 0, 0).expect("an instant")
}

/// A conversation holding [`ERASED`], then [`KEPT`], written through `instance`'s stores as the
/// box's own writer does.
#[cfg(feature = "fixture")]
fn written(instance: &Instance) -> ConversationId {
    use suzatary_conversations::secretary::model::Turn;
    use suzatary_conversations::secretary::store::{Entry, Kind};
    let stores = instance.conversation_stores();
    let id = ConversationId::new("2026-10-25-1").expect("a slug");
    let opened = Entry::Opened {
        at: at(0),
        title: suzatary_memory::secretary::words::Words::kept("Folders".to_owned()),
        model: "scripted".to_owned(),
        kind: Kind::Topic,
    };
    transcript::start(stores.conversations(), &id, &opened).expect("opened");
    for (minute, text) in (1..).zip([ERASED, KEPT]) {
        let said = Entry::Said {
            at: at(minute),
            turn: Turn::Person {
                text: text.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("owner".into()),
        };
        transcript::push(stores.conversations(), &id, &said).expect("said");
    }
    id
}

/// Erase the first slot of the first person's line, through `instance`'s own handles on the
/// stores: what a second process's erasure does.
fn erase_first_person_line(instance: &Instance, id: &ConversationId) {
    let stores = instance.conversation_stores();
    let read = transcript::read(stores.conversations(), id).expect("reads");
    let keys = read
        .lines()
        .iter()
        .find(|written| format!("{:?}", written.line).contains("Person"))
        .expect("a person's line")
        .line
        .keys()
        .cloned()
        .collect();
    transcript::erase(
        stores.conversations(),
        id,
        keys,
        Erasure::Asked {
            by: common::actor("ada"),
        },
        at(9),
    )
    .expect("erased");
}

/// Every answer the server gives that can hold a conversation's words, as one text.
fn served(base: &str, id: &str) -> String {
    let mut text = String::new();
    for path in [
        "/api/secretary/memory",
        "/api/secretary/record",
        "/api/stores/lines/skeleton?since=",
    ] {
        let mut answer = common::get(&format!("{base}{path}"));
        text.push_str(&answer.body_mut().read_to_string().expect("a body"));
    }
    text.push_str(&common::told(&common::reading(base, id, &[])));
    text
}

/// A fixture box's instance, and its server as a process of its own with the address it answers at.
#[cfg(feature = "fixture")]
fn running_server() -> (Instance, crate::binary::Serving, String) {
    let root = suzatary_testkit::this_test().join("box-root");
    std::fs::create_dir_all(root.join("state")).expect("a directory");
    let port = crate::ports::outside_ephemeral();
    let built = crate::binary::keyed_fixture(&root)
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
    let path = root.join("box/instance.toml");
    let instance = Instance::read(
        &path,
        &suzatary_cli::paths::Env::under(path.with_file_name("home"), path.with_file_name("run")),
    )
    .expect("the fixture's instance reads");
    let serving = crate::binary::Serving::start(&root, &path);
    (instance, serving, format!("http://127.0.0.1:{port}"))
}

/// Law: the server, a process of its own that has served the conversation, serves none of an
/// erased line's words at its next read of the store, and the line beside it stands.
#[cfg(feature = "fixture")]
#[test]
fn a_line_erased_by_another_process_is_gone_from_a_running_server() {
    let (instance, _serving, base) = running_server();
    let id = written(&instance);
    let before = served(&base, id.as_str());
    assert!(
        before.contains(ERASED) && before.contains(KEPT),
        "the server has read both lines: {before}"
    );

    erase_first_person_line(&instance, &id);

    let after = served(&base, id.as_str());
    assert!(!after.contains(ERASED), "{after}");
    assert!(after.contains(KEPT), "{after}");
}

/// Law: a fact another process forgets is no longer listed by the server that listed it.
#[cfg(feature = "fixture")]
#[test]
fn a_fact_forgotten_by_another_process_is_gone_from_a_running_server() {
    use suzatary_memory::secretary::memory::{Fact, Kind, Origin};
    let (instance, _serving, base) = running_server();
    let id = written(&instance);
    let remembering = suzatary_cli::wiring::Remembering::reader(&instance);
    let memories = remembering.memories();
    let said = Origin::Turn {
        conversation: id.clone(),
        turn: 0,
    };
    let fact = memories
        .remember(&Fact::new(FACT).expect("a fact"), Kind::Owner, &said, at(5))
        .expect("remembers")
        .id;
    assert!(served(&base, id.as_str()).contains(FACT));

    memories
        .forget(fact, at(6), &common::actor("ada"))
        .expect("forgets");

    assert!(!served(&base, id.as_str()).contains(FACT));
}

/// Law: what a model is told of the conversation, in the round after another process erased a
/// line the warm server had read, holds none of the erased words.
#[test]
fn a_model_is_not_told_a_line_another_process_erased() {
    let instance = fresh_instance("warm-erase-told");
    let first = instance.chains.first();
    let chain =
        suzatary_io_chains::objects::Objects::chain(suzatary_record::record::EventStore::new(
            first.events.clone(),
            first.policy.untrusted.clone(),
        ));
    let fakes = Arc::new(
        common::Fakes::over(instance.data().to_path_buf(), chain)
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .model(common::FakeModel::saying([
                r#"{"say": "Noted."}"#,
                r#"{"say": "Again."}"#,
            ])),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let id = common::opened(&base, "Folders");
    let _ = common::say(&base, &id, ERASED);
    common::poll_conversation(&base, &id, |read| read["state"] == "idle");
    assert!(served(&base, &id).contains(ERASED));

    erase_first_person_line(&instance, &ConversationId::new(&id).expect("an id"));

    let asked = fakes.model.asked().len();
    let _ = common::say(&base, &id, "and now?");
    common::poll_conversation(&base, &id, |read| {
        read["state"] == "idle" && read["turns"].as_array().is_some_and(|t| t.len() >= 4)
    });
    let told = format!("{:?}", &fakes.model.asked()[asked..]);
    assert!(
        told.contains("and now?"),
        "the second round was asked: {told}"
    );
    assert!(!told.contains(ERASED), "{told}");
}
