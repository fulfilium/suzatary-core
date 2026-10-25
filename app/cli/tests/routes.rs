//! The small routes: chain segments, `/api/tip`, device tokens and a read's tag.

mod common;

use common::{Fakes, actor, deployment, login, v4};
use suzatary_core::store::Reads as _;

fn writer() -> (String, Fakes) {
    let fakes =
        Fakes::new("routes").whois(common::FakeWhois::new().naming(v4(), login("her@x.test")));
    (String::new(), fakes)
}

/// A bare path means the first chain, answering byte for byte as its named path does.
#[test]
fn a_bare_path_and_the_first_chains_path_are_one_answer() {
    let fakes = Fakes::new("routes-chain-bare");
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment([], []),
        common::loopback_is_this_box(),
    );
    // Clock-free routes only, since a folded answer carries the instant it was read at.
    for (bare, named) in [
        ("/api/chain", "/api/life/chain"),
        ("/api/tip", "/api/life/tip"),
    ] {
        let mut one = common::get(&format!("{base}{bare}"));
        let mut two = common::get(&format!("{base}{named}"));
        assert_eq!(common::status(&one), 200, "{bare}");
        assert_eq!(common::status(&two), 200, "{named}");
        assert_eq!(
            common::body_json(&mut one),
            common::body_json(&mut two),
            "{bare} and {named} must be one answer about one store"
        );
    }
    // A fold carries the instant it was read at, so its two answers are compared by their todos.
    let todos = |path: &str| -> Vec<String> {
        let mut fold = common::get(&format!("{base}{path}"));
        assert_eq!(common::status(&fold), 200, "{path}");
        let mut todos: Vec<String> = common::body_json(&mut fold)["entries"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|entry| entry["todo"].as_str().unwrap_or_default().to_owned())
            .collect();
        todos.sort();
        todos
    };
    assert_eq!(todos("/api/fold"), todos("/api/life/fold"));
}

/// Asks the router, in both directions, rather than trusting the reserved list.
#[test]
fn the_reserved_names_are_exactly_the_ones_a_static_route_shadows() {
    let fakes = Fakes::new("routes-chain-reserved");
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment([], []),
        common::loopback_is_this_box(),
    );
    let reaches_the_chain_router = |name: &str| -> bool {
        let mut response = common::get(&format!("{base}/api/{name}/fold"));
        common::body_json(&mut response)["error"]
            .as_str()
            .unwrap_or_default()
            .contains("no chain named")
    };
    for name in suzatary_cli::instance::RESERVED {
        assert!(
            !reaches_the_chain_router(name),
            "{name:?} reaches the chain router, so it need not be reserved"
        );
    }
    // `mail` is usable because matchit backtracks past a static prefix that has no match.
    for usable in [
        "mail", "chain", "fold", "me", "tip", "tree", "objects", "author", "content", "stores",
    ] {
        assert!(
            reaches_the_chain_router(usable),
            "{usable:?} is a usable chain name and must not be reserved"
        );
        assert!(
            !suzatary_cli::instance::RESERVED.contains(&usable),
            "{usable:?} is reserved and need not be"
        );
    }
}

/// A phone polls the tip, so a cached answer would tell it nothing moved when something did.
#[test]
fn the_tip_route_is_tiny_and_true_and_never_cached() {
    let fakes = Fakes::new("routes-tip");
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment([], []),
        common::loopback_is_this_box(),
    );
    let mut response = common::get(&format!("{base}/api/tip"));
    assert_eq!(common::status(&response), 200);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "a polled tip must never come from a cache"
    );
    let payload = common::body_json(&mut response);
    let heads = payload["heads"].as_array().expect("the heads");
    assert!(!heads.is_empty(), "a chain has a head: {payload}");
    assert!(
        heads
            .iter()
            .all(|head| head.as_str().map(str::len) == Some(64)),
        "a head is an object hash"
    );
    assert!(
        payload.to_string().len() < 20 + 67 * heads.len(),
        "the tip payload is its heads and nothing else: {payload}"
    );
    assert_eq!(payload.as_object().expect("an object").len(), 1);
}

/// A push token appends nothing, so no fold would notice this gate failing.
#[test]
fn a_named_peer_registers_a_token_and_loopback_may_not() {
    let (_, fakes) = writer();
    let heads_before = { fakes.chain.tips().expect("derives") };
    let base = common::Server::new(
        std::sync::Arc::new(fakes),
        deployment([login("her@x.test")], [(login("her@x.test"), actor("ada"))]),
        Vec::new(),
    )
    .start()
    .base;
    let token = serde_json::json!({"platform": "android", "token": "a-token-from-the-phone"});
    let mut response = common::act(&format!("{base}/api/act/device"), &token);
    assert_eq!(common::status(&response), 200);
    let payload = common::body_json(&mut response);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["devices"], 1);

    let mut tip = common::get(&format!("{base}/api/tip"));
    assert_eq!(
        common::body_json(&mut tip)["heads"],
        serde_json::json!(
            heads_before
                .iter()
                .map(prodrome_core::event::Hash::as_str)
                .collect::<Vec<_>>()
        ),
        "registering a device must not append an object"
    );
}

#[test]
fn a_device_registration_from_loopback_is_refused() {
    let fakes = Fakes::new("routes-device-loopback");
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment([login("her@x.test")], [(login("her@x.test"), actor("ada"))]),
        common::loopback_is_this_box(),
    );
    let mut response = common::act(
        &format!("{base}/api/act/device"),
        &serde_json::json!({"platform": "android", "token": "t"}),
    );
    assert_eq!(common::status(&response), 403);
    assert_eq!(
        common::body_json(&mut response)["error"],
        "a write must come from a named tailnet peer; loopback and the local shell use the CLI"
    );
}

/// The page's report of the Android app's token (`platform`, `token`) is taken, and a second
/// report of the same token answers the same: one phone, whatever the count of reports.
#[test]
fn the_android_apps_report_and_its_retry_register_one_phone() {
    let (_, fakes) = writer();
    let base = common::Server::new(
        std::sync::Arc::new(fakes),
        deployment([login("her@x.test")], [(login("her@x.test"), actor("ada"))]),
        Vec::new(),
    )
    .start()
    .base;
    let report = serde_json::json!({
        "platform": "android",
        "token": "a-token-from-the-phone",
    });
    for _ in 0..2 {
        let mut response = common::act(&format!("{base}/api/act/device"), &report);
        assert_eq!(common::status(&response), 200);
        assert_eq!(
            common::body_json(&mut response),
            serde_json::json!({"ok": true, "devices": 1})
        );
    }
}

/// A token is one line and a platform one of two, since the file's format is lines.
#[test]
fn a_malformed_platform_or_token_is_refused_before_it_is_written() {
    let (_, fakes) = writer();
    let base = common::Server::new(
        std::sync::Arc::new(fakes),
        deployment([login("her@x.test")], [(login("her@x.test"), actor("ada"))]),
        Vec::new(),
    )
    .start()
    .base;
    for (body, why) in [
        (
            serde_json::json!({"platform": "windows", "token": "t"}),
            "unknown variant `windows`, expected `android` or `ios`",
        ),
        (
            serde_json::json!({"platform": "android", "token": "t", "app": "x"}),
            "unknown field `app`, expected `platform` or `token`",
        ),
        (
            serde_json::json!({"platform": "android", "token": ""}),
            "token must be one non-empty line",
        ),
        (
            serde_json::json!({"platform": "android", "token": "two\tfields"}),
            "token must be one non-empty line",
        ),
    ] {
        let mut response = common::act(&format!("{base}/api/act/device"), &body);
        assert_eq!(common::status(&response), 400, "{body}");
        assert_eq!(common::body_json(&mut response)["error"], why);
    }
}

/// A read's tag is a digest of its bytes: the same answer asked with its tag is a 304 with no
/// body, and an answer the chain moved under carries a new tag and its new bytes.
#[test]
fn a_read_answers_304_to_its_own_tag_and_a_new_tag_once_the_chain_moves() {
    let root = common::scratch("routes-etag");
    let events = common::six_todo_chain(&root);
    let base = common::start(
        std::sync::Arc::new(Fakes::with_store("routes-etag", events)),
        deployment([], []),
        common::loopback_is_this_box(),
    );
    let asked = |held: Option<&str>| {
        let request = common::agent().get(&format!("{base}/api/tip"));
        let request = match held {
            Some(tag) => request.header("if-none-match", tag),
            None => request,
        };
        request.call().expect("the request completes")
    };
    let tag_of = |response: &ureq::http::Response<ureq::Body>| {
        response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .expect("a read carries a tag")
            .to_owned()
    };

    let mut first = asked(None);
    assert_eq!(common::status(&first), 200);
    let tag = tag_of(&first);
    assert!(
        tag.starts_with("W/\""),
        "weak, so it holds under every coding: {tag}"
    );
    let was = common::body_json(&mut first);

    let mut same = asked(Some(&tag));
    assert_eq!(common::status(&same), 304);
    assert_eq!(tag_of(&same), tag);
    assert_eq!(
        same.headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "a 304 says what the 200 would"
    );
    assert!(
        same.body_mut()
            .read_to_vec()
            .expect("an empty body")
            .is_empty()
    );

    let events = suzatary_core::record::EventStore::new(
        root.join("events"),
        prodrome_core::policy::Untrusted::of([actor("triage")]),
    );
    suzatary_core::store::append_to(
        &events,
        suzatary_core::record::GENESIS,
        prodrome_core::event::mk_created("a-new-todo", common::now(), "ada", "A new todo", "")
            .expect("valid"),
    )
    .expect("appends");
    let mut after = asked(Some(&tag));
    assert_eq!(
        common::status(&after),
        200,
        "the bytes moved, so the tag did"
    );
    assert_ne!(tag_of(&after), tag);
    assert_ne!(common::body_json(&mut after), was);
}
