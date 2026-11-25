//! `suzatary prodrome init|draw|pages|texts|record`: a todo store begun at a path, such as a project's
//! roadmap, its open list drawn, the store written as static pages, and what door 1 reads of its
//! objects written out for door 2.

mod pages;
mod picture;
mod record;

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
    /// Draw a public store's open list as `roadmap.svg` and `roadmap-dark.svg`, and write the
    /// `README.md` that shows them, beside the store; or print, for door 2, what they are as
    /// drawn now.
    Draw(picture::Drawing),
    /// Write a store as static pages: the open list and each todo's page, at one instant.
    Pages(pages::Paging),
    /// Record each store's heads under its lock after a program changed its `objects/` by
    /// another door: a `git pull` or merge, a copy, a restore. Run after any of them; `verify`
    /// names a store left behind (its record of heads is stale).
    Record {
        #[command(flatten)]
        store: super::store_at::StoreAt,
        /// Every store, or only those opened as their program's alone, whose heads the data
        /// push records by itself where `verify` finds them stale.
        #[arg(long, value_enum, default_value_t = record::Opened::Every)]
        opened: record::Opened,
    },
    /// For door 2: write what door 1 reads of each object file (`door::Reading`) beside it as
    /// `<object>.texts`, and print each object one of whose markup fields does not draw.
    Texts {
        /// Object files of a store, each one printed object.
        #[arg(required = true, value_name = "OBJECT")]
        objects: Vec<std::path::PathBuf>,
    },
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    match &args.action {
        Action::Init { store, public } => init(
            store,
            if *public {
                Charter::Public
            } else {
                Charter::Private
            },
        ),
        Action::Draw(drawing) => picture::run(drawing),
        Action::Pages(paging) => pages::run(paging),
        Action::Texts { objects } => texts(objects),
        Action::Record { store, opened } => record::run(path, store, *opened),
    }
}

fn texts(objects: &[std::path::PathBuf]) -> anyhow::Result<()> {
    let typeset = suzatary_io_typeset::typeset::Closed::this_binary();
    for path in objects {
        let literal =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let envelope = prodrome_data::event::parse_envelope(
            &suzatary_record::record::TodoVocabulary::default(),
            &literal,
        )
        .map_err(|e| anyhow::anyhow!("{} is no object of a todo store: {e}", path.display()))?;
        let Some(event) = envelope.event() else {
            continue;
        };
        let reading = super::door::Reading::of(event, &typeset);
        let mut texts = String::new();
        for (_, text) in &reading.texts {
            texts.push_str(text);
            texts.push('\n');
        }
        let mut beside = path.clone().into_os_string();
        beside.push(".texts");
        suzatary_os::durable::replace(
            Path::new(&beside),
            texts.as_bytes(),
            suzatary_os::durable::Mode::Readable,
        )
        .with_context(|| format!("writing {}", Path::new(&beside).display()))?;
        if reading.undrawn.is_some() {
            println!("{}", path.display());
        }
    }
    Ok(())
}

/// Refuses a store begun under another charter, or none, so no object reaches a public store
/// unchartered. The `.gitignore` goes first, so the store keeps it in place of its own, and
/// names the store's derived files exactly: `flake.lock` beside them is committed, and a public
/// store's `_site`, where its Pages workflow draws it.
fn init(dir: &Path, charter: Charter) -> anyhow::Result<()> {
    let begun = !super::chain_at(dir.to_owned(), prodrome_data::policy::Untrusted::none())
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
    unless_there(
        &dir.join(".gitignore"),
        match charter {
            Charter::Public => suzatary_io_chains::ignores::PUBLIC_CHARTER,
            Charter::Private => suzatary_io_chains::ignores::PRIVATE_CHARTER,
        },
    )?;
    unless_there(&dir.join(".gitattributes"), ATTRIBUTES)?;
    super::init::begin_chain(dir)?;
    unless_there(
        &dir.join("flake.nix"),
        include_str!("prodrome/roadmap.flake.nix"),
    )?;
    // Only a public store is published: the workflow publishes every page of it.
    if charter == Charter::Public {
        unless_there(
            &dir.join(".github/workflows/pages.yml"),
            include_str!("prodrome/roadmap.pages.yml"),
        )?;
    }
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
    suzatary_os::durable::replace(path, text.as_bytes(), suzatary_os::durable::Mode::Readable)
        .with_context(|| format!("writing {}", path.display()))
}
