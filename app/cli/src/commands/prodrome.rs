//! `suzatary prodrome init`: a todo store begun at a path, such as a project's roadmap.

use std::path::Path;

use anyhow::{Context as _, bail};
use suzatary_store::store::Reads as _;

use crate::instance::charter::{self, Charter};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Subcommand)]
pub enum Action {
    /// Begin a todo store at a path, which `--store` then reads.
    Init {
        /// The directory to begin it in, made where absent.
        #[arg(long, value_name = "DIR")]
        store: std::path::PathBuf,
        /// Charter it public: everything it holds may be published.
        #[arg(long)]
        public: bool,
    },
}

pub fn run(args: &Args) -> anyhow::Result<()> {
    let Action::Init { store, public } = &args.action;
    init(
        store,
        if *public {
            Charter::Public
        } else {
            Charter::Private
        },
    )
}

/// Refuses a store begun under another charter, or none, so no object reaches a public store
/// unchartered. The `.gitignore` goes first, so the store keeps it in place of its own, and
/// names the store's derived files exactly: `flake.lock` beside them is committed.
fn init(dir: &Path, charter: Charter) -> anyhow::Result<()> {
    let begun = !super::chain_at(dir.to_owned(), prodrome_core::policy::Untrusted::none())
        .tips()?
        .is_empty();
    if (begun || dir.join(charter::FILE).exists()) && Charter::read(dir)? != charter {
        bail!(
            "{} is begun under another charter, which init does not rewrite",
            dir.display()
        );
    }
    std::fs::create_dir_all(dir)?;
    charter.write(dir)?;
    unless_there(&dir.join(".gitignore"), "/.heads\n/.lock\n")?;
    super::init::begin_chain(dir)?;
    unless_there(
        &dir.join("flake.nix"),
        include_str!("prodrome/roadmap.flake.nix"),
    )?;
    println!("began {} as {charter:?}", dir.display());
    Ok(())
}

fn unless_there(path: &Path, text: &str) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    suzatary_io::durable::replace(path, text.as_bytes(), suzatary_io::durable::Mode::Readable)
        .with_context(|| format!("writing {}", path.display()))
}
