//! The context budget (`context.toml`). Its goal is that no unit is over `budget.use` or
//! `budget.develop`; the table names each unit still over and by how much. The test refuses a
//! unit's USE over its budget or its ratchet, a reading set grown past the ratchet, and a plan
//! that no longer matches the code. It prints every unit's figures and their change against
//! `origin/master`, so each gate's log shows what a change costs to read.

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
