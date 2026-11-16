//! `suzatary prodrome init|carry|draw`: a todo store begun at a path, such as a project's
//! roadmap, the upkeep chain's project todos carried to it, and its open list drawn.

mod carry;
mod list;
mod picture;

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
    /// Carry the upkeep chain's open todos that the carried list carries, and its releases, to a
    /// public store. Without `--apply` it prints what it would write, to stdout alone.
    Carry(Carrying),
    /// Draw a public store's open list as `roadmap.svg` and `roadmap-dark.svg`, and write the
    /// `README.md` that shows them, beside the store; or print, for door 2, what they are as
    /// drawn now.
    Draw(picture::Drawing),
}

/// What `prodrome carry` takes.
#[derive(Debug, clap::Args)]
pub struct Carrying {
    /// The public store, begun with `prodrome init --public`.
    #[arg(long, value_name = "DIR")]
    to: std::path::PathBuf,
    /// The carried list; absent is the box-local tier's `carried-list.toml`.
    #[arg(long, value_name = "FILE")]
    list: Option<std::path::PathBuf>,
    /// The name the deployment will mount the public store as, which each successor's
    /// reference names.
    #[arg(long, value_name = "NAME", default_value = "roadmap")]
    named: String,
    /// Who restates and cancels; the upkeep chain's owner when absent.
    #[arg(long)]
    actor: Option<String>,
    /// Write it: the destination first, then each source's `Cancelled`.
    #[arg(long)]
    apply: bool,
}

pub fn run(path: &std::path::Path, args: &Args) -> anyhow::Result<()> {
    match &args.action {
        Action::Init { store, public } => init(
            store,
            if *public {
                Charter::Public
            } else {
                Charter::Private
            },
        ),
        Action::Carry(carrying) => carry::run(path, carrying),
        Action::Draw(drawing) => picture::run(drawing),
    }
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
    unless_there(&dir.join(".gitattributes"), ATTRIBUTES)?;
    super::init::begin_chain(dir)?;
    unless_there(
        &dir.join("flake.nix"),
        include_str!("prodrome/roadmap.flake.nix"),
    )?;
    println!("began {} as {charter:?}", dir.display());
    Ok(())
}

/// How git keeps a store's files: an object is named by the SHA-256 of its bytes, so git never
/// rewrites one; the pictures `draw` writes are drawn again at each push, so a merge keeps either
/// side as bytes and the next draw replaces it.
const ATTRIBUTES: &str = "objects/** -text -diff\n/roadmap.svg -text -diff -merge\n\
                          /roadmap-dark.svg -text -diff -merge\n";

fn unless_there(path: &Path, text: &str) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }
    suzatary_io::durable::replace(path, text.as_bytes(), suzatary_io::durable::Mode::Readable)
        .with_context(|| format!("writing {}", path.display()))
}
