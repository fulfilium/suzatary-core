//! `suzatary migrate-ignores`: each store's `.gitignore` brought to its present form
//! (`suzatary_io::ignores`).

use std::path::{Path, PathBuf};

use anyhow::bail;
use suzatary_io::ignores::{self, Lacks};

use super::store_at::StoreAt;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Say what each `.gitignore` lacks, and write nothing.
    #[arg(long)]
    check: bool,
    #[command(flatten)]
    store: StoreAt,
}

/// Every store of objects of the instance, or the store `--store` names, one line each. Ends
/// non-zero unless every store is settled ([`Lacks::settled`]): with `--check`, where any store
/// lacks a line; else where any is left alone in a form no push commits, or as a link.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let stores: Vec<(String, PathBuf)> = args.store.or_instance(
        path,
        |dir| Ok(vec![("store".to_owned(), dir.to_owned())]),
        |instance| {
            Ok(instance
                .object_stores()
                .into_iter()
                .map(|(_, name, root)| (name, root))
                .collect())
        },
    )?;
    let step = if args.check {
        ignores::lacks
    } else {
        ignores::bring
    };
    let mut unsettled = Vec::new();
    for (name, root) in stores {
        let lacked = step(&root)
            .map_err(|error| anyhow::anyhow!("{name}: {}/.gitignore: {error}", root.display()))?;
        let said = match (&lacked, args.check) {
            (Lacks::NoStore, _) => "no objects/, so no store; left alone".to_owned(),
            (Lacks::NoFile, _) => {
                "no .gitignore; the store writes its own at its next write".to_owned()
            }
            (Lacks::Linked, _) => ".gitignore is a symbolic link; left alone".to_owned(),
            (Lacks::Nothing, _) => "names every local file".to_owned(),
            (Lacks::Lines(lines), true) => format!("lacks {}", lines.join(" ")),
            (Lacks::Lines(lines), false) => format!("added {}", lines.join(" ")),
            (Lacks::Other(lines), _) if lines.is_empty() => {
                "in a form no push commits; left alone: look at it".to_owned()
            }
            (Lacks::Other(lines), _) => format!(
                "lacks {}, and would then be in a form no push commits; left alone: look at it",
                lines.join(" ")
            ),
        };
        println!("{name}: {said} — {}", root.display());
        let settled = match (&lacked, args.check) {
            (Lacks::Lines(_), false) => true,
            _ => lacked.settled(),
        };
        if !settled {
            unsettled.push(name);
        }
    }
    if !unsettled.is_empty() {
        bail!("not settled: {}", unsettled.join(", "));
    }
    Ok(())
}
