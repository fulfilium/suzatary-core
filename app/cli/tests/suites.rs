//! Every test file runs: each crate's integration tests are one binary, `tests/all.rs`, with
//! test discovery off, so a file that `all.rs` does not declare, and that is not a target of its
//! own in the crate's manifest, would compile into nothing and pass by saying nothing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli/.. is the workspace")
        .to_path_buf()
}

/// The `.rs` files directly under `dir`.
fn files(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
                .filter_map(|path| Some(path.file_name()?.to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn declared(all: &str) -> BTreeSet<String> {
    all.lines()
        .filter_map(|line| line.trim().strip_prefix("#[path = \""))
        .filter_map(|rest| rest.strip_suffix("\"]"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_test_file_is_a_module_of_all_or_a_target_of_its_own() {
    let mut crates = 0;
    for entry in std::fs::read_dir(workspace()).expect("the workspace lists") {
        let dir = entry.expect("an entry").path();
        let tests = files(&dir.join("tests"));
        let Ok(manifest) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        if tests.is_empty() {
            continue;
        }
        crates += 1;
        let manifest: toml::Table = manifest.parse().expect("the manifest parses");
        let targets: BTreeSet<String> = manifest
            .get("test")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|target| target.get("path")?.as_str())
            .filter_map(|path| path.strip_prefix("tests/"))
            .map(str::to_owned)
            .collect();
        if !targets.contains("all.rs") {
            for file in &tests {
                assert!(
                    targets.contains(file),
                    "{}/tests/{file} is not a target, and there is no all.rs to hold it",
                    dir.display()
                );
            }
            continue;
        }
        assert_eq!(
            manifest["package"]
                .get("autotests")
                .and_then(toml::Value::as_bool),
            Some(false),
            "{}: with `all.rs`, discovery would build every file twice",
            dir.display()
        );
        let all = std::fs::read_to_string(dir.join("tests/all.rs")).expect("all.rs reads");
        let declared = declared(&all);
        for file in &tests {
            assert!(
                declared.contains(file) || targets.contains(file),
                "{}/tests/{file} is neither a module of all.rs nor a target of its own, so it never runs",
                dir.display()
            );
        }
        for file in &declared {
            assert!(
                tests.contains(file),
                "{}/tests/all.rs declares {file}, which is not there",
                dir.display()
            );
        }
    }
    assert!(crates >= 5, "found the tests of only {crates} crates");
}

/// Every `.rs` file under `dir`, at any depth.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .flat_map(|path| {
            if path.is_dir() {
                sources(&path)
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// A test runs the binary only through `binary.rs`, whose runs start from a clean environment:
/// a spawn anywhere else would inherit the units' `SUZATARY_REPO` and read the box's own stores.
#[test]
fn binary_spawns_only_here() {
    let needle = concat!("CARGO_BIN_EXE_", "suzatary");
    let tests = workspace().join("cli").join("tests");
    let elsewhere: Vec<String> = sources(&tests)
        .into_iter()
        .filter(|path| path.file_name().is_none_or(|name| name != "binary.rs"))
        .filter(|path| std::fs::read_to_string(path).is_ok_and(|text| text.contains(needle)))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        elsewhere.is_empty(),
        "these run the binary outside tests/binary.rs, with this process's environment: {elsewhere:?}"
    );
    assert!(
        std::fs::read_to_string(tests.join("binary.rs")).is_ok_and(|text| text.contains(needle)),
        "tests/binary.rs runs the binary"
    );
}
