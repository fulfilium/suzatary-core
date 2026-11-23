//! What suzatary-core's runners build from a publication or a gate commit, pinned from the
//! workflow's source: `publication.yml` is the one writer of the binary cache, run only as
//! master's copy, running nothing of the tree but Nix's evaluation and its sandboxed builds.

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

/// The publication cache runs only on master's ref when the owner started it, never on a pull
/// request or a gate branch, and keeps the token from the tree: `build` names no secret, and
/// `push`, in the environment `cachix`, hands the token to its last step alone.
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

/// The build job takes its input only as a full commit id through the environment and runs
/// nothing of that tree but Nix's evaluation and sandboxed builds (never `nix run`, `nix develop`
/// or `nix shell`), and the push job reads nothing the build made until master's cachix is built.
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

/// What a push adds is measured by `scripts/cache-unserved.sh` in both jobs against 1 GB and the
/// same two caches, either failing before the push runs, and the push sends the second list.
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

/// Two stub caches behind a stub `curl` answering a narinfo 200 when held and 404 when not, or
/// failing a set number of times first, and failing any question asked without the script's
/// timeouts; questions are logged in `asked` as `<cache> <hash>` and pauses in `slept`.
struct StubCaches {
    scratch: Scratch,
}

const CURL: &str = r#"#!/bin/sh
for timeout in "--connect-timeout 5" "--max-time 10"; do
  case " $* " in
    *" $timeout "*) ;;
    *) echo "curl without $timeout: $*" >&2 && exit 2 ;;
  esac
done
for url; do :; done
host="${url#http://}" file="${url##*/}"
cache="${host%%.test/*}" hash="${file%.narinfo}"
echo "$cache $hash" >>"$STUB/asked"
if [ -f "$STUB/$cache/$hash.fail" ]; then
  read -r left how <"$STUB/$cache/$hash.fail"
  if [ "$left" -gt 0 ]; then
    echo "$((left - 1)) $how" >"$STUB/$cache/$hash.fail"
    case "$how" in
      timeout) printf 000 && exit 28 ;;
      *) printf %s "$how" && exit 0 ;;
    esac
  fi
fi
if [ -f "$STUB/$cache/$hash.narinfo" ]; then printf 200; else printf 404; fi
"#;

impl StubCaches {
    fn new(sizes: &[(&str, u64)]) -> Self {
        let scratch = Scratch::new("cache-unserved");
        for dir in ["upstream", "cache", "tmp"] {
            std::fs::create_dir_all(scratch.join(dir)).expect("a directory");
        }
        std::fs::write(
            scratch.join("sizes"),
            lines(sizes.iter().map(|(p, n)| format!("{p} {n}"))),
        )
        .expect("sizes");
        let bin = scratch.join("bin");
        let _ = suzatary_testkit::fake_program(
            &bin,
            "nix",
            "#!/bin/sh\n[ \"$1 $2\" = \"path-info --size\" ] || exit 64\nshift 2\n\
             for p; do grep -F \"$p \" \"$STUB/sizes\" || exit 1; done\n",
        );
        let _ = suzatary_testkit::fake_program(&bin, "curl", CURL);
        let _ = suzatary_testkit::fake_program(
            &bin,
            "sleep",
            "#!/bin/sh\necho \"$1\" >>\"$STUB/slept\"\n",
        );
        StubCaches { scratch }
    }

    fn file(&self, cache: &str, path: &str, ending: &str) -> std::path::PathBuf {
        let hash = &path["/nix/store/".len()..][..32];
        self.scratch.join(format!("{cache}/{hash}.{ending}"))
    }

    /// `cache` (`upstream` or `cache`) answers the narinfo of `path` from now on.
    fn serve(&self, cache: &str, path: &str) {
        std::fs::write(self.file(cache, path, "narinfo"), "").expect("a narinfo");
    }

    /// `cache`'s next `times` answers about `path` are `how`: `timeout`, curl's exit 28, or a
    /// status.
    fn fail(&self, cache: &str, path: &str, times: u32, how: &str) {
        std::fs::write(self.file(cache, path, "fail"), format!("{times} {how}\n"))
            .expect("a failure");
    }

    /// Every child's `ask` answers about `path` by running `body` instead, through `BASH_ENV`,
    /// which `bash -c` reads after it imports the script's exported `ask`.
    fn replace_ask(&self, path: &str, body: &str) {
        std::fs::write(
            self.scratch.join("ask.sh"),
            format!(
                "if declare -f ask >/dev/null; then\n  eval \"real_$(declare -f ask)\"\n  \
                 ask() {{ if [ \"$2\" = {path} ]; then {body}; else real_ask \"$@\"; fi; }}\nfi\n"
            ),
        )
        .expect("the replaced ask");
    }

    fn wrote(&self, out: &str, name: &str) -> String {
        std::fs::read_to_string(self.scratch.join(out).join(name)).expect("the run wrote it")
    }

    fn asked(&self, cache: &str, path: &str) -> usize {
        let line = format!("{cache} {}", &path["/nix/store/".len()..][..32]);
        std::fs::read_to_string(self.scratch.join("asked"))
            .unwrap_or_default()
            .lines()
            .filter(|l| *l == line)
            .count()
    }

    fn slept(&self) -> String {
        std::fs::read_to_string(self.scratch.join("slept")).unwrap_or_default()
    }

    /// The script's exit code, what it printed, and the unserved list it wrote, if it wrote one.
    fn run(
        &self,
        mode: &str,
        paths: &[&str],
        out: &str,
        limit: &str,
    ) -> (Option<i32>, String, Option<String>) {
        let input = self.scratch.join(format!("{out}.in"));
        std::fs::write(&input, lines(paths.iter())).expect("the list");
        let path = format!(
            "{}:{}",
            self.scratch.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(repo().join("scripts/cache-unserved.sh"));
        let ask = self.scratch.join("ask.sh");
        if ask.exists() {
            command.env("BASH_ENV", ask);
        }
        let done = command
            .args([
                mode,
                &input.to_string_lossy(),
                &self.scratch.join(out).to_string_lossy(),
            ])
            .env("PATH", path)
            .env("STUB", self.scratch.join(""))
            .env("TMPDIR", self.scratch.join("tmp"))
            .env("UPSTREAM", "http://upstream.test")
            .env("CACHE", "http://cache.test")
            .env("LIMIT", limit)
            .output()
            .expect("cache-unserved.sh runs");
        let unserved = std::fs::read_to_string(self.scratch.join(out).join("unserved.txt")).ok();
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

/// Two publication runs sharing a path: the push measure skips what the other run pushed after
/// the build measure, and a list naming a path upstream serves or over the limit fails, naming why.
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
    assert_eq!(
        listed,
        Some(lines([a, b, c].iter())),
        "what neither cache serves"
    );
    assert!(said.contains("the push adds 3 paths"), "{said}");

    // The other run pushes the shared path between this run's two measures.
    caches.serve("cache", b);
    let (code, said, pushed) = caches.run("list", &[a, b, c], "push", gigabyte);
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(
        pushed,
        Some(lines([a, c].iter())),
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

/// A store path whose hash is `n` in 32 digits.
fn numbered(n: usize, name: &str) -> String {
    format!("/nix/store/{n:032}-{name}")
}

/// Among hundreds of questions, one that fails once is asked again, and the list names only what
/// both caches said they do not hold.
#[test]
fn cache_unserved_asks_again_when_one_question_among_hundreds_fails() {
    let paths: Vec<String> = (0..400).map(|n| numbered(n, "dependency")).collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let (new, built) = (paths[123], &paths[390..]);
    let caches = StubCaches::new(&built.iter().map(|p| (*p, 1_000_000)).collect::<Vec<_>>());
    for path in &paths[..390] {
        caches.serve("upstream", path);
    }
    caches.fail("upstream", new, 1, "timeout");

    let (code, said, listed) = caches.run("closure", &paths, "build", "1000000000");
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(listed, Some(lines(built.iter())), "{said}");
    let mut both: Vec<String> = [caches.wrote("build", "served.txt"), lines(built.iter())]
        .concat()
        .lines()
        .map(str::to_owned)
        .collect();
    both.sort();
    assert_eq!(both, paths, "served and unserved partition the input");
    assert_eq!(caches.asked("upstream", new), 2, "asked again once");
    assert_eq!(caches.asked("cache", new), 0, "upstream's yes is enough");
    assert_eq!(caches.slept(), "1\n");
}

/// A question that fails is asked again, five tries in all with pauses of 1, 2, 4 and 8
/// seconds, whether it failed with a curl error or a status other than 200 and 404; an answer
/// on any try is the path's answer.
#[test]
fn cache_unserved_asks_a_failed_question_again_with_growing_pauses() {
    let (late_yes, late_no) = (numbered(1, "late-yes"), numbered(2, "late-no"));
    let (late_yes, late_no) = (late_yes.as_str(), late_no.as_str());
    let caches = StubCaches::new(&[(late_no, 1_000_000)]);
    caches.serve("upstream", late_yes);
    caches.fail("upstream", late_yes, 4, "503");
    caches.fail("cache", late_no, 2, "timeout");

    let (code, said, listed) = caches.run("closure", &[late_yes, late_no], "build", "1000000000");
    assert_eq!(code, Some(0), "{said}");
    assert_eq!(listed, Some(lines([late_no].iter())), "{said}");
    assert_eq!(caches.asked("upstream", late_yes), 5);
    assert_eq!(caches.asked("cache", late_no), 3);
    assert_eq!(caches.slept(), "1\n2\n4\n8\n1\n2\n");
}

/// A path a cache gives no answer about in five tries fails the measure, in either job, naming
/// the cache, the count and the path with its last failure, and writes no list. Upstream's
/// failure stops the run before the project's cache is asked.
#[test]
fn cache_unserved_fails_by_name_when_a_cache_cannot_be_asked() {
    let (fine, mute) = (numbered(1, "fine"), numbered(2, "mute"));
    let (fine, mute) = (fine.as_str(), mute.as_str());
    for (mode, cache, how, why) in [
        (
            "closure",
            "upstream",
            "503",
            "the upstream cache could not be asked about 1 paths",
        ),
        (
            "list",
            "upstream",
            "timeout",
            "the upstream cache could not be asked about 1 paths",
        ),
        (
            "closure",
            "cache",
            "403",
            "the project's cache could not be asked about 1 paths",
        ),
        (
            "list",
            "cache",
            "timeout",
            "the project's cache could not be asked about 1 paths",
        ),
    ] {
        let caches = StubCaches::new(&[(fine, 1_000_000), (mute, 1_000_000)]);
        caches.fail(cache, mute, 5, how);
        let (code, said, listed) = caches.run(mode, &[fine, mute], "out", "1000000000");
        let case = format!("{mode} {cache} {how}: {said}");
        assert_eq!(code, Some(1), "{case}");
        assert!(said.contains(why), "{case}");
        let last = if how == "timeout" {
            "curl-exit-28"
        } else {
            how
        };
        assert!(said.contains(&format!("{mute} ({last})")), "{case}");
        assert_eq!(listed, None, "{case}");
        assert_eq!(caches.asked(cache, mute), 5, "{case}");
        if cache == "upstream" {
            assert_eq!(caches.asked("cache", fine), 0, "{case}");
        }
    }
}

/// The measure reads its children's answers whole: an answer that is none of the three, a line
/// that holds more than one answer, a path with no answer, or a child that fails stops the run,
/// naming the fault, and writes no list.
#[test]
fn cache_unserved_refuses_answers_it_cannot_read() {
    let (fine, odd) = (numbered(1, "fine"), numbered(2, "odd"));
    let (fine, odd) = (fine.as_str(), odd.as_str());
    for (name, body, why) in [
        (
            "a fourth answer",
            "echo \"maybe $2\"",
            format!("is none of held <path>, absent <path> and unknown <path> <last>: maybe {odd}"),
        ),
        (
            "two answers on a line",
            "echo \"held $2 absent $2\"",
            "is none of held".to_owned(),
        ),
        ("no answer", ":", format!("asked, with no answer: {odd}")),
        (
            "a child that fails",
            "exit 3",
            "did not run to its answer (xargs exit 123".to_owned(),
        ),
    ] {
        let caches = StubCaches::new(&[(fine, 1_000_000), (odd, 1_000_000)]);
        caches.replace_ask(odd, body);
        let (code, said, listed) = caches.run("closure", &[fine, odd], "out", "1000000000");
        assert_eq!(code, Some(1), "{name}: {said}");
        assert!(said.contains(&why), "{name}: {said}");
        assert_eq!(listed, None, "{name}");
    }
}

/// `scripts/core-settings.sh`'s one ruleset lets only a repository administrator move master, so
/// no workflow, whatever it asks for, can move master to run code of its own in the publication.
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

/// No workflow but publication.yml, and no script or Nix file of the box but
/// `scripts/core-settings.sh`, which hands it to GitHub, names the Cachix token or pushes to a
/// binary cache, so the box, holding the private tree, has no path to the cache.
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

/// The train reads a failed publication run as the tree's red only where the build job's build
/// step failed (`scripts/publication-run.sh`'s `unbuilt`), naming that step: the build job holds
/// a step of that name.
#[test]
fn publication_run_names_the_build_step_publication_yml_has() {
    let script = std::fs::read_to_string(repo().join("scripts/publication-run.sh"))
        .expect("publication-run.sh reads");
    let step = script
        .lines()
        .find_map(|line| line.strip_prefix("build_step=\""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("publication-run.sh names the build step");
    let text = workflow("publication.yml");
    let build = section(&text, "\n  build:");
    assert!(
        build.contains(&format!("- name: {step}\n")),
        "publication.yml's build job has no step {step:?}"
    );
}
