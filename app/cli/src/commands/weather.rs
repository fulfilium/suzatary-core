//! `suzatary weather`: the one line the briefing reads, which carries any warning about an
//! untrusted feed, since nobody sees stderr.

use std::path::Path;

use crate::instance;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let instance = instance::Instance::open(path)?;
    println!(
        "{}",
        suzatary_io::weather::line(chrono::Utc::now(), &instance.persona)?
    );
    Ok(())
}
