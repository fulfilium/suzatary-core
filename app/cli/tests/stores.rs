//! The secretary's stores as replicas a tab pulls: what a replica lacks, by heads, and the
//! decorations only the box can give, by content; and the topic stores, which a tab also writes.

use crate::common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_base::capability::Login {
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
    common::exchange(base, "The deed", "write to counsel").0
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
    assert_eq!(
        common::status(&common::get(&format!("{base}/api/stores/lines/objects"))),
        404,
        "the lines are pulled by their skeleton, whose content never sends a dead seal"
    );
    {
        let store = "inbox";
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

/// A page from before the lines is told to load the new version rather than answered that
/// nothing is there.
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
    let sha = suzatary_base::answer::sha(block["source"].as_str().expect("a source"));
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
    let drawn = common::body_json(&mut drawn);
    assert_eq!(
        drawn["proposals"],
        serde_json::json!([reading["proposals"][0], reading["proposals"][0]]),
        "an email draws nothing against a chain, so its row is the replica's, once per index asked"
    );
    assert_eq!(
        drawn["title"], reading["title"],
        "with the conversation's title"
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

/// Each object of a store by name, with its literal: the inbox's from its whole pull, the lines'
/// asked by the names their skeleton gives, each checked against its name.
fn literals(base: &str, store: &str) -> std::collections::BTreeMap<String, String> {
    let objects = if store == "lines" {
        let mut names: Vec<String> = skeleton(base)["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .map(|node| node["hash"].as_str().expect("a name").to_owned())
            .collect();
        names.sort_unstable();
        let (status, _, body) = content(&format!(
            "{base}/api/stores/lines/content?hashes={}",
            names.join(",")
        ));
        assert_eq!(status, 200, "{body}");
        serde_json::from_str::<serde_json::Value>(&body).expect("JSON")["objects"].clone()
    } else {
        objects(base, store, "")["objects"].clone()
    };
    objects
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| {
            let (hash, literal) = (
                object["hash"].as_str().expect("a name"),
                object["literal"].as_str().expect("a literal"),
            );
            assert_eq!(
                hex::encode(<sha2::Sha256 as sha2::Digest>::digest(literal.as_bytes())),
                hash
            );
            (hash.to_owned(), literal.to_owned())
        })
        .collect()
}

fn skeleton(base: &str) -> serde_json::Value {
    let mut asked = common::get(&format!("{base}/api/stores/lines/skeleton"));
    assert_eq!(common::status(&asked), 200);
    let skeleton = common::body_json(&mut asked);
    assert_eq!(skeleton["rest"], 0, "the fixture's skeleton fits one piece");
    skeleton
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
/// good but the lines', whose line becomes its husk once its last key retires; the same question
/// asked after the store moved answers the same bytes.
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
        assert_eq!(
            cache,
            if store == "lines" { "no-store" } else { KEPT },
            "{store}"
        );
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
        let _ = common::opened(&base, &format!("After {store}"));
        assert_ne!(literals(&base, "lines").len(), 0);
        assert_eq!(content(&url), (status, cache, body), "{store}: asked again");
    }
    // Every other answer stays `no-store`, a stream's included, whose own header says less.
    for path in [
        "/api/stores/inbox/objects",
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
    let names: Vec<String> = (0..=suzatary_api::query::MOST_HASHES)
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
        at(&names[..suzatary_api::query::MOST_HASHES].join(",")).0,
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
    held.truncate(suzatary_api::query::MOST_HASHES);
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

/// Each node hashes to its name and is sized as the skeleton says, and after an erasure a line no
/// sent key opens comes as its husk, its words never leaving the box.
#[test]
fn the_lines_skeleton_and_their_content_are_the_whole_pull() {
    let base = writing("stores-skeleton");
    let id = answered(&base);
    let whole = skeleton(&base);
    assert_eq!(whole["heads"]["are"], "these");
    let held = literals(&base, "lines");
    for node in whole["nodes"].as_array().expect("nodes") {
        let hash = node["hash"].as_str().expect("a name");
        assert_eq!(node["size"], held[hash].len(), "{hash}");
    }
    let erased = common::act(
        &format!("{base}/api/secretary/conversations/{id}/erase"),
        &common::erasing(&base, &id, &[0]),
    );
    assert_eq!(common::status(&erased), 200);
    let after = skeleton(&base);
    let keys = after["keys"].as_object().expect("keys");
    let nodes = after["nodes"].as_array().expect("nodes");
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
    let answered: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    let by_name: std::collections::BTreeMap<&str, &serde_json::Value> = answered["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| (object["hash"].as_str().expect("a name"), object))
        .collect();
    let mut husks = 0;
    for node in nodes {
        let hash = node["hash"].as_str().expect("a name");
        let line_keys: Vec<&str> = node["header"]["keys"]
            .as_array()
            .map(|keys| {
                keys.iter()
                    .map(|key| key.as_str().expect("a key"))
                    .collect()
            })
            .unwrap_or_default();
        let opens = line_keys.iter().any(|key| keys.contains_key(*key));
        let object = by_name[hash];
        match object["held"].as_str().expect("held") {
            "husk" => {
                husks += 1;
                assert!(
                    !line_keys.is_empty() && !opens,
                    "{hash}: husked though a key opens it"
                );
                let husk = object["husk"].as_str().expect("a husk");
                assert_eq!(node["size"], husk.len(), "{hash}");
                assert!(
                    husk.len() < held[hash].len(),
                    "{hash}: the husk drops the words"
                );
            }
            held_as => {
                assert_eq!(held_as, "literal");
                assert!(
                    line_keys.is_empty() || opens,
                    "{hash}: no key opens it, yet its words were sent"
                );
                let literal = object["literal"].as_str().expect("a literal");
                assert_eq!(
                    hex::encode(<sha2::Sha256 as sha2::Digest>::digest(literal.as_bytes())),
                    hash
                );
            }
        }
    }
    assert!(husks > 0, "the erased turn's line comes as its husk");
    let mut none = common::get(&format!(
        "{base}/api/stores/lines/skeleton?since={}",
        after["frontier"]["digest"].as_str().expect("a frontier")
    ));
    let none = common::body_json(&mut none);
    assert_eq!(none["nodes"], serde_json::json!([]));
    assert_eq!(none["keys"], serde_json::json!({}));
    assert_eq!(none["heads"], serde_json::json!({"are": "held"}));
}

fn topics_begun(base: &str) -> (prodrome_data::event::Hash, Vec<String>) {
    let all = objects(base, "topics", "");
    let objects = all["objects"].as_array().expect("objects");
    assert_eq!(objects.len(), 1, "the box began the store: {all}");
    let genesis = prodrome_data::event::Hash::new(objects[0]["hash"].as_str().expect("a name"))
        .expect("a name");
    let heads = all["heads"]
        .as_array()
        .expect("heads")
        .iter()
        .map(|head| head.as_str().expect("a name").to_owned())
        .collect();
    (genesis, heads)
}

/// A tab's topic on the first round of `default`, written as `actor` over `genesis` and `deps`.
fn added(
    genesis: &prodrome_data::event::Hash,
    deps: Vec<prodrome_data::event::Hash>,
    actor: &str,
) -> prodrome_data::event::Envelope<suzatary_topics::secretary::topics::reading::MembershipEvent> {
    let event =
        prodrome_tags::membership::added::<suzatary_topics::secretary::topics::reading::Topics>(
            "round:default/0"
                .parse::<suzatary_base::topic_target::Target>()
                .expect("a target"),
            suzatary_topics::secretary::topics::topic::Topic::new("travel").expect("a topic"),
            prodrome_data::literal::Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("an instant"),
            common::actor(actor),
        );
    prodrome_data::event::Envelope::Change(
        prodrome_data::change::mk_change(genesis.clone(), deps, event).expect("a change"),
    )
}

/// The change's print and the print of `key`'s signature of it, as a tab posts them.
fn signed_by(
    change: &prodrome_data::event::Envelope<
        suzatary_topics::secretary::topics::reading::MembershipEvent,
    >,
    key: &prodrome_dag::sign::Secret,
) -> Vec<String> {
    let signature: prodrome_data::event::Envelope<
        suzatary_topics::secretary::topics::reading::MembershipEvent,
    > = prodrome_data::event::Envelope::Signed(key.sign(&prodrome_data::event::seal_hash(change)));
    vec![
        prodrome_data::event::canonical_envelope(change),
        prodrome_data::event::canonical_envelope(&signature),
    ]
}

fn phone() -> prodrome_dag::sign::Secret {
    suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE)
}

/// The asks store's refusals, by their prints.
fn recorded(base: &str) -> Vec<String> {
    objects(base, "asks", "")["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|o| o["literal"].as_str())
        .filter(|literal| literal.starts_with("Change("))
        .map(str::to_owned)
        .collect()
}

/// A change resting on what the box does not hold waits recorded nowhere, and one resting on a
/// change refused for good is refused for good and recorded once in the asks, however often posted.
#[test]
fn a_trusted_tab_writes_a_topic_as_its_own_actor() {
    let base = writing("stores-topic");
    let (genesis, _) = topics_begun(&base);
    let url = format!("{base}/api/stores/topics/objects");
    let prints = signed_by(&added(&genesis, Vec::new(), "ada"), &phone());
    let mut taken = common::act(&url, &serde_json::json!({ "objects": prints }));
    assert_eq!(common::status(&taken), 200);
    let receipt = common::body_json(&mut taken);
    assert_eq!(
        receipt["adopted"].as_array().map(Vec::len),
        Some(2),
        "{receipt}"
    );
    let heads = &receipt["heads"];
    assert_eq!(objects(&base, "topics", "")["heads"], *heads);
    let mut again = common::act(&url, &serde_json::json!({ "objects": prints }));
    assert_eq!(
        common::body_json(&mut again)["already"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let theirs = signed_by(&added(&genesis, Vec::new(), "triage"), &phone());
    let mut foreign = common::act(&url, &serde_json::json!({ "objects": theirs }));
    let refused = common::body_json(&mut foreign);
    assert!(
        refused["refused"][0]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("triage's")),
        "{refused}"
    );
    assert_eq!(objects(&base, "topics", "")["heads"], *heads);
    assert_eq!(recorded(&base), Vec::<String>::new());

    let unheld = prodrome_data::event::Hash::new("f".repeat(64)).expect("a name");
    let orphan = added(&genesis, vec![unheld], "ada");
    let posted = serde_json::json!({ "objects": signed_by(&orphan, &phone()) });
    let mut waits = common::act(&url, &posted);
    assert_eq!(common::body_json(&mut waits)["refused"][0]["kind"], "later");
    assert_eq!(recorded(&base), Vec::<String>::new());

    let triage = added(&genesis, Vec::new(), "triage");
    let on_theirs = added(
        &genesis,
        vec![prodrome_data::event::seal_hash(&triage)],
        "ada",
    );
    let mut objects = vec![prodrome_data::event::canonical_envelope(&triage)];
    objects.extend(signed_by(&on_theirs, &phone()));
    objects.push(prodrome_data::event::canonical_envelope(&on_theirs));
    for _ in 0..2 {
        let mut rests = common::act(&url, &serde_json::json!({ "objects": objects }));
        let receipt = common::body_json(&mut rests);
        assert!(
            receipt["refused"]
                .as_array()
                .is_some_and(|all| all.iter().all(|r| r["kind"] == "final")),
            "{receipt}"
        );
    }
    let refusals = recorded(&base);
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(
        refusals[0].contains(prodrome_data::event::seal_hash(&on_theirs).as_str())
            && refusals[0].contains("store='topics'"),
        "{}",
        refusals[0]
    );
}

/// An untrusted or unsigned change is refused and recorded nowhere, and the box alone writes the
/// inbox, the lines and the asks.
#[test]
fn only_a_trusted_device_of_a_mapped_peer_writes_a_topic() {
    let base = writing("stores-topic-refused");
    let (genesis, heads) = topics_begun(&base);
    let url = format!("{base}/api/stores/topics/objects");
    let change = added(&genesis, Vec::new(), "ada");
    let stranger = prodrome_dag::sign::Secret::new(&"7".repeat(64)).expect("a seed");
    for posted in [
        signed_by(&change, &stranger),
        vec![prodrome_data::event::canonical_envelope(&change)],
    ] {
        let mut answer = common::act(&url, &serde_json::json!({ "objects": posted }));
        assert_eq!(common::status(&answer), 200);
        let receipt = common::body_json(&mut answer);
        assert_eq!(receipt["refused"][0]["kind"], "trust", "{receipt}");
        assert_eq!(receipt["adopted"].as_array().map(Vec::len), Some(0));
    }
    assert_eq!(recorded(&base), Vec::<String>::new());
    let viewer = {
        let whois = common::FakeWhois::new().naming(common::v4(), ada());
        let fakes = Fakes::new("stores-topic-viewer").whois(whois);
        let deployment = common::deployment([ada()], []);
        common::Server::new(Arc::new(fakes), deployment, Vec::new())
            .start()
            .base
    };
    let (viewer_genesis, viewer_heads) = topics_begun(&viewer);
    let asked = serde_json::json!({
        "objects": signed_by(&added(&viewer_genesis, Vec::new(), "ada"), &phone())
    });
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
    for store in ["inbox", "asks"] {
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

/// Each key one more than the last.
struct Counting(std::sync::atomic::AtomicU8);

impl suzatary_base::capability::Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        Ok(())
    }
}

/// The box keeps an opening line's key for the lines' pull to hand every device, and a key posted
/// to a store that takes none refuses the post.
#[test]
fn a_tab_opens_a_conversation_and_a_refused_line_s_key_is_kept_nowhere() {
    use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Title};
    use suzatary_lines::secretary::line_event::LineEvent;
    use suzatary_memory::secretary::words::Words;
    let base = writing("stores-lines");
    let _ = answered(&base);
    let skeleton = |base: &str| {
        let mut answer = common::get(&format!("{base}/api/stores/lines/skeleton"));
        common::body_json(&mut answer)
    };
    let genesis = prodrome_data::event::Hash::new(
        skeleton(&base)["geneses"][0]
            .as_str()
            .expect("the lines began"),
    )
    .expect("a name");
    let at = prodrome_data::literal::Datetime::new(2026, 9, 2, 11, 0, 0, 0).expect("an instant");
    let entropy = Counting(std::sync::atomic::AtomicU8::new(7));
    let change = |line: LineEvent| {
        prodrome_data::event::Envelope::Change(
            prodrome_data::change::mk_change(genesis.clone(), Vec::new(), line).expect("a change"),
        )
    };
    let (open, open_keys) = LineEvent::begin(
        common::actor("ada"),
        &Entry::Opened {
            at,
            title: Words::kept("Trip".to_owned()),
            model: String::new(),
            kind: Kind::Topic,
        },
        &entropy,
    )
    .expect("a line");
    let (forged, forged_keys) = LineEvent::write(
        open.conversation().clone(),
        common::actor("ada"),
        1,
        &Entry::Titled {
            at,
            title: Words::kept(Title::new("Theirs").expect("a title")),
            by: Namer::Person {
                actor: "bob".to_owned(),
            },
        },
        &entropy,
    )
    .expect("a line");
    let (open, forged) = (change(open), change(forged));
    let hex = |keys: &[suzatary_memory::secretary::locked_text::TextKey]| -> Vec<String> {
        keys.iter().map(|key| hex::encode(key.bytes())).collect()
    };
    let name = |o: &prodrome_data::event::Envelope<LineEvent>| {
        prodrome_data::event::seal_hash(o).as_str().to_owned()
    };
    let signed = |o: &prodrome_data::event::Envelope<LineEvent>| {
        let signature: prodrome_data::event::Envelope<LineEvent> =
            prodrome_data::event::Envelope::Signed(
                phone().sign(&prodrome_data::event::seal_hash(o)),
            );
        [
            prodrome_data::event::canonical_envelope(o),
            prodrome_data::event::canonical_envelope(&signature),
        ]
    };
    let posted = serde_json::json!({
        "objects": signed(&open).into_iter().chain(signed(&forged)).collect::<Vec<_>>(),
        "keys": { name(&open): hex(&open_keys), name(&forged): hex(&forged_keys) },
    });
    let mut answer = common::act(&format!("{base}/api/stores/lines/objects"), &posted);
    let status = common::status(&answer);
    let receipt = common::body_json(&mut answer);
    assert_eq!(status, 200, "{receipt}");
    assert_eq!(
        receipt["adopted"].as_array().map(Vec::len),
        Some(2),
        "{receipt}"
    );
    assert_eq!(receipt["refused"][0]["hash"], name(&forged), "{receipt}");
    assert_eq!(receipt["refused"][0]["kind"], "final", "{receipt}");
    let keys = skeleton(&base)["keys"].clone();
    assert!(
        keys.get(open_keys[0].name().to_string()).is_some(),
        "{keys}"
    );
    assert!(
        keys.get(forged_keys[0].name().to_string()).is_none(),
        "{keys}"
    );

    let keyed = serde_json::json!({ "objects": [], "keys": { name(&open): hex(&open_keys) } });
    assert_eq!(
        common::status(&common::act(
            &format!("{base}/api/stores/topics/objects"),
            &keyed
        )),
        422,
        "the post's own fault, in this build's word for it"
    );
}

/// Keys travel beside the lines they open and nothing else, and no key of a refused post is kept.
#[test]
fn a_key_beside_anything_but_a_posted_line_is_refused() {
    let base = writing("stores-keys-beside");
    let id = common::opened(&base, "keys beside");
    let posted = common::sealed(
        &base,
        &id,
        &common::question(&base, "is it due?", Vec::new(), Vec::new()),
    );
    let objects = posted.body["objects"].as_array().expect("objects").clone();
    let signature = objects
        .iter()
        .filter_map(|o| o.as_str())
        .find(|o| o.starts_with("Signed("))
        .expect("a signature");
    let signature_name = prodrome_data::event::seal_hash(
        &prodrome_data::event::parse_envelope::<suzatary_lines::secretary::line_event::LineEvent>(
            &suzatary_lines::secretary::line_event::LineVocabulary,
            signature,
        )
        .expect("parses"),
    );
    let junk = hex::encode([0x5a_u8; 32]);
    let mut body = posted.body.clone();
    body["keys"][signature_name.as_str()] = serde_json::json!([junk]);
    let mut answer = common::act(&format!("{base}/api/stores/lines/objects"), &body);
    let status = common::status(&answer);
    let said = common::body_json(&mut answer);
    assert_eq!(status, 422, "{said}");
}

/// A post this build does not read is another build's (409, which the page posts again), never the
/// post's fault, and one it reads and refuses whole stays a 400.
#[test]
fn a_post_of_another_build_names_the_build() {
    let base = writing("stores-another-build");
    let id = common::opened(&base, "another build");
    let posted = common::sealed(
        &base,
        &id,
        &common::question(&base, "is it due?", Vec::new(), Vec::new()),
    );
    let mut shaped = posted.body.clone();
    shaped["asked_by"] = serde_json::json!("a field this build does not know");
    let mut answer = common::act(&format!("{base}/api/stores/lines/objects"), &shaped);
    let status = common::status(&answer);
    let said = common::body_json(&mut answer);
    assert_eq!(status, 409, "{said}");
    assert!(said.to_string().contains("another build"), "{said}");

    let mut answer = common::act(&format!("{base}/api/stores/notes/objects"), &posted.body);
    let status = common::status(&answer);
    let said = common::body_json(&mut answer);
    assert_eq!(status, 409, "{said}");
    assert!(said.to_string().contains("another build"), "{said}");
}

/// The page's table (`written.failed`) refuses a 413 or 422 piece for good, while a 400 would be
/// read as another build's and the piece posted again for ever.
#[test]
fn every_fault_of_a_post_is_413_or_422() {
    let base = writing("stores-faults");
    let url = format!("{base}/api/stores/lines/objects");
    let objects = |n: usize| serde_json::json!(vec!["Change(junk)"; n]);
    let faults = [
        (
            "over the cap",
            serde_json::json!({ "objects": [ "x".repeat(suzatary_api::routes::MAX_POSTED_BYTES) ], "since": "", "keys": {} }),
            413,
        ),
        ("an array", serde_json::json!([]), 422),
        ("a string", serde_json::json!("post"), 422),
        (
            "a bad frontier",
            serde_json::json!({ "objects": [], "since": "not a frontier", "keys": {} }),
            422,
        ),
        (
            "a key that is no key",
            serde_json::json!({ "objects": [], "since": "", "keys": { "0".repeat(64): ["zz"] } }),
            422,
        ),
        (
            "a key beside no line",
            serde_json::json!({ "objects": [], "since": "", "keys": { "0".repeat(64): ["0".repeat(64)] } }),
            422,
        ),
        (
            "too many objects",
            serde_json::json!({ "objects": objects(suzatary_api::routes::MAX_POSTED + 1), "since": "", "keys": {} }),
            422,
        ),
    ];
    for (fault, body, status) in faults {
        let mut answer = common::act(&url, &body);
        let got = common::status(&answer);
        let said = common::body_json(&mut answer);
        assert_eq!(got, status, "{fault}: {said}");
    }
}
