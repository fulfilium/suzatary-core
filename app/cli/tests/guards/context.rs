//! The context budget (`context.toml`): the test refuses a plan that no longer matches the code; a
//! unit's USE and every other derived figure is the gate's history half (`scripts/context.sh
//! growth`), against what the commits declare.

use super::repo;

#[test]
fn context_every_unit_is_within_its_budget() {
    let (table, findings) = suzatary_tooling::context::check(&repo()).expect("it measures");
    println!("{table}");
    let refused: Vec<String> = findings.iter().map(ToString::to_string).collect();
    assert!(
        refused.is_empty(),
        "the context budget refuses this tree:\n{}",
        refused.join("\n")
    );
}
