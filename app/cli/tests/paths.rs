//! Every path this box derives outside the instance file, for one
//! representative instance under two environments, printed and compared with
//! `tests/paths.golden`: the live box's files must not move.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use suzatary_cli::instance::Instance;

const CHILD: &str = "SUZATARY_PATHS_CHILD";

const INSTANCE: &str = r#"
repo = "/srv/suzatary/code"
data = "/srv/suzatary/data"
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
notmuch = "notmuch"
mbsyncrc = "mbsyncrc"
heartbeat = "triage-heartbeat"
[[mail.accounts]]
email = "her@example.test"
subdir = "her"
sync = "ms-imap"
authority = "organizations"
inbox_folders = ["Inbox"]
sent_folder = "Sent Items"
"#;

/// Each path, one `name = path` line each.
fn resolved(instance: &Instance) -> Vec<(&'static str, &Path)> {
    let paths = &instance.paths;
    vec![
        ("secretspec", &paths.secretspec),
        ("oauth", &paths.oauth),
        ("imap_state", &paths.imap_state),
        ("devices", &paths.devices),
        ("markup_cache", &paths.markup_cache),
        ("repos", &paths.repos),
        ("usage_cache", &paths.usage_cache),
        ("last_sync", &paths.last_sync),
        ("transcripts", &paths.transcripts),
        ("home", &paths.home),
        ("drive_cli", &paths.drive_cli),
        ("drive_scratch", &paths.drive_scratch),
        ("bridge_cert", &paths.bridge_cert),
        ("whois_socket", &paths.whois_socket),
    ]
}

fn print_paths(instance_file: &Path) -> String {
    let instance = Instance::open(instance_file).expect("the representative instance reads");
    resolved(&instance)
        .into_iter()
        .fold(String::new(), |mut printed, (name, path)| {
            let _ = writeln!(printed, "{name} = {}", path.display());
            printed
        })
}

/// Run this test again in a child with exactly `env`, and read what it printed.
fn printed_under(instance_file: &Path, env: &[(&str, &str)]) -> String {
    let out = child(instance_file, env);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let begin = stdout
        .find("<<paths\n")
        .expect("the child printed its paths")
        + 8;
    let end = stdout[begin..].find("paths>>").expect("the child finished") + begin;
    stdout[begin..end].to_owned()
}

/// This test again in a child with exactly `env`.
fn child(instance_file: &Path, env: &[(&str, &str)]) -> std::process::Output {
    // The test's name inside its binary: this module's path below the crate root, then the
    // function, so the child finds it whether this file is a crate or a module of one.
    let test = module_path!().split_once("::").map_or_else(
        || "every_path_resolves_as_the_golden_file_says".to_owned(),
        |(_, module)| format!("{module}::every_path_resolves_as_the_golden_file_says"),
    );
    Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", &test, "--nocapture"])
        .env_clear()
        .env(CHILD, instance_file)
        .envs(env.iter().copied())
        .output()
        .expect("the test binary runs")
}

#[test]
fn every_path_resolves_as_the_golden_file_says() {
    if let Some(instance_file) = std::env::var_os(CHILD) {
        print!("<<paths\n{}paths>>", print_paths(Path::new(&instance_file)));
        return;
    }
    let dir = std::env::temp_dir().join(format!("suzatary-paths-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let instance_file = dir.join("instance.toml");
    std::fs::write(&instance_file, INSTANCE).expect("writes");
    let bare = printed_under(
        &instance_file,
        &[
            ("HOME", "/home/ada"),
            ("TMPDIR", "/scratch"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ],
    );
    // No fallback to `/` or to a directory every user shares: each one missing is refused.
    for (missing, env) in [
        (
            "XDG_RUNTIME_DIR",
            [("HOME", "/home/ada"), ("TMPDIR", "/scratch")],
        ),
        (
            "HOME",
            [
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("TMPDIR", "/scratch"),
            ],
        ),
    ] {
        let out = child(&instance_file, &env);
        assert!(!out.status.success(), "{missing} unset is refused");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(&format!("{missing} is unset")),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let xdg = printed_under(
        &instance_file,
        &[
            ("HOME", "/home/ada"),
            ("TMPDIR", "/scratch"),
            ("XDG_STATE_HOME", "/xdg/state"),
            ("XDG_CACHE_HOME", "/xdg/cache"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ],
    );
    let _ = std::fs::remove_dir_all(&dir);
    let printed = format!("# HOME and the runtime directory\n{bare}\n# with XDG\n{xdg}");
    assert_eq!(printed, include_str!("paths.golden"));
}
