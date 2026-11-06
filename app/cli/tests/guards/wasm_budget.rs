//! The shipped wasm modules' brotli budget (`web/wasm-budget`) only falls: `scripts/build-web.sh`
//! refuses a module over its line, and this refuses a line raised against the merge base with
//! `origin/master`, so a module can grow only by a change that says so where a reviewer reads it.

use std::collections::BTreeMap;
use std::process::Command;

use super::repo;

const BUDGET: &str = "web/wasm-budget";

/// The systems the gate and the box build for, each of which needs a line for every module.
const SYSTEMS: [&str; 2] = ["x86_64-linux", "aarch64-linux"];

/// Each line by its module and system: `<module>.wasm <system> <bytes>`. A line that is not that
/// fails here, so the script's lookup never reads a malformed one.
fn lines(text: &str) -> BTreeMap<(String, String), u64> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut words = l.split_whitespace();
            let (Some(name), Some(system), Some(bytes), None) =
                (words.next(), words.next(), words.next(), words.next())
            else {
                panic!("{BUDGET}: `{l}` is not `<module>.wasm <system> <bytes>`");
            };
            assert!(
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|e| e == "wasm"),
                "{BUDGET}: `{name}` is not a wasm module"
            );
            assert!(
                SYSTEMS.contains(&system),
                "{BUDGET}: `{system}` is not one of {SYSTEMS:?}"
            );
            let bytes = bytes
                .parse()
                .unwrap_or_else(|_| panic!("{BUDGET}: `{bytes}` is not a number of bytes"));
            ((name.to_owned(), system.to_owned()), bytes)
        })
        .collect()
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo())
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn wasm_budget_no_module_s_line_rises() {
    let now = lines(&std::fs::read_to_string(repo().join(BUDGET)).expect("the budget is tracked"));
    assert!(!now.is_empty(), "{BUDGET} names no module");
    let modules: std::collections::BTreeSet<&String> = now.keys().map(|(m, _)| m).collect();
    let missing: Vec<String> = modules
        .iter()
        .flat_map(|m| SYSTEMS.iter().map(move |s| (*m, *s)))
        .filter(|(m, s)| !now.contains_key(&((*m).clone(), (*s).to_owned())))
        .map(|(m, s)| format!("{m} {s}"))
        .collect();
    assert!(
        missing.is_empty(),
        "{BUDGET} has no line for:\n{}",
        missing.join("\n")
    );
    let Some(base) = git(&["merge-base", "HEAD", "origin/master"]) else {
        eprintln!("skipped: this checkout has no origin/master to compare with");
        return;
    };
    // A merge base before the budget existed holds no line to rise above.
    let Some(was) = git(&["show", &format!("{}:{BUDGET}", base.trim())]) else {
        return;
    };
    let raised: Vec<String> = lines(&was)
        .into_iter()
        .filter_map(|((name, system), before)| {
            let after = *now.get(&(name.clone(), system.clone()))?;
            (after > before).then(|| format!("{name} {system}: {before} → {after}"))
        })
        .collect();
    assert!(
        raised.is_empty(),
        "{BUDGET} may only fall:\n{}",
        raised.join("\n")
    );
}
