//! `--store <DIR>`, one flag on `decision`, `todo show`, `todos open` and `verify`.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use prodrome_core::policy::Untrusted;
use suzatary_base::chains::ChainName;
use suzatary_signatures::chain_set::Read;
use suzatary_store::store::Reads as _;

use crate::instance::Instance;

#[derive(Debug, Clone, clap::Args)]
pub struct StoreAt {
    /// The store at this directory, read with no instance file; absent is the instance's.
    #[arg(long = "store", global = true, value_name = "DIR")]
    store: Option<PathBuf>,
}

impl StoreAt {
    /// `by_path` of the named directory, else `mounted` of the instance file at `path`, which is
    /// opened only then.
    pub(crate) fn or_instance<T>(
        &self,
        path: &Path,
        by_path: impl FnOnce(&Path) -> anyhow::Result<T>,
        mounted: impl FnOnce(Instance) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        match &self.store {
            Some(dir) => by_path(dir),
            None => mounted(Instance::open(path)?),
        }
    }

    /// Every chain read once, and the name of the one read: the instance's `chain` (else its
    /// first) beside the others, or the path's store alone, believing every writer.
    pub(crate) fn todos(
        &self,
        path: &Path,
        chain: Option<&str>,
    ) -> anyhow::Result<(Read, ChainName)> {
        self.or_instance(
            path,
            |dir| {
                if let Some(name) = chain {
                    bail!("--chain {name} names a chain of an instance file; --store reads none");
                }
                let name = ChainName::new("store")?;
                let objects = super::chain_at(dir.to_owned(), Untrusted::none())
                    .objects()
                    .context("reading the store")?;
                Ok((
                    Read::of_objects([(name.clone(), Untrusted::none(), objects)]),
                    name,
                ))
            },
            |instance| {
                let name = instance.chain(chain)?.policy.name.clone();
                Ok((super::chains_read(&instance), name))
            },
        )
    }
}
