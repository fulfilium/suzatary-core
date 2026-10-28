//! Every verb's answer to a call a model might send, pinned line by line.
//!
//! The cases are generated from each surface's tool list: a valid call, then
//! one fault at a time (an argument missing, null, of the wrong type, out of
//! range), so each line names the one thing wrong with its call. The effects
//! are inert, so what is pinned is how a call is read, not what the box holds.
//! `SUZATARY_UPDATE_CALLS=1 cargo test -p suzatary-core --test all calls::` rewrites
//! the snapshot; a change to it is a change the models see.

use std::fmt::Write as _;
use std::path::PathBuf;

use prodrome_core::event::{Actor, Hash};
use prodrome_core::policy::Untrusted;
use serde_json::{Map, Value, json};
use suzatary_core::capability::{
    Datetime, Junk, JunkFailure, MailError, MailSync, Mailbox, Nothing, Query, RepoFailure,
    Repositories, SyncFailure, Synced,
};
use suzatary_core::chains::{ChainName, ChainPolicy, Chains};
use suzatary_core::deployment::Deployment;
use suzatary_core::mail::junk::{ChurnList, JunkHeader, JunkMessage, Uid};
use suzatary_core::mail::verbs::{AccountSync, SyncReport};
use suzatary_core::mail::{AttachmentFile, AttachmentMeta, MailMessage, MailRow};
use suzatary_core::record::{Envelope, EventStore, Node, TodoEvent};
use suzatary_core::refusal::Refusal;
use suzatary_core::repos::{CommitId, Patch, RepoName, RepoPath, RepoRow, Rev, Visibility};
use suzatary_core::secretary::Bound;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::store::Entry;
use suzatary_core::store::{Prodrome, Reads, StoreError, WriteLock, append_to, linearised};

fn chain() -> ChainPolicy {
    let actor = |name: &str| Actor::new(name).expect("a roster name");
    ChainPolicy {
        name: ChainName::new("life").expect("a slug"),
        untrusted: Untrusted::of([actor("triage")]),
        roster: [actor("owner"), actor("triage")].into_iter().collect(),
        root: suzatary_core::cast::keys::root(),
    }
}

fn deployment() -> Deployment {
    Deployment {
        viewers: std::collections::BTreeSet::new(),
        actors: std::collections::BTreeMap::new(),
        chains: Chains::of(vec![chain()]).expect("one chain"),
        categories: [("home".to_owned(), 1), ("other".to_owned(), 2)]
            .into_iter()
            .collect(),
        app_url: "https://example.test".to_owned(),
        coding: None,
        zone: suzatary_core::zone::Zone::UTC,
    }
}

/// One todo, `alpha`, on a chain nothing is appended to, and one open conversation that
/// keeps no line, so every case meets the same box.
struct Fixed {
    root: PathBuf,
    events: EventStore,
}

impl Drop for Fixed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct NoLock;
impl WriteLock for NoLock {}

impl Fixed {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("suzatary-calls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let events = EventStore::new(root.join("events"), chain().untrusted);
        let alpha = suzatary_core::authoring::build_todo(
            &suzatary_core::authoring::NewTodo {
                todo: "alpha".to_owned(),
                fields: suzatary_core::todo::TodoFields {
                    body: "Book the flight".to_owned(),
                    rationale: "A trip this month.".to_owned(),
                    kind: "todo".to_owned(),
                    category: "home".to_owned(),
                    price: suzatary_core::todo::Price::Flat { percent: 45 },
                    detail: String::new(),
                    waiting_on: String::new(),
                },
                source: None,
                subtodos: Vec::new(),
            },
            suzatary_core::zone::Moment {
                at: suzatary_core::capability::Datetime::new(2026, 9, 1, 0, 0, 0, 0)
                    .expect("a day"),
                zone: suzatary_core::zone::Zone::UTC,
            },
            "owner",
            &["home", "other"],
            &suzatary_core::capability::Parses,
        )
        .expect("a todo");
        append_to(&events, suzatary_core::record::GENESIS, alpha).expect("appends");
        Self { root, events }
    }
}

impl Reads<TodoEvent> for Fixed {
    fn objects(&self) -> Result<Vec<(Hash, Envelope)>, StoreError> {
        linearised(&self.events).map_err(|e| StoreError::unreadable("chain", e.to_string()))
    }
    fn nodes(&self) -> Result<Vec<Node>, StoreError> {
        prodrome_core::dag::Dag::from_iter(self.objects()?)
            .nodes()
            .map_err(|e| StoreError::unreadable("chain", e.to_string()))
    }
    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        Ok(std::collections::BTreeSet::new())
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        Ok(Vec::new())
    }
    fn verify(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Prodrome<TodoEvent> for Fixed {
    fn append(&self, _event: TodoEvent) -> Result<Hash, StoreError> {
        Err(StoreError::not_appended("chain", "nothing is written"))
    }
    fn accept(&self, _objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        Err(StoreError::not_appended("chain", "nothing is written"))
    }
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        Ok(Box::new(NoLock))
    }
}

/// Where a line pushed to [`Fixed`]'s conversation keeps its keys.
static KEYS: std::sync::LazyLock<suzatary_core::store::InMemoryKeys> =
    std::sync::LazyLock::new(Default::default);
static ENTROPY: std::sync::LazyLock<suzatary_core::secretary::conversations::Counting> =
    std::sync::LazyLock::new(Default::default);
/// The watches `watch_mail` sets.
static WATCHES: std::sync::LazyLock<
    suzatary_core::store::InMemoryLog<
        suzatary_core::mail::watch::WatchId,
        suzatary_core::mail::watch::Line,
    >,
> = std::sync::LazyLock::new(|| suzatary_core::store::InMemoryLog::new("watch"));
/// The words opened, kept for the calls' conversation.
static OPENED: std::sync::LazyLock<suzatary_core::secretary::opened::Opened> =
    std::sync::LazyLock::new(suzatary_core::secretary::opened::Opened::default);

/// The one open conversation, `default`, which keeps no line pushed to it: its title's key is
/// kept nowhere, so it reads as missing.
impl Reads<LineEvent> for Fixed {
    fn objects(
        &self,
    ) -> Result<Vec<(Hash, prodrome_core::event::Envelope<LineEvent>)>, StoreError> {
        let (opened, _) = LineEvent::write(
            suzatary_core::secretary::store::DEFAULT.clone(),
            Actor::new("assistant").expect("an actor"),
            0,
            &Entry::Opened {
                at: suzatary_core::capability::Datetime::new(2026, 9, 7, 0, 0, 0, 0)
                    .expect("an instant"),
                title: suzatary_core::secretary::words::Words::kept(String::new()),
                model: "nothing".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Topic,
            },
            &Nothing,
        )
        .expect("a line");
        let object = prodrome_core::event::mk_sealed(None, opened);
        Ok(vec![(prodrome_core::event::seal_hash(&object), object)])
    }
    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        Ok(std::collections::BTreeSet::new())
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        Ok(Reads::<LineEvent>::objects(self)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }
    fn verify(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Prodrome<LineEvent> for Fixed {
    fn append(&self, event: LineEvent) -> Result<Hash, StoreError> {
        Ok(prodrome_core::event::seal_hash(
            &prodrome_core::event::mk_sealed(None, event),
        ))
    }
    fn accept(&self, _objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        Ok(Vec::new())
    }
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        Ok(Box::new(NoLock))
    }
}

/// One mail account, whose Junk is readable and empty.
const ACCOUNT: &str = "a@b.test";

impl Junk for Fixed {
    fn junk_accounts(&self) -> Vec<String> {
        vec![ACCOUNT.to_owned()]
    }
    fn churn(&self) -> ChurnList {
        ChurnList::default()
    }
    fn junk_headers(
        &self,
        _account: &str,
        _since: chrono::NaiveDate,
    ) -> Result<Vec<JunkHeader>, JunkFailure> {
        Ok(Vec::new())
    }
    fn junk_message(&self, account: &str, uid: Uid) -> Result<JunkMessage, JunkFailure> {
        Err(JunkFailure::Missing {
            account: account.to_owned(),
            uid: uid.0,
        })
    }
}

impl Mailbox for Fixed {
    fn accounts(&self) -> Vec<String> {
        vec![ACCOUNT.to_owned()]
    }
    fn inbox(
        &self,
        account: Option<&str>,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        Nothing.inbox(account, limit, before)
    }
    fn search(
        &self,
        query: &Query,
        account: Option<&str>,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        Mailbox::search(&Nothing, query, account, limit, before)
    }
    fn message(&self, id: &str) -> Result<MailMessage, MailError> {
        Nothing.message(id)
    }
    fn thread(&self, thread: &str, limit: usize) -> Result<Vec<MailMessage>, MailError> {
        Nothing.thread(thread, limit)
    }
    fn attachments(&self, id: &str) -> Result<Vec<AttachmentMeta>, MailError> {
        Nothing.attachments(id)
    }
    fn stored(&self, id: &str, cap: u64) -> Result<suzatary_core::mail::Stored, MailError> {
        Nothing.stored(id, cap)
    }
    fn attachment(&self, id: &str, name: &str) -> Result<AttachmentFile, MailError> {
        Nothing.attachment(id, name)
    }
    fn read_attachment(
        &self,
        id: &str,
        name: &str,
    ) -> Result<suzatary_core::file_read::FileRead, MailError> {
        Nothing.read_attachment(id, name)
    }
}

/// One repository, `owner/name`, whose every read answers and every patch applies.
impl Repositories for Fixed {
    fn repos(&self) -> Result<Vec<RepoRow>, RepoFailure> {
        Ok(vec![RepoRow {
            repo: "owner/name".parse().expect("a repository name"),
            visibility: Visibility::Private,
            default_branch: "main".to_owned(),
            description: String::new(),
        }])
    }
    /// A commit resolves to itself; any other name, or none, to the zero commit.
    fn resolve(&self, _repo: &RepoName, at: Option<&Rev>) -> Result<CommitId, RepoFailure> {
        Ok(at
            .and_then(|rev| CommitId::try_from(rev.as_str().to_owned()).ok())
            .unwrap_or_else(|| CommitId::try_from("0".repeat(40)).expect("a commit")))
    }
    fn tree(
        &self,
        _repo: &RepoName,
        _at: &CommitId,
        _dir: &RepoPath,
    ) -> Result<Vec<String>, RepoFailure> {
        Ok(vec!["src/".to_owned()])
    }
    fn blob(
        &self,
        _repo: &RepoName,
        _at: &CommitId,
        _path: &RepoPath,
    ) -> Result<Vec<u8>, RepoFailure> {
        Ok(b"fn main() {}".to_vec())
    }
    fn grep(
        &self,
        _repo: &RepoName,
        _at: &CommitId,
        _needle: &str,
    ) -> Result<Vec<String>, RepoFailure> {
        Ok(Vec::new())
    }
    fn applies(
        &self,
        _repo: &RepoName,
        _base: &CommitId,
        _patch: &Patch,
    ) -> Result<(), RepoFailure> {
        Ok(())
    }
    fn pushable(&self, _repo: &RepoName, _visibility: Visibility) -> Result<(), RepoFailure> {
        Ok(())
    }
}

impl MailSync for Fixed {
    fn last(&self) -> Result<Option<Synced>, SyncFailure> {
        Ok(Some(Synced {
            at: Datetime::new(2026, 9, 6, 23, 59, 30, 0).expect("a real date"),
            report: SyncReport {
                unheld: None,
                synced: vec![AccountSync {
                    account: suzatary_core::mail::account_email::AccountEmail::new(ACCOUNT)
                        .expect("an address"),
                    outcome: suzatary_core::health::Outcome::Succeeded,
                    tail: "Sent: 1 new".to_owned(),
                }],
                notmuch: "Added 1 new message.".to_owned(),
            },
        }))
    }
    fn start(&self, _since: Datetime) -> Result<Option<Synced>, SyncFailure> {
        Err(SyncFailure(
            "a fresh sync is answered without starting one".to_owned(),
        ))
    }
}

/// A value the verb reads as valid, so a case's fault is the one it names.
fn valid(verb: &str, name: &str) -> Value {
    match (verb, name) {
        ("record", "kind") => json!("completed"),
        ("add_topics", "topics") => json!(["garden"]),
        ("add_topics", "on") => json!("round"),
        ("todo_add", "todo_id") => json!("beta"),
        ("remember", "kind") => json!("person"),
        ("watch_mail", "on") => json!("sender"),
        ("propose_act", "act") => json!("auth"),
        ("propose_forward", "as") => json!("attached"),
        ("search_older", "query") => json!("subject:invite"),
        ("search_older", "since") => json!("2023-01-01"),
        ("drive_read", "path") => json!("/my-files/a.pdf"),
        ("drive_list", "path") => json!("/my-files"),
        ("repo_tree" | "repo_read", "path") => json!("src/main.rs"),
        ("proposals_accept" | "proposals_reject", "objects") | ("upload_read", "file") => {
            json!("0".repeat(64))
        }
        ("forget", "id")
        | (_, "uid" | "index" | "revises" | "recur_every_days" | "periodic_every_days") => {
            json!(3)
        }
        (_, "account" | "source_sender") => json!("a@b.test"),
        (_, "todo_id" | "todo" | "parent") => json!("alpha"),
        (_, "body") => json!("Book the flight"),
        (_, "rationale" | "why" | "note" | "detail" | "detail_append" | "waiting_on") => {
            json!("Because it matters.")
        }
        (_, "kind") => json!("todo"),
        (_, "category") => json!("home"),
        (_, "query") => json!("from:a@b.test"),
        (_, "id" | "hash" | "native_id" | "source_hash") => json!("abcdef123456"),
        (_, "limit" | "part" | "days" | "deadline_lead_up_weeks") => json!(2),
        (_, "disposition") => json!("skip"),
        (_, "sender" | "to" | "cc") => json!("c@d.test"),
        (_, "date" | "deadline_end_date" | "since" | "day" | "source_date") => json!("2026-10-01"),
        (_, "deadline_start_date") => json!("2026-09-20"),
        (_, "name") => json!("MEMORY"),
        (_, "spec_json") => json!({"kind": "flat", "value": 0.5}),
        (_, "at_iso") => json!("2026-09-07T00:00:00"),
        (_, "chain") => json!("life"),
        (_, "priority" | "cycle_from_priority") => json!("45%"),
        (_, "deadline_start_priority" | "cycle_to_priority") => json!("60%"),
        (_, "deadline_end_priority") => json!("5%"),
        (_, "source_subject" | "subject" | "title" | "message" | "task") => json!("Hello"),
        (_, "clear_waiting_on") => json!(true),
        (_, "fact") => json!("The owner drinks tea."),
        (_, "sources" | "serves") => json!("todo:life/alpha"),
        (_, "acts") => json!([{"act": "complete", "todo": "alpha"}]),
        (_, "attach") => json!([{"upload": "0123456789abcdef0123456789abcdef"}]),
        (_, "search") => json!("weather"),
        (_, "url") => json!("https://example.test/a"),
        (_, "always") => json!("example.test"),
        (_, "repo") => json!("owner/name"),
        (_, "at" | "base") => json!("0123456789abcdef0123456789abcdef01234567"),
        (_, "text") => json!("main"),
        (_, "patch") => json!("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n"),
        _ => json!("x"),
    }
}

/// What else a call needs to be valid beyond its required arguments.
fn companions(verb: &str) -> &'static [&'static str] {
    match verb {
        "todo_add" => &["priority"],
        "todo_update" => &["priority", "rationale"],
        "propose_act" => &["account"],
        "web" => &["search"],
        _ => &[],
    }
}

/// A value of some other type than the schema's.
fn mistyped(schema: &Value) -> Value {
    let kind = match &schema["type"] {
        Value::Array(kinds) => kinds[0].clone(),
        kind => kind.clone(),
    };
    match kind.as_str() {
        Some("string") => json!(7),
        Some("integer") => json!("7"),
        Some("boolean") => json!("yes"),
        _ => json!("x"),
    }
}

/// Every case of one tool: its name and its arguments.
fn cases(tool: &Value) -> Vec<(String, Map<String, Value>)> {
    let verb = tool["name"].as_str().expect("a name");
    let properties = tool["inputSchema"]["properties"]
        .as_object()
        .expect("properties");
    let required: Vec<&str> = tool["inputSchema"]["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|name| name.as_str().expect("a name"))
        .collect();
    let mut base = Map::new();
    for name in required.iter().chain(companions(verb)) {
        base.insert((*name).to_owned(), valid(verb, name));
    }
    let with = |name: &str, value: Value| {
        let mut args = base.clone();
        args.insert(name.to_owned(), value);
        args
    };
    let mut out = vec![("valid".to_owned(), base.clone())];
    for (name, schema) in properties {
        if required.contains(&name.as_str()) {
            let mut args = base.clone();
            args.remove(name);
            out.push((format!("{name} missing"), args));
        } else {
            out.push((format!("{name} given"), with(name, valid(verb, name))));
        }
        out.push((format!("{name} null"), with(name, Value::Null)));
        out.push((format!("{name} mistyped"), with(name, mistyped(schema))));
        if schema["type"] == "string" || schema["type"][0] == "string" {
            out.push((format!("{name} garbled"), with(name, json!("?"))));
        }
        if schema["type"] == "integer" || schema["type"][0] == "integer" {
            out.push((format!("{name} negative"), with(name, json!(-1))));
            out.push((format!("{name} fractional"), with(name, json!(1.5))));
        }
    }
    out.push(("chain unknown".to_owned(), with("chain", json!("nowhere"))));
    out.push(("chain mistyped".to_owned(), with("chain", json!(7))));
    out
}

fn line(out: &mut String, surface: &str, verb: &str, case: &str, said: Result<Value, Refusal>) {
    let said = match said {
        Ok(answer) => format!("answered {answer}"),
        Err(refusal) => format!("refused {}: {}", refusal.status().code(), refusal.message()),
    };
    let _ = writeln!(out, "{surface} {verb} [{case}] {said}");
}

fn agent(
    fixed: &Fixed,
    deployment: &Deployment,
    verb: &str,
    args: &Map<String, Value>,
) -> Result<Value, Refusal> {
    let caps = suzatary_core::capability::Agent {
        chain: fixed,
        ..Nothing::agent()
    };
    let records = [("life".to_owned(), caps)].into_iter().collect();
    let conversations = suzatary_core::secretary::conversations::InMemory::default();
    let inbox = suzatary_core::store::InMemory::<suzatary_core::secretary::inbox_event::InboxEvent>::default();
    let cards =
        suzatary_core::secretary::filed::TriageCards::new(conversations.conversations(), &inbox);
    suzatary_core::agent::call(&records, &cards, deployment, verb, args)
}

fn secretary(
    fixed: &Fixed,
    deployment: &Deployment,
    verb: &str,
    args: &Map<String, Value>,
) -> Result<Value, Refusal> {
    let policy = chain();
    let chains = [Bound {
        name: "life",
        caps: suzatary_core::capability::Secretary {
            chain: fixed,
            conversations: suzatary_core::secretary::conversations::Conversations {
                lines: fixed,
                keys: &*KEYS,
                entropy: &*ENTROPY,
                renders: &suzatary_core::capability::Nothing,
                opened: &OPENED,
            },
            junk: fixed,
            mail: fixed,
            sync: fixed,
            repositories: fixed,
            watches: &*WATCHES,
            ..Nothing::secretary()
        },
        policy: &policy,
    }];
    suzatary_core::secretary::call(
        deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &suzatary_core::secretary::store::DEFAULT.clone(),
        verb,
        args,
    )
    .map(|answer| answer.value)
}

/// Calls whose arguments depend on each other: a price's parts, and a changeset's acts.
fn shaped(verb: &str) -> Vec<(String, Map<String, Value>)> {
    match verb {
        "todo_add" => priced("beta"),
        "todo_update" => priced("alpha"),
        "propose_changes" => changesets(),
        "watch_mail" => vec![
            named(
                "a sender",
                json!({"on": "sender", "text": "desk@library.example.test", "days": 14}),
            ),
            named(
                "too long",
                json!({"on": "subject", "text": "Your parcel", "days": 61}),
            ),
        ],
        _ => Vec::new(),
    }
}

fn named(case: &str, args: Value) -> (String, Map<String, Value>) {
    let Value::Object(args) = args else {
        panic!("arguments are an object: {args}");
    };
    (case.to_owned(), args)
}

/// A todo's price and source, each part given alone and with the parts it needs.
fn priced(id: &str) -> Vec<(String, Map<String, Value>)> {
    let todo = |extra: Value| {
        let mut args = json!({"todo_id": id, "body": "Book it", "rationale": "Because."});
        if let (Some(args), Value::Object(extra)) = (args.as_object_mut(), extra) {
            args.extend(extra);
        }
        args
    };
    [
        ("no price", json!({})),
        (
            "two prices",
            json!({"priority": "45%", "deadline_end_date": "2026-10-01"}),
        ),
        ("deadline", json!({"deadline_end_date": "2026-10-01"})),
        (
            "deadline whole",
            json!({"deadline_end_date": "2026-10-01",
            "deadline_start_priority": "70%", "deadline_end_priority": "10%",
            "deadline_start_date": "2026-09-20", "deadline_lead_up_weeks": 3}),
        ),
        (
            "deadline start mistyped",
            json!({"deadline_end_date": "2026-10-01",
            "deadline_start_priority": 7}),
        ),
        (
            "deadline end garbled",
            json!({"deadline_end_date": "2026-10-01",
            "deadline_end_priority": "5"}),
        ),
        (
            "deadline start date garbled",
            json!({"deadline_end_date": "2026-10-01",
            "deadline_start_date": "soon"}),
        ),
        (
            "deadline weeks mistyped",
            json!({"deadline_end_date": "2026-10-01",
            "deadline_lead_up_weeks": "3"}),
        ),
        (
            "deadline weeks negative",
            json!({"deadline_end_date": "2026-10-01",
            "deadline_lead_up_weeks": -3}),
        ),
        (
            "recur whole",
            json!({"recur_every_days": 30, "cycle_from_priority": "90%",
            "cycle_to_priority": "30%"}),
        ),
        (
            "recur from mistyped",
            json!({"recur_every_days": 30, "cycle_from_priority": 9,
            "cycle_to_priority": "30%"}),
        ),
        (
            "recur to missing",
            json!({"recur_every_days": 30, "cycle_from_priority": "90%"}),
        ),
        (
            "periodic whole",
            json!({"periodic_every_days": 30, "cycle_from_priority": "90%",
            "cycle_to_priority": "30%"}),
        ),
        (
            "cycle ends alone",
            json!({"priority": "45%", "cycle_from_priority": "90%",
            "cycle_to_priority": 3}),
        ),
        (
            "cycle end mistyped alone",
            json!({"priority": "45%", "cycle_to_priority": 3}),
        ),
        (
            "source whole",
            json!({"priority": "45%", "source_sender": "a@b.test",
            "source_subject": "Hi", "source_date": "2026-09-01", "source_hash": "abc"}),
        ),
        (
            "source date garbled",
            json!({"priority": "45%", "source_sender": "a@b.test",
            "source_subject": "Hi", "source_date": "soon", "source_hash": "abc"}),
        ),
        ("rationale alone", json!({"rationale": "Only the reason."})),
        (
            "detail both",
            json!({"priority": "45%", "detail": "d", "detail_append": "e"}),
        ),
    ]
    .into_iter()
    .map(|(case, extra)| named(case, todo(extra)))
    .collect()
}

/// A changeset's acts, one fault each.
fn changesets() -> Vec<(String, Map<String, Value>)> {
    let acts = |acts: Value| json!({"acts": acts, "why": "Because.", "sources": "todo:life/alpha"});
    [
        (
            "add",
            json!([{"act": "add", "todo": "beta", "body": "B", "rationale": "R",
            "priority": "45%"}]),
        ),
        (
            "add body missing",
            json!([{"act": "add", "todo": "beta", "rationale": "R",
            "priority": "45%"}]),
        ),
        (
            "add kind mistyped",
            json!([{"act": "add", "todo": "beta", "body": "B",
            "rationale": "R", "priority": "45%", "kind": 7}]),
        ),
        (
            "add no price",
            json!([{"act": "add", "todo": "beta", "body": "B",
            "rationale": "R"}]),
        ),
        (
            "add two prices",
            json!([{"act": "add", "todo": "beta", "body": "B",
            "rationale": "R", "priority": "45%", "recur_every_days": 3,
            "cycle_from_priority": "90%", "cycle_to_priority": "30%"}]),
        ),
        (
            "add then complete",
            json!([{"act": "add", "todo": "beta", "body": "B",
            "rationale": "R", "priority": "45%"}, {"act": "complete", "todo": "beta"}]),
        ),
        (
            "update body mistyped",
            json!([{"act": "update", "todo": "alpha", "body": 7}]),
        ),
        (
            "update clear mistyped",
            json!([{"act": "update", "todo": "alpha",
            "clear_waiting_on": "yes"}]),
        ),
        (
            "complete note mistyped",
            json!([{"act": "complete", "todo": "alpha", "note": 7}]),
        ),
        (
            "complete body mistyped",
            json!([{"act": "complete", "todo": "alpha", "body": 7}]),
        ),
        (
            "complete priority mistyped",
            json!([{"act": "complete", "todo": "alpha",
            "priority": 7}]),
        ),
        (
            "tend",
            json!([{"act": "tend", "todo": "alpha", "note": "again"}]),
        ),
        ("cancel", json!([{"act": "cancel", "todo": "alpha"}])),
        (
            "reprice no price",
            json!([{"act": "reprice", "todo": "alpha", "rationale": "R"}]),
        ),
        (
            "reprice no rationale",
            json!([{"act": "reprice", "todo": "alpha",
            "priority": "45%"}]),
        ),
        ("unknown act", json!([{"act": "fly", "todo": "alpha"}])),
        ("act missing", json!([{"todo": "alpha"}])),
        ("todo missing", json!([{"act": "add"}])),
        (
            "todo_id instead",
            json!([{"act": "complete", "todo_id": "alpha"}]),
        ),
        ("not an object", json!(["complete alpha"])),
        ("empty", json!([])),
        ("not an array", json!({"act": "complete", "todo": "alpha"})),
    ]
    .into_iter()
    .map(|(case, list)| named(case, acts(list)))
    .collect()
}

type Call = fn(&Fixed, &Deployment, &str, &Map<String, Value>) -> Result<Value, Refusal>;

fn printed() -> String {
    let deployment = deployment();
    let fixed = Fixed::new();
    let mut out = String::new();
    let surfaces: [(&str, Value, Call); 2] = [
        ("agent", suzatary_core::agent::tools(), agent),
        ("secretary", suzatary_core::secretary::tools(), secretary),
    ];
    for (surface, tools, call) in surfaces {
        for tool in tools.as_array().expect("a tool list") {
            let verb = tool["name"].as_str().expect("a name");
            for (case, args) in cases(tool).into_iter().chain(shaped(verb)) {
                line(
                    &mut out,
                    surface,
                    verb,
                    &case,
                    call(&fixed, &deployment, verb, &args),
                );
            }
        }
        line(
            &mut out,
            surface,
            "send",
            "no such verb",
            call(&fixed, &deployment, "send", &Map::new()),
        );
    }
    out
}

#[test]
fn every_call_answers_as_the_snapshot_says() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/calls.txt");
    let printed = printed();
    if std::env::var_os("SUZATARY_UPDATE_CALLS").is_some() {
        std::fs::write(&file, &printed).expect("the snapshot is writable");
        return;
    }
    let stored = std::fs::read_to_string(&file).expect("the snapshot is committed");
    for (at, (stored, printed)) in stored.lines().zip(printed.lines()).enumerate() {
        assert_eq!(stored, printed, "line {} of calls.txt", at + 1);
    }
    assert_eq!(stored.lines().count(), printed.lines().count());
}
