//! The user repository's template (`templates/user`, `nix flake init -t …#user`): copied into a
//! scratch repository as `nix flake init` copies it, `suzatary init` run in it, and every file
//! the box would then leave in its tree either one the template ships, one the agent's push
//! stages and its own pattern accepts, or one the template's `.gitignore` keeps out.

use super::{
    Scratch, agent_add_paths, agent_pattern, matches_allowed_pattern, repo, safe_push_script,
};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use suzatary_cli::commands::{doctor, init};
use suzatary_cli::instance::Instance;

#[derive(Debug, Parser)]
struct Wrapper {
    #[command(flatten)]
    args: init::Args,
}

fn template() -> PathBuf {
    repo().join("templates").join("user")
}

/// Every file under `dir`, relative to it.
fn files(dir: &Path, under: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("reads").flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, under, out);
        } else {
            out.push(path.strip_prefix(under).expect("under").to_path_buf());
        }
    }
}

/// One template file placed as `nix flake init` places it: fresh and writable, whatever mode
/// the tree it came from has (a store path's files are read-only), executable where it was.
fn place(from: &Path, to: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::create_dir_all(to.parent().expect("a parent")).expect("dirs");
    if std::fs::metadata(from).expect("stat").permissions().mode() & 0o111 == 0 {
        std::fs::write(to, std::fs::read(from).expect("the template file reads")).expect("placed");
        std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    } else {
        suzatary_testkit::copy_program(from, to);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The data pattern the push adds once the data is a repository of its own, as this one is.
fn data_pattern() -> String {
    let script = safe_push_script();
    super::assignments(&script, "DATA_EXTRA_PATTERN")
        .into_iter()
        .find(|pattern| pattern != "^$")
        .expect("DATA_EXTRA_PATTERN names the data repository's own files")
}

#[test]
fn the_template_begins_a_deployment_whose_every_file_is_shipped_staged_or_ignored() {
    let user = Scratch::new("user-template");
    let mut shipped = Vec::new();
    files(&template(), &template(), &mut shipped);
    for file in &shipped {
        place(&template().join(file), &user.join(file));
    }
    git(&user, &["init", "-q"]);

    let path = user.join("instance.toml");
    let args = Wrapper::parse_from([
        "init",
        "--name",
        "Ada",
        "--zone",
        "Europe/London",
        "--box",
        "ada-box.tailnet.ts.net",
        "--login",
        "her@example.test",
        "--email",
        "her@example.test",
        "--mail-provider",
        "proton",
    ])
    .args;
    init::run_keyed(&path, args, &env_beside(&path), |_| {
        use suzatary_core::cast::keys;
        Ok(suzatary_core::signatures::BoxKeys::of(
            &keys::root(),
            &suzatary_core::capability::Secret::new(keys::ROOT),
            &suzatary_core::capability::Secret::new(keys::AGENT),
        )?)
    })
    .expect("init writes the deployment");
    let instance = Instance::read(&path, &env_beside(&path)).expect("what init wrote reads back");
    doctor::run(&path, &doctor::Args::default()).expect("doctor reads it");

    // What the box writes beside the chains once it runs, each where the instance puts it: a
    // lock beside every store, the memory's words, the cache tier, mail and the heartbeat.
    let name = "a".repeat(64);
    let secretary = &instance.secretary;
    let written: Vec<PathBuf> = [
        secretary.inbox.join("submit.lock"),
        secretary.line_store().join("lines.lock"),
        secretary.line_keys().join(&name),
        secretary.memory.join("memory.lock"),
        secretary.memory_texts().join(&name),
        secretary.digests.join("digests.lock"),
        secretary.digest_texts().join(&name),
        secretary.topics.memberships.join("topics.lock"),
        secretary.topics.aliases.join("topic-aliases.lock"),
        secretary.conversations.join("upload.pdf"),
        secretary.renders.join(format!("{name}.html")),
        instance.decisions().join("decisions.lock"),
        instance.plugins.cache.join("plugins/ics/store/store.lock"),
        instance.plugins.cache.join("plugins/ics/cursor"),
        instance.mail.root.join("her-at-example-test/Inbox/cur/1"),
        instance.mail.heartbeat.clone(),
        instance.mail.state.clone(),
    ]
    .into_iter()
    .chain(
        instance
            .chains
            .iter()
            .map(|chain| chain.events.join(".lock")),
    )
    .collect();
    for file in &written {
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("dirs");
        std::fs::write(file, "").expect("written");
    }

    let staged = agent_add_paths();
    let (pattern, data) = (agent_pattern(), data_pattern());
    let listed = git(&user, &["status", "--porcelain", "--untracked-files=all"]);
    let mut stray = Vec::new();
    for line in listed.lines() {
        let file = line.get(3..).expect("a porcelain line");
        let ships = shipped.iter().any(|shipped| shipped == Path::new(file));
        let pushed = staged.iter().any(|path| file.starts_with(path.as_str()))
            && matches_allowed_pattern(&pattern, file);
        if !(ships || pushed || matches_allowed_pattern(&data, file)) {
            stray.push(file.to_owned());
        }
    }
    assert!(
        stray.is_empty(),
        "files neither shipped, pushed nor ignored, which would sit untracked forever: {stray:?}\n{listed}"
    );
    // And the push carries what it must: both chains' objects and their own attributes.
    for chain in ["events", "events-suzatary"] {
        assert!(
            listed.contains(&format!("{chain}/.gitattributes")),
            "{chain}'s attributes are not in the tree:\n{listed}"
        );
    }
    assert!(listed.contains("state.json"), "{listed}");
    assert!(listed.contains("instance.toml"), "{listed}");
}

/// The template's `./suzatary` finds the app from `flake.lock` alone, read against a store that
/// refuses every write, so running the app never copies the repository, data and all, into the
/// world-readable Nix store; and its flake has no output that would.
#[test]
fn the_template_runs_the_app_its_lock_names_without_copying_itself_anywhere() {
    let user = Scratch::new("user-template-script");
    let mut shipped = Vec::new();
    files(&template(), &template(), &mut shipped);
    for file in &shipped {
        place(&template().join(file), &user.join(file));
    }
    let rev = "0123456789abcdef0123456789abcdef01234567";
    std::fs::write(
        user.join("flake.lock"),
        format!(
            r#"{{"nodes":{{"root":{{"inputs":{{"suzatary":"suzatary"}}}},"suzatary":{{"locked":{{"narHash":"sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","ref":"master","rev":"{rev}","type":"git","url":"https://example.invalid/app"}},"original":{{"ref":"master","type":"git","url":"https://example.invalid/app"}}}}}},"root":"root","version":7}}"#
        ),
    )
    .expect("a lock");
    std::fs::write(user.join("instance.toml"), "a data file nothing may copy\n").expect("data");

    let script = user.join("suzatary");
    let out = Command::new(&script)
        .arg("which")
        .current_dir("/")
        .output()
        .expect("the script runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        format!("git+https://example.invalid/app?ref=master&rev={rev}")
    );

    let text = std::fs::read_to_string(&script).expect("reads");
    assert!(
        text.contains("--store dummy://"),
        "the lock is read against a store that writes"
    );
    let flake = std::fs::read_to_string(user.join("flake.nix")).expect("reads");
    assert!(
        flake.contains("outputs = _: { };"),
        "the user flake grew an output, and evaluating it copies the repository"
    );
}

/// An environment beside the instance file at `path`, so a test reads none of this process's.
fn env_beside(path: &std::path::Path) -> suzatary_cli::paths::Env {
    suzatary_cli::paths::Env::under(path.with_file_name("home"), path.with_file_name("run"))
}
