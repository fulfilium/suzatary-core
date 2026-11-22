//! The context budget (`context.toml`). Its goal is that no unit is over `budget.use` or
//! `budget.develop`; the table names each unit still over and by how much. The test refuses a
//! unit's USE over its budget or its ratchet and a plan that no longer matches the code; a reading
//! set grown past what the commits since the merge base declare is the gate's history half
//! (`scripts/context.sh growth`), since its answer reads the commits. It prints every unit's figures and their change against
//! `origin/master`, so each gate's log shows what a change costs to read.

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
