//! `suzatary web OUTDIR` — the static attestable-history site.
//!
//! The chain's SECOND CONSUMER, written to a directory: every todo's lifecycle
//! with each event's object hash and the objects it rests on. READ-ONLY over
//! the chain, and the writes go nowhere but the output directory — the site is
//! a rendering, and nothing reads it back.

use std::path::{Path, PathBuf};

use anyhow::Context;
use suzatary_core::capability::Clock;
use suzatary_core::capability::RenderError;
use suzatary_core::rendering::SITE;
use suzatary_core::site;
use suzatary_core::site::page::SitePage;
use suzatary_io::clock::SystemClock;
use suzatary_io::render::Typst;

use crate::instance::Instance;

/// What `suzatary web` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Directory to write into (created if absent); generated output,
    /// never committed.
    outdir: PathBuf,
    /// WHICH CHAIN to render (docs/ARCHITECTURE.md §10.2). Absent is the FIRST
    /// chain the instance file names — `life`, the owner's own todos — so a
    /// command written before there were two means what it always did.
    #[arg(long, value_name = "NAME")]
    chain: Option<String>,
}

/// `suzatary web OUTDIR` — generate the static attestable-history site.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = &Instance::open(path)?;
    let outdir = args.outdir.as_path();
    let chain = instance.chain(args.chain.as_deref())?;
    let now = SystemClock.now();
    let read = super::chains_read(instance);
    let set = read.set(now);
    // The findings over the objects the set already read, so the site and its problems are one
    // read of the chain.
    let problems: Vec<String> = set.held(&chain.policy.name).map_or_else(
        |unread| vec![unread.to_string()],
        |held| {
            suzatary_core::signatures::since_epoch(held.dag, held.dag.verify(held.untrusted))
                .iter()
                .map(ToString::to_string)
                .collect()
        },
    );
    let files =
        site::render_site(&chain.policy.name, &set, &problems).context("rendering the site")?;
    let typst = Typst::new(
        instance.paths.markup_cache.clone(),
        suzatary_io::typeset::Closed::this_binary(),
    );
    let drawn = |page: &SitePage| {
        SITE.call(page)
            .map_err(|e| RenderError::Refused(e.to_string()))
            .and_then(|called| typst.rendered(&called))
            .map(|html| html.as_str().to_owned())
    };
    for (relative, page) in &files.pages {
        let html = match drawn(page) {
            Ok(html) => html,
            Err(RenderError::Suspicious(reason)) => {
                eprintln!("suzatary web: {relative} refused: {reason}");
                drawn(&SitePage::refused(relative, &reason, &files.footer))
                    .with_context(|| format!("rendering the refusal of {relative}"))?
            }
            Err(refused) => {
                return Err(refused).with_context(|| format!("rendering {relative}"));
            }
        };
        write(outdir, relative, &html)?;
    }
    for (relative, object) in &files.objects {
        write(outdir, relative, object)?;
    }
    let entries = set
        .entries(&chain.policy.name)
        .context("folding the chain")?;
    let objects = set
        .held(&chain.policy.name)
        .context("reading the chain")?
        .dag
        .objects();
    println!(
        "{}",
        site::summary(
            &outdir.display().to_string(),
            files.pages.len() + files.objects.len(),
            objects.len(),
            &entries,
            &problems,
            now,
        )
    );
    Ok(())
}

fn write(outdir: &Path, relative: &str, content: &str) -> anyhow::Result<()> {
    let path = outdir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}
