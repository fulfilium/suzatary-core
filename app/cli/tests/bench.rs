//! `suzatary-dev bench run` over the `Scripted` model and a temp chain; everything
//! else (instance, event store, run directory) is real.
//!
//! Every path written is absolute under a scratch directory, `repo` points at this checkout only
//! to read `OPERATOR.md`, and notmuch is a fake program, so nothing live is touched.

#![cfg(feature = "fixture")]

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

/// An absolute scratch directory unique to this process and thread.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "suzatary-bench-test-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a writable scratch directory");
    dir
}

/// This checkout's root, read only, so the system prompt is the real `OPERATOR.md`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/")
        .parent()
        .expect("the repo root")
        .to_path_buf()
}

/// A path as a TOML string, escaped by the `toml` crate so any path stays valid.
fn toml_path(path: &std::path::Path) -> String {
    toml::Value::String(path.display().to_string()).to_string()
}

/// A notmuch that answers nothing and logs, one line a call, its `NOTMUCH_CONFIG`, the
/// `hook_dir` that config names, its `XDG_CONFIG_HOME`, and its arguments, tab-separated.
fn fake_notmuch(dir: &std::path::Path) -> PathBuf {
    suzatary_testkit::fake_program(
        &dir.join("bin"),
        "notmuch",
        "hooks=$(sed -n 's/^hook_dir=//p' \"$NOTMUCH_CONFIG\" 2>/dev/null)\n\
         printf '%s\\t%s\\t%s\\t%s\\n' \"$NOTMUCH_CONFIG\" \"$hooks\" \"$XDG_CONFIG_HOME\" \"$*\" \
         >>\"$0.log\"\n",
    )
}

/// A minimal instance file: `repo` is the real checkout, `data` is the scratch `dir`.
fn write_instance(dir: &std::path::Path) -> PathBuf {
    let text = format!(
        r#"
repo = {repo}
data = {data}
[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["ada", "triage"]
owner = "ada"
[web]
viewers = ["her@example.test"]
app_url = "https://example.test/"
[web.act_as]
"her@example.test" = "ada"
[web.serving]
host = "127.0.0.1"
port = 8787
tls_dir = {tls}
dist = "web/dist"
briefings = "briefings"
[categories]
home = 3
[mail]
root = {mail_root}
state = "state.json"
notmuch = {notmuch}
mbsyncrc = "mbsyncrc"
heartbeat = {heartbeat}
[[mail.accounts]]
email = "her@example.test"
subdir = "her"
sync = "ms-imap"
authority = "organizations"
inbox_folders = ["Inbox"]
sent_folder = "Sent Items"
"#,
        repo = toml_path(&repo_root()),
        data = toml_path(dir),
        tls = toml_path(&dir.join("tls")),
        mail_root = toml_path(&dir.join("Mail")),
        heartbeat = toml_path(&dir.join("triage-heartbeat")),
        notmuch = toml_path(&fake_notmuch(dir)),
    );
    let path = dir.join("instance.toml");
    std::fs::write(&path, text).expect("a writable instance file");
    path
}

/// One triage case the incumbent declined, so a write proves the candidate's own.
fn write_case_file(dir: &std::path::Path) -> PathBuf {
    let text = r#"{
        "extracted_at": "2026-09-20T00:00:00Z",
        "cases": [
            {
                "kind": "triage",
                "id": "triage:deadbeefcafefeed",
                "message_hash": "deadbeefcafefeed",
                "chain_as_stood": { "chain": "life", "tip": null, "basis": "extraction" },
                "truths": { "incumbent": { "decided": "declined" }, "settled": null }
            }
        ],
        "mail": {}
    }"#;
    let path = dir.join("cases.json");
    std::fs::write(&path, text).expect("a writable case file");
    path
}

/// A script whose fallback line asks for a valid `todo_add`, then ends the round.
fn write_script(dir: &std::path::Path) -> PathBuf {
    let text = r#"[
        {
            "when": "list_open_todos",
            "reply": "{\"say\": \"filed\"}"
        },
        {
            "reply": "{\"say\": \"filing it\", \"calls\": [{\"verb\": \"todo_add\", \"args\": {\"todo_id\": \"benchtest1\", \"body\": \"a scripted candidate wrote this\", \"rationale\": \"the script said so\", \"category\": \"home\", \"priority\": \"65%\", \"why\": \"a message asks for it\", \"sources\": \"briefing:2026-09-02\"}}, {\"verb\": \"list_open_todos\", \"args\": {}}]}"
        }
    ]"#;
    let path = dir.join("script.json");
    std::fs::write(&path, text).expect("a writable script file");
    path
}

#[test]
fn a_scripted_todo_add_lands_as_a_real_object_and_a_run_resumes() {
    let dir = scratch("run");
    let instance_path = write_instance(&dir);
    let cases_path = write_case_file(&dir);
    let script_path = write_script(&dir);
    let out = dir.join("out");

    suzatary_cli::commands::bench::run_cmd(
        &suzatary_cli::instance::Instance::read(
            &instance_path,
            &suzatary_cli::paths::Env::under(&dir, &dir),
        )
        .expect("the instance reads"),
        &crate::binary::typeset(),
        None,
        Some(&script_path),
        &cases_path,
        &out,
        None,
        false,
        false,
    )
    .expect("a run over a scripted candidate and a temp chain");

    let answer_path = out.join("triage:deadbeefcafefeed.json");
    let first_text = std::fs::read_to_string(&answer_path).expect("an answer file");
    let first: suzatary_bench::run::RunEntry =
        serde_json::from_str(&first_text).expect("a run entry");

    // The run holds the owner's mail, so it must not rely on the umask.
    let dir_mode = std::fs::metadata(&out)
        .expect("stat the run dir")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dir_mode, 0o700, "the run directory must be private");
    let file_mode = std::fs::metadata(&answer_path)
        .expect("stat the answer file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(file_mode, 0o600, "the answer file must be private");
    let suzatary_bench::run::Outcome::Answered(first_answer) = &first.outcome else {
        panic!("a scripted todo_add must answer: {first:?}");
    };

    let round = &first_answer.rounds[0];
    // The script reads the open list after filing, so its next cue names no verb the opening does.
    assert_eq!(first_answer.rounds.len(), 2, "{first:?}");
    assert_eq!(round.calls.len(), 2);
    assert_eq!(round.calls[0].verb, "todo_add");
    let suzatary_bench::run::CallOutcome::Answered { value } = &round.calls[0].outcome else {
        panic!("todo_add was refused: {:?}", round.calls[0].outcome);
    };
    assert_eq!(value["filed"], serde_json::json!("proposal:triage/0"));

    // The verb's answer carries no hash, so these came from the scratch inbox itself: its
    // `Genesis`, since the card is its first write, and the card.
    assert_eq!(first_answer.objects.len(), 2, "{first:?}");
    let hash = &first_answer.objects[0];
    assert_eq!(hash.len(), 64, "{hash:?} is not a sha256 hex digest");
    assert!(
        hash.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "{hash:?} is not lowercase hex"
    );
    assert!(first_answer.model.starts_with("scripted/"));
    // A candidate that cannot measure its cost reports `None`, never a zero.
    assert!(first_answer.usage.is_none());

    assert!(!out.join(".chains").join("triage:deadbeefcafefeed").exists());
    assert!(!dir.join("events").exists());

    // A re-run without `--force` skips the answered case.
    suzatary_cli::commands::bench::run_cmd(
        &suzatary_cli::instance::Instance::read(
            &instance_path,
            &suzatary_cli::paths::Env::under(&dir, &dir),
        )
        .expect("the instance reads"),
        &crate::binary::typeset(),
        None,
        Some(&script_path),
        &cases_path,
        &out,
        None,
        false,
        false,
    )
    .expect("a resumed run");
    let second_text = std::fs::read_to_string(&answer_path).expect("the answer file, unchanged");
    assert_eq!(first_text, second_text, "a resumed run must not re-ask");
}

/// A run made only of failures must not look like a quiet success.
#[test]
fn a_failed_turn_is_recorded_failed_and_the_run_reports_it_and_exits_nonzero() {
    let dir = scratch("failed");
    let instance_path = write_instance(&dir);
    let cases_path = write_case_file(&dir);
    // An empty script refuses every cue, so the first `Model::turn` fails.
    let script_path = dir.join("empty-script.json");
    std::fs::write(&script_path, "[]").expect("a writable script file");
    let out = dir.join("out");

    let refused = suzatary_cli::commands::bench::run_cmd(
        &suzatary_cli::instance::Instance::read(
            &instance_path,
            &suzatary_cli::paths::Env::under(&dir, &dir),
        )
        .expect("the instance reads"),
        &crate::binary::typeset(),
        None,
        Some(&script_path),
        &cases_path,
        &out,
        None,
        false,
        false,
    )
    .expect_err("a run made entirely of failures must not exit zero");
    assert!(refused.to_string().contains("failed"), "{refused}");

    let answer_path = out.join("triage:deadbeefcafefeed.json");
    let text = std::fs::read_to_string(&answer_path).expect("an answer file, even for a failure");
    let recorded: suzatary_bench::run::RunEntry = serde_json::from_str(&text).expect("a run entry");
    match recorded.outcome {
        suzatary_bench::run::Outcome::Failed { error } => {
            assert!(error.contains("no line"), "{error}");
        }
        suzatary_bench::run::Outcome::Answered(_) => {
            panic!("a script with no matching line must fail, not answer")
        }
    }

    // Resume re-asks a failed case without `--force`.
    let recorded_at_before = recorded.recorded_at.clone();
    let _ = suzatary_cli::commands::bench::run_cmd(
        &suzatary_cli::instance::Instance::read(
            &instance_path,
            &suzatary_cli::paths::Env::under(&dir, &dir),
        )
        .expect("the instance reads"),
        &crate::binary::typeset(),
        None,
        Some(&script_path),
        &cases_path,
        &out,
        None,
        false,
        false,
    );
    let reread: suzatary_bench::run::RunEntry =
        serde_json::from_str(&std::fs::read_to_string(&answer_path).expect("an answer file"))
            .expect("a run entry");
    assert_ne!(
        reread.recorded_at, recorded_at_before,
        "a failed case must be re-asked on resume, not skipped"
    );

    // `--skip-failed` leaves it standing, so no new failure makes this call fail.
    let before_skip = std::fs::read_to_string(&answer_path).expect("an answer file");
    suzatary_cli::commands::bench::run_cmd(
        &suzatary_cli::instance::Instance::read(
            &instance_path,
            &suzatary_cli::paths::Env::under(&dir, &dir),
        )
        .expect("the instance reads"),
        &crate::binary::typeset(),
        None,
        Some(&script_path),
        &cases_path,
        &out,
        None,
        false,
        true,
    )
    .expect("--skip-failed leaves the prior failure alone rather than re-asking it");
    let after_skip = std::fs::read_to_string(&answer_path).expect("an answer file");
    assert_eq!(
        before_skip, after_skip,
        "--skip-failed must leave a prior failure's file untouched"
    );
}

/// A real model builds its prompt with `prompt_of`, so this is the one builder
/// to check the opening turn and system text against.
#[test]
fn the_opening_turn_and_the_system_prompt_are_the_cases_own_input() {
    let case: suzatary_bench::cases::Case = serde_json::from_str(
        r#"{
            "kind": "triage",
            "id": "triage:deadbeefcafefeed",
            "message_hash": "deadbeefcafefeed",
            "chain_as_stood": { "chain": "life", "tip": null, "basis": "extraction" },
            "truths": { "incumbent": { "decided": "declined" }, "settled": null }
        }"#,
    )
    .expect("a triage case");
    let (chain, tip, opening) =
        suzatary_cli::commands::bench::opening_of(&case).expect("a triage case has an opening");
    assert_eq!(chain, "life");
    assert_eq!(tip, None);
    assert!(opening.contains("deadbeefcafefeed"));
    assert!(
        !opening.contains("declined"),
        "the incumbent's answer must never reach the candidate"
    );

    let manual = suzatary_io::manuals::Handbook::new(
        repo_root(),
        suzatary_core::persona::Persona {
            name: "(unconfigured persona)".to_owned(),
            city: String::new(),
            zone: suzatary_core::zone::Zone::UTC,
            prose: suzatary_core::persona::Prose::default(),
        },
    );
    let system = suzatary_core::capability::Manuals::document(
        &manual,
        suzatary_core::capability::Manual::Operator,
    )
    .expect("this checkout carries OPERATOR.md");
    let conversation = suzatary_core::capability::Conversation {
        system: system.clone(),
        turns: vec![suzatary_core::capability::Turn::Person {
            text: opening,
            references: Vec::new(),
            attachments: Vec::new(),
        }],
    };
    let prompt = suzatary_io::prompt::prompt_of(
        &conversation,
        &suzatary_core::agent::tools(),
        suzatary_io::prompt::Reads::Files,
    );
    assert_eq!(prompt.system.as_str(), system);
    assert!(prompt.user.contains("deadbeefcafefeed"));
}

/// The instance `write_instance` writes, plus a second chain named `suzatary`.
fn write_instance_with_project_chain(dir: &std::path::Path) -> PathBuf {
    let text = format!(
        r#"
repo = {repo}
data = {data}
[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["ada", "triage"]
owner = "ada"
[[chains]]
name = "suzatary"
events = "events-suzatary"
untrusted = ["triage"]
actors = ["ada", "triage"]
owner = "ada"
[web]
viewers = ["her@example.test"]
app_url = "https://example.test/"
[web.act_as]
"her@example.test" = "ada"
[web.serving]
host = "127.0.0.1"
port = 8787
tls_dir = {tls}
dist = "web/dist"
briefings = "briefings"
[categories]
home = 3
[mail]
root = {mail_root}
state = "state.json"
notmuch = {notmuch}
mbsyncrc = "mbsyncrc"
heartbeat = {heartbeat}
[[mail.accounts]]
email = "her@example.test"
subdir = "her"
sync = "ms-imap"
authority = "organizations"
inbox_folders = ["Inbox"]
sent_folder = "Sent Items"
"#,
        repo = toml_path(&repo_root()),
        data = toml_path(dir),
        tls = toml_path(&dir.join("tls")),
        mail_root = toml_path(&dir.join("Mail")),
        heartbeat = toml_path(&dir.join("triage-heartbeat")),
        notmuch = toml_path(&fake_notmuch(dir)),
    );
    let path = dir.join("instance.toml");
    std::fs::write(&path, text).expect("a writable instance file");
    path
}

/// A script whose `todo_add` on a `life` case names the `suzatary` chain.
fn write_project_chain_script(dir: &std::path::Path) -> PathBuf {
    let text = r#"[
        {
            "when": "list_open_todos",
            "reply": "{\"say\": \"filed on the project chain\"}"
        },
        {
            "reply": "{\"say\": \"this is about the system, not Ada's life\", \"calls\": [{\"verb\": \"todo_add\", \"args\": {\"chain\": \"suzatary\", \"todo_id\": \"projecttest1\", \"body\": \"a system fact routed to the project chain\", \"rationale\": \"the script said so\", \"category\": \"home\", \"priority\": \"65%\", \"why\": \"a message asks for it\", \"sources\": \"briefing:2026-09-02\"}}, {\"verb\": \"list_open_todos\", \"args\": {}}]}"
        }
    ]"#;
    let path = dir.join("script.json");
    std::fs::write(&path, text).expect("a writable script file");
    path
}

/// A replay binds every chain, so routing a write to another chain is not scored
/// as a failure.
#[test]
fn a_call_naming_the_project_chain_lands_there_not_as_a_refusal() {
    let dir = scratch("project-chain");
    let instance_path = write_instance_with_project_chain(&dir);
    let cases_path = write_case_file(&dir);
    let script_path = write_project_chain_script(&dir);
    let out = dir.join("out");

    suzatary_cli::commands::bench::run_cmd(
        &suzatary_cli::instance::Instance::read(
            &instance_path,
            &suzatary_cli::paths::Env::under(&dir, &dir),
        )
        .expect("the instance reads"),
        &crate::binary::typeset(),
        None,
        Some(&script_path),
        &cases_path,
        &out,
        None,
        false,
        false,
    )
    .expect("a run whose candidate writes to the project chain");

    let answer_path = out.join("triage:deadbeefcafefeed.json");
    let text = std::fs::read_to_string(&answer_path).expect("an answer file");
    let entry: suzatary_bench::run::RunEntry = serde_json::from_str(&text).expect("a run entry");
    let suzatary_bench::run::Outcome::Answered(answer) = &entry.outcome else {
        panic!("a candidate that routed its write must still answer: {entry:?}");
    };

    let round = &answer.rounds[0];
    assert_eq!(answer.rounds.len(), 2, "{answer:?}");
    assert_eq!(round.calls.len(), 2);
    assert_eq!(round.calls[0].verb, "todo_add");
    assert_eq!(
        round.calls[0].args.get("chain"),
        Some(&serde_json::json!("suzatary"))
    );
    let suzatary_bench::run::CallOutcome::Answered { value } = &round.calls[0].outcome else {
        panic!(
            "a call naming a bound chain must be answered, not refused: {:?}",
            round.calls[0].outcome
        );
    };
    assert_eq!(
        value["filed"],
        serde_json::json!("proposal:triage/0"),
        "the card for the project chain must be filed: {value:?}"
    );

    assert_eq!(
        answer.objects.len(),
        2,
        "the inbox's `Genesis` and the card: {answer:?}"
    );

    assert!(!dir.join("events").exists());
    assert!(!dir.join("events-suzatary").exists());
    assert!(!out.join(".chains").exists());
}

/// A bench candidate is another vendor's model, and a case is an old message: the box's memory
/// reaches neither, so the candidate's `context` remembers nothing whatever the box remembers.
#[test]
fn a_bench_candidate_is_told_no_memory_of_the_box() {
    let dir = scratch("memory");
    let instance_path = write_instance(&dir);
    let instance = suzatary_cli::instance::Instance::read(
        &instance_path,
        &suzatary_cli::paths::Env::under(&dir, &dir),
    )
    .expect("the instance reads");
    let remembering = suzatary_cli::wiring::Remembering::reader(&instance);
    remembering
        .memories()
        .remember(
            &suzatary_core::secretary::memory::Fact::new("The landlord is Marguerite")
                .expect("a fact"),
            suzatary_core::secretary::memory::Kind::Person,
            &suzatary_core::secretary::memory::Origin::Turn {
                conversation: suzatary_core::secretary::store::ConversationId::new("a")
                    .expect("a slug"),
                turn: 0,
            },
            suzatary_core::capability::Clock::now(&suzatary_io::clock::SystemClock),
        )
        .expect("the box remembers");
    let script = dir.join("script.json");
    std::fs::write(
        &script,
        r#"[
            { "when": "context", "reply": "{\"say\": \"read\"}" },
            { "reply": "{\"say\": \"reading\", \"calls\": [{\"verb\": \"context\", \"args\": {}}]}" }
        ]"#,
    )
    .expect("a writable script file");
    let out = dir.join("out");
    suzatary_cli::commands::bench::run_cmd(
        &instance,
        &crate::binary::typeset(),
        None,
        Some(&script),
        &write_case_file(&dir),
        &out,
        None,
        false,
        false,
    )
    .expect("a run over a scripted candidate");
    let entry: suzatary_bench::run::RunEntry = serde_json::from_str(
        &std::fs::read_to_string(out.join("triage:deadbeefcafefeed.json")).expect("an answer"),
    )
    .expect("a run entry");
    let suzatary_bench::run::Outcome::Answered(answer) = &entry.outcome else {
        panic!("a scripted context must answer: {entry:?}");
    };
    let suzatary_bench::run::CallOutcome::Answered { value } = &answer.rounds[0].calls[0].outcome
    else {
        panic!("context was refused: {answer:?}");
    };
    assert_eq!(
        value["memory"],
        serde_json::json!("The secretary remembers nothing yet."),
        "{value}"
    );
}

/// Every file under `root` with its bytes, so two readings are equal exactly when nothing under
/// `root` was written, created or removed between them.
fn snapshot(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("a readable directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                files.insert(path.clone(), Vec::new());
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("a readable file");
                files.insert(path, bytes);
            }
        }
    }
    files
}

/// A box whose data root `data` holds a maildir, an index config and the instance file, and
/// whose home `home` is its state directory; answers the instance file's path.
fn write_box_with_mail(
    dir: &std::path::Path,
    data: &std::path::Path,
    home: &std::path::Path,
) -> PathBuf {
    std::fs::create_dir_all(data.join("Mail/her/cur")).expect("the box's maildir");
    std::fs::create_dir_all(home).expect("the box's home");
    std::fs::write(
        data.join("Mail/her/cur/1:2,S"),
        "Subject: the box's own\n\nmail\n",
    )
    .expect("a message of the box's");
    std::fs::write(data.join("notmuch-config"), "[database]\npath=elsewhere\n")
        .expect("the box's index config");
    let text = format!(
        r#"
repo = {repo}
data = {data}
[signatures]
root = "836884e1723c7b17f8b87e76a0272f012de6cff08f134eb9dcdb2670695b1818"

[[chains]]
name = "life"
events = "events"
untrusted = ["triage"]
actors = ["ada", "triage"]
owner = "ada"
[web]
viewers = ["her@example.test"]
app_url = "https://example.test/"
[web.act_as]
"her@example.test" = "ada"
[web.serving]
host = "127.0.0.1"
port = 8787
tls_dir = "tls"
dist = "web/dist"
briefings = "briefings"
[categories]
home = 3
[mail]
root = "Mail"
state = "state.json"
notmuch = {notmuch}
notmuch_config = "notmuch-config"
mbsyncrc = "mbsyncrc"
heartbeat = "triage-heartbeat"
[[mail.accounts]]
email = "her@example.test"
subdir = "her"
sync = "ms-imap"
authority = "organizations"
inbox_folders = ["Inbox"]
sent_folder = "Sent Items"
"#,
        repo = toml_path(&repo_root()),
        data = toml_path(data),
        notmuch = toml_path(&fake_notmuch(dir)),
    );
    let path = data.join("instance.toml");
    std::fs::write(&path, text).expect("a writable instance file");
    path
}

/// A case whose message the case file freezes, and a script that marks it, adds a noise sender
/// and syncs in one round.
fn write_mail_writing_case(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let message = b"Message-ID: <case@example.test>\nSubject: the case's own\n\nbody\n";
    let cases = dir.join("cases.json");
    std::fs::write(
        &cases,
        serde_json::json!({
            "extracted_at": "2026-09-20T00:00:00Z",
            "cases": [{
                "kind": "triage",
                "id": "triage:deadbeefcafefeed",
                "message_hash": "deadbeefcafefeed",
                "chain_as_stood": { "chain": "life", "tip": null, "basis": "extraction" },
                "truths": { "incumbent": { "decided": "declined" }, "settled": null }
            }],
            "mail": {
                "deadbeefcafefeed": {
                    "id": "case@example.test",
                    "files": [{
                        "path": "her/cur/2:2,S",
                        "bytes": base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            message,
                        ),
                    }],
                },
            },
        })
        .to_string(),
    )
    .expect("a writable case file");
    let script = dir.join("script.json");
    std::fs::write(
        &script,
        r#"[
            { "when": "sync", "reply": "{\"say\": \"done\"}" },
            { "reply": "{\"say\": \"handling it\", \"calls\": [{\"verb\": \"mark\", \"args\": {\"hash\": \"deadbeefcafefeed\", \"disposition\": \"skip\", \"native_id\": \"case@example.test\"}}, {\"verb\": \"noise_add\", \"args\": {\"sender\": \"alerts@example.test\"}}, {\"verb\": \"sync\", \"args\": {}}]}" }
        ]"#,
    )
    .expect("a writable script file");
    (cases, script)
}

/// A candidate's mail writes and sync are the case's alone: after `mark`, `noise_add` and `sync`,
/// the instance's data root (its chains, ledger, box-local tier and maildir) and its state
/// directory hold the same bytes as before, and every index call named the case's own config.
#[test]
fn a_candidates_mark_noise_and_sync_change_nothing_outside_the_cases_scratch() {
    let dir = scratch("sandbox");
    let data = dir.join("data");
    let home = dir.join("home");
    let instance = suzatary_cli::instance::Instance::read(
        &write_box_with_mail(&dir, &data, &home),
        &suzatary_cli::paths::Env::under(&home, dir.join("run")),
    )
    .expect("the instance reads");
    let (cases, script) = write_mail_writing_case(&dir);

    let before = (snapshot(&data), snapshot(&home));
    let out = dir.join("out");
    suzatary_cli::commands::bench::run_cmd(
        &instance,
        &crate::binary::typeset(),
        None,
        Some(&script),
        &cases,
        &out,
        None,
        false,
        false,
    )
    .expect("a run over a scripted candidate");
    assert_eq!(
        (snapshot(&data), snapshot(&home)),
        before,
        "a bench candidate changed the box's data or state"
    );

    let entry: suzatary_bench::run::RunEntry = serde_json::from_str(
        &std::fs::read_to_string(out.join("triage:deadbeefcafefeed.json")).expect("an answer"),
    )
    .expect("a run entry");
    let suzatary_bench::run::Outcome::Answered(answer) = &entry.outcome else {
        panic!("a scripted round must answer: {entry:?}");
    };
    assert_eq!(answer.rounds.len(), 2, "{answer:?}");
    let calls = &answer.rounds[0].calls;
    assert_eq!(
        calls
            .iter()
            .map(|call| call.verb.as_str())
            .collect::<Vec<_>>(),
        ["mark", "noise_add", "sync"]
    );
    for call in calls {
        assert!(
            matches!(
                call.outcome,
                suzatary_bench::run::CallOutcome::Answered { .. }
            ),
            "{} was refused, so it wrote nowhere at all: {:?}",
            call.verb,
            call.outcome
        );
    }

    // A hook from the box's `$XDG_CONFIG_HOME` would run as the box: the case's config names
    // its own, empty, hook directory.
    let case_scratch = out.join(".chains").join("triage:deadbeefcafefeed");
    let config = case_scratch.join("notmuch-config").display().to_string();
    let hooks = case_scratch.join("hooks").display().to_string();
    let log = std::fs::read_to_string(dir.join("bin").join("notmuch.sh.log"))
        .expect("the fake notmuch was called");
    let calls: Vec<Vec<&str>> = log.lines().map(|line| line.split('\t').collect()).collect();
    assert!(calls.iter().any(|call| call[3] == "new"), "{log}");
    for call in &calls {
        assert_eq!(
            (call[0], call[1]),
            (config.as_str(), hooks.as_str()),
            "a notmuch call named another config or hook directory: {call:?}"
        );
    }
    assert!(!out.join(".chains").exists());
}

/// One case in each stratum: acted, overturned and quiet.
fn write_stratified_case_file(dir: &std::path::Path) -> PathBuf {
    let text = r#"{
        "extracted_at": "2026-09-20T00:00:00Z",
        "cases": [
            {
                "kind": "triage",
                "id": "triage:acted",
                "message_hash": "aaaaaaaaaaaaaaaa",
                "chain_as_stood": { "chain": "life", "tip": null, "basis": "extraction" },
                "truths": {
                    "incumbent": {
                        "decided": "wrote",
                        "chain": "life",
                        "todo": "t1",
                        "object": "0000000000000000000000000000000000000000000000000000000000000001"
                    },
                    "settled": null
                }
            },
            {
                "kind": "triage",
                "id": "triage:overturned",
                "message_hash": "bbbbbbbbbbbbbbbb",
                "chain_as_stood": { "chain": "life", "tip": null, "basis": "extraction" },
                "truths": {
                    "incumbent": { "decided": "declined" },
                    "settled": {
                        "decided": "wrote",
                        "chain": "life",
                        "todo": "t2",
                        "object": "0000000000000000000000000000000000000000000000000000000000000002"
                    }
                }
            },
            {
                "kind": "triage",
                "id": "triage:quiet",
                "message_hash": "cccccccccccccccc",
                "chain_as_stood": { "chain": "life", "tip": null, "basis": "extraction" },
                "truths": { "incumbent": { "decided": "declined" }, "settled": null }
            }
        ],
        "mail": {}
    }"#;
    let path = dir.join("stratified-cases.json");
    std::fs::write(&path, text).expect("a writable case file");
    path
}

/// A key naming `A` the incumbent and `B` a candidate on every case.
fn write_stratified_key(dir: &std::path::Path) -> PathBuf {
    let text = r#"{
        "entries": {
            "triage:acted": { "A": "incumbent", "B": "candidate/one" },
            "triage:overturned": { "A": "incumbent", "B": "candidate/one" },
            "triage:quiet": { "A": "incumbent", "B": "candidate/one" }
        }
    }"#;
    let path = dir.join("key.json");
    std::fs::write(&path, text).expect("a writable key file");
    path
}

/// A scores file judging both labels on every case.
fn write_stratified_scores(dir: &std::path::Path) -> PathBuf {
    let score = r#"{ "right_todo": true, "wording": 4, "priority_defensible": true, "would_embarrass": false }"#;
    let text = format!(
        r#"{{
            "triage:acted": {{ "A": {score}, "B": {score} }},
            "triage:overturned": {{ "A": {score}, "B": {score} }},
            "triage:quiet": {{ "A": {score}, "B": {score} }}
        }}"#
    );
    let path = dir.join("scores.json");
    std::fs::write(&path, text).expect("a writable scores file");
    path
}

#[test]
fn join_stratifies_every_model_across_all_three_strata() {
    let dir = scratch("join-strata");
    let cases_path = write_stratified_case_file(&dir);
    let key_path = write_stratified_key(&dir);
    let scores_path = write_stratified_scores(&dir);
    let out = dir.join("joined.json");

    suzatary_cli::commands::bench::join_cmd(&scores_path, &key_path, &cases_path, &out)
        .expect("a join over a stratified case file");

    let text = std::fs::read_to_string(&out).expect("the joined report");
    let value: serde_json::Value = serde_json::from_str(&text).expect("joined JSON");

    let by_model = value["by_model"]
        .as_object()
        .expect("by_model is an object");
    assert_eq!(
        by_model.len(),
        2,
        "both the incumbent and the candidate must appear: {value}"
    );

    let by_stratum = value["by_stratum"]
        .as_array()
        .expect("by_stratum is an array");
    assert_eq!(
        by_stratum.len(),
        6,
        "two models across three non-empty strata each: {value}"
    );
    for model in ["incumbent", "candidate/one"] {
        for stratum in ["acted", "overturned", "quiet"] {
            let row = by_stratum
                .iter()
                .find(|row| row["model"] == model && row["stratum"] == stratum)
                .unwrap_or_else(|| panic!("no {stratum} row for {model} in {value}"));
            assert_eq!(
                row["summary"]["right_todo"]["applied"],
                serde_json::json!(1),
                "{model}/{stratum} must carry its one case: {row}"
            );
            // The judged bar is the incumbent's rate: the candidate earns a verdict, the
            // incumbent none.
            assert_eq!(
                row["right_todo"].is_null(),
                model == "incumbent",
                "{model}/{stratum}: {row}"
            );
        }
    }
}
