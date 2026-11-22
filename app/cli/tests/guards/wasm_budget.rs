//! The shipped wasm modules' budget (`web/wasm-budget`), in bytes after wasm-opt: its own form.
//! `scripts/build-web.sh` refuses a module over its line; whether a line rose past what the
//! commits since the merge base declare is the train's tool's (`scripts/context.sh growth`), the
//! gate's history half, since the answer reads the commits and not the tree.

use std::collections::BTreeMap;

use super::repo;

const BUDGET: &str = "web/wasm-budget";

/// Every line is a multiple of this: its system's measured figure rounded up to the next one.
const STEP: u64 = 10_000;

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

#[test]
fn wasm_budget_every_module_has_a_rounded_line_per_system() {
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
    let unrounded: Vec<String> = now
        .iter()
        .filter(|(_, bytes)| *bytes % STEP != 0)
        .map(|((m, s), bytes)| format!("{m} {s} {bytes}"))
        .collect();
    assert!(
        unrounded.is_empty(),
        "{BUDGET}: lines not rounded up to a multiple of {STEP}:\n{}",
        unrounded.join("\n")
    );
}
