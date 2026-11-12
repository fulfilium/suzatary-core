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
/// What the fake routine answers, in the `cse_` spelling the live one answers.
const URL: &str = "https://claude.ai/code/cse_01fake";
/// What the fake session writes in its report, in the `session_` spelling the live one writes.
const WRITTEN: &str = "https://claude.ai/code/session_01fake";

fn owner() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    suzatary_core::deployment::Deployment {
        coding: Some(suzatary_core::connect::Coding {
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
    ) -> Result<Option<suzatary_base::capability::Secret>, suzatary_io::secrets::SecretError> {
        Ok((name == suzatary_core::connect::ROUTINE_TOKEN)
            .then(|| suzatary_base::capability::Secret::new(TOKEN)))
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
            r#"{{"claude_code_session_id":"cse_01fake","claude_code_session_url":"{URL}"}}"#
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
    let (running, fakes) = served(
        tag,
        firing,
        common::FakeWhois::new().naming(common::v4(), owner()),
        Vec::new(),
    );
    (running.base, fakes)
}

/// [`scene`] with `whois` naming its peers and `this_box` the box's own addresses.
fn served(
    tag: &str,
    firing: Firing,
    whois: common::FakeWhois,
    this_box: Vec<std::net::IpAddr>,
) -> (common::Running, Arc<Fakes>) {
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(whois)
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
    let server = common::Server::new(Arc::clone(&fakes), deployment(), this_box)
        .real_acts(
            tag,
            "her@study.test",
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .firing(firing);
    (server.start(), fakes)
}

fn proposed(base: &str) -> (String, serde_json::Value) {
    proposed_in(base, &serde_json::json!({"title": "The CV"}))
}

/// [`proposed`] in a conversation opened with `opening`.
fn proposed_in(base: &str, opening: &serde_json::Value) -> (String, serde_json::Value) {
    let mut opened = common::act(&format!("{base}/api/secretary/conversations"), opening);
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let payload = common::said(base, &id, "retitle my cv properly");
    (id, payload)
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
/// verb refuses and says why, and the one card filed is the box's, which asks again afresh.
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
    let kinds: Vec<&serde_json::Value> = payload["proposals"]
        .as_array()
        .expect("proposals")
        .iter()
        .map(|proposal| &proposal["act"]["kind"])
        .collect();
    assert_eq!(kinds, ["afresh"], "{payload}");
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

/// The operator at the box's terminal asks for the session and taps its card: the tap fires
/// the routine through the route a device's tap takes, and the box records it under the root
/// key, which no device holds.
#[test]
fn an_operators_tap_starts_a_session() {
    use suzatary_cli::commands::{ask, tap};
    let (routine, served_routine) = fake_routine();
    let (running, _fakes) = served(
        "coding-operator",
        Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        },
        common::FakeWhois::new(),
        common::loopback_is_this_box(),
    );
    let base = &running.base;
    let operator = common::operator(base);
    let asked = ask::ask(&operator, &ask::Conversation::New, "retitle my cv properly")
        .expect("the operator asks");
    let id = asked.conversation;
    let proposal = &conversation(base, id.as_str())["proposals"][0];
    assert_eq!(proposal["act"]["kind"], "coding_session", "{proposal}");

    let card = suzatary_base::reference::ProposalRef {
        conversation: id,
        index: 0,
    };
    let tapped = tap::tap(
        &operator,
        &card,
        "",
        &suzatary_api::secretary::routes::TapAnswer::Plain,
    )
    .expect("the operator taps");
    let proposal = &tapped.conversation["proposals"][0];
    assert_eq!(proposal["state"], "acted", "{proposal}");
    assert_eq!(proposal["coding"]["state"], "fired");
    let (_, body) = served_routine.join().expect("the fake ran");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("json"),
        serde_json::json!({ "text": TASK.replace("\\n", "\n") })
    );

    let record = running
        .requests
        .get(&tapped.request)
        .expect("the records read")
        .expect("the tap is recorded");
    assert_eq!(record.key, common::cast_keys().root().public().as_str());
    assert_eq!(record.actor, CAST.owner.actor);
    assert_eq!(record.status, 200);
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
    let repo: suzatary_base::repo_ids::RepoName = "bmabsout/cv".parse().expect("a name");
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
        format!("# Retitle\n\nSession: {WRITTEN}\n\nGates: cargo test green.\n"),
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

/// The server's poll tick never takes the server down: a repository the instance does not
/// declare (the fixture world's) and one whose remote is gone are each logged and move nothing,
/// the server still answers, and the next tick reads a report that is not UTF-8.
#[test]
fn a_poll_that_fails_leaves_the_server_answering() {
    let (routine, served) = fake_routine();
    let (base, fakes) = scene(
        "coding-poll-fails",
        Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        },
    );
    let (id, _) = proposed(&base);
    assert_eq!(answer(&base, &id, "tap", &serde_json::json!({})).0, 200);
    served.join().expect("the fake ran");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let conversations: &'static _ = Box::leak(Box::new(fakes.conversations.reopened()));
    let workers = suzatary_cli::commands::serve::Workers::new();
    let tick = |mirrors: suzatary_io::repos::Mirrors, repo: &str| {
        let repo: suzatary_base::repo_ids::RepoName = repo.parse().expect("a name");
        runtime.block_on(suzatary_cli::commands::coding::poll_once(
            conversations,
            &Arc::new(mirrors),
            &repo,
            &workers,
        ))
    };

    let root = common::scratch("coding-poll-fails-repos");
    assert_eq!(tick(common::local_mirrors(&root), "bmabsout/fixture"), 0);
    assert_eq!(
        tick(common::local_mirrors(&root), "bmabsout/cv"),
        0,
        "no remote yet"
    );
    assert_eq!(
        conversation(&base, &id)["proposals"][0]["coding"]["state"],
        "fired",
        "the server answers"
    );

    let _ = common::local_repos(&root);
    let work = root.join("cv-work");
    let remote = root.join("remotes/bmabsout/cv.git");
    git(&work, &["checkout", "-q", "-b", "claude/retitle"]);
    std::fs::create_dir_all(work.join("docs/reports")).expect("reports");
    let mut report = format!("# Retitle\n\nSession: {WRITTEN}\n\n").into_bytes();
    report.extend_from_slice(b"Gates: \xff\xfe green.\n");
    std::fs::write(work.join("docs/reports/retitle.md"), report).expect("a report");
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "Retitle"]);
    git(
        &work,
        &[
            "push",
            "-q",
            remote.to_str().expect("utf-8"),
            "claude/retitle",
        ],
    );
    assert_eq!(tick(common::local_mirrors(&root), "bmabsout/cv"), 1);
    assert_eq!(
        conversation(&base, &id)["proposals"][0]["coding"]["state"],
        "reported"
    );
}

/// The live case of 2026-11-01: a session fired in a new, untitled conversation with a `cse_`
/// id, and among many `claude/` branches (some merged, some reporting other sessions, one with
/// no report) the one whose only change is a report naming the `session_` URL, on a base the
/// default branch has since moved past. The explanation says which branch reports it, and the
/// poll moves it.
#[test]
fn a_new_conversations_session_is_reported_by_one_branch_among_many() {
    let (routine, served) = fake_routine();
    let (base, fakes) = scene(
        "coding-among-many",
        Firing::Endpoint {
            base: routine,
            routine: "trig_01fake".parse().expect("a routine id"),
            secrets: Arc::new(Holding),
        },
    );
    let (id, _) = proposed_in(&base, &serde_json::json!({"title": ""}));
    assert_eq!(answer(&base, &id, "tap", &serde_json::json!({})).0, 200);
    served.join().expect("the fake ran");

    let root = common::scratch("coding-among-many-repos");
    let _ = common::local_repos(&root);
    let work = root.join("cv-work");
    let remote = root.join("remotes/bmabsout/cv.git");
    let remote = remote.to_str().expect("a utf-8 path");
    let report = |name: &str, text: &str| {
        std::fs::create_dir_all(work.join("docs/reports")).expect("reports");
        std::fs::write(work.join(format!("docs/reports/{name}.md")), text).expect("a report");
    };
    git(&work, &["checkout", "-q", "-b", "claude/changelog-typo"]);
    report(
        "changelog-typo",
        &format!("# Report: changelog-typo\n\nSession: {WRITTEN}\n\n## The task\n"),
    );
    git(&work, &["add", "."]);
    git(
        &work,
        &[
            "commit",
            "-q",
            "-m",
            "docs: report the changelog typo check",
        ],
    );
    git(&work, &["push", "-q", remote, "claude/changelog-typo"]);
    for n in 0..24 {
        let branch = format!("claude/other-{n:02}");
        git(&work, &["checkout", "-q", "main"]);
        git(&work, &["checkout", "-q", "-b", &branch]);
        match n % 3 {
            0 => report(
                &format!("other-{n:02}"),
                &format!("Session: https://claude.ai/code/session_01other{n:02}\n"),
            ),
            1 => std::fs::write(work.join(format!("other-{n:02}.txt")), "no report\n")
                .expect("a file"),
            _ => report(&format!("other-{n:02}"), "Session: none written\n"),
        }
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", &branch]);
        git(&work, &["push", "-q", remote, &branch]);
        if n % 4 == 0 {
            git(&work, &["checkout", "-q", "main"]);
            git(&work, &["merge", "-q", "--no-edit", &branch]);
            git(&work, &["push", "-q", remote, "main"]);
        }
    }

    let mirrors = common::local_mirrors(&root);
    let repo: suzatary_base::repo_ids::RepoName = "bmabsout/cv".parse().expect("a name");
    let explained = suzatary_cli::commands::coding::explain(&fakes.conversations, &mirrors, &repo)
        .expect("explains");
    let said = explained.join("\n");
    assert!(
        said.contains(&format!("{id} proposal 0: fired 01fake")),
        "{said}"
    );
    assert!(said.contains("25 branch(es) seen"), "{said}");
    assert!(
        said.contains("docs/reports/changelog-typo.md names session_01fake"),
        "{said}"
    );
    assert!(
        said.contains(&format!(
            "{id} proposal 0: reported by claude/changelog-typo in docs/reports/changelog-typo.md"
        )),
        "{said}"
    );
    let moved =
        suzatary_cli::commands::coding::poll(&fakes.conversations, &mirrors, &repo).expect("polls");
    assert_eq!(moved.len(), 1, "{said}");
    let coding = conversation(&base, &id)["proposals"][0]["coding"].clone();
    assert_eq!(coding["state"], "reported", "{coding}");
    assert_eq!(coding["branch"], "claude/changelog-typo");
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
        chains: suzatary_base::chains::Chains::of(vec![suzatary_base::chains::ChainPolicy {
            name: suzatary_base::chains::ChainName::new(plan.chain()).expect("a chain"),
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

/// A round that read mail is refused a session, and the box files one card that asks the
/// owner's request again. Its tap opens a conversation whose first line is the owner's request,
/// byte for byte, under the same stamp and clean; no other line of the old conversation is in
/// it. The secretary there proposes the session, and its tap fires the routine.
#[test]
fn a_refused_session_starts_again_in_a_fresh_conversation() {
    use suzatary_cli::commands::{ask, tap};
    let (routine, served_routine) = fake_routine();
    let (running, fakes) = afresh_box(routine);
    let base = &running.base;
    let operator = common::operator(base);
    let asked = ask::ask(&operator, &ask::Conversation::New, "retitle my cv properly")
        .expect("the operator asks");
    let old = asked.conversation;
    let proposals = conversation(base, old.as_str())["proposals"].clone();
    assert_eq!(proposals.as_array().map(Vec::len), Some(1), "{proposals}");
    assert_eq!(proposals[0]["act"]["kind"], "afresh", "{proposals}");
    assert_eq!(
        proposals[0]["card"]["answers"][0]["label"],
        "Start it in a fresh conversation"
    );
    // The old topic's title job reads the script too, so it runs before the tap.
    eventually(|| conversation(base, old.as_str())["title"] != "");

    let card = |conversation| suzatary_base::reference::ProposalRef {
        conversation,
        index: 0,
    };
    let plain = suzatary_api::secretary::routes::TapAnswer::Plain;
    let tapped = tap::tap(&operator, &card(old.clone()), "", &plain).expect("the operator taps");
    assert_eq!(tapped.conversation["proposals"][0]["state"], "acted");
    let conversations = || fakes.conversations.conversations();
    let fresh = suzatary_core::secretary::transcript::keys(conversations())
        .expect("the conversations read")
        .into_iter()
        .find(|id| *id != old && id.as_str() != "default")
        .expect("a fresh conversation");
    eventually(|| {
        let now = conversation(base, fresh.as_str());
        now["state"] == "idle" && !now["proposals"].as_array().is_none_or(Vec::is_empty)
    });
    only_the_request_crossed(
        &suzatary_core::secretary::conversations::read(conversations(), &old).expect("reads"),
        &suzatary_core::secretary::conversations::read(conversations(), &fresh).expect("reads"),
    );
    let proposal = &conversation(base, fresh.as_str())["proposals"][0];
    assert_eq!(proposal["act"]["kind"], "coding_session", "{proposal}");

    let started =
        tap::tap(&operator, &card(fresh), "", &plain).expect("the owner taps the session's card");
    assert_eq!(
        started.conversation["proposals"][0]["coding"]["state"],
        "fired"
    );
    let (_, body) = served_routine.join().expect("the fake ran");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("json"),
        serde_json::json!({ "text": TASK.replace("\\n", "\n") })
    );
}

/// A box whose loopback is its own terminal, firing at `routine`, with a model that searches
/// mail, is refused a session, answers, titles the topic, then proposes the session afresh.
fn afresh_box(routine: String) -> (common::Running, Arc<Fakes>) {
    let propose = format!(
        r#"{{"say": "", "calls": [{{"verb": "propose_coding_session", "args": {{
            "task": "{TASK}",
            "why": "The title is out of date and the change needs the gates.",
            "sources": "todo:life/permitfiling"
        }}}}]}}"#
    )
    .leak();
    let fakes = Arc::new(Fakes::new("coding-afresh").model(FakeModel::saying([
        r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "cv"}}]}"#,
        &*propose,
        r#"{"say": "This conversation read mail; tap the card to start it afresh."}"#,
        r#"{"say": "Retitling the CV"}"#,
        &*propose,
        r#"{"say": "Proposed."}"#,
    ])));
    let running = common::Server::new(
        Arc::clone(&fakes),
        deployment(),
        common::loopback_is_this_box(),
    )
    .real_acts(
        "coding-afresh",
        "her@study.test",
        common::again(&fakes.conversations),
        common::ScriptedOauth::new(Vec::new()),
    )
    .firing(Firing::Endpoint {
        base: routine,
        routine: "trig_01fake".parse().expect("a routine id"),
        secrets: Arc::new(Holding),
    })
    .start();
    (running, fakes)
}

/// The law of a fresh conversation: its first line is the owner's request in `before`, byte for
/// byte and under the same stamp, and clean; every later line is the fresh round's own, the
/// model's calls and their answers, so no other line of `before` is in it.
fn only_the_request_crossed(
    before: &suzatary_core::secretary::store::Stored,
    after: &suzatary_core::secretary::store::Stored,
) {
    use suzatary_core::secretary::model::Turn;
    use suzatary_core::secretary::taint::{self, Taint};
    let request = before
        .turns
        .iter()
        .find(|said| matches!(said.turn, Turn::Person { .. }))
        .expect("the owner's request");
    let first = &after.turns[0];
    assert_eq!(
        first.turn, request.turn,
        "the owner's request, byte for byte"
    );
    assert_eq!(first.by, request.by, "under the owner's own stamp");
    assert_eq!(taint::turn(first), Taint::Clean);
    assert!(
        matches!(after.turns[1].turn, Turn::Model { .. }),
        "the model answered the request alone"
    );
    for said in &after.turns[1..] {
        assert!(
            matches!(said.turn, Turn::Model { .. } | Turn::Tool { .. }),
            "a line the fresh round did not write: {said:?}"
        );
    }
}

/// Wait for `done`, which the box's worker makes true on its own thread.
fn eventually(done: impl Fn() -> bool) {
    for _ in 0..400 {
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the box never got there");
}
