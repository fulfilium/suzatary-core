//! `suzatary secretspec`: the file a terminal's `secretspec` reads this deployment's names from.

use std::path::Path;

use anyhow::Context as _;

use crate::instance::Instance;

/// Write and print the file declaring every secret name this instance may set, beside the code
/// root's own: what `secretspec set <NAME> --file` takes.
pub fn run(path: &Path) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let declared = instance
        .declarations
        .opened()
        .context("declaring this instance's secret names")?;
    println!("{}", declared.file().display());
    Ok(())
}
