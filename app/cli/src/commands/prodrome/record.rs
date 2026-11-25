//! `suzatary prodrome record`: each store's heads recorded under its lock after a program changed
//! its `objects/` by a door the store does not own (a `git pull` or merge, a copy, a restore), so
//! that its record of heads and the mark of its last hold say what it holds (SPEC §3, law 62).

use std::path::{Path, PathBuf};

use anyhow::bail;
use prodrome_data::schema::Schema;
use suzatary_io_chains::objects::Objects;
use suzatary_io_chains::plugin_store::PluginStore;
use suzatary_store::store::Reads as _;
use suzatary_stores::stores::Kind;

use crate::commands::store_at::StoreAt;

/// Which stores a run records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Opened {
    Every,
    /// Those whose kind opens them as their program's alone (`Kind::doors`).
    Alone,
}

impl Opened {
    fn takes(self, kind: Kind) -> bool {
        match (self, kind.doors()) {
            (Opened::Every, _) | (Opened::Alone, prodrome_store::store::Doors::Alone) => true,
            (Opened::Alone, prodrome_store::store::Doors::Shared) => false,
        }
    }
}

/// The todo store `--store` names, refused where it does not read as one; else every store of
/// the instance ([`crate::instance::Instance::object_stores`]) at its kind's schema, of those
/// `opened` takes. One line each; a store that fails is said and the rest are recorded, and the
/// run then ends non-zero.
pub(crate) fn run(path: &Path, at: &StoreAt, opened: Opened) -> anyhow::Result<()> {
    let mut stores = at.or_instance(
        path,
        |dir| {
            if opened == Opened::Alone {
                bail!(
                    "--store names a todo store, which is opened as shared; --opened alone \
                     records the instance's stores opened as alone"
                );
            }
            let store =
                super::super::chain_at(dir.to_owned(), prodrome_data::policy::Untrusted::none());
            let unread = store
                .excluded()
                .map_err(|refused| anyhow::anyhow!("{refused}"))?;
            if !unread.is_empty() {
                bail!(
                    "{} holds {} objects a todo store does not read: another store's, which the \
                     instance names (run it without --store), or a damaged one (`suzatary verify`)",
                    dir.display(),
                    unread.len()
                );
            }
            Ok(vec![(Kind::Chain, "store".to_owned(), dir.to_owned())])
        },
        |instance| Ok(instance.object_stores()),
    )?;
    stores.retain(|(kind, _, _)| opened.takes(*kind));
    let failed: Vec<String> = every(&stores)
        .into_iter()
        .filter_map(|(name, said)| match said {
            Ok(said) => {
                println!("{name}: {said}");
                None
            }
            Err(error) => {
                eprintln!("{name}: {error}");
                Some(name)
            }
        })
        .collect();
    if !failed.is_empty() {
        bail!("no heads recorded for {}", failed.join(", "));
    }
    Ok(())
}

/// What recording each store did, by its name: a store never begun (no `objects/`) is said and
/// left as it is.
fn every(stores: &[(Kind, String, PathBuf)]) -> Vec<(String, anyhow::Result<String>)> {
    stores
        .iter()
        .map(|(kind, name, root)| (name.clone(), recorded(*kind, root)))
        .collect()
}

fn recorded(kind: Kind, root: &Path) -> anyhow::Result<String> {
    if !root.join("objects").is_dir() {
        return Ok(format!(
            "never begun, nothing to record — {}",
            root.display()
        ));
    }
    match kind {
        Kind::Chain => heads(&super::super::chain_at(
            root.to_owned(),
            prodrome_data::policy::Untrusted::none(),
        )),
        Kind::Inbox => {
            heads(&Objects::<suzatary_inbox::secretary::inbox_event::InboxEvent>::open(root))
        }
        Kind::Lines => {
            heads(&Objects::<suzatary_lines::secretary::line_event::LineEvent>::open(root))
        }
        Kind::Memory => heads(&Objects::<
            suzatary_memory::secretary::memory_event::MemoryEvent,
        >::open(root)),
        Kind::Digests => heads(&Objects::<
            suzatary_digest::secretary::digest_event::DigestEvent,
        >::open(root)),
        Kind::Decisions => heads(&Objects::<suzatary_todo::decisions::DecisionEvent>::open(
            root,
        )),
        Kind::Topics => heads(&Objects::<
            suzatary_topics::secretary::topics::reading::MembershipEvent,
        >::open(root)),
        Kind::TopicAliases => heads(&Objects::<
            suzatary_topics::secretary::topics::reading::AliasEvent,
        >::open(root)),
        Kind::Asks => heads(&Objects::<suzatary_stores::asks::AskEvent>::open(root)),
        // Under the sync's hold, so the record never lands while a sync at another schema
        // empties the store.
        Kind::Plugin => match PluginStore::at(root).read_held()? {
            Some(begun) => heads(begun.store()),
            None => Ok(format!(
                "no schema beside it, nothing to record — {}",
                root.display()
            )),
        },
        Kind::LineKeys
        | Kind::MemoryTexts
        | Kind::DigestTexts
        | Kind::Triage
        | Kind::Local
        | Kind::Uploads
        | Kind::Kept
        | Kind::Requests => bail!("{} is no store of objects", root.display()),
    }
}

fn heads<E: Schema>(store: &Objects<E>) -> anyhow::Result<String> {
    let root = store.inner().root();
    let heads = store
        .inner()
        .record_heads()
        .map_err(|refused| anyhow::anyhow!("{}: {refused}", root.display()))?;
    Ok(format!(
        "{} objects, {} heads recorded — {}",
        heads.count(),
        heads.tips().len(),
        root.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every store is recorded at its own schema, a store never begun is said and left as it is,
    /// a plugin with no store is said, and a store that fails is said without stopping the rest.
    #[test]
    fn every_store_is_recorded_and_a_failure_stops_none_of_the_rest() {
        let dir = suzatary_testkit::scratch("record");
        let chain = dir.path().join("events");
        let begun = suzatary_record::record::EventStore::new(
            &chain,
            prodrome_data::policy::Untrusted::none(),
        );
        begun
            .init(suzatary_record::record::GENESIS)
            .expect("begins");
        let broken = dir.path().join("inbox");
        std::fs::create_dir_all(&broken).expect("a directory");
        std::fs::write(broken.join("objects"), "not a directory").expect("written");
        let broken_objects = dir.path().join("asks");
        std::fs::create_dir_all(broken_objects.join("objects")).expect("a store");
        std::fs::create_dir_all(broken_objects.join(".heads")).expect("a directory in its place");
        let stores = [
            (Kind::Asks, "asks".to_owned(), broken_objects.clone()),
            (
                Kind::Decisions,
                "decisions".to_owned(),
                dir.path().join("decisions"),
            ),
            (Kind::Inbox, "inbox".to_owned(), broken),
            (
                Kind::Plugin,
                "plugin ics".to_owned(),
                dir.path().join("cache/plugins/ics/store"),
            ),
            (Kind::Chain, "life".to_owned(), chain.clone()),
        ];
        std::fs::create_dir_all(dir.path().join("cache/plugins/ics/store/objects"))
            .expect("a store with no schema");
        let said = every(&stores);
        let line = |name: &str| {
            said.iter()
                .find(|(at, _)| at == name)
                .map(|(_, said)| match said {
                    Ok(said) => said.clone(),
                    Err(error) => format!("ERR {error}"),
                })
                .expect("a line")
        };
        assert!(line("asks").starts_with("ERR "), "{}", line("asks"));
        assert!(
            line("decisions").starts_with("never begun"),
            "{}",
            line("decisions")
        );
        assert!(!dir.path().join("decisions").exists(), "nothing written");
        assert!(
            line("inbox").starts_with("never begun"),
            "{}",
            line("inbox")
        );
        assert!(
            line("plugin ics").starts_with("no schema"),
            "{}",
            line("plugin ics")
        );
        assert!(
            line("life").starts_with("1 objects, 1 heads recorded"),
            "{}",
            line("life")
        );
        assert_eq!(said.len(), 5);
        let recorded = prodrome_dag::dag::Heads::parse(
            &std::fs::read_to_string(chain.join(".heads")).expect("a record"),
        )
        .expect("parses");
        assert_eq!(recorded.count(), 1);
    }
}
