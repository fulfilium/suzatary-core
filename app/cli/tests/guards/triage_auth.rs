//! The triage agent's credential (`scripts/claude-auth.sh`, sourced by the triage wrapper): the
//! secret `suzatary init`'s model steps set is the one the wrapper puts in `claude`'s
//! environment, so the agent is never left "Not logged in".

use super::{Scratch, repo};
use std::process::Command;

use suzatary_core::connect::Reach;

/// The variable `claude` reads the subscription's token from.
const TOKEN: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// A `secretspec` that holds `name` alone.
fn secretspec_holding(dir: &std::path::Path, name: &str) {
    let script = format!(
        "#!/usr/bin/env bash\n[ \"$1\" = get ] && [ \"$2\" = {name} ] && {{ echo held-value; exit 0; }}\nexit 1\n"
    );
    let _ = suzatary_testkit::fake_program(dir, "secretspec", &script);
}

#[test]
fn the_token_init_sets_is_the_credential_the_triage_agent_runs_under() {
    let auth = repo().join("scripts/claude-auth.sh");
    assert!(
        suzatary_cli::commands::init::model_steps(&Reach::Installed)
            .iter()
            .any(|step| step.command.contains(&format!("secretspec set {TOKEN} "))),
        "the model's steps do not set {TOKEN}"
    );
    let stub = Scratch::new(&format!("claude-auth-{TOKEN}"));
    secretspec_holding(&stub, TOKEN);
    let out = Command::new("bash")
        .arg("-c")
        .arg(format!(". {} && claude_auth && env", auth.display()))
        .env(
            "PATH",
            format!(
                "{}:{}",
                stub.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env_remove(TOKEN)
        .output()
        .expect("bash runs");
    assert!(out.status.success(), "no credential resolved");
    let env = String::from_utf8_lossy(&out.stdout);
    assert!(
        env.lines()
            .any(|line| line == format!("{TOKEN}=held-value")),
        "the environment holds no {TOKEN}"
    );

    // With no token set, the wrapper falls back to the interactive login and says so.
    let none = Scratch::new("claude-auth-none");
    secretspec_holding(&none, "NOTHING");
    let out = Command::new("bash")
        .arg("-c")
        .arg(format!(". {} && claude_auth", auth.display()))
        .env(
            "PATH",
            format!(
                "{}:{}",
                none.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("bash runs");
    assert!(!out.status.success());
}

/// The triage agent's `claude` is started by the wrapper, not the binary, so the wrapper
/// exports every switch [`suzatary_io::model::QUIET`] names, and copies no memory directory.
#[test]
fn the_triage_agent_runs_quiet_and_reads_no_main_session_memory() {
    let wrapper = std::fs::read_to_string(repo().join("scripts/hourly-triage.sh"))
        .expect("the wrapper reads");
    let exported: String = wrapper
        .lines()
        .skip_while(|line| !line.starts_with("export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"))
        .take_while(|line| !line.starts_with("claude "))
        .collect::<Vec<_>>()
        .join(" ");
    for (name, value) in suzatary_io::model::QUIET {
        assert!(
            exported.contains(&format!("{name}={value}")),
            "hourly-triage.sh does not export {name}={value}"
        );
    }
    assert!(
        !wrapper.contains("cp -a \"$MEMORY_SRC\""),
        "the wrapper copies the main session's memory"
    );
    let call: Vec<&str> = wrapper
        .lines()
        .skip_while(|line| !line.starts_with("claude -p"))
        .take_while(|line| line.trim_end().ends_with('\\'))
        .collect();
    assert!(
        call.iter()
            .any(|line| line.contains("--no-session-persistence")),
        "the triage agent's session, mail included, would be kept under ~/.claude/projects"
    );
}
