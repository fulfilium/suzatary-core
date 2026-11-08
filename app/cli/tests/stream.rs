//! The live stream, `GET /api/secretary/conversations/<id>/stream`, over a real server with
//! `v4` committed to `ada@example.com` (see `common`'s header on peer identities). Only a
//! `resume` streams deltas (`SessionModel::open` carries no `on_delta`), so the delta scenes
//! read an established session's second round.

mod common;

use std::io::BufRead;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use common::{FakeModel, Fakes, Gate};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn writer(tag: &str, model: FakeModel) -> (String, Arc<suzatary_cli::serve::Workers>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new(tag).whois(whois).model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, workers, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    (base, workers)
}

fn open(base: &str) -> String {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    assert_eq!(common::status(&opened), 200);
    common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

fn say(base: &str, id: &str, text: &str) -> ureq::http::Response<ureq::Body> {
    common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": text}),
    )
}

/// Subscribe synchronously: `.call()` returns once the headers arrive, which for
/// `Sse::into_response()` is after `Workers::subscribe` ran, so a later `say` cannot race the
/// subscription. The body is read on a background thread, over a channel.
fn subscribe(url: &str) -> mpsc::Receiver<String> {
    let response = common::agent()
        .get(url)
        .call()
        .expect("the stream connects");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(response.into_body().into_reader());
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if tx.send(data.to_owned()).is_err() {
                break;
            }
        }
    });
    rx
}

/// The next frame, as parsed JSON; a timeout is a failure, not a retry.
fn next(rx: &mpsc::Receiver<String>) -> serde_json::Value {
    let line = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a frame arrives");
    serde_json::from_str(&line).unwrap_or_else(|failed| panic!("{line} is not JSON: {failed}"))
}

/// Subscribe, then say "first" and read that round's frames to its last.
///
/// The transcript reads idle as soon as a round appends its answer, before the worker
/// broadcasts the round's `turn` and `state`, so a subscriber that waited on the transcript
/// instead could connect between the two and read the first round's tail as the second's.
fn first_round(base: &str, id: &str) -> mpsc::Receiver<String> {
    let rx = subscribe(&format!("{base}/api/secretary/conversations/{id}/stream"));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "idle"}),
        "the connect-time frame, before anything is asked"
    );
    assert_eq!(common::status(&say(base, id, "first")), 202);
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "thinking"})
    );
    assert_eq!(next(&rx), serde_json::json!({"kind": "turn"}));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "idle"})
    );
    rx
}

/// A subscriber connected before the say sees the round frame by frame,
/// in order, with the exact shape `wire::Frame` declares.
#[test]
fn a_connected_subscriber_sees_the_round_frame_by_frame() {
    let (base, _workers) = writer(
        "stream-connected",
        FakeModel::saying(["{\"say\": \"hello\"}", "{\"say\": \"got both\"}"])
            .streaming([vec!["Hel", "lo"]]),
    );
    let id = open(&base);

    // The first round opens the session, so it is the second, a resume, that streams.
    let rx = first_round(&base, &id);

    assert_eq!(common::status(&say(&base, &id, "second")), 202);

    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "thinking"})
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "Hel"})
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "lo"})
    );
    assert_eq!(next(&rx), serde_json::json!({"kind": "turn"}));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "idle"})
    );

    // And the folded payload agrees: `state` reads `idle` and neither person turn is `queued`.
    let payload = common::settled(&base, &id);
    assert_eq!(payload["state"], "idle");
    let turns = payload["turns"].as_array().expect("turns");
    assert!(
        turns
            .iter()
            .filter(|turn| turn["who"] == "person")
            .all(|turn| turn["queued"] == serde_json::Value::Bool(false)),
        "every person turn was answered: {turns:?}"
    );
}

/// A subscriber connecting mid-turn receives the accumulated partial as
/// one `delta`, whole, before anything live.
#[test]
fn a_late_subscriber_receives_the_accumulated_partial_first() {
    let mid = Gate::new();
    let (base, _workers) = writer(
        "stream-midturn",
        FakeModel::saying(["{\"say\": \"hello\"}", "{\"say\": \"answer\"}"])
            .streaming([vec!["Hel", "lo, ", "world"]])
            .mid_stream(mid.clone()),
    );
    let id = open(&base);

    assert_eq!(common::status(&say(&base, &id, "first")), 202);
    common::settled(&base, &id);

    assert_eq!(common::status(&say(&base, &id, "second")), 202);
    // The mid-stream gate holds after "Hel" is sent, so the partial is "Hel" and the round is
    // still running while this connects.
    mid.wait_until_entered();

    let rx = subscribe(&format!("{base}/api/secretary/conversations/{id}/stream"));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "answering"}),
        "a nonempty partial is reported as `answering`, never bare `thinking`"
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "Hel"}),
        "the accumulated partial, whole, as ONE delta"
    );

    mid.release();

    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "lo, "})
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "world"})
    );
    assert_eq!(next(&rx), serde_json::json!({"kind": "turn"}));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "idle"})
    );
}

/// A `working` frame broadcasts before the verb's own turn: the model asks for `context` in its
/// first round (a `working` frame is no `Delta`, so `open` shows it), between `thinking` and
/// `idle`.
#[test]
fn a_working_frame_broadcasts_before_a_verbs_own_turn() {
    let (base, _workers) = writer(
        "stream-working",
        FakeModel::saying([
            "{\"say\": \"Checking.\", \"calls\": [{\"verb\": \"context\", \"args\": {}}]}",
            "{\"say\": \"All set.\"}",
        ]),
    );
    let id = open(&base);

    let rx = subscribe(&format!("{base}/api/secretary/conversations/{id}/stream"));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "idle"}),
        "the connect-time frame, before anything is asked"
    );

    assert_eq!(
        common::status(&say(&base, &id, "what should I check?")),
        202
    );

    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "thinking"})
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "working", "verb": "context"}),
        "the working frame lands BEFORE the verb's own turn is on the transcript"
    );
    // The second ask's reply arrives as one `delta`, since no fragments were configured; this
    // scene is about ordering, not fragmentation.
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "{\"say\": \"All set.\"}"})
    );
    assert_eq!(next(&rx), serde_json::json!({"kind": "turn"}));
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "idle"})
    );

    // And the folded payload agrees: the verb ran, and its answer is on the transcript.
    let payload = common::settled(&base, &id);
    let turns = payload["turns"].as_array().expect("turns");
    assert!(
        turns
            .iter()
            .any(|turn| turn["who"] == "verb" && turn["verb"] == "context"),
        "the context verb's own turn is on the transcript: {turns:?}"
    );
}

/// A block closes at a top-level blank line: its frame names it by sha and carries its source,
/// the sources tile the front of the deltas, a late subscriber is replayed the text and then
/// the blocks, and the render route answers by that sha alone; a sha no round closed is a 404,
/// whatever text the page might have sent.
#[test]
fn a_closed_block_streams_by_its_sha_and_renders_by_it_alone() {
    let mid = Gate::new();
    let (base, _workers) = writer(
        "stream-blocks",
        FakeModel::saying(["{\"say\": \"hello\"}", "{\"say\": \"= Plan\\n\\nFirst.\"}"])
            .streaming([vec!["= Pl", "an\n\nFir", "st."]])
            .mid_stream(mid.clone()),
    );
    let id = open(&base);
    let rx = first_round(&base, &id);
    assert_eq!(common::status(&say(&base, &id, "second")), 202);
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "state", "state": "thinking"})
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "= Pl"})
    );
    mid.wait_until_entered();
    mid.release();
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "an\n\nFir"})
    );
    let block = next(&rx);
    let sha = suzatary_core::answer::sha("= Plan\n\n");
    assert_eq!(
        block,
        serde_json::json!({"kind": "block", "n": 0, "sha": sha, "source": "= Plan\n\n"}),
        "the heading closed at the blank line; \"Fir\" is still the open tail"
    );
    assert_eq!(
        next(&rx),
        serde_json::json!({"kind": "delta", "text": "st."})
    );
    assert_eq!(next(&rx), serde_json::json!({"kind": "turn"}));
    // The round is over and nothing streams, but the block is the stored answer's now, so it
    // still renders by its sha.
    assert_eq!(
        common::status(&common::get(&format!(
            "{base}/api/secretary/conversations/{id}/blocks/{sha}"
        ))),
        200
    );
}

/// While a round streams, a closed block renders by its sha, and a sha no
/// answer holds is a 404: the page cannot make the box render text.
#[test]
fn a_streaming_block_renders_by_its_sha() {
    let mid = Gate::new();
    let (base, _workers) = writer(
        "stream-block-render",
        FakeModel::saying([
            "{\"say\": \"hello\"}",
            "{\"say\": \"*Due* Friday.\\n\\nMore\"}",
        ])
        .streaming([vec!["*Due* Friday.\n\nMo", "re"]])
        .mid_stream(mid.clone()),
    );
    let id = open(&base);
    assert_eq!(common::status(&say(&base, &id, "first")), 202);
    common::settled(&base, &id);
    assert_eq!(common::status(&say(&base, &id, "second")), 202);
    mid.wait_until_entered();

    let sha = suzatary_core::answer::sha("*Due* Friday.\n\n");
    let mut rendered = common::get(&format!(
        "{base}/api/secretary/conversations/{id}/blocks/{sha}"
    ));
    assert_eq!(common::status(&rendered), 200);
    assert_eq!(
        common::body_json(&mut rendered),
        serde_json::json!({
            "html": "<p>*Due* Friday.</p>",
            "key": suzatary_core::rendering::ANSWER.key(&suzatary_core::answer::Renderable::of("*Due* Friday.\n\n")).expect("a block prints"),
        }),
        "the fake renderer's answer for exactly the block the model wrote"
    );
    let unknown = suzatary_core::answer::sha("#html.elem(\"script\")[]");
    assert_eq!(
        common::status(&common::get(&format!(
            "{base}/api/secretary/conversations/{id}/blocks/{unknown}"
        ))),
        404
    );
    mid.release();
    common::settled(&base, &id);
}
