//! THE SERVER'S RESIDENT MEMORY DOES NOT GROW WITH THE ROUNDS IT ANSWERS (servememory,
//! 2026-11-02): the real `suzatary-dev serve` over a `fixture-chain` store of the live shape's
//! kind, driven by rounds of the page's request mix (a warm load's reads, a cold pull of the
//! lines, the inbox and the topics, a window of the lines' literals, a said line), with the
//! allocator as `scripts/serve-web.sh` runs it. After `ROUNDS` more rounds the process's resident
//! set has grown by no more than `SLACK_MB`, and its high-water mark stays under `BOUND_MB`.
//! It is the whole server's bound; `the_store_keeps_only_its_newest_version` (`io/src/objects.rs`)
//! is the store's law beneath it.
//!
//! The shape is the e2e lines server's, a tenth of it, so the law runs in the suite; the live
//! shape is `SUZATARY_RESIDENT_LINES=14400 SUZATARY_RESIDENT_CONVERSATIONS=100` (each count
//! below has its variable), which a release build measured at 0.86 to 0.92 GB over 24 rounds.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The rounds before the first reading, and again before the second.
const ROUNDS: usize = 4;
/// How much the second reading may exceed the first: what one round's answer and an allocator's
/// page reuse can leave, never a store's size.
const SLACK_MB: u64 = 64;
/// The most the process may ever hold at the default shape, in a debug build: it measured 167 MB.
/// A larger shape names its own, `SUZATARY_RESIDENT_BOUND_MB`.
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

/// The fixture binary, from a clean environment ([`crate::binary::fixture`]), with the box's
/// signing seeds and a state directory of its own, as `scripts/web-e2e.sh` runs it.
fn fixture(root: &Path) -> Command {
    use suzatary_cli::commands::fixture::{AGENT_SEED, BOX_SEED};
    let mut command = crate::binary::fixture();
    command
        .env("XDG_STATE_HOME", root.join("state"))
        .env(suzatary_core::signatures::BOX_KEY, BOX_SEED)
        .env(suzatary_core::signatures::AGENT_KEY, AGENT_SEED)
        .env(
            suzatary_core::mail::dedup_key::DEDUP_KEY,
            "e2e0000000000000000000000000000000000000000000000000000000000d3d",
        )
        .env("MALLOC_ARENA_MAX", "2");
    command
}

/// The server, ended when the test is.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The fixture's chain and stores at the test's shape, then its server, answering once its log
/// says it listens.
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
    let script = root.join("model-script.json");
    std::fs::write(
        &script,
        r#"[{"when": "", "reply": "{\"say\": \"Noted.\"}"}]"#,
    )
    .expect("the script writes");
    let mut child = fixture(root)
        .arg("serve")
        .arg("--instance")
        .arg(root.join("box/instance.toml"))
        .args(["--whois-fixture", "e2e-writer@example.invalid"])
        .arg("--model-fixture")
        .arg(&script)
        .arg("--smtp-fixture")
        .arg(root.join("sent"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let said = child.stdout.take().expect("piped");
    let mut why = child.stderr.take().expect("piped");
    let mut serving = Serving(child);
    // The server says it serves once its port is bound; the rest of its log is read and dropped,
    // so the pipe never fills under the server.
    let mut lines = std::io::BufRead::lines(std::io::BufReader::new(said));
    let listened = lines
        .by_ref()
        .map_while(Result::ok)
        .any(|line| line.contains("serve: http"));
    if !listened {
        let _ = serving.0.wait();
        let mut said = String::new();
        let _ = std::io::Read::read_to_string(&mut why, &mut said);
        panic!("the server ended before it listened on {port}: {said}");
    }
    std::thread::spawn(move || std::io::copy(&mut why, &mut std::io::sink()));
    std::thread::spawn(move || lines.for_each(drop));
    (serving, format!("http://127.0.0.1:{port}"))
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
        "/api/usage",
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
    for hashes in window.chunks(suzatary_core::query::MOST_HASHES) {
        let joined: Vec<&str> = hashes.iter().map(|hash| hash.as_str()).collect();
        get(
            base,
            &format!("/api/stores/lines/content?hashes={}", joined.join(",")),
        );
    }
    let said = format!("/api/secretary/conversations/bulk-{}/say", round % 20);
    let body = serde_json::to_vec(&serde_json::json!({"say": format!("one more, round {round}")}))
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
                suzatary_core::gate::CLIENT_HEADER,
                suzatary_core::gate::CLIENT_MIN.to_string(),
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
    let pid = serving.0.id();
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
