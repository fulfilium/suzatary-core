//! The server's resident memory does not grow with the rounds of the page's request mix it
//! answers, with the allocator as `scripts/serve-web.sh` runs it: after `ROUNDS` more rounds the
//! resident set has grown by no more than `SLACK_MB`, and its high-water mark stays under
//! `BOUND_MB`, at a tenth of the e2e lines server's shape (each count has its
//! `SUZATARY_RESIDENT_*` variable).

use crate::common;

use std::path::{Path, PathBuf};

use crate::binary::{Serving, keyed_fixture as fixture};

/// The rounds before the first reading, and again before the second.
const ROUNDS: usize = 4;
/// How much the second reading may exceed the first: what one round's answer and an allocator's
/// page reuse can leave, never a store's size.
const SLACK_MB: u64 = 64;
/// The most the process may hold at the default shape in a debug build; a larger shape names its
/// own, `SUZATARY_RESIDENT_BOUND_MB`.
const BOUND_MB: usize = 384;

/// A count from the environment, or its default.
fn shape(name: &str, default: usize) -> String {
    std::env::var(format!("SUZATARY_RESIDENT_{name}")).unwrap_or_else(|_| default.to_string())
}

/// A field of `/proc/<pid>/status`, in MB.
fn status_mb(pid: u32, field: &str) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("the status reads");
    status
        .lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|value| value.trim().strip_suffix(" kB")?.parse::<u64>().ok())
        .map(|kb| kb / 1024)
        .expect("the field is there")
}

/// The fixture's chain and stores at the test's shape, then its server.
fn served(root: &Path) -> (Serving, String) {
    let port = crate::ports::outside_ephemeral();
    std::fs::create_dir_all(root.join("state")).expect("a directory");
    let built = fixture(root)
        .args(["fixture-chain", "--root"])
        .arg(root.join("box"))
        .args(["--port", &port.to_string(), "--repo"])
        // Only `--settings` reads the checkout, and no page is loaded, so the frontend is not needed.
        .arg(root)
        .args(["--lines", &shape("LINES", 1_440)])
        .args(["--conversations", &shape("CONVERSATIONS", 20)])
        .args(["--reseals", &shape("RESEALS", 150)])
        .args(["--proposals", &shape("PROPOSALS", 40)])
        .args(["--topics", &shape("TOPICS", 90)])
        .args(["--todos", &shape("TODOS", 60)])
        .output()
        .expect("the fixture runs");
    assert!(
        built.status.success(),
        "fixture-chain: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let serving = Serving::start(root, &root.join("box/instance.toml"));
    let base = format!("http://127.0.0.1:{port}");
    (serving, base)
}

/// A key the box trusts, trusted at the terminal as `suzatary device trust` does.
fn trusted(root: &Path) -> prodrome_core::sign::Secret {
    let key = prodrome_core::sign::Secret::new(&"7".repeat(64)).expect("a seed");
    let trusted = fixture(root)
        .arg("--instance")
        .arg(root.join("box/instance.toml"))
        .args(["device", "trust", key.public().as_str()])
        .output()
        .expect("the verb runs");
    assert!(
        trusted.status.success(),
        "device trust: {}",
        String::from_utf8_lossy(&trusted.stderr)
    );
    key
}

fn get(base: &str, path: &str) -> serde_json::Value {
    let mut answer = ureq::get(&format!("{base}{path}"))
        .call()
        .unwrap_or_else(|failed| panic!("GET {path}: {failed}"));
    answer
        .body_mut()
        .read_json()
        .unwrap_or(serde_json::Value::Null)
}

/// A store pulled from nothing to its end, piece by piece, as a cold tab pulls it: every name it
/// was sent.
fn pulled(base: &str, path: &str) -> Vec<String> {
    let mut since = String::new();
    let mut names = Vec::new();
    loop {
        let query = if since.is_empty() {
            String::new()
        } else {
            format!("?since={since}")
        };
        let piece = get(base, &format!("{path}{query}"));
        let nodes = piece["nodes"].as_array().or(piece["objects"].as_array());
        names.extend(
            nodes
                .into_iter()
                .flatten()
                .filter_map(|node| node["hash"].as_str().map(str::to_owned)),
        );
        piece["frontier"]["digest"]
            .as_str()
            .unwrap_or("")
            .clone_into(&mut since);
        if piece["rest"].as_u64().unwrap_or(0) == 0 {
            return names;
        }
    }
}

/// A tab's question in conversation `id`, sealed over its heads and signed by `key`, as the post
/// that hands it to the lines.
fn question(
    base: &str,
    key: &prodrome_core::sign::Secret,
    id: &str,
    round: usize,
) -> serde_json::Value {
    use suzatary_lines::secretary::line_event::LineEvent;
    let (mut nodes, mut genesis, mut since) = (Vec::new(), None, String::new());
    loop {
        let query = if since.is_empty() {
            String::new()
        } else {
            format!("?since={since}")
        };
        let piece = get(base, &format!("/api/stores/lines/skeleton{query}"));
        genesis = genesis.or_else(|| piece["geneses"][0].as_str().map(str::to_owned));
        nodes.extend(piece["nodes"].as_array().into_iter().flatten().cloned());
        piece["frontier"]["digest"]
            .as_str()
            .unwrap_or("")
            .clone_into(&mut since);
        if piece["rest"].as_u64().unwrap_or(0) == 0 {
            break;
        }
    }
    let ours: Vec<&serde_json::Value> = nodes
        .iter()
        .filter(|n| n["header"]["conversation"] == id)
        .collect();
    let under: std::collections::BTreeSet<&str> = ours
        .iter()
        .flat_map(|n| n["parents"].as_array().into_iter().flatten())
        .filter_map(serde_json::Value::as_str)
        .collect();
    let heads = ours
        .iter()
        .filter_map(|n| n["hash"].as_str())
        .filter(|n| !under.contains(n))
        .map(|n| prodrome_core::event::Hash::new(n).expect("a name"))
        .collect();
    let actor = get(base, "/api/me")["actor"]
        .as_str()
        .expect("an actor")
        .to_owned();
    let entry = suzatary_conversations::secretary::store::Entry::Said {
        at: prodrome_core::literal::Datetime::new(
            2026,
            11,
            17,
            9,
            0,
            0,
            u32::try_from(round).unwrap_or(0),
        )
        .expect("an instant"),
        turn: suzatary_conversations::secretary::model::Turn::Person {
            text: format!("one more, round {round}"),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(suzatary_inbox::secretary::stamp::Stamp::Actor(
            suzatary_inbox::secretary::stamp::Speaker::device(&actor),
        )),
    };
    let (line, keys) = LineEvent::write(
        suzatary_base::conversation_id::ConversationId::new(id).expect("an id"),
        prodrome_core::event::Actor::new(&actor).expect("an actor"),
        u32::try_from(ours.len()).expect("a count"),
        &entry,
        &suzatary_io_local::texts::SystemEntropy,
    )
    .expect("a line");
    let genesis =
        prodrome_core::event::Hash::new(genesis.expect("the lines began")).expect("a name");
    let change = prodrome_core::event::Envelope::Change(
        prodrome_core::change::mk_change(genesis, heads, line).expect("a change"),
    );
    let name = prodrome_core::event::seal_hash(&change);
    let signature: prodrome_core::event::Envelope<LineEvent> =
        prodrome_core::event::Envelope::Signed(key.sign(&name));
    serde_json::json!({
        "objects": [
            prodrome_core::event::canonical_envelope(&change),
            prodrome_core::event::canonical_envelope(&signature),
        ],
        "keys": { name.as_str(): keys.iter().map(|k| hex::encode(k.bytes())).collect::<Vec<_>>() },
    })
}

/// One round of the page's request mix; `round` picks the conversation and the window of lines.
fn round(base: &str, key: &prodrome_core::sign::Secret, round: usize) {
    for path in [
        "/api/about",
        "/api/me",
        "/api/overview",
        "/api/waiting",
        "/api/fold",
        "/api/life/fold",
        "/api/secretary/record",
        "/api/secretary/memory",
        "/api/briefings",
    ] {
        get(base, path);
    }
    let lines = pulled(base, "/api/stores/lines/skeleton");
    for store in ["inbox", "topics", "topic-aliases"] {
        pulled(base, &format!("/api/stores/{store}/objects"));
    }
    let window: std::collections::BTreeSet<&String> = lines
        .iter()
        .skip(round * 256 % lines.len().max(1))
        .take(256)
        .collect();
    let window: Vec<&String> = window.into_iter().collect();
    for hashes in window.chunks(suzatary_api::query::MOST_HASHES) {
        let joined: Vec<&str> = hashes.iter().map(|hash| hash.as_str()).collect();
        get(
            base,
            &format!("/api/stores/lines/content?hashes={}", joined.join(",")),
        );
    }
    let said = "/api/stores/lines/objects".to_owned();
    let body = serde_json::to_vec(&question(base, key, &format!("bulk-{}", round % 20), round))
        .expect("JSON prints");
    let at = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let nonce = format!("{round:032x}");
    let signed = suzatary_cli::commands::call::Signed::of("POST", &said, &body, key, &at, &nonce);
    // A refusal is read, not raised, so its status and the box's words reach the failure.
    let agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent();
    let request = signed.headers.into_iter().fold(
        agent
            .post(&format!("{base}{said}"))
            .header("x-suzatary-act", "1")
            .header(
                suzatary_signatures::signatures::CLIENT_HEADER,
                suzatary_signatures::signatures::CLIENT_MIN.to_string(),
            )
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json"),
        |request, (name, value)| request.header(name, value),
    );
    let mut answer = request
        .send(&body[..])
        .unwrap_or_else(|failed| panic!("POST {said}: {failed}"));
    let status = answer.status();
    assert!(
        status.is_success(),
        "POST {said} at {at}: {status}: {}",
        answer.body_mut().read_to_string().unwrap_or_default()
    );
}

#[test]
fn the_servers_resident_memory_does_not_grow_with_its_rounds() {
    let root: PathBuf = common::scratch("resident");
    let (serving, base) = served(&root);
    let key = trusted(&root);
    let pid = serving.id();
    let mut at = 0;
    let mut reading = || {
        for _ in 0..ROUNDS {
            round(&base, &key, at);
            at += 1;
        }
        status_mb(pid, "VmRSS")
    };
    let first = reading();
    let second = reading();
    let high = status_mb(pid, "VmHWM");
    eprintln!(
        "resident: {first} MB after {ROUNDS} rounds, {second} MB after {at}, high-water {high} MB"
    );
    assert!(
        second <= first + SLACK_MB,
        "the resident set grew from {first} MB to {second} MB over {ROUNDS} more rounds"
    );
    let bound: u64 = shape("BOUND_MB", BOUND_MB).parse().expect("a number of MB");
    assert!(
        high <= bound,
        "the high-water mark is {high} MB, over {bound} MB"
    );
}
