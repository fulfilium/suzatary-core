//! One module per verb family; this file holds only what several families share.

pub mod ask;
pub(crate) mod at;
pub mod call;
pub mod coding;
pub mod context;
pub mod decision;
pub mod device;
pub mod doctor;
pub mod door;
pub mod follow;
// The browser suite's fixtures, test support.
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
pub mod migrate_ignores;
pub mod notify;
pub mod outbound;
pub mod plugin;
pub mod prodrome;
pub mod ran;
pub mod release;
pub mod secretary;
pub mod secretspec;
pub mod serve;
pub(crate) mod store_at;
pub mod tap;
pub mod todo;
pub mod todos;
pub mod typeset;
pub mod up;
pub mod vault;
pub mod verify;
pub mod weather;

use anyhow::Context;
use suzatary_io_chains::objects::Objects;
use suzatary_record::record::EventStore;

use crate::instance::{ChainDir, Instance};

pub(crate) fn store_of(chain: &ChainDir) -> Objects<suzatary_record::record::TodoEvent> {
    opened(chain, door::Opened::ByTheBox)
}

/// The store of one chain opened from `opened`, with that same chain's trust policy and, where
/// its charter is public, door 1.
pub(crate) fn opened(
    chain: &ChainDir,
    opened: door::Opened,
) -> Objects<suzatary_record::record::TodoEvent> {
    let store = chain_at(chain.events.clone(), chain.policy.untrusted.clone());
    match &chain.charter {
        crate::instance::Charted::Private => store,
        crate::instance::Charted::Public(lists) => store.public(
            door::Door::of(
                lists.clone(),
                &chain.policy,
                opened,
                std::sync::Arc::new(suzatary_io_typeset::typeset::Closed::this_binary()),
            )
            .rule(),
        ),
    }
}

pub(crate) fn chain_at(
    events: std::path::PathBuf,
    untrusted: prodrome_data::policy::Untrusted,
) -> Objects<suzatary_record::record::TodoEvent> {
    Objects::chain(EventStore::new(events, untrusted))
}

/// Every chain this instance serves, each store read once: the set every row and proposal is
/// priced through.
#[must_use]
pub fn chains_read(instance: &Instance) -> suzatary_signatures::chain_set::Read {
    let stores: Vec<_> = instance
        .chains
        .iter()
        .map(|chain| (&chain.policy, store_of(chain)))
        .collect();
    suzatary_signatures::chain_set::Read::of_stores(stores.iter().map(|(policy, store)| {
        (
            *policy,
            store as &dyn suzatary_store::store::Prodrome<suzatary_record::record::TodoEvent>,
        )
    }))
}

/// [`chains_read`] with every followed store beside the chains; a copy no fetch has confirmed is
/// not held, so the set answers it as absent.
#[must_use]
pub fn plan_read(instance: &Instance) -> suzatary_signatures::chain_set::Read {
    use suzatary_store::store::Reads as _;
    instance
        .follows
        .iter()
        .filter(|followed| followed.copy().held())
        .fold(chains_read(instance), |read, followed| {
            let objects = followed
                .reading()
                .and_then(|reading| reading.objects().map_err(|failed| failed.to_string()));
            read.with(followed.name.clone(), followed.untrusted.clone(), objects)
        })
}

/// One chain as the triage agent reads it: under its own policy, and never written.
pub(crate) fn reading_of(chain: &ChainDir) -> suzatary_io_chains::reading::Reading {
    suzatary_io_chains::reading::chain(chain.events.clone(), chain.policy.untrusted.clone())
}

/// Run one of the repository's operator scripts; `vault` is human-only and on no agent trait.
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
