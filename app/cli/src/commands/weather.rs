//! `suzatary weather`: the one line the briefing reads.
//!
//! A warning about an untrusted feed rides on that stdout line, not stderr, where nobody sees it.

use std::path::Path;

use crate::instance;

/// `suzatary weather`.
pub fn run(path: &Path) -> anyhow::Result<()> {
    let instance = instance::Instance::open(path)?;
    println!(
        "{}",
        suzatary_io::weather::line(chrono::Utc::now(), &instance.persona)?
    );
    Ok(())
}
