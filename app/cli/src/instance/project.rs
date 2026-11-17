//! The plan store: the one mount beside whose objects a `releases.toml` names the genesis that
//! store holds. A deployment knows the project's releases only by finding that file there.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use suzatary_base::chains::ChainName;
use suzatary_core::release::Plan;

/// The file beside a store that declares its releases.
pub(crate) const PLAN: &str = "releases.toml";

/// Where a deployment mounts each chain's store, in the instance file's order.
#[derive(Debug, Clone, Default)]
pub struct Mounts(pub Vec<(ChainName, PathBuf)>);

/// The mount whose store holds the release plan's genesis, and the plan.
#[derive(Debug, Clone)]
pub struct Project {
    pub chain: ChainName,
    pub store: PathBuf,
    pub plan: Plan,
}

/// The plan beside `store`, or `None` where it has no `releases.toml`.
///
/// # Errors
/// The file unreadable, not a plan, or naming a genesis `store` does not hold: a plan copied
/// beside another store is not that store's.
pub fn beside(store: &Path) -> anyhow::Result<Option<Plan>> {
    let path = store.join(PLAN);
    let text = match std::fs::read_to_string(&path) {
        Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        read => read.with_context(|| format!("reading {}", path.display()))?,
    };
    let plan = Plan::parse(&text).with_context(|| path.display().to_string())?;
    let genesis = store
        .join("objects")
        .join(format!("{}.py", plan.genesis().as_str()));
    let held = std::fs::read_to_string(&genesis).is_ok_and(|object| object.starts_with("Genesis("));
    if !held {
        bail!(
            "{} names the genesis {}, which the store beside it does not hold",
            path.display(),
            plan.genesis().as_str()
        );
    }
    Ok(Some(plan))
}

impl Mounts {
    /// The plan store, `None` where no mount holds a plan.
    ///
    /// # Errors
    /// A plan that does not read, or two mounts holding one.
    pub fn project(&self) -> anyhow::Result<Option<Project>> {
        let mut found: Option<Project> = None;
        for (chain, store) in &self.0 {
            let Some(plan) = beside(store)? else {
                continue;
            };
            if let Some(first) = &found {
                bail!(
                    "chains {} and {chain} both hold a release plan, and a deployment counts one \
                     project's releases",
                    first.chain
                );
            }
            found = Some(Project {
                chain: chain.clone(),
                store: store.clone(),
                plan,
            });
        }
        Ok(found)
    }
}
