//! `suzatary vault backup`: an encrypted state snapshot, human-only and on no agent trait.

use std::path::Path;

use crate::instance::Instance;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Snapshot the box's state, encrypted, to Proton Drive.
    Backup,
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    match args.action {
        Action::Backup => super::script(&Instance::open(path)?, "vault-backup.sh"),
    }
}
