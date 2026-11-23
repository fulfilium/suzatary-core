//! A whole triage pass (`scripts/hourly-triage.sh`) against a stand-in `claude` that keeps what
//! it was given and prints a stream chosen by the test, with the real fence, the real `suzatary
//! ran` and stubs for every other program. The pass hands the CLI a server it starts from the
//! binary the pass resolved, marked always loaded, and says which failure it met: verbs never
//! offered (the tooling's) or verbs offered and none called (the model's).

use super::{Scratch, repo};
use std::process::{Command, Output, Stdio};

/// What the stand-in prints: its `init` event offers `offered`, and its one turn calls `calls`.
fn stream(offered: &[&str], calls: &[&str]) -> String {
    let mut lines = vec![serde_json::json!({
        "type": "system", "subtype": "init", "tools": offered,
        "mcp_servers": [{"name": "suzatary", "status": "connected"}],
    })];
    lines.extend(calls.iter().map(|name| {
        serde_json::json!({
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "id": "t1", "name": name, "input": {}}]},
        })
    }));
    lines.push(serde_json::json!({
        "type": "result", "subtype": "success", "is_error": false, "num_turns": 1,
        "duration_ms": 2400, "result": "Done.",
        "session_id": "0f0e0d0c-0b0a-4009-8008-070605040302",
    }));
    lines.iter().map(|line| line.to_string() + "\n").collect()
}

/// This process's PATH without any directory that holds a mail program, as the unit's PATH is
/// on a system the deploy did not rebuild.
fn unit_path() -> Vec<std::path::PathBuf> {
    let host = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&host)
        .filter(|dir| {
            ["notmuch", "mbsync", "gmi"]
                .iter()
                .all(|tool| !dir.join(tool).exists())
        })
        .collect()
}

/// One pass in a scratch repository; the stand-in keeps its arguments under `<root>/capture`.
struct Pass {
    root: Scratch,
    out: Output,
}

impl Pass {
    fn run(case: &str, said: &str) -> Pass {
        let root = Scratch::new(&format!("triage-pass-{case}"));
        for dir in [
            "scripts",
            ".claude",
            "bin",
            "capture",
            "store/bin",
            "store/libexec/suzatary/bin",
            "state",
            "home",
        ] {
            std::fs::create_dir_all(root.join(dir)).expect("a directory");
        }
        for script in [
            "hourly-triage.sh",
            "claude-auth.sh",
            "triage-fence.sh",
            "tool-guard.sh",
            "edit-guard.sh",
        ] {
            suzatary_testkit::copy_program(
                &repo().join("scripts").join(script),
                &root.join("scripts").join(script),
            );
        }
        for file in ["triage-settings.json", "mcp-suzatary.json"] {
            std::fs::copy(
                repo().join(".claude").join(file),
                root.join(".claude").join(file),
            )
            .expect("copies");
        }
        std::fs::write(root.join("instance.toml"), "").expect("an instance file");
        std::fs::write(root.join("stream.jsonl"), said).expect("the stream");
        let bin = root.join("bin");
        let _ = suzatary_testkit::fake_program(
            &bin,
            "nix",
            "while [ \"$1\" != --command ]; do shift; done\nshift\nexec \"$@\"\n",
        );
        let _ = suzatary_testkit::fake_program(&bin, "secretspec", "echo held-value\n");
        let _ = suzatary_testkit::fake_program(
            &bin,
            "claude",
            "while [ $# -gt 0 ]; do\n  case \"$1\" in\n    \
             --mcp-config) printf '%s' \"$2\" > \"$CAPTURE/mcp.json\"; shift 2 ;;\n    \
             --settings) printf '%s' \"$2\" > \"$CAPTURE/settings.json\"; shift 2 ;;\n    \
             -p) shift 2 ;;\n    \
             *) printf '%s\\n' \"$1\" >> \"$CAPTURE/argv\"; shift ;;\n  esac\ndone\n\
             server=$(sed -n 's/.*\"command\": *\"\\([^\"]*\\)\".*/\\1/p' \"$CAPTURE/mcp.json\")\n\
             \"$server\" mcp\n\
             cat \"$STREAM\"\n",
        );
        let _ = suzatary_testkit::fake_program(
            &root.join("scripts"),
            "app-bin.sh",
            "case \"${1:-}\" in\n  \
             '') printf '%s\\n' \"$STORE\" ;;\n  \
             ran) exec \"$SUZATARY_BUILT\" \"$@\" ;;\n  \
             gate) echo '1 new message' ;;\n  \
             heartbeat) shift; printf '%s\\n' \"$*\" >> \"$CAPTURE/heartbeat\" ;;\n  \
             *) echo '{}' ;;\nesac\n",
        );
        let _ = suzatary_testkit::fake_program(&root.join("scripts"), "safe-push.sh", "exit 0\n");
        // The package's server reads mail with the package's notmuch.
        let _ = suzatary_testkit::fake_program(
            &root.join("store/bin"),
            "suzatary",
            "notmuch count > \"$CAPTURE/mail\"\n",
        );
        let _ = suzatary_testkit::fake_program(
            &root.join("store/libexec/suzatary/bin"),
            "notmuch",
            "echo read\n",
        );
        let out = Command::new("bash")
            .arg(root.join("scripts/hourly-triage.sh"))
            .env_clear()
            .env(
                "PATH",
                std::env::join_paths(std::iter::once(bin.clone()).chain(unit_path()))
                    .expect("a PATH"),
            )
            .env("HOME", root.join("home"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("SUZATARY_REPO", &*root)
            .env("SUZATARY_INSTANCE", root.join("instance.toml"))
            .env("SUZATARY_BUILT", crate::binary::path())
            .env("STORE", root.join("store"))
            .env("CAPTURE", root.join("capture"))
            .env("STREAM", root.join("stream.jsonl"))
            .stdin(Stdio::null())
            .output()
            .expect("bash runs the pass");
        Pass { root, out }
    }

    fn said(&self) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&self.out.stdout),
            String::from_utf8_lossy(&self.out.stderr)
        )
    }

    fn captured(&self, name: &str) -> String {
        std::fs::read_to_string(self.root.join("capture").join(name))
            .unwrap_or_else(|e| panic!("the stand-in kept no {name}: {e}\n{}", self.said()))
    }
}

/// A run whose `init` event offers only `ToolSearch` fails the pass as the tooling's, and the CLI
/// was given the resolved binary's server, always loaded, with the package's mail programs and
/// inside the same fence.
#[test]
fn a_pass_whose_verbs_were_not_offered_says_so_and_its_server_is_the_resolved_binary() {
    let pass = Pass::run("not-offered", &stream(&["ToolSearch"], &[]));
    let said = pass.said();
    assert_eq!(pass.out.status.code(), Some(1), "{said}");
    assert!(said.contains("offered 1 tool(s)"), "{said}");
    assert!(
        said.contains("the suzatary verbs were not offered to claude"),
        "{said}"
    );
    assert!(!said.contains("called none"), "{said}");
    assert_eq!(pass.captured("heartbeat").trim(), "failed triage", "{said}");

    let mcp: serde_json::Value =
        serde_json::from_str(&pass.captured("mcp.json")).expect("the config is JSON");
    let servers = mcp["mcpServers"].as_object().expect("mcpServers");
    assert_eq!(servers.len(), 1, "one server: {mcp}");
    let server = &servers["suzatary"];
    assert_eq!(server["alwaysLoad"], true, "{mcp}");
    assert_eq!(
        server["command"],
        pass.root.join("store/bin/suzatary").display().to_string(),
        "the server is the binary the pass resolved, with no second shell or resolve: {mcp}"
    );
    assert_eq!(server["args"], serde_json::json!(["mcp"]), "{mcp}");
    assert_eq!(
        pass.captured("mail").trim(),
        "read",
        "the server reached the package's notmuch on a PATH that holds no mail program"
    );

    let argv = pass.captured("argv");
    let argv: Vec<&str> = argv.lines().collect();
    for flag in ["--strict-mcp-config", "--no-session-persistence"] {
        assert!(argv.contains(&flag), "{flag}: {argv:?}");
    }
    for (flag, value) in [("--tools", "ToolSearch"), ("--permission-mode", "dontAsk")] {
        let at = argv.iter().position(|arg| *arg == flag).expect(flag);
        assert_eq!(argv.get(at + 1), Some(&value), "{argv:?}");
    }

    // The fence script prints settings only after its hook denied a shell and allowed a verb.
    let fence = Command::new(pass.root.join("scripts/triage-fence.sh"))
        .arg(&*pass.root)
        .output()
        .expect("the fence runs");
    assert!(fence.status.success());
    assert_eq!(
        pass.captured("settings.json").trim(),
        String::from_utf8_lossy(&fence.stdout).trim()
    );
}

/// Offered the verbs and called none is the model's failure; offered and called is a clean pass.
#[test]
fn a_pass_tells_a_model_that_called_no_verb_from_one_that_did() {
    let offered = ["ToolSearch", "mcp__suzatary__context"];
    let none = Pass::run("called-none", &stream(&offered, &[]));
    let said = none.said();
    assert_eq!(none.out.status.code(), Some(1), "{said}");
    assert!(
        said.contains("offered the suzatary verbs and called none"),
        "{said}"
    );
    assert!(!said.contains("not offered"), "{said}");

    let clean = Pass::run("called", &stream(&offered, &["mcp__suzatary__context"]));
    let said = clean.said();
    assert_eq!(clean.out.status.code(), Some(0), "{said}");
    assert_eq!(clean.captured("heartbeat").trim(), "ok", "{said}");
}
