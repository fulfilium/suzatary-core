//! The data push's lists (`scripts/safe-push.sh`) and the vault's (`scripts/vault-backup.sh`),
//! derived from the stores' rows for the deployment `suzatary init` writes
//! (`instance.sample.toml`). The scripts spell the lists out, so a quiet push runs git and no
//! other program; each test holds one spelling equal to its derivation.

use super::{Scratch, assignments, repo, safe_push_script};
use suzatary_cli::instance::Instance;
use suzatary_core::store::{Files, Kind, Tier};

/// Every store of the sample deployment, by its row and its path under the data root.
fn placed() -> Vec<(Kind, String)> {
    let scratch = Scratch::new("lists");
    let path = scratch.join("instance.toml");
    std::fs::copy(repo().join("instance.sample.toml"), &path).expect("the sample copies");
    let env = suzatary_cli::paths::Env::under(scratch.join("home"), scratch.join("run"));
    let instance = Instance::read(&path, &env).expect("the sample reads");
    Kind::ALL
        .iter()
        .flat_map(|kind| {
            instance.directories(*kind).into_iter().map(|(_, dir)| {
                let relative = dir
                    .strip_prefix(instance.data())
                    .unwrap_or_else(|_| panic!("{} is outside the data root", dir.display()));
                (*kind, relative.to_string_lossy().into_owned())
            })
        })
        .collect()
}

/// The stores of `tier`, each path as a list names it: a directory with its slash.
fn listed(tier: Tier) -> Vec<(Kind, String)> {
    placed()
        .into_iter()
        .filter(|(kind, _)| kind.tier() == tier)
        .collect()
}

/// `path` as `grep -E` reads it literally.
fn literal(path: &str) -> String {
    path.chars()
        .flat_map(|c| {
            let special = ".[]()*+?{}|^$\\".contains(c);
            special.then_some('\\').into_iter().chain([c])
        })
        .collect()
}

/// The one file of each pushed store's that the push may commit.
fn admitted((kind, path): &(Kind, String)) -> String {
    let objects = r"objects/[0-9a-f]{64}\.py";
    let dir = literal(path);
    match kind.files() {
        Files::Chain => format!(r"{dir}/({objects}|\.lock|\.gitattributes)"),
        Files::Objects => format!(r"{dir}/({objects}|\.gitattributes)"),
        Files::Records => format!(r"{dir}/([0-9a-f]{{64}}\.json|\.gitignore)"),
        Files::File => dir,
        Files::Directory => format!("{dir}/.+"),
    }
}

/// The one assignment of `key` in `script`.
fn assigned(script: &str, key: &str) -> String {
    let all = assignments(script, key);
    assert_eq!(all.len(), 1, "{key} is assigned once: {all:?}");
    all[0].clone()
}

#[test]
fn the_push_stages_every_pushed_store() {
    let staged: Vec<String> = listed(Tier::Pushed)
        .into_iter()
        .map(|(kind, path)| match kind.files() {
            Files::File => path,
            _ => format!("{path}/"),
        })
        .collect();
    assert_eq!(assigned(&safe_push_script(), "ADD_PATHS"), staged.join(" "));
}

#[test]
fn the_push_admits_only_the_pushed_stores_files() {
    let alternatives: Vec<String> = listed(Tier::Pushed).iter().map(admitted).collect();
    assert_eq!(
        assigned(&safe_push_script(), "ALLOWED_PATTERN"),
        format!("^({})$", alternatives.join("|"))
    );
}

/// Every pushed store whose objects are appended and never edited is walked for a rewrite.
#[test]
fn the_push_refuses_a_rewrite_of_every_append_only_store() {
    let walked: Vec<String> = listed(Tier::Pushed)
        .into_iter()
        .filter(|(kind, _)| kind.append_only())
        .map(|(kind, path)| match kind.files() {
            Files::Records => format!("{path}/"),
            _ => format!("{path}/objects/"),
        })
        .collect();
    assert_eq!(assigned(&safe_push_script(), "WALKED"), walked.join(" "));
}

#[test]
fn the_vault_carries_every_box_local_store() {
    let script = std::fs::read_to_string(repo().join("scripts/vault-backup.sh"))
        .expect("reads vault-backup.sh");
    let vaulted: Vec<String> = listed(Tier::Local)
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    assert_eq!(assigned(&script, "VAULTED"), vaulted.join(" "));
}
