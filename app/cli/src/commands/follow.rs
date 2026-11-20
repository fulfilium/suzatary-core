//! `suzatary follow fetch`: bring each followed store's copy to its upstream, the one command
//! that writes a followed store.

use std::path::Path;

use anyhow::bail;

use crate::instance::Instance;

/// What `suzatary follow` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Subcommand)]
enum Action {
    /// Fetch every `[[follows]]` store, or the one named, fast-forward only.
    Fetch {
        /// Only this store.
        name: Option<String>,
    },
}

/// `suzatary follow`.
///
/// # Errors
/// A name the instance does not follow, or any store whose fetch was refused, all named.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    match &args.action {
        Action::Fetch { name } => {
            let chosen: Vec<_> = instance
                .follows
                .iter()
                .filter(|followed| name.as_deref().is_none_or(|n| followed.name.as_str() == n))
                .collect();
            if let Some(name) = name
                && chosen.is_empty()
            {
                bail!("{} follows no store named {name:?}", path.display());
            }
            let now = suzatary_base::capability::Clock::now(&suzatary_io::clock::SystemClock);
            let refused: Vec<String> = chosen
                .into_iter()
                .filter_map(|followed| {
                    followed
                        .fetch(now)
                        .err()
                        .map(|why| format!("{}: {why:#}", followed.name))
                })
                .collect();
            if refused.is_empty() {
                Ok(())
            } else {
                bail!(
                    "not every followed store was fetched:\n{}",
                    refused.join("\n")
                )
            }
        }
    }
}
