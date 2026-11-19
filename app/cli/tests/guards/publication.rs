//! What suzatary-core's runners build from a publication or a gate commit, pinned from the
//! workflow's source: the deploy's builds pushed to the binary cache (`publication.yml`), run only
//! as master's copy of the workflow, on a push to master or a dispatch naming the commit, with
//! nothing of the tree run but Nix's evaluation and its sandboxed builds, a tripwire over the tree
//! (`scripts/public-source.sh`), measured against the cache's budget before they are pushed, and
//! nothing built from the deployment's own names. It is the one workflow, and no script is
//! another, that writes to the cache.

use super::{Scratch, repo};
use std::collections::BTreeSet;
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
    // Every build is of the tree, with no flake configuration of its own and no lock written, and of
    // an output this list names: what a deploy builds, what the gate's live half reads, and the
    // development modules the gate's tests read.
    let allowed = [
        "suzatary",
        "suzatary-web",
        "suzatary-plugins",
        "suzatary-fixture",
        "suzatary-web-ts",
        "suzatary-vector-json",
        "suzatary-wasm",
        "suzatary-views-wasm",
        "suzatary-typst-wasm",
        "suzatary-wasm-dev",
        "suzatary-views-wasm-dev",
    ];
    let builds: Vec<&str> = build
        .lines()
        .filter(|l| l.contains("nix build --"))
        .collect();
    assert_eq!(builds.len(), 1, "one build: {builds:?}");
    assert!(
        builds[0].contains(
            "nix build --option accept-flake-config false --no-update-lock-file \
             --print-build-logs --no-link --json \"${outputs[@]/#*=/./tree#}\""
        ),
        "the build of the outputs listed, with no flake configuration of its own and no lock \
         written: {}",
        builds[0]
    );
    assert_eq!(
        published_outputs(&text)
            .into_iter()
            .collect::<BTreeSet<_>>(),
        allowed.iter().map(|a| (*a).to_owned()).collect(),
        "every allowed output is built, and nothing else"
    );
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

/// The flake outputs publication.yml's build job lists (`outputs=(<link>=<output> ...)`).
pub(super) fn published_outputs(text: &str) -> Vec<String> {
    let start = text.find("outputs=(").expect("the build lists its outputs") + "outputs=(".len();
    let end = start + text[start..].find(')').expect("the list ends");
    text[start..end]
        .split_whitespace()
        .filter_map(|o| o.split_once('=').map(|(_, output)| output.to_owned()))
        .collect()
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

/// What a push adds is measured by `scripts/cache-unserved.sh` in the build job and again in the
/// push job, over what was imported, both against 1 GB and the same two caches; either fails
/// before the push, so the push never runs. The push sends the second measure's list with
/// master's cachix.
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
    assert!(first.contains(
        "        env: &measure\n          LIMIT: 1000000000\n          \
         UPSTREAM: https://cache.nixos.org\n          CACHE: https://bmabsout.cachix.org\n"
    ));
    assert!(first.contains("\n          scripts/cache-unserved.sh closure closure.txt .\n"));
    assert!(again.contains("        env: *measure\n"));
    assert!(again.contains("run: scripts/cache-unserved.sh list publication/unserved.txt push\n"));
    for measure in [first, again] {
        assert!(!measure.contains("continue-on-error"));
    }
    assert!(!send.contains("if:"));
    assert!(
        send.contains("./cachix-bin/bin/cachix push bmabsout <push/unserved.txt\n"),
        "master's cachix pushes what was measured"
    );
}

/// Two stub caches (`file://` directories of narinfos) and a stub `nix path-info --size` over a
/// table of sizes, for `scripts/cache-unserved.sh`.
struct StubCaches {
    scratch: Scratch,
}

impl StubCaches {
    fn new(sizes: &[(&str, u64)]) -> Self {
        let scratch = Scratch::new("cache-unserved");
        for dir in ["upstream", "cache", "bin"] {
            std::fs::create_dir_all(scratch.join(dir)).expect("a directory");
        }
        std::fs::write(
            scratch.join("sizes"),
            lines(sizes.iter().map(|(p, n)| format!("{p} {n}"))),
        )
        .expect("sizes");
        let nix = scratch.join("bin/nix");
        std::fs::write(
            &nix,
            "#!/bin/sh\n[ \"$1 $2\" = \"path-info --size\" ] || exit 64\nshift 2\n\
             for p; do grep -F \"$p \" \"$SIZES\" || exit 1; done\n",
        )
        .expect("the stub nix");
        std::fs::set_permissions(&nix, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("executable");
        StubCaches { scratch }
    }

    /// `cache` (`upstream` or `cache`) answers the narinfo of `path` from now on.
    fn serve(&self, cache: &str, path: &str) {
        let hash = &path["/nix/store/".len()..][..32];
        std::fs::write(self.scratch.join(format!("{cache}/{hash}.narinfo")), "")
            .expect("a narinfo");
    }

    /// The script's exit code, what it printed, and the unserved list it wrote.
    fn run(
        &self,
        mode: &str,
        paths: &[&str],
        out: &str,
        limit: &str,
    ) -> (Option<i32>, String, String) {
        let input = self.scratch.join(format!("{out}.in"));
        std::fs::write(&input, lines(paths.iter())).expect("the list");
        let path = format!(
            "{}:{}",
            self.scratch.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let url = |cache: &str| format!("file://{}", self.scratch.join(cache).display());
        let done = Command::new(repo().join("scripts/cache-unserved.sh"))
            .args([
                mode,
                &input.to_string_lossy(),
                &self.scratch.join(out).to_string_lossy(),
            ])
            .env("PATH", path)
            .env("SIZES", self.scratch.join("sizes"))
            .env("UPSTREAM", url("upstream"))
            .env("CACHE", url("cache"))
            .env("LIMIT", limit)
            .output()
            .expect("cache-unserved.sh runs");
        let unserved = std::fs::read_to_string(self.scratch.join(out).join("unserved.txt"))
            .unwrap_or_default();
        (
            done.status.code(),
            String::from_utf8_lossy(&done.stdout).into_owned(),
            unserved,
        )
    }
}

fn lines(items: impl Iterator<Item = impl std::fmt::Display>) -> String {
    items.fold(String::new(), |mut text, item| {
        use std::fmt::Write;
        writeln!(text, "{item}").expect("a String takes a line");
        text
    })
}

/// `scripts/cache-unserved.sh` over stub caches, as two publication runs whose trees share a
/// path meet them. The build job's measure lists what neither cache serves; a path the other run
/// pushes before this run's push is skipped by the push job's measure, and the rest is pushed. A
/// list naming a path upstream serves, or adding more than the limit, fails, naming why.
#[test]
fn cache_unserved_skips_what_another_run_pushed_and_refuses_the_rest() {
    let path = |c: char, name: &str| format!("/nix/store/{}-{name}", c.to_string().repeat(32));
    let (up, mine) = (path('0', "glibc"), path('1', "pushed-before"));
    let (a, b, c) = (
        path('2', "app"),
        path('3', "cargo-package-shared"),
        path('4', "web"),
    );
    let (a, b, c, up, mine) = (
        a.as_str(),
        b.as_str(),
        c.as_str(),
        up.as_str(),
        mine.as_str(),
    );
    let caches = StubCaches::new(&[
        (up, 5_000_000_000),
        (mine, 5_000_000_000),
        (a, 300_000_000),
        (b, 300_000_000),
        (c, 300_000_000),
    ]);
    caches.serve("upstream", up);
    caches.serve("cache", mine);
    let gigabyte = "1000000000";

    let (code, said, listed) = caches.run("closure", &[up, mine, a, b, c], "build", gigabyte);
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(listed, lines([a, b, c].iter()), "what neither cache serves");
    assert!(said.contains("the push adds 3 paths"), "{said}");

    // The other run pushes the shared path between this run's two measures.
    caches.serve("cache", b);
    let (code, said, pushed) = caches.run("list", &[a, b, c], "push", gigabyte);
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(
        pushed,
        lines([a, c].iter()),
        "the list less what was pushed since"
    );
    assert!(
        said.contains(&format!("skipped, pushed since the list was taken: {b}")),
        "{said}"
    );
    assert!(said.contains("the push adds 2 paths"), "{said}");

    for (name, mode, paths, limit, why) in [
        (
            "upstream",
            "list",
            [a, up],
            gigabyte,
            "made it wrongly: nothing pushed",
        ),
        (
            "over",
            "list",
            [a, c],
            "500000000",
            "over the 500 MB one publication may add",
        ),
        (
            "over-build",
            "closure",
            [a, c],
            "500000000",
            "nothing pushed",
        ),
    ] {
        let (code, said, _) = caches.run(mode, &paths, name, limit);
        assert_eq!(code, Some(1), "{name}: {said}");
        assert!(said.contains(why), "{name}: {said}");
    }
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
