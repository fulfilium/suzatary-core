//! Every GitHub workflow and composite action this repository holds or writes, read as an attacker
//! who controls one third-party action or one ref name: an action runs only from a commit, a
//! workflow's token is declared and not GitHub's default, a ref or event field never reaches a
//! shell as code, and the APK signing key is named by one job that holds no write token and runs
//! no code another job could have planted.

use super::repo;
use std::collections::BTreeMap;
use std::process::Command;

/// The workflow `prodrome init --public` writes into a public store, which this repository owns.
const PAGES: &str = "app/cli/src/commands/prodrome/roadmap.pages.yml";

/// The workflows, by path from the repository root.
fn workflows() -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(repo().join(".github/workflows"))
        .expect("the workflows")
        .map(|entry| {
            format!(
                ".github/workflows/{}",
                entry.expect("an entry").file_name().to_string_lossy()
            )
        })
        .collect();
    found.push(PAGES.to_owned());
    found.sort();
    found
}

/// The workflows and the composite actions.
fn files() -> Vec<String> {
    let mut found = workflows();
    for entry in std::fs::read_dir(repo().join(".github/actions")).expect("the actions") {
        let name = entry.expect("an entry").file_name();
        found.push(format!(
            ".github/actions/{}/action.yml",
            name.to_string_lossy()
        ));
    }
    found.sort();
    found
}

/// A workflow's lines that are not comments.
pub(super) fn code(rel: &str) -> String {
    std::fs::read_to_string(repo().join(rel))
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each job's lines, by its name.
fn jobs(body: &str) -> BTreeMap<&str, Vec<&str>> {
    let mut jobs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut current = None;
    for line in body.lines().skip_while(|line| *line != "jobs:").skip(1) {
        match line.strip_prefix("  ").and_then(|l| l.strip_suffix(':')) {
            Some(name) if !name.starts_with(' ') => current = Some(name),
            _ => {
                if let Some(name) = current {
                    jobs.entry(name).or_default().push(line);
                }
            }
        }
    }
    jobs
}

/// The text of every `run:` script, inline or a block.
fn scripts(body: &str) -> Vec<String> {
    let lines: Vec<&str> = body.lines().collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((lead, value)) = line.split_once("run: ") else {
            continue;
        };
        if !lead
            .trim_start_matches(' ')
            .trim_start_matches("- ")
            .is_empty()
        {
            continue;
        }
        if value.starts_with('|') || value.starts_with('>') {
            let block: Vec<&str> = lines[i + 1..]
                .iter()
                .take_while(|l| l.is_empty() || l.len() - l.trim_start().len() > lead.len())
                .copied()
                .collect();
            found.push(block.join("\n"));
        } else {
            found.push(value.to_owned());
        }
    }
    found
}

/// Every `uses:` runs the action at a commit: by its 40-hex id, or `./`, this checkout itself. A
/// tag or a branch is the action's owner's to move, under every job that runs it.
#[test]
fn every_action_runs_from_a_commit() {
    for rel in files() {
        for line in code(&rel).lines().filter(|l| l.contains("uses: ")) {
            let action = line.split_once("uses: ").map_or("", |(_, a)| a);
            let pin = action
                .rsplit_once('@')
                .and_then(|(_, at)| at.split_whitespace().next());
            assert!(
                action.starts_with("./")
                    || pin.is_some_and(
                        |sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
                    ),
                "{rel}: pinned by commit: {line}"
            );
        }
    }
}

/// Every workflow declares its token's permissions at the top, so no job gets the repository's
/// default token.
#[test]
fn every_workflow_declares_its_token() {
    for rel in workflows() {
        assert!(
            code(&rel)
                .lines()
                .any(|line| line.starts_with("permissions:")),
            "{rel} declares a top-level permissions block"
        );
    }
}

/// A cache purge deletes entries through the API, which needs `actions: write`; a workflow that
/// purges without it fails, or keeps every old entry.
#[test]
fn every_cache_purge_holds_the_token_that_deletes() {
    for rel in workflows() {
        let body = code(&rel);
        assert!(
            !body.contains("purge: true") || body.contains("actions: write"),
            "{rel} purges a cache with a token that cannot delete one"
        );
    }
}

/// A ref name or an event field is text someone else chose, so it reaches a script through
/// `env:` and never as `${{ }}` in the script, where the runner would paste it in as code.
#[test]
fn no_ref_or_event_field_is_pasted_into_a_script() {
    let tainted = [
        "github.ref_name",
        "github.head_ref",
        "github.base_ref",
        "github.event.",
    ];
    for rel in files() {
        for script in scripts(&code(&rel)) {
            for expr in script.split("${{").skip(1) {
                let expr = expr.split("}}").next().unwrap_or_default();
                assert!(
                    !tainted.iter().any(|t| expr.contains(t)),
                    "{rel}: pass ${{{{{expr}}}}} through env:"
                );
            }
        }
    }
    let probe = "    steps:\n      - run: echo \"${{ github.ref_name }}\"\n";
    assert_eq!(
        scripts(probe),
        ["echo \"${{ github.ref_name }}\""],
        "an inline script is read"
    );
}

/// The APK signing key is named by one job alone, and that job's token reads only. It restores no
/// cache another job saved, it builds the signer before it downloads the unsigned APK, and it
/// downloads that artifact alone, so no file the build job made runs beside the key. A job that
/// writes the release checks out nothing, builds nothing and names no secret.
#[test]
fn the_signing_key_is_held_by_one_job_that_runs_nothing_another_job_made() {
    let body = code(".github/workflows/release-apk.yml");
    assert!(
        body.contains("\npermissions:\n  contents: read\n"),
        "the workflow's token reads only"
    );
    let jobs = jobs(&body);
    let holding: Vec<&&str> = jobs
        .iter()
        .filter(|(_, lines)| lines.iter().any(|l| l.contains("secrets.")))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(holding.len(), 1, "one job names a secret: {holding:?}");
    let signer = &jobs[holding[0]];
    let text = signer.join("\n");
    assert!(
        text.contains("secrets.ANDROID_KEYSTORE_B64"),
        "and it is the key"
    );
    assert!(
        !text.contains("permissions:"),
        "it takes the workflow's read-only token and grants itself nothing"
    );
    assert!(
        !text.contains("cache-nix-action") && !text.contains("actions/cache"),
        "it restores no cache"
    );
    assert_eq!(
        text.matches("download-artifact").count(),
        1,
        "it downloads one artifact"
    );
    assert!(
        text.contains("name: suzatary-unsigned\n"),
        "and that is the unsigned APK, which holds nothing else"
    );
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("the signing job has {needle:?}"))
    };
    assert!(
        at(".#apk-sign") < at("download-artifact") && at("download-artifact") < at("secrets."),
        "the signer is built before the APK arrives, and the key after"
    );
    for (name, lines) in jobs.iter().filter(|(name, _)| *name != holding[0]) {
        assert!(
            !lines.iter().any(|l| l.trim() == "contents: write")
                || !lines
                    .iter()
                    .any(|l| l.contains("nix ") || l.contains("actions/checkout")),
            "{name} writes a release, so it checks out and builds nothing"
        );
    }
}

/// actionlint passes every workflow, the Pages template included: its syntax, its expressions, its
/// runner labels and the shell of each step.
#[test]
fn every_workflow_passes_actionlint() {
    let out = Command::new("actionlint")
        .args(workflows())
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
