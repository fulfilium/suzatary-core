//! The context budget (`context.toml`): no unit's USE is over its budget, the repository's
//! reading set does not grow past the ratchet, and the plan matches the code. The test prints
//! every unit's figures and their change against `origin/master`, so each gate's log shows what a
//! change costs to read.

use super::repo;

#[test]
fn context_every_unit_is_within_its_budget_and_the_reading_set_within_the_ratchet() {
    let (table, findings) = suzatary_cli::commands::context::check(&repo()).expect("it measures");
    println!("{table}");
    let refused: Vec<String> = findings.iter().map(ToString::to_string).collect();
    assert!(
        refused.is_empty(),
        "the context budget refuses this tree:\n{}",
        refused.join("\n")
    );
}
