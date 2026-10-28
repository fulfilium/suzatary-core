//! What suzatary-core's runners build from a publication or a gate commit, pinned from the
//! workflow's source: the deploy's builds pushed to the binary cache (`publication.yml`), run only
//! as master's copy of the workflow, on a push to master or a dispatch naming the commit, with
//! nothing of the tree run but Nix's evaluation and its sandboxed builds, a tripwire over the tree
//! (`scripts/public-source.sh`), measured against the cache's budget before they are pushed, and
//! nothing built from the deployment's own names. It is the one workflow, and no script is
//! another, that writes to the cache.

use super::{Scratch, repo};
use std::path::Path;
use std::process::Command;

fn workflow(name: &str) -> String {
    std::fs::read_to_string(repo().join(".github/workflows").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The text from `start` up to the next non-blank line indented no deeper than it: one YAML
/// key's block, comments before the next key excluded.
fn section<'a>(text: &'a str, start: &str) -> &'a str {
    let from = text.find(start).unwrap_or_else(|| panic!("no {start:?}"));
    let indent = start.len() - start.trim_start().len();
    let rest = &text[from + start.len()..];
    let to = rest
        .lines()
        .scan(0, |at, line| {
            let here = *at;
            *at += line.len() + 1;
            Some((here, line))
        })
        .skip(1)
        .find(|(_, line)| {
            let trimmed = line.trim_start();
            !trimmed.is_empty() && line.len() - trimmed.len() <= indent
        })
        .map_or(rest.len(), |(at, _)| at);
    &text[from..from + start.len() + to]
}

/// The publication cache runs on a push to master and on a dispatch, in the public repository
/// alone, only on master's ref and only when the owner started it, never on a pull request or a
/// gate branch, and every run is named by the commit it builds. Its two jobs keep the token from
/// the tree: `build` enters no environment and names no secret, and `push`, which enters the
/// environment `cachix`, needs `build` and hands the token to its last step alone.
#[test]
fn publication_cache_holds_its_token_on_master_alone() {
    let text = workflow("publication.yml");
    assert_eq!(
        section(&text, "\non:").trim(),
        "on:\n  push:\n    branches: [master]\n  workflow_dispatch:\n    inputs:\n      commit:\n        \
         description: The commit of suzatary-core to build, a full commit id\n        \
         required: true\n        type: string",
        "a push to master or a dispatch, and nothing else"
    );
    assert!(text.contains("\nrun-name: Publication cache ${{ inputs.commit || github.sha }}\n"));
    let build = section(&text, "\n  build:");
    let push = section(&text, "\n  push:\n    needs: build");
    let gate = "    if: ${{ !github.event.repository.private && github.ref == 'refs/heads/master' \
                && github.triggering_actor == 'bmabsout' }}\n";
    for (name, job) in [("build", build), ("push", push)] {
        assert!(
            job.contains(gate),
            "{name} runs on master, started by the owner: {job}"
        );
        assert!(
            job.contains("    permissions:\n      contents: read\n"),
            "{name}'s token reads only"
        );
    }
    assert!(
        !build.contains("environment:") && !build.contains("secrets."),
        "the build job holds no secret"
    );
    assert!(push.contains("    needs: build\n") && push.contains("    environment: cachix\n"));
    assert_eq!(text.matches("environment:").count(), 1);
    assert_eq!(text.matches("secrets.").count(), 1, "one secret, once");
    let last = push.rfind("      - name:").expect("steps");
    assert!(
        push[last..].contains("CACHIX_AUTH_TOKEN: ${{ secrets.CACHIX_AUTH_TOKEN }}"),
        "the token is the last step's alone"
    );
    // The APK bakes in the deployment's box name and Firebase project, read from the
    // environment under --impure alone: a public cache holds nothing built that way.
    let code: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect();
    for never in ["--impure", "PORTAL", "apk", "vars."] {
        assert!(!code.contains(never), "builds nothing with {never}");
    }
    assert!(!code.contains("gate/"), "no gate branch runs it");
    let settings =
        std::fs::read_to_string(repo().join("scripts/core-settings.sh")).expect("core-settings.sh");
    assert!(
        !settings.contains("gate/"),
        "the environment admits master alone"
    );
}

/// The build job reads the input only through the environment and refuses it unless it is a full
/// commit id; the tree it names is checked out apart, at `tree`, and nothing of it runs but Nix's
/// evaluation and its sandboxed builds: master's own check runs over it, every build is of
/// `./tree` without its flake configuration or a lock file written, and nothing runs `nix run`,
/// `nix develop` or `nix shell`. The gate's store is restored, never saved. The push job checks
/// out master alone and reads nothing the build job made until master's cachix is built.
#[test]
fn publication_cache_runs_only_masters_code_over_the_tree() {
    let text = workflow("publication.yml");
    let build = section(&text, "\n  build:");
    let push = section(&text, "\n  push:\n    needs: build");
    let inputs: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("inputs."))
        .map(str::trim)
        .collect();
    assert_eq!(
        inputs,
        [
            "run-name: Publication cache ${{ inputs.commit || github.sha }}",
            "COMMIT: ${{ inputs.commit || github.sha }}",
        ],
        "the input is the run-name's and an environment variable's alone"
    );
    let commit = section(build, "      - name: The commit");
    assert!(commit.contains(r#"[[ "$COMMIT" =~ ^[0-9a-f]{40}$ ]] ||"#));
    assert_eq!(build.matches("persist-credentials: false").count(), 2);
    assert_eq!(push.matches("persist-credentials: false").count(), 1);
    assert_eq!(push.matches("actions/checkout@").count(), 1, "master alone");
    assert!(!push.contains("ref:") && !push.contains("path: tree"));
    let tree = section(build, "      - name: Checkout the tree to build");
    assert!(
        tree.contains("ref: ${{ steps.commit.outputs.sha }}\n") && tree.contains("path: tree\n")
    );
    let at = |job: &str, needle: &str| {
        job.find(needle)
            .unwrap_or_else(|| panic!("the job has {needle:?}"))
    };
    let check = section(build, "      - name: The tree holds no private history");
    assert!(
        check.contains("run: scripts/public-source.sh tree\n")
            && !check.contains("continue-on-error"),
        "master's tripwire over the tree, a step that fails the job: {check}"
    );
    assert!(at(build, "cache-nix-action/restore@") < at(build, check));
    assert!(at(build, check) < at(build, "      - name: Build what a deploy builds"));
    assert!(!text.contains("cache-nix-action/save@"), "never saved");
    let builds: Vec<&str> = build.lines().filter(|l| l.contains("nix build")).collect();
    assert_eq!(builds.len(), 3, "{builds:?}");
    for (line, attr) in builds
        .iter()
        .zip(["suzatary", "suzatary-web", "suzatary-plugins"])
    {
        assert!(
            line.trim_start()
                .starts_with("nix build --option accept-flake-config false --no-update-lock-file ")
                && line.trim_end().ends_with(&format!(" ./tree#{attr}")),
            "a build of ./tree#{attr} without its flake configuration or a lock written: {line}"
        );
    }
    let code: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    for never in ["nix run", "nix develop", "nix shell"] {
        assert!(
            !code.iter().any(|l| l.contains(never)),
            "nothing runs {never}"
        );
    }
    assert!(
        at(
            push,
            "run: nix build --inputs-from . --out-link cachix nixpkgs#cachix\n"
        ) < at(push, "actions/download-artifact@"),
        "master's cachix is built before anything of the build job is read"
    );
    let import = section(push, "      - name: Import the paths");
    assert!(
        import.find("nix-store --realise") < import.find("nix-store --import"),
        "the served references come from the caches first"
    );
}

/// `scripts/public-source.sh` passes a checkout none of whose files `.gitattributes` marks
/// `export-ignore`, and refuses one holding such a file, naming it, whether the mark is on the
/// file or on a directory above it.
#[test]
fn public_source_refuses_a_checkout_holding_the_private_history() {
    let scratch = Scratch::new("public-source");
    let checkout = |name: &str, files: &[(&str, &str)]| {
        let dir = scratch.join(name);
        for (rel, body) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(path, body).expect("a file");
        }
        git(&dir, &["init", "-q"]);
        git(&dir, &["add", "-A"]);
        dir
    };
    let check = |dir: &Path| {
        let out = Command::new(repo().join("scripts/public-source.sh"))
            .arg(dir)
            .output()
            .expect("public-source.sh runs");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let public = checkout(
        "public",
        &[("flake.nix", "{}\n"), ("CHANGELOG.md", "# Changelog\n")],
    );
    assert_eq!(check(&public).0, Some(0), "{}", check(&public).1);
    for (name, mark) in [
        ("file", "/docs/history.md export-ignore\n"),
        ("directory", "/docs/** export-ignore\n"),
    ] {
        let private = checkout(
            name,
            &[
                ("flake.nix", "{}\n"),
                ("docs/history.md", "private\n"),
                (".gitattributes", mark),
            ],
        );
        let (code, said) = check(&private);
        assert_eq!(code, Some(1), "{name}: {said}");
        assert!(said.contains("docs/history.md"), "{name}: {said}");
        assert!(!said.contains("flake.nix"), "{name}: {said}");
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// What a push adds is measured in the build job and again in the push job, over what was
/// imported, and over 1 GB fails each before the push, so the push never runs; the push sends the
/// measured list with master's cachix.
#[test]
fn publication_cache_measures_its_push_and_refuses_over_a_gigabyte() {
    let text = workflow("publication.yml");
    let build = section(&text, "\n  build:");
    let push = section(&text, "\n  push:\n    needs: build");
    let first = section(build, "      - name: Measure what the push adds");
    let again = section(push, "      - name: Measure the push again");
    let send = section(push, "      - name: Push the builds to the cache");
    assert!(
        push.find(again) < push.find(send),
        "measured before it is pushed"
    );
    for measure in [first, again] {
        assert!(measure.contains("LIMIT: 1000000000\n"), "a limit of 1 GB");
        assert!(
            measure.contains("if ((bytes > LIMIT)); then") && measure.contains("exit 1\n"),
            "fails over it"
        );
        assert!(measure.contains("echo \"the push adds "), "prints the size");
        assert!(!measure.contains("continue-on-error"));
    }
    assert!(first.contains(">unserved.txt"));
    assert!(again.contains("<publication/unserved.txt"));
    assert!(!send.contains("if:"));
    assert!(
        send.contains("./cachix-bin/bin/cachix push bmabsout <publication/unserved.txt\n"),
        "master's cachix pushes what was measured"
    );
}

/// `scripts/core-settings.sh` sets one ruleset under which only a repository administrator moves
/// master: an `update` rule on `refs/heads/master`, active, whose one bypass is the administrator
/// role. So no workflow, whatever it asks for, can move master to run code of its own in the
/// publication run.
#[test]
fn core_settings_lets_only_an_administrator_move_master() {
    let settings =
        std::fs::read_to_string(repo().join("scripts/core-settings.sh")).expect("core-settings.sh");
    let from = settings
        .find("ruleset=\"$(cat <<JSON\n")
        .expect("the ruleset");
    let body = &settings[from..];
    let body = &body[..body.find("\nJSON\n").expect("its end")];
    for part in [
        r#""target": "branch", "enforcement": "active""#,
        r#""conditions": {"ref_name": {"include": ["refs/heads/master"], "exclude": []}}"#,
        r#""rules": [{"type": "update"}]"#,
        r#""bypass_actors": [{"actor_type": "RepositoryRole", "actor_id": 5, "bypass_mode": "always"}]"#,
    ] {
        assert!(body.contains(part), "the ruleset holds {part}: {body}");
    }
    assert_eq!(body.matches("actor_type").count(), 1, "one bypass");
    assert!(
        settings.contains(
            r#"gh api --silent -X PUT "repos/$repo/rulesets/$id" --input - <<<"$ruleset""#
        )
    );
    assert!(
        settings
            .contains(r#"gh api --silent -X POST "repos/$repo/rulesets" --input - <<<"$ruleset""#)
    );
}

/// No workflow but publication.yml names the Cachix's token or pushes to a binary cache, and no
/// script or Nix file of the box does either: the token's one other reader,
/// `scripts/core-settings.sh`, hands it to GitHub and pushes nothing. So the box, which holds
/// the private tree and the owner's data, has no path to the public cache.
#[test]
fn publication_yml_alone_writes_to_the_cachix() {
    let pushes = [
        "cachix push",
        "cachix -- push",
        "cachix-action",
        "authToken",
        "environment: cachix",
    ];
    let mut writers = Vec::new();
    let mut namers = Vec::new();
    for dir in [".github/workflows", "scripts", "scripts/hooks", "nix"] {
        for entry in std::fs::read_dir(repo().join(dir)).expect(dir) {
            let path = entry.expect("a file").path();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(repo())
                .expect("in the repository")
                .to_string_lossy()
                .into_owned();
            if pushes.iter().any(|word| text.contains(word)) {
                writers.push(rel.clone());
            }
            if text.contains("CACHIX_AUTH_TOKEN") {
                namers.push(rel);
            }
        }
    }
    let flake = std::fs::read_to_string(repo().join("flake.nix")).expect("flake.nix");
    assert!(
        !pushes.iter().any(|word| flake.contains(word)) && !flake.contains("CACHIX_AUTH_TOKEN")
    );
    namers.sort();
    assert_eq!(writers, [".github/workflows/publication.yml"]);
    assert_eq!(
        namers,
        [
            ".github/workflows/publication.yml",
            "scripts/core-settings.sh"
        ]
    );
}
