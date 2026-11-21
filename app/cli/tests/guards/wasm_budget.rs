//! The shipped wasm modules' budget (`web/wasm-budget`), in bytes after wasm-opt:
//! `scripts/build-web.sh` refuses a module over its line, and this refuses a line raised against
//! the merge base with `origin/master` by more than [`ALLOWANCE`], unless a commit since the merge
//! base declares the growth in a `Wasm-growth: <module> +<bytes>: <reason>` trailer, read as git reads
//! trailers. A pull request says it on such a line of its description, which `scripts/train.sh`
//! writes into its merge commit; a branch may say it in a trailer of a commit of its own. So a module grows past the allowance only by
//! a change that says why where a reviewer reads it.

use std::collections::BTreeMap;
use std::process::Command;

use super::repo;

const BUDGET: &str = "web/wasm-budget";

/// The bytes a line may rise by against the merge base without a word, per module and system: a
/// feature's code, not a new dependency. More than [`STEP`], since a line moves by a whole
/// step when its module crosses a multiple of one.
const ALLOWANCE: u64 = 16_000;

/// Every line is a multiple of this: its system's measured figure rounded up to the next one.
const STEP: u64 = 10_000;

/// The `Wasm-growth:` trailers of `messages`, read as git reads trailers (a line of prose that
/// begins with the key is none): what each declares a module may grow by, summed over the
/// trailers naming it. A trailer naming no number of bytes, or no reason, fails here, so a
/// declaration is never read as something it does not say.
fn grants(messages: &[&str]) -> BTreeMap<String, u64> {
    let mut grants = BTreeMap::new();
    let values = messages.iter().flat_map(|message| {
        suzatary_tooling::git::trailers("Wasm-growth", message).expect("git reads trailers")
    });
    for said in values {
        let line = format!("Wasm-growth: {said}");
        let (Some((name, bytes)), reason) = (
            said.split_once(':')
                .and_then(|(head, _)| head.trim().split_once(" +")),
            said.split_once(':').map_or("", |(_, why)| why.trim()),
        ) else {
            panic!("`{line}` is not `Wasm-growth: <module> +<bytes>: <reason>`");
        };
        let bytes: u64 = bytes
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("`{line}`: `{bytes}` is not a number of bytes"));
        assert!(!reason.is_empty(), "`{line}` gives no reason");
        *grants.entry(name.trim().to_owned()).or_insert(0) += bytes;
    }
    grants
}

/// Each line raised past the allowance and past what the grants declare for its module.
fn refused(
    was: &BTreeMap<(String, String), u64>,
    now: &BTreeMap<(String, String), u64>,
    grants: &BTreeMap<String, u64>,
) -> Vec<String> {
    was.iter()
        .filter_map(|((name, system), &before)| {
            let after = *now.get(&(name.clone(), system.clone()))?;
            let granted = grants.get(name).copied().unwrap_or(0);
            (after > before + ALLOWANCE.max(granted)).then(|| {
                format!(
                    "{name} {system}: {before} → {after}, {} over the {ALLOWANCE}-byte allowance{}",
                    after - before - ALLOWANCE.min(after - before),
                    if granted == 0 {
                        String::new()
                    } else {
                        format!(" and past the {granted} bytes declared")
                    }
                )
            })
        })
        .collect()
}

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
    let Some(base) = git(&["merge-base", "HEAD", "origin/master"]) else {
        eprintln!("skipped: this checkout has no origin/master to compare with");
        return;
    };
    // A merge base before the budget existed holds no line to rise above.
    let Some(was) = git(&["show", &format!("{}:{BUDGET}", base.trim())]) else {
        return;
    };
    let messages =
        git(&["log", "--format=%B%x00", &format!("{}..HEAD", base.trim())]).unwrap_or_default();
    let messages: Vec<&str> = messages.split('\0').map(str::trim_start).collect();
    let raised = refused(&lines(&was), &now, &grants(&messages));
    assert!(
        raised.is_empty(),
        "{BUDGET} rises past what this change declares (a `Wasm-growth: <module> +<bytes>: <reason>` \
         line in the pull request's description, or in a commit):\n{}",
        raised.join("\n")
    );
}

#[test]
fn wasm_budget_a_line_rises_by_the_allowance_or_by_what_is_declared() {
    let line = |name: &str, bytes| ((name.to_owned(), "x86_64-linux".to_owned()), bytes);
    let was = BTreeMap::from([
        line("views_bg.wasm", 490_000),
        line("prodrome_bg.wasm", 1_100_000),
    ]);
    let within = BTreeMap::from([
        line("views_bg.wasm", 490_000 + ALLOWANCE),
        line("prodrome_bg.wasm", 1_090_000),
    ]);
    assert!(
        refused(&was, &within, &BTreeMap::new()).is_empty(),
        "the allowance, and a fall"
    );
    let past = BTreeMap::from([
        line("views_bg.wasm", 490_000 + ALLOWANCE + 1),
        line("prodrome_bg.wasm", 1_100_000),
    ]);
    assert_eq!(
        refused(&was, &past, &BTreeMap::new()).len(),
        1,
        "a byte past it"
    );
    let said = grants(&["Merge #1: x\n\nWasm-growth: views_bg.wasm +19000: master's topic PRs\n"]);
    let grown = BTreeMap::from([
        line("views_bg.wasm", 509_000),
        line("prodrome_bg.wasm", 1_100_000),
    ]);
    assert!(refused(&was, &grown, &said).is_empty(), "declared");
    let more = BTreeMap::from([
        line("views_bg.wasm", 509_001),
        line("prodrome_bg.wasm", 1_100_000),
    ]);
    assert_eq!(
        refused(&was, &more, &said).len(),
        1,
        "past what was declared"
    );
    assert!(
        refused(
            &was,
            &grown,
            &grants(&["x\n\nWasm-growth: prodrome_bg.wasm +19000: another\n"])
        )
        .len()
            == 1,
        "a declaration is its module's alone"
    );
}

#[test]
#[should_panic(expected = "gives no reason")]
fn wasm_budget_a_declaration_without_a_reason_is_refused() {
    grants(&["x\n\nWasm-growth: views_bg.wasm +19000:\n"]);
}

/// A line of a message's prose that begins with the key is no trailer, so it declares nothing and
/// fails nothing: the body of a commit that wraps a sentence onto `Wasm-growth:`.
#[test]
fn wasm_budget_a_line_of_prose_is_no_declaration() {
    let prose = "train: a change\n\nIts growth lines ride its merge as trailers beside\nWasm-growth:. After the last merge the build runs.\n";
    assert!(grants(&[prose]).is_empty());
}
