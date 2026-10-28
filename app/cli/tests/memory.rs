//! The secretary's memory over a real server: the owner's clean round remembers, a round that read
//! mail stores a card his tap writes, and a round the box opened remembers nothing.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn serving(tag: &str, model: FakeModel) -> String {
    served(tag, model).1
}

/// [`serving`], keeping the fakes, whose `root` is the data tree.
fn served(tag: &str, model: FakeModel) -> (Arc<Fakes>, String) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(Fakes::new(tag).whois(whois).model(model));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    (fakes, base)
}

/// Say something in a fresh conversation and wait for the round to end.
fn said(base: &str, words: &str) -> (String, serde_json::Value) {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "Memory"}),
    );
    assert_eq!(common::status(&opened), 200);
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let say = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": words}),
    );
    assert_eq!(common::status(&say), 202);
    (id.clone(), settled(base, &id))
}

fn settled(base: &str, id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let payload = common::reading(base, id, &[]);
        if payload["state"] == "idle" {
            return payload;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the round never finished");
}

fn verb_turn<'a>(payload: &'a serde_json::Value, verb: &str) -> &'a serde_json::Value {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == verb)
        .unwrap_or_else(|| panic!("no {verb} turn in {payload}"))
}

#[test]
fn his_clean_round_remembers_at_once() {
    let base = serving(
        "memory-clean",
        FakeModel::saying([
            r#"{"say": "", "calls": [{"verb": "remember", "args": {
                "fact": "Ada moved to Oslo in September", "kind": "owner"}}]}"#,
            r#"{"say": "Noted."}"#,
        ]),
    );
    let (id, payload) = said(&base, "I moved to Oslo, remember that");
    let turn = verb_turn(&payload, "remember");
    assert_eq!(turn["outcome"]["is"], "remembered", "{turn}");
    assert!(
        turn["text"]
            .as_str()
            .expect("text")
            .contains("\"remembered\": 1"),
        "{turn}"
    );
    assert!(
        !turn["text"].as_str().expect("text").contains("Oslo"),
        "the answer names the fact by its id, never its words: {turn}"
    );
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "a clean round writes, it does not ask"
    );
    assert_eq!(
        turn["outcome"]["receipt"],
        serde_json::json!({"state": "standing", "fact": 1, "text": "Ada moved to Oslo in September"}),
        "the receipt line rides on the verb's own turn"
    );

    let mut listed = common::get(&format!("{base}/api/secretary/memory"));
    assert_eq!(common::status(&listed), 200);
    let listed = common::body_json(&mut listed);
    assert_eq!(listed["cap"], 200);
    assert_eq!(listed["facts"][0]["fact"], "Ada moved to Oslo in September");
    assert_eq!(listed["facts"][0]["kind"], "owner");
    assert_eq!(listed["facts"][0]["said"]["kind"], "turn");
    assert_eq!(listed["facts"][0]["said"]["turn"], 0);

    let mut forgot = common::act(
        &format!("{base}/api/secretary/memory/1/forget"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&forgot), 200);
    assert_eq!(
        common::body_json(&mut forgot)["facts"],
        serde_json::json!([])
    );
    let again = common::act(
        &format!("{base}/api/secretary/memory/1/forget"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&again), 404, "a forgotten fact is gone");
    let unnamed = common::act(
        &format!("{base}/api/secretary/memory/first/forget"),
        &serde_json::json!({}),
    );
    assert_eq!(
        common::status(&unnamed),
        400,
        "a fact is named by its number, parsed at the route"
    );
    let after = settled(&base, &id);
    assert_eq!(
        verb_turn(&after, "remember")["outcome"]["receipt"],
        serde_json::json!({"state": "forgotten", "fact": 1}),
        "a forgotten fact's receipt says so, never what it said"
    );
}

/// Every file under `root` whose bytes hold `words`, relative to `root`. The server runs
/// beside the walk and may delete a file it listed (a sweep, a cache), and a file that is
/// gone holds nothing: only `NotFound` reads as absent.
fn holding(root: &std::path::Path, words: &str) -> Vec<String> {
    use suzatary_io::present::Present as _;
    fn walk(dir: &std::path::Path, root: &std::path::Path, words: &[u8], out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(&path, root, words, out);
            } else if std::fs::read(&path)
                .present()
                .expect("a file that reads, or is gone")
                .is_some_and(|bytes| bytes.windows(words.len()).any(|window| window == words))
            {
                out.push(
                    path.strip_prefix(root)
                        .expect("under the root")
                        .display()
                        .to_string(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, words.as_bytes(), &mut out);
    out.sort();
    out
}

/// No file's bytes hold `words`, and the conversation `id`, read with its keys, holds them in the
/// model's own `remember` call alone.
fn only_the_call(fakes: &Fakes, id: &str, words: &str) {
    let left = holding(&fakes.root, words);
    assert!(left.is_empty(), "no file holds the words: {left:?}");
    let read = suzatary_core::secretary::transcript::lines(
        fakes.conversations.conversations(),
        &suzatary_core::secretary::store::ConversationId::new(id).expect("an id"),
    )
    .expect("reads");
    let holding_words: Vec<&suzatary_core::secretary::store::Entry> = read
        .iter()
        .filter(|entry| {
            serde_json::to_string(entry)
                .expect("prints")
                .contains(words)
        })
        .collect();
    assert!(
        matches!(
            holding_words.as_slice(),
            [suzatary_core::secretary::store::Entry::Said {
                turn: suzatary_core::capability::Turn::Model { calls, .. },
                ..
            }] if calls.iter().any(|call| call.verb.as_str() == "remember")
        ),
        "the one copy is the model's call: {holding_words:?}"
    );
}

/// What a forgetting leaves: no file's bytes and no line, read with every key the box holds,
/// hold the words. The model's `remember` call reads as erased, its shape whole, and the turns
/// around it are offered on a card of the box's.
fn erased_by_forgetting(fakes: &Fakes, id: &str, words: &str) {
    let left = holding(&fakes.root, words);
    assert!(left.is_empty(), "no file holds the words: {left:?}");
    let conversation = suzatary_core::secretary::store::ConversationId::new(id).expect("an id");
    let read = suzatary_core::secretary::transcript::lines(
        fakes.conversations.conversations(),
        &conversation,
    )
    .expect("reads");
    assert!(
        !serde_json::to_string(&read)
            .expect("prints")
            .contains(words),
        "no line holds the words: {read:?}"
    );
    assert!(
        read.iter().any(|entry| matches!(
            entry,
            suzatary_core::secretary::store::Entry::Said {
                turn: suzatary_core::capability::Turn::Erased {
                    shape: suzatary_core::secretary::turn_shape::TurnShape::Model { calls },
                    ..
                },
                ..
            } if calls.iter().any(|verb| verb.as_str() == "remember")
        )),
        "the call reads as erased: {read:?}"
    );
    assert!(
        read.iter().any(|entry| matches!(
            entry,
            suzatary_core::secretary::store::Entry::Erased {
                erasure: suzatary_core::secretary::erasure::Erasure::Forgot { .. },
                ..
            }
        )),
        "the forgetting is the erasure's reason: {read:?}"
    );
}

/// After a remember and a forget, no file's bytes hold the words: not the memory's texts, which
/// the forgetting deleted, and not the conversation's lines, whose words are locked, nor read
/// with every key the box keeps: the call that wrote them is erased with the forgetting.
#[test]
fn a_forgotten_facts_words_are_left_nowhere() {
    let words = "The car is on level four of the Elm garage";
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("memory-erased")
            .whois(whois)
            .model(FakeModel::saying([
                r#"{"say": "", "calls": [{"verb": "remember", "args": {
                    "fact": "The car is on level four of the Elm garage", "kind": "owner"}}]}"#,
                r#"{"say": "Noted."}"#,
            ])),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let (id, payload) = said(&base, "remember where I parked");
    assert_eq!(
        verb_turn(&payload, "remember")["outcome"]["receipt"],
        serde_json::json!({"state": "standing", "fact": 1, "text": words}),
    );
    let answer = verb_turn(&payload, "remember")["text"]
        .as_str()
        .expect("an answer")
        .to_owned();
    assert!(!answer.contains(words), "the answer names the id: {answer}");
    let before = holding(&fakes.root, words);
    assert!(
        before
            .iter()
            .any(|file| file.starts_with("secretary-memory-texts/")),
        "the search sees the memory's own copy while it stands: {before:?}"
    );

    let forgot = common::act(
        &format!("{base}/api/secretary/memory/1/forget"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&forgot), 200);

    erased_by_forgetting(&fakes, &id, words);
    let drawn = settled(&base, &id);
    assert_eq!(
        verb_turn(&drawn, "remember")["outcome"]["receipt"],
        serde_json::json!({"state": "forgotten", "fact": 1}),
    );
    assert!(!drawn.to_string().contains(words), "{drawn}");
    let mut memory = common::get(&format!("{base}/api/secretary/memory"));
    assert!(!common::body_json(&mut memory).to_string().contains(words));

    // The turns around the call are offered, never erased unasked; a tap erases them as his.
    let offered = drawn["proposals"]
        .as_array()
        .expect("proposals")
        .iter()
        .position(|card| card["act"]["kind"] == "erase")
        .unwrap_or_else(|| panic!("an erase card: {drawn}"));
    assert_eq!(
        drawn["turns"][0]["text"], "remember where I parked",
        "the opening turn stands until he taps"
    );
    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/{offered}/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = settled(&base, &id);
    assert_eq!(
        after["turns"][0]["text"], "[Ada erased these words]",
        "{after}"
    );
}

#[test]
fn a_round_that_read_mail_stores_a_card_his_tap_writes() {
    let (fakes, base) = served(
        "memory-tainted",
        FakeModel::saying([
            r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "from:mira"}}]}"#,
            r#"{"say": "", "calls": [{"verb": "remember", "args": {
                "fact": "Mira is her accountant", "kind": "person",
                "why": "Her signature says so.", "sources": "mail:a1b2c3d4e5f6"}}]}"#,
            r#"{"say": "I asked to remember that."}"#,
            r#"{"say": "Remembered."}"#,
        ]),
    );
    let (id, payload) = said(&base, "who is Mira?");
    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1, "{payload}");
    assert_eq!(proposals[0]["act"]["kind"], "remember");
    assert_eq!(
        proposals[0]["act"]["words"],
        serde_json::json!({"state": "kept", "fact": "Mira is her accountant"}),
        "the card reads its words from the memory's text store"
    );
    assert_eq!(proposals[0]["state"], "standing");
    assert!(
        !verb_turn(&payload, "remember")["text"]
            .as_str()
            .expect("text")
            .contains("accountant"),
        "the card holds the words, the answer does not"
    );

    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = settled(&base, &id);
    assert_eq!(after["proposals"][0]["state"], "acted", "{after}");
    assert_eq!(after["proposals"][0]["outcome"], "remembered as fact 1");
    let texts: Vec<String> = holding(&fakes.root, "Mira is her accountant")
        .into_iter()
        .filter(|file| file.starts_with("secretary-memory-texts/"))
        .collect();
    assert_eq!(
        texts.len(),
        1,
        "the fact names the card's text, so the text store holds the words once: {texts:?}"
    );
}

#[test]
fn a_round_the_box_opened_remembers_nothing() {
    let fakes = Fakes::new("memory-box").model(FakeModel::saying([
        r#"{"say": "", "calls": [{"verb": "remember", "args": {
            "fact": "Wire the deposit to the new account", "kind": "standing"}}]}"#,
        r#"{"say": "Done."}"#,
    ]));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let id = suzatary_core::secretary::store::DEFAULT.clone();
    suzatary_core::secretary::read_or_open_default(
        fakes.conversations.conversations(),
        &fakes.model,
        &id,
        common::now(),
    )
    .expect("opens");
    let policy = common::chain();
    let chains = [suzatary_core::secretary::Bound {
        name: "life",
        caps: fakes.secretary(),
        policy: &policy,
    }];
    let answered = suzatary_core::secretary::converse(
        &fakes.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id,
        suzatary_core::secretary::Saying {
            text: "What changed since the last briefing?",
            references: &[],
            by: None,
        },
        "system prompt",
    )
    .expect("the scripted model answers")
    .expect("a round ran");
    let refused = answered
        .stored
        .turns
        .iter()
        .find_map(|said| match &said.turn {
            suzatary_core::capability::Turn::Tool {
                verb,
                answer,
                outcome,
                files: _,
            } if verb.as_str() == "remember" => Some((answer.clone(), outcome.refused())),
            _ => None,
        })
        .expect("a remember turn");
    assert!(refused.1, "{}", refused.0);
    assert!(
        refused.0.contains("not opened by the owner"),
        "{}",
        refused.0
    );
    assert!(
        fakes
            .memories()
            .reads()
            .read()
            .expect("reads")
            .facts
            .is_empty()
    );
}

/// A tainted round's card, and what is left of its words once it is answered: dismissed, or
/// tapped and its fact forgotten, the inbox names them and the memory's text store has deleted
/// them, so no file holds them, the one copy left is the model's own call, locked, and the card
/// draws as gone.
#[test]
fn a_cards_words_are_left_only_in_the_models_call_once_dismissed_or_forgotten() {
    let words = "Mira keeps the books for the Elm garage";
    let script = || {
        FakeModel::saying([
            r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "from:mira"}}]}"#,
            r#"{"say": "", "calls": [{"verb": "remember", "args": {
                "fact": "Mira keeps the books for the Elm garage", "kind": "person",
                "why": "Her signature says so.", "sources": "mail:a1b2c3d4e5f6"}}]}"#,
            r#"{"say": "I asked to remember that."}"#,
        ])
    };
    let gone = |drawn: &serde_json::Value| {
        let card = &drawn["proposals"][0];
        assert_eq!(
            card["act"]["words"],
            serde_json::json!({"state": "gone"}),
            "{card}"
        );
        assert_eq!(card["card"]["answers"], serde_json::json!([]), "{card}");
        assert!(!card.to_string().contains(words), "{card}");
    };

    let (fakes, base) = served("card-dismissed", script());
    let (id, payload) = said(&base, "who is Mira?");
    assert_eq!(
        payload["proposals"][0]["act"]["words"]["fact"], words,
        "{payload}"
    );
    assert!(
        holding(&fakes.root, words)
            .iter()
            .any(|file| file.starts_with("secretary-memory-texts/")),
        "the search sees the text store's copy while the card stands"
    );
    let dismissed = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/dismiss"),
        &serde_json::json!({"reason": "not from a newsletter"}),
    );
    assert_eq!(common::status(&dismissed), 200);
    only_the_call(&fakes, &id, words);
    gone(&settled(&base, &id));
    let listed = serde_json::Value::from(common::inbox_listing(&base, "2026-10-03T12:00:00"));
    assert!(
        listed
            .to_string()
            .contains("remember (person): its words are deleted"),
        "the Inbox, read as the tab reads it, says the words are gone: {listed}"
    );
    assert!(!listed.to_string().contains(words), "{listed}");

    let (fakes, base) = served("card-forgotten", script());
    let (id, _) = said(&base, "who is Mira?");
    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let forgot = common::act(
        &format!("{base}/api/secretary/memory/1/forget"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&forgot), 200);
    erased_by_forgetting(&fakes, &id, words);
    gone(&settled(&base, &id));
}
