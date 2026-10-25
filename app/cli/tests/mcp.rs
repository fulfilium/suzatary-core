//! `suzatary mcp` driven as the agent drives it, a real process over stdio, with the write checked
//! on disk.
//!
//! It runs against a temp chain and a temp instance file, and calls no verb that reaches mail or
//! shells out.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use prodrome_core::event::{Actor, mk_created};
use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use suzatary_core::record::{EventStore, mk_authored};

/// A scratch root unique to this process and thread.
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "suzatary-mcp-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a scratch root");
    root
}

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 9, 0, 0, 0).expect("a real instant")
}

/// One trusted todo the agent can talk about, on a chain of its own.
fn a_chain(root: &Path) -> EventStore {
    let untrusted = prodrome_core::policy::Untrusted::of([Actor::new("triage").expect("valid")]);
    let store = EventStore::new(root.join("events"), untrusted);
    suzatary_core::store::append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_created("filething", at(1), "ada", "permit renewal", "").expect("valid"),
    )
    .expect("appends");
    suzatary_core::store::append_to(
        &store,
        suzatary_core::record::GENESIS,
        mk_authored(
            "filething",
            at(2),
            "ada",
            "todo",
            at(2),
            suzatary_core::record::MarkupSource::from_chain(r"File the \*permit\* renewal"),
            fpl::mk_flat(0.35).expect("a term"),
            vec!["Missing it is not recoverable.".to_owned()],
            "work",
            "",
            suzatary_core::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("valid"),
    )
    .expect("appends");
    store
}

/// An instance file naming that chain, and nothing this box depends on.
fn an_instance(root: &Path) -> PathBuf {
    let path = root.join("instance.toml");
    std::fs::write(
        &path,
        format!(
            r#"
repo = "{repo}"

[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

# TWO CHAINS (docs/ARCHITECTURE.md §10.2). `life` is the default — what a verb
# with no `chain` writes to — and `suzatary` is the one a verb has to NAME.
[[chains]]
name = "life"
events = "events"
proposals = "events-triage"
untrusted = ["triage"]
actors = ["assistant", "ada", "triage"]
owner = "ada"

[[chains]]
name = "suzatary"
events = "events-suzatary"
proposals = "events-triage-suzatary"
untrusted = ["triage"]
actors = ["assistant", "ada", "triage"]
owner = "ada"

[web]
viewers = ["someone@example.com"]
app_url = "https://example.invalid"

[web.act_as]
"someone@example.com" = "ada"

[web.serving]
host = "127.0.0.1"
port = 8788
tls_dir = "tls"
dist = "dist"
briefings = "briefings"

[categories]
work = 0
home = 3
other = 4

[mail]
root = "mail"
state = "state.json"
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
heartbeat = "triage-heartbeat"

[[mail.accounts]]
email = "someone@example.com"
subdir = "someone"
sync = "gmail"
"#,
            repo = root.display()
        ),
    )
    .expect("an instance file");
    path
}

/// The server, and the two pipes it speaks over.
struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn start(instance: &Path) -> Session {
        Session::started(crate::binary::keyed().args([
            "--instance",
            &instance.to_string_lossy(),
            "mcp",
        ]))
    }

    /// The agent's process as the triage pass starts it on a box that signs: its own key in the
    /// environment, and no secret store of the machine's.
    fn signing(instance: &Path, agent_seed: &str) -> Session {
        let isolated = instance
            .parent()
            .expect("an instance in a directory")
            .join("home");
        Session::started(
            crate::binary::suzatary()
                .args(["--instance", &instance.to_string_lossy(), "mcp"])
                .env(suzatary_core::signatures::AGENT_KEY, agent_seed)
                .env("HOME", &isolated)
                .env("XDG_CONFIG_HOME", isolated.join("config")),
        )
    }

    fn started(command: &mut Command) -> Session {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the binary runs");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Session {
            child,
            stdin,
            stdout,
            next_id: 0,
        }
    }

    /// Send a request and read the answer with the matching id, skipping anything sent unprompted.
    fn ask(&mut self, method: &str, params: &serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = self.next_id;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{request}").expect("the pipe takes a request");
        self.stdin.flush().expect("flushes");
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("the pipe answers");
            assert!(
                read > 0,
                "the server closed stdout while answering {method}"
            );
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
                panic!("the server wrote a non-JSON line on stdout: {line:?}");
            };
            if message.get("id") == Some(&serde_json::json!(id)) {
                assert!(
                    message.get("error").is_none(),
                    "{method} answered an error: {message}"
                );
                return message["result"].clone();
            }
        }
    }

    /// The handshake, and the server's own claim about itself.
    fn hello(&mut self) -> serde_json::Value {
        let hello = self.ask(
            "initialize",
            &serde_json::json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "the stdio test", "version": "0"},
            }),
        );
        self.notify("notifications/initialized");
        hello
    }

    fn notify(&mut self, method: &str) {
        let notification = serde_json::json!({"jsonrpc": "2.0", "method": method});
        writeln!(self.stdin, "{notification}").expect("the pipe takes a notification");
        self.stdin.flush().expect("flushes");
    }

    /// Call one verb and read its answer, a refusal included.
    fn answered(&mut self, name: &str, arguments: &serde_json::Value) -> serde_json::Value {
        self.ask(
            "tools/call",
            &serde_json::json!({"name": name, "arguments": arguments}),
        )
    }

    /// Call one verb and read its single text block.
    fn call(&mut self, name: &str, arguments: &serde_json::Value) -> String {
        let answered = self.answered(name, arguments);
        assert_eq!(
            answered["isError"],
            serde_json::json!(false),
            "{name} refused: {answered}"
        );
        answered["content"][0]["text"]
            .as_str()
            .expect("one text block")
            .to_owned()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Every object a store directory holds, by name.
fn names(dir: &Path) -> std::collections::BTreeSet<String> {
    std::fs::read_dir(dir.join("objects"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The reasons every write of the agent carries.
fn reasoned(mut fields: serde_json::Value) -> serde_json::Value {
    fields["why"] = serde_json::json!("a message asks for it");
    fields["sources"] = serde_json::json!("briefing:2026-09-02");
    fields
}

/// THE TRUST BOUNDARY, OVER A REAL PROCESS: initialize, read the chain, write on both chains,
/// then read the disk. Every write is a card in the instance's own Inbox, asked by the triage
/// agent; no chain and no replica gains an object, and markup outside the closed world is
/// refused before anything is filed.
#[test]
fn the_agent_speaks_to_a_real_server_and_its_writes_are_cards() {
    let root = scratch("session");
    let _store = a_chain(&root);
    let instance = an_instance(&root);
    let chains = ["events", "events-suzatary"].map(|dir| (dir, names(&root.join(dir))));
    let mut session = Session::start(&instance);

    let hello = session.hello();
    assert_eq!(hello["serverInfo"]["name"], serde_json::json!("suzatary"));
    let open: serde_json::Value =
        serde_json::from_str(&session.call("list_open_todos", &serde_json::json!({})))
            .expect("the open list is JSON");
    let open = open.as_array().expect("an array");
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["id"], serde_json::json!("filething"));

    let recorded: serde_json::Value = serde_json::from_str(&session.call(
        "record",
        &reasoned(
            serde_json::json!({"kind": "completed", "todo": "filething", "note": "filed it"}),
        ),
    ))
    .expect("the answer is JSON");
    assert_eq!(recorded["filed"], serde_json::json!("proposal:triage/0"));
    let added: serde_json::Value = serde_json::from_str(&session.call(
        "todo_add",
        &reasoned(
            serde_json::json!({"chain": "suzatary", "todo_id": "fixthegate",
            "body": "Fix the *gate*", "rationale": "it is red", "category": "work",
            "priority": "60%"}),
        ),
    ))
    .expect("the answer is JSON");
    assert_eq!(added["filed"], serde_json::json!("proposal:triage/1"));
    let refused = session.answered(
        "todo_add",
        &reasoned(serde_json::json!({"todo_id": "htmlelem",
            "body": "#html.elem(\"button\")[x]", "rationale": "a test of the door",
            "category": "work", "priority": "70%"})),
    );
    assert_eq!(refused["isError"], serde_json::json!(true), "{refused}");
    assert!(refused.to_string().contains("html"), "{refused}");
    drop(session);

    for (dir, before) in &chains {
        assert_eq!(&names(&root.join(dir)), before, "{dir} gained an object");
    }
    // A replica is synced from its chain before a run and holds nothing else.
    for (replica, (_, chain)) in ["events-triage", "events-triage-suzatary"]
        .into_iter()
        .zip(&chains)
    {
        assert!(names(&root.join(replica)).is_subset(chain), "{replica}");
    }
    let read = suzatary_cli::instance::Instance::read(
        &instance,
        &suzatary_cli::paths::Env::under(root.join("home"), root.join("run")),
    )
    .expect("reads");
    let fold = suzatary_core::secretary::inbox::fold(&read.inbox_store()).expect("reads");
    let mut cards: Vec<(String, String, Vec<&str>)> = fold
        .objects()
        .map(|object| {
            assert_eq!(
                object.asked.proposer,
                suzatary_core::secretary::proposer::Proposer::Triage
            );
            match &object.asked.proposal {
                suzatary_core::secretary::store::Proposal::Changes(changeset) => (
                    object.asked.asked.to_string(),
                    changeset.chain().to_owned(),
                    changeset
                        .acts()
                        .iter()
                        .map(suzatary_core::secretary::changes::Act::word)
                        .collect(),
                ),
                other => panic!("a triage card is a changeset, not {other:?}"),
            }
        })
        .collect();
    cards.sort();
    assert_eq!(
        cards,
        vec![
            (
                "proposal:triage/0".to_owned(),
                "life".to_owned(),
                vec!["complete"]
            ),
            (
                "proposal:triage/1".to_owned(),
                "suzatary".to_owned(),
                vec!["add"]
            ),
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The names a served process actually lists match the versioned surface.
#[test]
fn the_wire_lists_the_verb_table_and_nothing_it_may_never_hold() {
    let root = scratch("surface");
    let _store = a_chain(&root);
    let instance = an_instance(&root);
    let mut session = Session::start(&instance);
    session.hello();

    let listed = session.ask("tools/list", &serde_json::json!({}));
    let tools = listed["tools"].as_array().expect("an array of tools");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(
        names.len(),
        suzatary_core::agent::TriageVerb::ALL.len(),
        "the wire and the sum disagree about how many verbs there are"
    );
    for expected in ["context", "list_open_todos", "todo_add", "record"] {
        assert!(
            names.contains(&expected),
            "{expected} is missing: {names:?}"
        );
    }
    // The names that may never appear, checked where a client would see them.
    for forbidden in ["confirm", "send", "reply"] {
        assert!(
            !names.iter().any(|name| name.contains(forbidden)),
            "{forbidden} reached the wire: {names:?}"
        );
    }

    // An unknown verb is a refusal the agent can read, not a protocol error.
    let answered = session.ask(
        "tools/call",
        &serde_json::json!({"name": "send_message", "arguments": {}}),
    );
    let shown = answered.to_string();
    assert!(
        shown.contains("send_message"),
        "the refusal names what was asked for: {shown}"
    );
    drop(session);
    let _ = std::fs::remove_dir_all(&root);
}

/// THE GATE FOR ADOPTION: a deployment shaped as the instance sample writes it (no actor
/// untrusted) adopts signatures, the agent's own process files its card, and the box's `verify`
/// passes. Adoption once registered the box's key for triage here, so every pass after it was
/// refused.
#[test]
fn after_adoption_the_agents_card_is_filed_and_verifies() {
    const OWN: &str = "9999999999999999999999999999999999999999999999999999999999999999";
    const AGENT: &str = "8888888888888888888888888888888888888888888888888888888888888888";
    let seeds = [
        (suzatary_core::signatures::BOX_KEY, OWN.to_owned()),
        (suzatary_core::signatures::AGENT_KEY, AGENT.to_owned()),
        (suzatary_core::mail::dedup_key::DEDUP_KEY, "7".repeat(64)),
    ];
    let root = scratch("adopted-agent");
    let _chain = a_chain(&root);
    let instance = an_instance(&root);
    let root_key = prodrome_core::sign::Secret::new(OWN)
        .expect("a seed")
        .public();
    let text = std::fs::read_to_string(&instance).expect("reads");
    std::fs::write(
        &instance,
        text.replace("untrusted = [\"triage\"]", "untrusted = []")
            .replace(
                suzatary_core::cast::keys::root().as_str(),
                root_key.as_str(),
            ),
    )
    .expect("writes");
    let ran = |args: &[&str]| {
        let output = super::todos::signing(&instance, args, &seeds);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    ran(&["device", "init"]);

    let mut session = Session::signing(&instance, AGENT);
    session.hello();
    session.call(
        "record",
        &reasoned(serde_json::json!({"kind": "tended", "todo": "filething", "note": "chased it"})),
    );
    drop(session);

    ran(&["verify"]);
    let _ = std::fs::remove_dir_all(&root);
}

/// Every file under `root`, by its path, with its bytes.
fn files(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut held = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("a readable directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                held.insert(path.clone(), std::fs::read(&path).expect("a readable file"));
            }
        }
    }
    held
}

/// The agent's reads of the memory write nothing anywhere under the data root: every file is there
/// with the same bytes after `context`, `recall` and `zoom` have each read the owner's line as
/// before them. And its server never
/// opens the secretary's answers' cache, which sweeps a block like this one when it opens.
#[test]
fn the_agents_reads_of_the_memory_write_no_file() {
    let root = scratch("reads");
    let _chain = a_chain(&root);
    let instance = an_instance(&root);
    let read = suzatary_cli::instance::Instance::read(
        &instance,
        &suzatary_cli::paths::Env::under(&root, &root),
    )
    .expect("the instance reads");
    suzatary_cli::wiring::Remembering::reader(&read)
        .memories()
        .remember(
            &suzatary_core::secretary::memory::Fact::new("The permit office is on Rue Neuve")
                .expect("a fact"),
            suzatary_core::secretary::memory::Kind::Owner,
            &suzatary_core::secretary::memory::Origin::Turn {
                conversation: suzatary_core::secretary::store::ConversationId::new("a")
                    .expect("a slug"),
                turn: 0,
            },
            suzatary_core::capability::Clock::now(&suzatary_io::clock::SystemClock),
        )
        .expect("the box remembers");
    let remembering = suzatary_cli::wiring::Remembering::reader(&read);
    let lines = remembering.memories().digests.lines;
    let id = suzatary_core::secretary::store::ConversationId::new("permits").expect("a slug");
    let at = suzatary_core::capability::Clock::now(&suzatary_io::clock::SystemClock);
    for (held, entry) in [
        suzatary_core::secretary::store::Entry::Opened {
            at,
            title: suzatary_core::secretary::words::Words::kept("Permits".to_owned()),
            model: "m".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        },
        suzatary_core::secretary::store::Entry::Said {
            at,
            turn: suzatary_core::capability::Turn::Person {
                text: "Renew the permit before it lapses".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        },
    ]
    .iter()
    .enumerate()
    {
        let (line, keys) = suzatary_core::secretary::line_event::LineEvent::write(
            id.clone(),
            Actor::new("ada").expect("an actor"),
            u32::try_from(held).expect("two lines"),
            entry,
            lines.entropy,
        )
        .expect("a line");
        for key in keys {
            suzatary_core::store::Doc::set(lines.keys, &key.name(), &key).expect("kept");
        }
        lines.lines.append(line).expect("said");
    }
    let answers = root.join("secretary-render").join("blocks");
    std::fs::create_dir_all(&answers).expect("a scratch directory");
    std::fs::write(answers.join("kept.html"), "<p>an answer</p>").expect("written");
    let mut session = Session::start(&instance);
    session.hello();
    let before = files(&root);
    let mut said = Vec::new();
    for (verb, args) in [
        ("context", serde_json::json!({})),
        ("recall", serde_json::json!({"query": "permit"})),
        ("zoom", serde_json::json!({"handle": "1+1"})),
    ] {
        said.push(session.call(verb, &args));
    }
    assert!(
        said.iter()
            .all(|answer| answer.contains("Renew the permit")),
        "each read reads the owner's line: {said:?}"
    );
    drop(session);
    assert_eq!(
        files(&root),
        before,
        "the agent's reads of the memory changed a file"
    );
    assert!(
        answers.join("kept.html").exists(),
        "the agent's server swept the secretary's answers when it opened"
    );
    let _ = std::fs::remove_dir_all(&root);
}
