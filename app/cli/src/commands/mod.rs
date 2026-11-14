//! One module per verb family; this file holds only what several families share.

pub mod ask;
pub mod call;
pub mod coding;
pub mod context;
pub mod decision;
pub mod device;
pub mod doctor;
// The browser suite's fixtures: test support, compiled only under the `fixture` feature.
#[cfg(feature = "fixture")]
#[path = "../../tests/support/fixture.rs"]
pub mod fixture;
#[cfg(feature = "fixture")]
#[path = "../../tests/support/fixture_live.rs"]
pub mod fixture_live;
pub mod google;
pub mod guide;
pub mod heartbeat;
pub mod init;
pub mod instance;
pub mod leaks;
pub mod mail;
pub mod mcp;
pub mod notify;
pub mod outbound;
pub mod plugin;
pub mod ran;
pub mod release;
pub mod secretary;
pub mod secretspec;
pub mod serve;
pub mod tap;
pub mod todo;
pub mod todos;
pub mod typeset;
pub mod up;
pub mod vault;
pub mod verify;
pub mod weather;

use anyhow::Context;
use suzatary_io::objects::Objects;
use suzatary_record::record::EventStore;

use crate::instance::{ChainDir, Instance};

/// The store one chain writes to, with that same chain's trust policy.
pub(crate) fn store_of(chain: &ChainDir) -> Objects<suzatary_record::record::TodoEvent> {
    Objects::chain(EventStore::new(
        chain.events.clone(),
        chain.policy.untrusted.clone(),
    ))
}

/// Every chain this instance serves, each store read once: the set every row and proposal is
/// priced through.
#[must_use]
pub fn chains_read(instance: &Instance) -> suzatary_core::chain_set::Read {
    let stores: Vec<_> = instance
        .chains
        .iter()
        .map(|chain| (&chain.policy, store_of(chain)))
        .collect();
    suzatary_core::chain_set::Read::of_stores(stores.iter().map(|(policy, store)| {
        (
            *policy,
            store as &dyn suzatary_store::store::Prodrome<suzatary_record::record::TodoEvent>,
        )
    }))
}

/// One chain as the triage agent reads it: under its own policy, and never written.
pub(crate) fn reading_of(chain: &ChainDir) -> suzatary_io::reading::Reading {
    suzatary_io::reading::chain(chain.events.clone(), chain.policy.untrusted.clone())
}

/// Run one of the repository's operator scripts, and exit with its status.
///
/// `vault` is human-only and on no agent trait.
pub fn script(instance: &Instance, name: &str) -> anyhow::Result<()> {
    let path = instance.code().join("scripts").join(name);
    // The operator's whole environment, on purpose: the script reads what the shell set up
    // (the store, the agent, the vault's key).
    let status = std::process::Command::new("bash")
        .arg(&path)
        .status()
        .with_context(|| format!("running {}", path.display()))?;
    std::process::exit(status.code().unwrap_or(1));
}
