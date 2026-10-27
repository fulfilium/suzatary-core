//! The secretary's stores as replicas a tab pulls: what a replica lacks, by heads, and the
//! decorations only the box can give, by content; and the topic stores, which a tab also writes.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

/// A server where `v4` writes as `ada`, whose model proposes one email and then answers.
fn writing(tag: &str) -> String {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let model = FakeModel::saying([
        r#"{"say": "= Drafted\n\nHere is the reply.",
            "calls": [{"verb": "propose_email", "args": {
              "account": "her@example.invalid",
              "to": "counsel@example.test",
              "subject": "The deed",
              "body": "Sending the deed today.",
              "why": "Counsel asked for it.",
              "sources": "mail:a1b2c3d4e5f6"
            }}]}"#,
        r#"{"say": "Proposed."}"#,
    ]);
    let fakes = Fakes::new(tag).whois(whois).model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    common::Server::new(Arc::new(fakes), deployment, Vec::new())
        .start()
        .base
}

/// A conversation with one round answered and one proposal standing.
fn answered(base: &str) -> String {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The deed"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "write to counsel"}),
    );
    assert_eq!(common::status(&said), 202);
    for _ in 0..200 {
        if common::reading(base, &id, &[])["state"] == "idle" {
            return id;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the round never finished");
}

fn objects(base: &str, store: &str, since: &str) -> serde_json::Value {
    let mut answer = common::get(&format!("{base}/api/stores/{store}/objects?since={since}"));
    assert_eq!(common::status(&answer), 200);
    common::body_json(&mut answer)
}

/// A replica holding the frontier a store's answer named lacks nothing, one holding nothing lacks
/// everything, and the objects come parents first.
#[test]
fn a_replica_pulls_what_it_lacks_by_frontier() {
    let base = writing("stores-pull");
    let _ = answered(&base);
    for store in ["inbox", "lines"] {
        let all = objects(&base, store, "");
        let heads: Vec<&str> = all["heads"]
            .as_array()
            .expect("heads")
            .iter()
            .map(|head| head.as_str().expect("a name"))
            .collect();
        assert!(!heads.is_empty(), "{store} has heads");
        assert_eq!(all["untrusted"], serde_json::json!([]));
        let names: Vec<&str> = all["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .map(|object| object["hash"].as_str().expect("a name"))
            .collect();
        assert!(heads.iter().all(|head| names.contains(head)), "{store}");
        let since = all["frontier"]["digest"].as_str().expect("a frontier");
        let none = objects(&base, store, since);
        assert_eq!(
            none["objects"],
            serde_json::json!([]),
            "{store} at its heads"
        );
        assert_eq!(none["heads"], all["heads"]);
        assert_eq!(all["rest"], 0, "{store} fits one piece");
        assert_eq!(none["rest"], 0);
    }
}

/// A page from before the lines asks for the store they replaced, and is told to load the new
/// version rather than answered that nothing is there.
#[test]
fn a_page_asking_for_the_retired_store_is_told_to_reload() {
    let base = writing("stores-retired");
    assert_eq!(
        common::status(&common::get(&format!(
            "{base}/api/stores/conversations/objects"
        ))),
        426
    );
}

/// What only the box can give is asked by content: a stored answer's block by its sha, and a
/// proposal's drawing by its index.
#[test]
fn the_decorations_are_asked_by_content() {
    let base = writing("stores-dress");
    let id = answered(&base);
    let reading = common::reading(&base, &id, &[]);
    let block = reading["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "model")
        .flat_map(|turn| turn["blocks"].as_array().expect("a model's blocks").clone())
        .next()
        .expect("the answer has a block");
    let sha = suzatary_core::secretary::answer::sha(block["source"].as_str().expect("a source"));
    let mut rendered = common::get(&format!(
        "{base}/api/secretary/conversations/{id}/blocks/{sha}"
    ));
    assert_eq!(common::status(&rendered), 200);
    let html = common::body_json(&mut rendered)["html"].clone();
    assert!(html.is_string());
    assert_eq!(
        block["html"], html,
        "the tab dresses the block with the box's rendering"
    );

    let mut drawn = common::get(&format!(
        "{base}/api/secretary/conversations/{id}/drawn?indices=0,0"
    ));
    assert_eq!(common::status(&drawn), 200);
    assert_eq!(
        common::body_json(&mut drawn)["proposals"],
        serde_json::json!([reading["proposals"][0], reading["proposals"][0]]),
        "an email draws nothing against a chain, so its row is the replica's, once per index asked"
    );
    assert_eq!(
        common::status(&common::get(&format!(
            "{base}/api/secretary/conversations/{id}/drawn?indices=0,9"
        ))),
        404,
        "one index there is no proposal of refuses the read"
    );
    assert_eq!(
        common::status(&common::get(&format!(
            "{base}/api/secretary/conversations/{id}/drawn?indices=x"
        ))),
        400
    );
    for asked in ["", "?indices=", "?indices=0,,1"] {
        assert_eq!(
            common::status(&common::get(&format!(
                "{base}/api/secretary/conversations/{id}/drawn{asked}"
            ))),
            400,
            "{asked:?} names no proposal, or one that is not an index"
        );
    }
}

/// How the box marks an answer a browser keeps for good.
const KEPT: &str = "private, max-age=31536000, immutable";

/// Each object of a store by name, with its literal, from its whole pull.
fn literals(base: &str, store: &str) -> std::collections::BTreeMap<String, String> {
    objects(base, store, "")["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| {
            (
                object["hash"].as_str().expect("a name").to_owned(),
                object["literal"].as_str().expect("a literal").to_owned(),
            )
        })
        .collect()
}

/// A content answer, its status, its `Cache-Control` and its body as text.
fn content(url: &str) -> (u16, String, String) {
    let mut answer = common::get(url);
    let cache = answer
        .headers()
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let status = common::status(&answer);
    let body = answer.body_mut().read_to_string().expect("a body");
    (status, cache, body)
}

/// Every object of each store is asked for by its name and answered with its literal, kept for
/// good; the same question asked after the store moved answers the same bytes.
#[test]
fn a_store_s_objects_are_asked_for_by_hash_and_kept_for_good() {
    let base = writing("stores-content");
    let _ = answered(&base);
    for store in ["inbox", "lines"] {
        let held = literals(&base, store);
        let names: Vec<&str> = held.keys().map(String::as_str).collect();
        let url = format!(
            "{base}/api/stores/{store}/content?hashes={}",
            names.join(",")
        );
        let (status, cache, body) = content(&url);
        assert_eq!(status, 200, "{store}: {body}");
        assert_eq!(cache, KEPT);
        let answered: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        let got: std::collections::BTreeMap<String, String> = answered["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .map(|object| {
                (
                    object["hash"].as_str().expect("a name").to_owned(),
                    object["literal"].as_str().expect("a literal").to_owned(),
                )
            })
            .collect();
        assert_eq!(got, held, "{store}: the literal of each object asked");
        // The lines and the inbox move: another conversation is opened.
        let opened = common::act(
            &format!("{base}/api/secretary/conversations"),
            &serde_json::json!({"title": format!("After {store}")}),
        );
        assert_eq!(common::status(&opened), 200);
        assert_ne!(literals(&base, "lines").len(), 0);
        assert_eq!(content(&url), (status, cache, body), "{store}: asked again");
    }
    // Every other answer stays `no-store`, a stream's included, whose own header says less.
    for path in [
        "/api/stores/lines/objects",
        "/api/stores/events",
        "/api/tip",
    ] {
        let answer = common::get(&format!("{base}{path}"));
        assert_eq!(
            answer
                .headers()
                .get("cache-control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store"),
            "{path}"
        );
    }
}

/// A hash a store does not hold is refused as any other is, whoever holds it: another store, a
/// chain, or nothing; and a refusal is never kept.
#[test]
fn a_hash_outside_the_store_is_refused_as_an_unknown_one() {
    let base = writing("stores-content-refused");
    let _ = answered(&base);
    let inbox = literals(&base, "inbox");
    let lines = literals(&base, "lines");
    let mut tip = common::get(&format!("{base}/api/tip"));
    let chain = common::body_json(&mut tip)["heads"][0]
        .as_str()
        .expect("a head")
        .to_owned();
    let nowhere = "0".repeat(64);
    let refused = |url: &str, hash: &str| {
        let (status, cache, body) = content(url);
        assert_eq!(cache, "no-store", "{url}: a refusal is never kept");
        (status, body.replace(hash, "<hash>"))
    };
    let first_of = |held: &std::collections::BTreeMap<String, String>| {
        held.keys().next().expect("an object").clone()
    };
    let unknown = refused(
        &format!("{base}/api/stores/lines/content?hashes={nowhere}"),
        &nowhere,
    );
    assert_eq!(unknown.0, 404);
    for (url, hash) in [
        (
            format!(
                "{base}/api/stores/lines/content?hashes={}",
                first_of(&inbox)
            ),
            first_of(&inbox),
        ),
        (
            format!("{base}/api/stores/lines/content?hashes={chain}"),
            chain.clone(),
        ),
        (
            format!(
                "{base}/api/stores/inbox/content?hashes={}",
                first_of(&lines)
            ),
            first_of(&lines),
        ),
        (
            format!("{base}/api/content?hashes={}", first_of(&lines)),
            first_of(&lines),
        ),
    ] {
        assert_eq!(refused(&url, &hash), unknown, "{url}");
    }
    // One held and one not refuses the whole question.
    let mut both = [first_of(&lines), nowhere.clone()];
    both.sort();
    assert_eq!(
        refused(
            &format!("{base}/api/stores/lines/content?hashes={}", both.join(",")),
            &nowhere
        ),
        unknown
    );
    // A store a tab does not hold is no store.
    assert_eq!(
        content(&format!(
            "{base}/api/stores/memory/content?hashes={nowhere}"
        ))
        .0,
        404
    );
}

/// The question is bounded and spelled one way: one to sixty-four names, ascending, none twice.
#[test]
fn a_content_question_is_bounded() {
    let base = writing("stores-content-bound");
    let names: Vec<String> = (0..=suzatary_core::query::MOST_HASHES)
        .map(|n| format!("{n:064x}"))
        .collect();
    let at = |hashes: &str| content(&format!("{base}/api/stores/lines/content?hashes={hashes}"));
    for asked in [
        String::new(),
        names.join(","),
        format!("{},{}", names[1], names[0]),
        format!("{},{}", names[0], names[0]),
        "not-a-hash".to_owned(),
    ] {
        let (status, cache, body) = at(&asked);
        assert_eq!(status, 400, "{asked:.80}: {body}");
        assert_eq!(cache, "no-store");
    }
    assert_eq!(
        content(&format!("{base}/api/stores/lines/content")).0,
        400,
        "no hashes at all"
    );
    assert_eq!(
        at(&names[..suzatary_core::query::MOST_HASHES].join(",")).0,
        404,
        "sixty-four names are a question, of objects this store does not hold"
    );
}

/// A chain's objects are asked for by hash too, at the bare path and the chain's own.
#[test]
fn a_chain_s_objects_are_asked_for_by_hash() {
    let base = writing("stores-content-chain");
    let mut me = common::get(&format!("{base}/api/me"));
    let name = common::body_json(&mut me)["chains"][0]
        .as_str()
        .expect("the chain's name")
        .to_owned();
    let mut pulled = common::get(&format!("{base}/api/objects"));
    let pulled = common::body_json(&mut pulled);
    let mut held: Vec<(&str, &str)> = pulled["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| {
            (
                object["hash"].as_str().expect("a name"),
                object["literal"].as_str().expect("a literal"),
            )
        })
        .collect();
    held.sort_unstable();
    held.truncate(suzatary_core::query::MOST_HASHES);
    let names: Vec<&str> = held.iter().map(|(name, _)| *name).collect();
    for path in ["/api", &format!("/api/{name}")] {
        let (status, cache, body) =
            content(&format!("{base}{path}/content?hashes={}", names.join(",")));
        assert_eq!(status, 200, "{path}: {body}");
        assert_eq!(cache, KEPT);
        let answered: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        let got: Vec<(&str, &str)> = answered["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .map(|object| {
                (
                    object["hash"].as_str().expect("a name"),
                    object["literal"].as_str().expect("a literal"),
                )
            })
            .collect();
        assert_eq!(got, held, "{path}");
    }
}

/// The lines' skeleton, and each node's literal asked for by hash, are the lines' whole pull:
/// the same objects in the same order, the same heads, prodromes and epoch, the same keys and
/// erasures; and a replica holding the heads lacks nothing.
#[test]
fn the_lines_skeleton_and_their_content_are_the_whole_pull() {
    let base = writing("stores-skeleton");
    let _ = answered(&base);
    let whole = objects(&base, "lines", "");
    let mut asked = common::get(&format!("{base}/api/stores/lines/skeleton"));
    assert_eq!(common::status(&asked), 200);
    let skeleton = common::body_json(&mut asked);
    assert_eq!(skeleton["rest"], 0);
    for field in ["heads", "geneses", "epoch", "keys", "erased"] {
        assert_eq!(skeleton[field], whole[field], "{field}");
    }
    let nodes = skeleton["nodes"].as_array().expect("nodes");
    let mut names: Vec<&str> = nodes
        .iter()
        .map(|node| node["hash"].as_str().expect("a name"))
        .collect();
    names.sort_unstable();
    let (status, _, body) = content(&format!(
        "{base}/api/stores/lines/content?hashes={}",
        names.join(",")
    ));
    assert_eq!(status, 200, "{body}");
    let fetched: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    let by_name: std::collections::BTreeMap<&str, &serde_json::Value> = fetched["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| (object["hash"].as_str().expect("a name"), &object["literal"]))
        .collect();
    let rebuilt: Vec<serde_json::Value> = nodes
        .iter()
        .map(|node| {
            let hash = node["hash"].as_str().expect("a name");
            assert_eq!(
                node["size"],
                by_name[hash].as_str().expect("a literal").len(),
                "{hash}"
            );
            serde_json::json!({"hash": hash, "literal": by_name[hash]})
        })
        .collect();
    assert_eq!(serde_json::Value::Array(rebuilt), whole["objects"]);
    let mut none = common::get(&format!(
        "{base}/api/stores/lines/skeleton?since={}",
        skeleton["frontier"]["digest"].as_str().expect("a frontier")
    ));
    assert_eq!(common::body_json(&mut none)["nodes"], serde_json::json!([]));
}

/// The topic store's one prodrome, as the box began it, and its heads.
fn topics_begun(base: &str) -> (prodrome_core::event::Hash, Vec<String>) {
    let all = objects(base, "topics", "");
    let objects = all["objects"].as_array().expect("objects");
    assert_eq!(objects.len(), 1, "the box began the store: {all}");
    let genesis = prodrome_core::event::Hash::new(objects[0]["hash"].as_str().expect("a name"))
        .expect("a name");
    let heads = all["heads"]
        .as_array()
        .expect("heads")
        .iter()
        .map(|head| head.as_str().expect("a name").to_owned())
        .collect();
    (genesis, heads)
}

/// A tab's topic on the first round of `default`, written as `actor` over `genesis`.
fn added(genesis: &prodrome_core::event::Hash, actor: &str) -> String {
    use suzatary_core::secretary::topics::membership_event;
    let event = membership_event::added(
        "round:default/0".parse().expect("a target"),
        suzatary_core::secretary::topics::topic::Topic::new("travel").expect("a topic"),
        prodrome_core::literal::Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("an instant"),
        common::actor(actor),
    );
    let change =
        prodrome_core::change::mk_change(genesis.clone(), Vec::new(), event).expect("a change");
    prodrome_core::event::canonical_envelope(&prodrome_core::event::Envelope::Change(change))
}

/// A tab posts a topic it wrote offline, and the box takes it as the actor its peer maps to, from
/// a device the history trusts; a re-post takes nothing, and a change naming another actor is
/// refused by name.
#[test]
fn a_trusted_tab_writes_a_topic_as_its_own_actor() {
    let base = writing("stores-topic");
    let (genesis, _) = topics_begun(&base);
    let url = format!("{base}/api/stores/topics/objects");
    let print = added(&genesis, "ada");
    let mut taken = common::act(&url, &serde_json::json!({"objects": [print]}));
    assert_eq!(common::status(&taken), 200);
    let receipt = common::body_json(&mut taken);
    assert_eq!(
        receipt["adopted"].as_array().map(Vec::len),
        Some(1),
        "{receipt}"
    );
    let heads = &receipt["heads"];
    assert_eq!(objects(&base, "topics", "")["heads"], *heads);
    let mut again = common::act(&url, &serde_json::json!({"objects": [print]}));
    assert_eq!(
        common::body_json(&mut again)["already"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let mut foreign = common::act(
        &url,
        &serde_json::json!({"objects": [added(&genesis, "triage")]}),
    );
    let refused = common::body_json(&mut foreign);
    assert!(
        refused["refused"][0]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("triage's")),
        "{refused}"
    );
    assert_eq!(objects(&base, "topics", "")["heads"], *heads);
}

/// Nobody but a trusted device on a peer `web.act_as` maps writes a topic: an untrusted device
/// key, an unsigned request and a viewer who acts as nobody are refused at the door, and the
/// store holds only its genesis after each. The box alone writes the inbox and the lines.
#[test]
fn only_a_trusted_device_of_a_mapped_peer_writes_a_topic() {
    let base = writing("stores-topic-refused");
    let (genesis, heads) = topics_begun(&base);
    let url = format!("{base}/api/stores/topics/objects");
    let body = serde_json::json!({"objects": [added(&genesis, "ada")]});
    let stranger = prodrome_core::sign::Secret::new(&"7".repeat(64)).expect("a seed");
    assert_eq!(
        common::status(&common::act_signed(&url, &body, &stranger)),
        403
    );
    assert_eq!(common::status(&common::act_unsigned(&url, &body)), 403);
    let viewer = {
        let whois = common::FakeWhois::new().naming(common::v4(), ada());
        let fakes = Fakes::new("stores-topic-viewer").whois(whois);
        let deployment = common::deployment([ada()], []);
        common::Server::new(Arc::new(fakes), deployment, Vec::new())
            .start()
            .base
    };
    let (viewer_genesis, viewer_heads) = topics_begun(&viewer);
    let asked = serde_json::json!({"objects": [added(&viewer_genesis, "ada")]});
    assert_eq!(
        common::status(&common::act(
            &format!("{viewer}/api/stores/topics/objects"),
            &asked
        )),
        403
    );
    assert_eq!(
        objects(&viewer, "topics", "")["heads"],
        serde_json::json!(viewer_heads)
    );
    assert_eq!(
        objects(&base, "topics", "")["heads"],
        serde_json::json!(heads)
    );
    for store in ["inbox", "lines"] {
        assert_eq!(
            common::status(&common::act(
                &format!("{base}/api/stores/{store}/objects"),
                &serde_json::json!({"objects": []})
            )),
            405,
            "{store}"
        );
    }
}
