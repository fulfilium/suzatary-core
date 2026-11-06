//! The tree half on GitHub's runners (`scripts/train.sh gate --remote github`): the steps the box
//! runs (`box_before` and `box_after` in `scripts/train.sh`) and the jobs of `.github/workflows/gate.yml` hold every
//! step of `scripts/merge-gate.sh`'s tree half between them; the workflow runs on a push to a gate
//! branch alone, with a read-only token, no secret and every action pinned, and restores the store
//! `.github/workflows/gate-store.yml` saves on master and saves nothing; and `scripts/e2e-part.sh`
//! deals the browser suite's files to parts that hold each once.

use super::merge_gate::{MADE_AND_READ, steps};
use super::{Scratch, repo};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The value of `<key>: '<value>'` (or unquoted, up to a comma or brace) in a flow mapping line.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.split_once(&format!("{key}: "))?.1;
    Some(match rest.strip_prefix('\'') {
        Some(quoted) => quoted.split_once('\'')?.0,
        None => rest.split([',', '}']).next()?.trim(),
    })
}

/// gate.yml's jobs, by lane: each one's steps and its e2e part.
fn jobs() -> BTreeMap<String, (Vec<String>, String)> {
    read(".github/workflows/gate.yml")
        .lines()
        .filter(|line| line.trim_start().starts_with("- { lane: "))
        .map(|line| {
            let lane = field(line, "lane").expect("a lane").to_owned();
            let steps = field(line, "steps")
                .expect("its steps")
                .split(',')
                .map(str::to_owned)
                .collect();
            let part = field(line, "part").expect("its part").to_owned();
            (lane, (steps, part))
        })
        .collect()
}

/// The steps `scripts/train.sh` runs on the box, each list one run of the gate: `box_before`,
/// before the push, and `box_after`, after it.
fn box_runs() -> Vec<(&'static str, Vec<String>)> {
    let train = read("scripts/train.sh");
    ["box_before", "box_after"]
        .into_iter()
        .map(|name| {
            let steps = train
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=\"")))
                .and_then(|list| list.strip_suffix('"'))
                .unwrap_or_else(|| panic!("train.sh names the box's steps, {name}"))
                .split(',')
                .map(str::to_owned)
                .collect();
            (name, steps)
        })
        .collect()
}

/// Every step of the tree half runs on the box or in a job, never both; each job and each of the
/// box's runs hold what each step they run reads from another; and a step runs in more than one job only as what
/// a job's own reader reads, or as the e2e parts, each a part of its scenes.
#[test]
fn gate_yml_and_the_box_run_every_tree_step() {
    let tree: BTreeSet<String> = steps()
        .into_iter()
        .filter(|s| s.half == "tree")
        .map(|s| s.name)
        .collect();
    let jobs = jobs();
    assert!(!jobs.is_empty(), "gate.yml names its jobs");
    let runs = box_runs();
    let on_box: BTreeSet<String> = runs.iter().flat_map(|(_, s)| s.clone()).collect();
    assert_eq!(
        on_box.len(),
        runs.iter().map(|(_, s)| s.len()).sum::<usize>(),
        "the box runs each step once"
    );
    let in_jobs: BTreeSet<String> = jobs.values().flat_map(|(s, _)| s.clone()).collect();
    for name in on_box.iter().chain(&in_jobs) {
        assert!(tree.contains(name), "{name} is no step of the tree half");
    }
    assert_eq!(
        on_box.union(&in_jobs).cloned().collect::<BTreeSet<_>>(),
        tree,
        "the box and the jobs run every step of the tree half"
    );
    assert_eq!(
        on_box.intersection(&in_jobs).count(),
        0,
        "no step runs both on the box and in a job"
    );
    let places = jobs
        .iter()
        .map(|(lane, (s, _))| (lane.as_str(), s.clone()))
        .chain(runs);
    for (place, held) in places {
        for &(made, reader) in MADE_AND_READ {
            assert!(
                !held.iter().any(|s| s == reader) || held.iter().any(|s| s == made),
                "{place} runs {reader}, which reads what {made} makes, so it runs {made} too"
            );
        }
    }
    for name in &in_jobs {
        let holding: Vec<&str> = jobs
            .iter()
            .filter(|(_, (s, _))| s.contains(name))
            .map(|(lane, _)| lane.as_str())
            .collect();
        let read_there = |lane: &str| {
            MADE_AND_READ
                .iter()
                .any(|&(made, reader)| made == name && jobs[lane].0.iter().any(|s| s == reader))
        };
        let unread: Vec<&&str> = holding.iter().filter(|lane| !read_there(lane)).collect();
        assert!(
            unread.len() <= 1 || unread.iter().all(|lane| !jobs[**lane].1.is_empty()),
            "{name} runs in {holding:?}, more than once for itself and not in parts"
        );
    }
}

/// The e2e jobs are the parts 1/n to n/n of one n, each once, and only they run e2e.
#[test]
fn gate_yml_e2e_jobs_are_every_part_once() {
    let jobs = jobs();
    let parts: Vec<&str> = jobs
        .values()
        .filter(|(s, _)| s.iter().any(|s| s == "e2e"))
        .map(|(_, part)| part.as_str())
        .collect();
    let n = parts.len();
    assert!(n > 1, "the scenes are split: {parts:?}");
    let want: BTreeSet<String> = (1..=n).map(|k| format!("{k}/{n}")).collect();
    assert_eq!(
        parts
            .iter()
            .map(|p| (*p).to_owned())
            .collect::<BTreeSet<_>>(),
        want
    );
    assert!(
        jobs.values()
            .all(|(s, part)| s.iter().any(|s| s == "e2e") || part.is_empty()),
        "a part only on an e2e job"
    );
}

/// A workflow's lines that are not comments.
fn code(rel: &str) -> String {
    read(rel)
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The trigger block: the lines under `on:`, trimmed.
fn trigger(body: &str) -> Vec<&str> {
    body.lines()
        .skip_while(|line| *line != "on:")
        .skip(1)
        .take_while(|line| line.starts_with(' ') || line.is_empty())
        .map(str::trim)
        .collect()
}

/// Every `uses:` names its action by a commit.
fn assert_pinned(body: &str) {
    for line in body.lines().filter(|l| l.contains("uses: ")) {
        let pin = line
            .rsplit_once('@')
            .map(|(_, at)| at.split_whitespace().next());
        assert!(
            pin.flatten()
                .is_some_and(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())),
            "pinned by commit: {line}"
        );
    }
}

/// The values of a `<key>: ` the text spells, each once per spelling.
fn values<'a>(body: &'a str, key: &str) -> BTreeSet<&'a str> {
    body.lines()
        .filter_map(|line| line.trim_start().strip_prefix(key)?.strip_prefix(": "))
        .collect()
}

/// The workflow runs on a push to `gate/**` and on nothing else (no pull request event, from a
/// fork or not), its token reads only, its checkout keeps no credential, every action is pinned
/// by its commit, and it names no secret: it reads the Cachix as a substituter, pushes to no
/// binary cache, and restores the Actions cache without saving to it.
#[test]
fn gate_yml_runs_on_a_gate_branch_alone_holding_no_secret() {
    let body = code(".github/workflows/gate.yml");
    let on = trigger(&body);
    assert_eq!(on, ["push:", "branches: [ 'gate/**' ]", ""], "{on:?}");
    assert!(!body.contains("pull_request"), "no pull request event");
    assert!(
        body.contains("permissions:\n  contents: read\n"),
        "the token reads only"
    );
    assert!(!body.contains(": write"), "and no job asks to write");
    assert!(body.contains("persist-credentials: false"));
    assert_pinned(&body);
    assert!(!body.contains("secrets."), "no secret");
    assert!(!body.contains("cachix-action"), "no Cachix action");
    assert!(!body.contains("cachix push"), "no push to the Cachix");
    assert!(body.contains("extra-substituters = https://bmabsout.cachix.org\n"));
    let caches: Vec<&str> = body
        .lines()
        .filter_map(|l| l.split_once("uses: nix-community/cache-nix-action"))
        .map(|(_, rest)| rest)
        .collect();
    assert_eq!(caches.len(), 1, "one use of the cache action: {caches:?}");
    assert!(
        caches[0].starts_with("/restore@"),
        "its restore alone, which has no saving post step: {caches:?}"
    );
    assert!(
        body.contains("continue-on-error: true"),
        "a failed restore is no failure"
    );
}

/// gate-store.yml runs on a push to master in the public repository alone, names no secret, is
/// the one workflow whose token may write (the Actions cache, for its purge), saves under the key
/// gate.yml restores, collects the store to its roots before saving, purges every older entry of
/// it, and fails when its entry is over half of the repository's 10 GB.
#[test]
fn gate_store_yml_saves_the_one_store_on_master() {
    let store = code(".github/workflows/gate-store.yml");
    let gate = code(".github/workflows/gate.yml");
    assert_eq!(
        trigger(&store),
        ["push:", "branches: [master]", ""],
        "a push to master and nothing else"
    );
    assert!(!store.contains("pull_request"));
    assert!(store.contains("if: ${{ !github.event.repository.private }}"));
    assert!(!store.contains("secrets."), "no secret");
    assert!(!store.contains("cachix push") && !store.contains("cachix-action"));
    assert!(store.contains("persist-credentials: false"));
    assert_pinned(&store);
    assert_eq!(store.matches(": write").count(), 1, "one permission writes");
    assert!(store.contains("      contents: read\n      actions: write\n"));
    let workflows = std::fs::read_dir(repo().join(".github/workflows")).expect("the workflows");
    for entry in workflows {
        let name = entry.expect("a workflow").file_name();
        let name = name.to_string_lossy();
        if name != "gate-store.yml" && name != "gate-passes.yml" {
            assert!(
                !code(&format!(".github/workflows/{name}")).contains("actions: write"),
                "{name} may not delete the Actions cache"
            );
        }
    }
    let keys = values(&store, "primary-key");
    assert_eq!(keys.len(), 1, "one key: {keys:?}");
    assert_eq!(
        values(&gate, "primary-key"),
        keys,
        "gate.yml restores what this saves"
    );
    let key = keys.first().expect("a key");
    for read in ["flake.nix", "flake.lock", "app/Cargo.lock"] {
        assert!(
            key.contains(&format!("'{read}'")),
            "the key reads {read}: {key}"
        );
    }
    let prefixes = values(&store, "restore-prefixes-first-match");
    assert_eq!(values(&gate, "restore-prefixes-first-match"), prefixes);
    assert_eq!(
        values(&store, "purge-prefixes"),
        prefixes,
        "purges its own entries"
    );
    let prefix = prefixes.first().expect("a prefix");
    assert!(key.starts_with(prefix), "{key} has the prefix {prefix}");
    assert_eq!(values(&store, "purge"), BTreeSet::from(["true"]));
    assert_eq!(values(&store, "purge-created"), BTreeSet::from(["0"]));
    assert_eq!(
        values(&store, "purge-primary-key"),
        BTreeSet::from(["never"])
    );
    let at = |needle: &str| {
        store
            .find(needle)
            .unwrap_or_else(|| panic!("gate-store.yml has {needle:?}"))
    };
    assert!(
        at("nix store gc") < at("cache-nix-action/save@"),
        "collected, then saved"
    );
    assert!(
        at("cache-nix-action/save@") < at("LIMIT: 5000000000"),
        "measured once saved"
    );
    assert!(store.contains("if ((bytes > LIMIT)); then"));
}

/// gate-passes.yml runs on master when a Gate run ends, in the public repository alone, names no
/// secret, saves the passes under a key with the prefix gate.yml restores, at the path it restores
/// them to, and purges its own older entries alone. Each gate job uploads what its gate kept,
/// whatever its verdict, and moves the restored passes out of its tree before the gate reads it.
#[test]
fn gate_passes_yml_hands_each_gate_the_passes_of_the_last() {
    let passes = code(".github/workflows/gate-passes.yml");
    let gate = code(".github/workflows/gate.yml");
    assert_eq!(
        trigger(&passes),
        [
            "workflow_run:",
            "workflows: [Gate]",
            "types: [completed]",
            ""
        ],
        "a completed Gate run and nothing else"
    );
    assert!(
        gate.starts_with("name: Gate\n"),
        "the run it follows is gate.yml's"
    );
    assert!(!passes.contains("pull_request"));
    assert!(!passes.contains("secrets."), "no secret");
    assert_pinned(&passes);
    assert!(passes.contains("      contents: read\n      actions: write\n"));
    assert_eq!(
        values(&passes, "restore-keys"),
        values(&gate, "restore-keys"),
        "gate.yml restores what this saves"
    );
    assert_eq!(
        values(&gate, "restore-keys"),
        BTreeSet::from(["gate-passes-"])
    );
    let keys = values(&passes, "key");
    assert_eq!(
        keys,
        BTreeSet::from(["gate-passes-${{ github.event.workflow_run.id }}"]),
        "one key, the run's, restored and saved"
    );
    assert!(
        values(&gate, "path").contains("passes") && values(&passes, "path").contains("passes"),
        "an entry restores only at the path it was saved from"
    );
    assert!(
        passes.contains("select(.key != env.KEY)") && passes.contains("caches?key=gate-passes-")
    );
    assert!(gate.contains("run: if [ -d passes ]; then mv passes .git/gate; fi"));
    let at = |needle: &str| {
        gate.find(needle)
            .unwrap_or_else(|| panic!("gate.yml has {needle:?}"))
    };
    assert!(at("mv passes .git/gate") < at("scripts/merge-gate.sh --no-live"));
    assert!(at("name: passes-${{ matrix.lane }}") < at("- name: red"));
    assert!(gate.contains("path: .git/gate\n          include-hidden-files: true"));
}

/// The `if:` of `job` in `body`, folded or not, as the set of its conjuncts; `None` where it is
/// not one `${{ }}` expression or a conjunct holds an `||`, so the set is the whole condition.
fn conjuncts(body: &str, job: &str) -> Option<BTreeSet<String>> {
    let mut lines = body
        .lines()
        .skip_while(|l| l.trim_end() != format!("  {job}:"))
        .skip(1);
    let first = lines.find_map(|l| l.strip_prefix("    if: "))?;
    let text = if first == ">-" {
        let folded: Vec<&str> = lines
            .take_while(|l| l.starts_with("      "))
            .map(str::trim)
            .collect();
        folded.join(" ")
    } else {
        first.to_owned()
    };
    let inner = text.strip_prefix("${{")?.strip_suffix("}}")?;
    inner
        .split("&&")
        .map(|c| (!c.contains("||")).then(|| c.trim().to_owned()))
        .collect()
}

/// The passes gate-passes.yml saves are the word of a gate job, so it acts only after a Gate run a
/// push to a `gate/` branch of this same repository started: a `workflow_run` job holds the
/// repository's token whatever started the run it follows, a fork's or a pull request's too.
#[test]
fn gate_passes_yml_takes_passes_from_this_repositorys_gate_pushes_alone() {
    let passes = code(".github/workflows/gate-passes.yml");
    assert_eq!(
        conjuncts(&passes, "passes"),
        Some(
            [
                "!github.event.repository.private",
                "github.event.workflow_run.event == 'push'",
                "github.event.workflow_run.head_repository.full_name == github.repository",
                "startsWith(github.event.workflow_run.head_branch, 'gate/')",
            ]
            .map(str::to_owned)
            .into()
        ),
        "public, after a push, from this repository, on a gate branch, and nothing looser"
    );
    assert_eq!(
        passes.matches("    if: ").count(),
        1,
        "the job's one condition"
    );
    let loose = "jobs:\n  passes:\n    if: ${{ a && b || c }}\n";
    assert_eq!(
        conjuncts(loose, "passes"),
        None,
        "a disjunction is no set of conjuncts"
    );
}

/// actionlint passes the workflows the gate and the caches rest on: their syntax, their
/// expressions, their runner labels and the shell of each step.
#[test]
fn gate_workflows_pass_actionlint() {
    let out = Command::new("actionlint")
        .args([
            ".github/workflows/gate.yml",
            ".github/workflows/gate-store.yml",
            ".github/workflows/gate-passes.yml",
            ".github/workflows/publication.yml",
        ])
        .current_dir(repo())
        .output()
        .expect("actionlint runs (the .#triage shell carries it)");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `scripts/e2e-part.sh <k>/<n>`: the n parts hold every file once, the same files on every run,
/// each part some file while there are files enough, and a part that is no part is refused.
#[test]
fn e2e_parts_hold_every_file_once() {
    let scratch = Scratch::new("e2e-part");
    let e2e = scratch.join("web/e2e");
    std::fs::create_dir_all(&e2e).expect("web/e2e");
    let files: BTreeSet<String> = (0..11).map(|i| format!("scene{i}")).collect();
    for (i, name) in files.iter().enumerate() {
        std::fs::write(
            e2e.join(format!("{name}.test.ts")),
            "x".repeat(100 + 37 * (i % 5)),
        )
        .expect("a scene");
    }
    std::fs::write(e2e.join("bench.bench.ts"), "not a scene").expect("a bench");
    let part = |arg: &str| {
        let out = Command::new(repo().join("scripts/e2e-part.sh"))
            .arg(arg)
            .current_dir(&*scratch)
            .output()
            .expect("e2e-part.sh runs");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )
    };
    let mut seen = Vec::new();
    for k in 1..=4 {
        let (code, names) = part(&format!("{k}/4"));
        assert_eq!(code, Some(0));
        assert!(!names.is_empty(), "part {k} holds a file");
        assert_eq!(part(&format!("{k}/4")).1, names, "the same part again");
        seen.extend(names);
    }
    seen.sort();
    assert_eq!(
        seen,
        files.into_iter().collect::<Vec<_>>(),
        "each file once"
    );
    for bad in ["5/4", "0/4", "1", "a/b", ""] {
        assert_eq!(part(bad).0, Some(64), "{bad:?} is refused");
    }
}

/// The live half's web step substitutes what it reads from the Cachix, which holds only what
/// publication.yml builds from the gate commit: so it builds every web output the step reads and
/// the fixture binary, and pushes their closures with the deploy's.
#[test]
fn publication_builds_what_the_live_web_step_reads() {
    let gate = std::fs::read_to_string(repo().join("scripts/merge-gate.sh")).expect("reads");
    let start = gate
        .find("\nweb_outputs=(")
        .expect("the web outputs are named")
        + "\nweb_outputs=(".len();
    let end = start + gate[start..].find(')').expect("the list ends");
    let outputs: Vec<&str> = gate[start..end]
        .split_whitespace()
        .filter_map(|o| o.split_once('=').map(|(_, out)| out))
        .collect();
    assert_eq!(
        outputs.len(),
        5,
        "every web output is named with its variable"
    );
    assert!(
        gate.contains("\"$tree#suzatary-fixture\""),
        "the fixture is the public tree's"
    );
    let publication = code(".github/workflows/publication.yml");
    let built: Vec<&str> = publication
        .lines()
        .filter_map(|l| l.trim().rsplit_once(" ./tree#").map(|(_, attr)| attr))
        .collect();
    for output in outputs.iter().chain(&["suzatary-fixture"]) {
        assert!(
            built.contains(output),
            "publication.yml does not build {output}"
        );
    }
    assert_eq!(
        publication.matches("./result-live-*").count(),
        2,
        "measured and pushed with the deploy's builds"
    );
}
