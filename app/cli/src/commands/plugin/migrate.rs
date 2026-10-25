//! `suzatary plugin migrate`: the instance file's `[[plugins]]` tables and each plugin's ledger,
//! from their shapes before the source world's 0.4.0 to today's.
//!
//! A table loses `world` and `from = "app"`, an actor's table goes (no host runs one), and each
//! `{ name, into = "url" }` credential becomes its name. A ledger loses `admitted` and its last
//! run's `refusals`, `log` and `dropped`. Every other byte of the file stays, and running it
//! again changes nothing.
//!
//! The old binary and the new one each refuse the other's shapes, so a run that rewrites the
//! tables stops there, and the ledgers are rewritten by a run on a file already in today's shape.
//! The order: run it on a copy of the live instance file, deploy with `scripts/deploy.sh <rev>
//! --instance <copy>`, then run it on the live file, which then rewrites the ledgers alone.

use std::path::Path;

use anyhow::{Context, bail};
use suzatary_core::plugin::Ledger;
use suzatary_io::durable;
use suzatary_io::present::Present as _;
use toml_edit::{Array, DocumentMut, Item, TableLike, Value};

use crate::instance::Instance;

pub(super) fn run(path: &Path) -> anyhow::Result<()> {
    {
        let _held = crate::instance::editing(path)?;
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading the instance file {}", path.display()))?;
        let written = plugins(&text)?;
        if written != text {
            durable::replace(path, written.as_bytes(), durable::Mode::Readable)
                .with_context(|| format!("writing {}", path.display()))?;
            println!(
                "plugin migrate: rewrote [[plugins]] in {}; the ledgers wait for a run on the \
                 instance the new app reads",
                path.display()
            );
            return Ok(());
        }
    }
    let instance = Instance::open(path)?;
    for manifest in &instance.plugins.manifests {
        let file = instance.plugins.beside(manifest.name()).join("ledger.json");
        let Some(bytes) = std::fs::read(&file).present()? else {
            continue;
        };
        let migrated =
            ledger(&bytes).with_context(|| format!("migrating the ledger {}", file.display()))?;
        if migrated != bytes {
            durable::replace(&file, &migrated, durable::Mode::Readable)
                .with_context(|| format!("writing {}", file.display()))?;
            println!("plugin migrate: rewrote {}", file.display());
        }
    }
    Ok(())
}

/// The instance file's text with every `[[plugins]]` table in today's shape.
fn plugins(text: &str) -> anyhow::Result<String> {
    let mut doc: DocumentMut = text.parse().context("parsing the instance file")?;
    if let Some(plugins) = doc
        .get_mut("plugins")
        .and_then(Item::as_array_of_tables_mut)
    {
        plugins.retain(|table| {
            let actor = world(table).is_some_and(|world| world.starts_with("actor@"));
            if actor {
                println!("plugin migrate: removed the actor {}", name(table));
            }
            !actor
        });
        for table in plugins.iter_mut() {
            table_of(table)?;
        }
    }
    Ok(doc.to_string())
}

/// A ledger's bytes in today's shape, read back as a [`Ledger`] before they are returned.
fn ledger(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut ledger: serde_json::Value = serde_json::from_slice(bytes)?;
    if let Some(held) = ledger.as_object_mut() {
        held.remove("admitted");
    }
    if let Some(last) = ledger
        .get_mut("last")
        .and_then(serde_json::Value::as_object_mut)
    {
        for gone in ["refusals", "log", "dropped"] {
            last.remove(gone);
        }
    }
    let read: Ledger = serde_json::from_value(ledger)?;
    Ok(serde_json::to_vec_pretty(&read)?)
}

fn world(table: &toml_edit::Table) -> Option<&str> {
    table.get("world").and_then(Item::as_str)
}

fn name(table: &toml_edit::Table) -> &str {
    table
        .get("name")
        .and_then(Item::as_str)
        .unwrap_or("with no name")
}

/// One source's table in today's shape.
fn table_of(table: &mut toml_edit::Table) -> anyhow::Result<()> {
    let plugin = name(table).to_owned();
    table.remove("world");
    match table.get("from").and_then(Item::as_str) {
        None => {}
        Some("app") => {
            table.remove("from");
        }
        Some(from) => bail!(
            "the plugin {plugin} is read from {from:?}: move its component and schema beside the \
             others and remove `from` and its pins by hand"
        ),
    }
    let credentials: Vec<&dyn TableLike> = match table.get("credentials") {
        None => return Ok(()),
        Some(Item::ArrayOfTables(list)) => list.iter().map(|one| one as &dyn TableLike).collect(),
        Some(Item::Value(Value::Array(list))) if list.iter().all(Value::is_str) => return Ok(()),
        Some(Item::Value(Value::Array(list))) => list
            .iter()
            .filter_map(Value::as_inline_table)
            .map(|one| one as &dyn TableLike)
            .collect(),
        Some(_) => bail!("the plugin {plugin}'s credentials are not a list"),
    };
    let mut links = Array::new();
    for credential in credentials {
        let named = credential.get("name").and_then(Item::as_str);
        match (named, credential.get("into").and_then(Item::as_str)) {
            (Some(named), Some("url")) => links.push(named),
            (named, into) => bail!(
                "the plugin {plugin}'s credential {named:?} goes into {into:?}, and only a link \
                 (`into = \"url\"`) is a credential now"
            ),
        }
    }
    table.insert("credentials", toml_edit::value(links));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shapes the box holds today migrate to the shapes the host reads, every other byte
    /// kept, and migrating again changes nothing.
    #[test]
    fn the_old_shapes_migrate_once_into_todays() {
        let before = r#"# kept
[[plugins]]
name = "ics"
component = "ics.wasm"
schema = "ics.schema" # kept too
world = "source@0.3.0"
origins = ["https://calendar.google.com"]

[[plugins.credentials]]
name = "PLUGIN_ICS_HOME"
into = "url"

[[plugins]]
name = "due"
component = "due.wasm"
schema = "ics.schema"
world = "actor@0.3.0"
reads = "ics"

[[plugins]]
name = "other"
from = "app"
component = "other.wasm"
schema = "other.schema"
world = "source@0.3.0"
credentials = [{ name = "PLUGIN_OTHER_A", into = "url" }]
"#;
        let after = r#"# kept
[[plugins]]
name = "ics"
component = "ics.wasm"
schema = "ics.schema" # kept too
origins = ["https://calendar.google.com"]
credentials = ["PLUGIN_ICS_HOME"]

[[plugins]]
name = "other"
component = "other.wasm"
schema = "other.schema"
credentials = ["PLUGIN_OTHER_A"]
"#;
        let migrated = plugins(before).expect("migrates");
        assert_eq!(migrated, after);
        assert_eq!(plugins(&migrated).expect("migrates"), migrated);
        for refused in [
            before.replace("from = \"app\"", "from = \"user\""),
            before.replace("into = \"url\"\n", "into = \"bearer\"\n"),
        ] {
            assert!(plugins(&refused).is_err(), "{refused}");
        }

        let old = br#"{"last":{"at":"2026-10-05T10:00:00","came":{"learnt":{"summary":"s"}},"refusals":[],"log":["Info: x"],"dropped":0},"links":{},"admitted":{"at":"2026-10-05T10:00:00","component":"a","schema":"b"}}"#;
        let migrated = ledger(old).expect("migrates");
        let read: Ledger = serde_json::from_slice(&migrated).expect("reads");
        assert_eq!(
            read.last.map(|last| last.came),
            Some(suzatary_core::plugin::Came::Learnt {
                summary: "s".to_owned()
            })
        );
        assert_eq!(ledger(&migrated).expect("migrates"), migrated);
    }
}
