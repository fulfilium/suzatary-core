//! A coding session from the phone, over a real server with a scripted model: the tap fires a
//! routine at a loopback fake of the documented endpoint, never the real one; the poll reads a
//! local repository's `claude/` branches and merges nothing; "Build it" files the card for a
//! todo, at most two at once.

mod common;

use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::sync::Arc;

use common::{FakeModel, Fakes};
use suzatary_cli::commands::coding::Firing;
use suzatary_core::cast::CAST;

const TOKEN: &str = "sk-ant-routine-fake";
const URL: &str = "https://claude.ai/code/session_01fake";

fn owner() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    suzatary_core::deployment::Deployment {
        coding: Some(suzatary_core::coding::Coding {
            routine: "trig_01fake".parse().expect("a routine id"),
            repo: "bmabsout/cv".parse().expect("a repository"),
        }),
        ..common::deployment([owner()], [(owner(), common::actor(CAST.owner.actor))])
    }
}

/// The routine's token, and nothing else, by its one name.
struct Holding;

impl suzatary_io::secrets::Resolves for Holding {
    fn get(
        &self,
        name: &str,
        _reason: &str,
    ) -> Result<Option<suzatary_core::capability::Secret>, suzatary_io::secrets::SecretError> {
        Ok((name == suzatary_core::coding::ROUTINE_TOKEN)
            .then(|| suzatary_core::capability::Secret::new(TOKEN)))
    }
}

/// A loopback fake of the fire endpoint: answers one request with the
/// documented body and hands back the request line, headers and body.
fn fake_routine() -> (String, std::thread::JoinHandle<(Vec<String>, String)>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    let served = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("a connection");
        let mut reader = BufReader::new(stream.try_clone().expect("clones"));
        let mut head = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("a line");
            if line.trim_end().is_empty() {
                break;
            }
            head.push(line.trim_end().to_owned());
        }
        let length: usize = head
            .iter()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|n| n.trim().parse().expect("a length"))
            })
            .expect("a length");
        let mut body = vec![0; length];
        reader.read_exact(&mut body).expect("the body");
        let answer = format!(
            r#"{{"claude_code_session_id":"session_01fake","claude_code_session_url":"{URL}"}}"#
        );
        let mut stream = stream;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
            answer.len()
        )
        .expect("answers");
        (head, String::from_utf8(body).expect("utf-8"))
    });
    (base, served)
}

const TASK: &str = "Retitle the CV to Principal investigator.\\nRun the gates.";

fn scene(tag: &str, firing: Firing) -> (String, Arc<Fakes>) {
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(common::FakeWhois::new().naming(common::v4(), owner()))
            .model(FakeModel::saying([
                format!(
                    r#"{{"say": "A session would do it.", "calls": [{{"verb": "propose_coding_session", "args": {{
                        "task": "{TASK}",
                        "why": "The title is out of date and the change needs the gates.",
                        "sources": "todo:life/permitfiling"
                    }}}}]}}"#
                )
                .leak(),
                r#"{"say": "Proposed."}"#,
            ])),
    );
    let server = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .real_acts(
            tag,
            "her@study.test",
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .firing(firing);
    (server.start().base, fakes)
}

fn proposed(base: &str) -> (String, serde_json::Value) {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The CV"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({ "say": "retitle my cv properly" }),
    );
    assert_eq!(common::status(&said), 202);
    for _ in 0..200 {
        let payload = conversation(base, &id);
        if payload["state"] == "idle" {
            return (id, payload);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the exchange never finished");
}

fn conversation(base: &str, id: &str) -> serde_json::Value {
    common::reading(base, id, &[])
}

fn answer(base: &str, id: &str, verb: &str, body: &serde_json::Value) -> (u16, serde_json::Value) {
    let mut response = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/{verb}"),
        body,
    );
    (common::status(&response), common::body_json(&mut response))
}

/// A task is the whole of what a session is told, so a round that read mail proposes none: the
/// verb refuses and says why, and no card is filed.
#[test]
fn a_round_that_read_mail_proposes_no_session() {
    let fakes = Arc::new(
        Fakes::new("coding-tainted")
            .whois(common::FakeWhois::new().naming(common::v4(), owner()))
            .model(FakeModel::saying([
                r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "cv"}}]}"#,
                format!(
                    r#"{{"say": "", "calls": [{{"verb": "propose_coding_session", "args": {{
                        "task": "{TASK}",
                        "why": "A mail asked for it.",
                        "sources": "mail:a1b2c3d4e5f6"
                    }}}}]}}"#
                )
                .leak(),
                r#"{"say": "Ask me again on its own."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = proposed(&base);
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "{payload}"
    );
    let refused = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == "propose_coding_session")
        .expect("the verb's turn");
    assert_eq!(refused["outcome"]["is"], "refused", "{refused}");
    assert!(
        refused["text"]
            .as_str()
            .expect("text")
            .contains("not the owner's own words"),
        "{refused}"
    );
}

/// The tap fires the routine with the stored task and nothing else, and the
/// session it answers is the proposal's, as `fired`.
#[test]
fn a_tapped_session_fires_the_routine_with_the_task_alone() {
    let (routine, served) = fake_routine();
    let (base, _fakes) = scene(
        "coding-fire",
        Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        },
    );
    let (id, payload) = proposed(&base);
    let proposal = &payload["proposals"][0];
    assert_eq!(proposal["act"]["kind"], "coding_session", "{proposal}");
    assert_eq!(proposal["card"]["answers"][0]["verb"], "start");
    assert_eq!(
        proposal["coding"],
        serde_json::Value::Null,
        "not tapped yet"
    );

    let (status, after) = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(status, 200, "{after}");
    let proposal = &after["proposals"][0];
    assert_eq!(proposal["state"], "acted", "{proposal}");
    assert_eq!(proposal["coding"]["state"], "fired");
    assert_eq!(proposal["coding"]["url"], URL);

    let (head, body) = served.join().expect("the fake ran");
    assert_eq!(
        head[0],
        "POST /v1/claude_code/routines/trig_01fake/fire HTTP/1.1"
    );
    let lower: Vec<String> = head.iter().map(|line| line.to_ascii_lowercase()).collect();
    assert!(
        lower.contains(&format!("authorization: bearer {TOKEN}")),
        "{head:?}"
    );
    assert!(
        lower.contains(&"anthropic-beta: experimental-cc-routine-2026-04-01".to_owned()),
        "{head:?}"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("json"),
        serde_json::json!({ "text": TASK.replace("\\n", "\n") }),
        "the payload is the task and nothing else"
    );
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The whole life: fired, reported by the branch whose report names it, then merged; a drop of
/// a merged session is refused.
#[test]
fn a_session_is_reported_then_merged() {
    let (routine, served) = fake_routine();
    let (base, fakes) = scene(
        "coding-life",
        Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        },
    );
    let (id, _) = proposed(&base);
    assert_eq!(answer(&base, &id, "tap", &serde_json::json!({})).0, 200);
    served.join().expect("the fake ran");

    let root = common::scratch("coding-life-repos");
    let mirrors = common::local_repos(&root);
    let repo: suzatary_core::repos::RepoName = "bmabsout/cv".parse().expect("a name");
    let poll = |mirrors: &suzatary_io::repos::Mirrors| {
        suzatary_cli::commands::coding::poll(&fakes.conversations, mirrors, &repo).expect("polls")
    };
    assert!(poll(&mirrors).is_empty(), "no branch speaks for it yet");

    let work = root.join("cv-work");
    let remote = root.join("remotes/bmabsout/cv.git");
    let remote = remote.to_str().expect("a utf-8 path");
    git(&work, &["checkout", "-q", "-b", "claude/retitle"]);
    std::fs::create_dir_all(work.join("docs/reports")).expect("reports");
    std::fs::write(
        work.join("docs/reports/retitle.md"),
        format!("# Retitle\n\nSession: {URL}\n\nGates: cargo test green.\n"),
    )
    .expect("a report");
    std::fs::write(work.join("cv.typ"), "= Ada Quill\nPrincipal investigator\n").expect("cv");
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "Retitle"]);
    git(&work, &["push", "-q", remote, "claude/retitle"]);
    let fresh = || common::local_mirrors(&root);
    let moved = poll(&fresh());
    assert_eq!(moved.len(), 1);
    let coding = conversation(&base, &id)["proposals"][0]["coding"].clone();
    assert_eq!(coding["state"], "reported", "{coding}");
    assert_eq!(coding["branch"], "claude/retitle");
    assert_eq!(coding["path"], "docs/reports/retitle.md");
    assert!(
        coding["report"]
            .as_str()
            .unwrap_or_default()
            .contains("Gates")
    );
    assert_eq!(
        coding["stat"],
        serde_json::json!([
            {"path": "cv.typ", "added": 1, "removed": 1},
            {"path": "docs/reports/retitle.md", "added": 5, "removed": 0},
        ])
    );
    git(&work, &["checkout", "-q", "main"]);
    git(&work, &["merge", "-q", "--ff-only", "claude/retitle"]);
    git(&work, &["push", "-q", remote, "main"]);
    assert_eq!(poll(&fresh()).len(), 1);
    let coding = conversation(&base, &id)["proposals"][0]["coding"].clone();
    assert_eq!(coding["state"], "merged", "{coding}");
    assert_eq!(
        answer(&base, &id, "drop", &serde_json::json!({"reason": "late"})).0,
        409,
        "a merged session is final"
    );
}

/// A drop needs a reason, and the reason is what the session ends on.
#[test]
fn a_fired_session_is_dropped_with_a_reason() {
    let (routine, served) = fake_routine();
    let (base, _fakes) = scene(
        "coding-drop",
        Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        },
    );
    let (id, _) = proposed(&base);
    assert_eq!(answer(&base, &id, "tap", &serde_json::json!({})).0, 200);
    served.join().expect("the fake ran");
    assert_eq!(
        answer(&base, &id, "drop", &serde_json::json!({"reason": " "})).0,
        400
    );
    let (status, dropped) = answer(
        &base,
        &id,
        "drop",
        &serde_json::json!({"reason": "done by hand"}),
    );
    assert_eq!(status, 200, "{dropped}");
    assert_eq!(dropped["proposals"][0]["coding"]["state"], "dropped");
    assert_eq!(dropped["proposals"][0]["coding"]["reason"], "done by hand");
}

/// A deployment whose one chain is the one a pull request's `Completes:` closes, and which
/// declares the routine.
fn building() -> suzatary_core::deployment::Deployment {
    let plan = suzatary_core::release::Plan::declared().expect("the plan reads");
    suzatary_core::deployment::Deployment {
        chains: suzatary_core::chains::Chains::of(vec![suzatary_core::chains::ChainPolicy {
            name: suzatary_core::chains::ChainName::new(plan.chain()).expect("a chain"),
            ..common::chain()
        }])
        .expect("one chain"),
        ..deployment()
    }
}

/// "Build it" on `todo`: the status and the answer.
fn build(base: &str, todo: &str) -> (u16, serde_json::Value) {
    let mut response = common::act(
        &format!("{base}/api/todo/{todo}/build"),
        &serde_json::json!({}),
    );
    (common::status(&response), common::body_json(&mut response))
}

/// The task a build of `todo` carries on its `attempt`.
fn task_of(chain: &str, todo: &suzatary_core::cast::Todo, attempt: usize) -> String {
    let id = todo.id;
    let detail = if todo.detail.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", todo.detail)
    };
    format!(
        "Build todo:{chain}/{id}, attempt {attempt}. Its text follows, as the owner's prodrome \
         holds it.\nWhen the pull request finishes it, put `Completes: {id}` on a line of its \
         description.\n\n{}{detail}",
        todo.body,
    )
}

/// "Build it" files today's session card serving the todo, its task the todo's whole text; the
/// tap sends that task and nothing else, and leaves the other build card standing. A todo is in
/// progress while its card is open or its session runs, so a second build of it and a third
/// todo are refused, naming what is in progress, until a card is dismissed; a dropped session's
/// todo is built again as a new card, its next attempt.
#[test]
fn a_todo_is_built_from_its_card_at_most_two_at_once() {
    let (routine, served) = fake_routine();
    let fakes = Arc::new(
        Fakes::new("coding-build").whois(common::FakeWhois::new().naming(common::v4(), owner())),
    );
    let base = common::Server::new(Arc::clone(&fakes), building(), Vec::new())
        .real_acts(
            "coding-build",
            "her@study.test",
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .firing(Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        })
        .start()
        .base;
    let chain = suzatary_core::release::Plan::declared()
        .expect("the plan reads")
        .chain()
        .to_owned();
    let todos = &CAST.todos;
    let card = |index: usize| common::reading(&base, "build", &[])["proposals"][index].clone();
    let answered = |index: usize, verb: &str, body: serde_json::Value| {
        common::status(&common::act(
            &format!("{base}/api/secretary/conversations/build/proposals/{index}/{verb}"),
            &body,
        ))
    };

    let (status, closed) = build(&base, todos.resolved.id);
    assert_eq!(status, 409, "a completed todo is not built: {closed}");

    let (status, filed) = build(&base, todos.checklist.id);
    assert_eq!(status, 200, "{filed}");
    let task = task_of(&chain, &todos.checklist, 1);
    assert_eq!(filed["proposals"][0]["act"]["kind"], "coding_session");
    assert_eq!(
        filed["proposals"][0]["card"]["body"],
        task.as_str(),
        "the card shows the whole task"
    );
    let fold = suzatary_core::secretary::inbox::fold(&fakes.inbox).expect("the inbox reads");
    let [object] = fold.objects().collect::<Vec<_>>()[..] else {
        panic!("one card")
    };
    assert_eq!(
        object.asked.price.serves().map(ToString::to_string),
        Some(format!("todo:{chain}/{}", todos.checklist.id)),
        "priced by the todo it builds"
    );
    assert_eq!(build(&base, todos.flat.id).0, 200);

    assert_eq!(answered(0, "tap", serde_json::json!({})), 200);
    let (_, body) = served.join().expect("the fake ran");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("json"),
        serde_json::json!({ "text": task }),
        "the tap sends the card's task and nothing else"
    );
    assert_eq!(
        card(1)["state"],
        "standing",
        "the box's sweep after a tap leaves the other build standing"
    );

    let (status, again) = build(&base, todos.checklist.id);
    assert_eq!(status, 409, "its session is running: {again}");
    let (status, third) = build(&base, todos.high.id);
    assert_eq!(status, 409, "{third}");
    let said = third["error"].as_str().expect("a sentence");
    for todo in [todos.checklist.id, todos.flat.id] {
        assert!(said.contains(&format!("todo:{chain}/{todo}")), "{said}");
    }

    assert_eq!(
        answered(1, "dismiss", serde_json::json!({ "reason": "later" })),
        200
    );
    assert_eq!(
        build(&base, todos.high.id).0,
        200,
        "one is no longer in progress"
    );

    assert_eq!(
        answered(0, "drop", serde_json::json!({ "reason": "wrong approach" })),
        200
    );
    let (status, retried) = build(&base, todos.checklist.id);
    assert_eq!(status, 200, "{retried}");
    let retry = card(3);
    assert_eq!(retry["state"], "standing", "{retry}");
    assert_eq!(
        retry["card"]["body"],
        task_of(&chain, &todos.checklist, 2).as_str(),
        "a retry is a new request, its attempt on the card"
    );
}

/// Only the chain a pull request's `Completes:` closes is built, and only where `[coding]` names
/// a routine: a personal todo never becomes a session's task.
#[test]
fn a_todo_of_another_chain_is_not_built() {
    let fakes = Arc::new(
        Fakes::new("coding-build-life")
            .whois(common::FakeWhois::new().naming(common::v4(), owner())),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (status, refused) = build(&base, CAST.todos.flat.id);
    assert_eq!(status, 409, "{refused}");
    let fold = suzatary_core::secretary::inbox::fold(&fakes.inbox).expect("the inbox reads");
    assert_eq!(fold.objects().count(), 0, "nothing was filed");
}
