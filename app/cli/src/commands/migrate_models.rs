//! `suzatary secretary migrate-models [--dry]`: drop what this build no longer reads of the model
//! providers and the model orders, once.
//!
//! The inbox may hold cards that changed a model order, the reserve or a provider
//! (`ConfigureOrder`, `ConfigureReserve`, `ConfigureProvider`), the subscription's sign-in under
//! its old name (`ClaudeSignIn`), and cards that connected or removed the Anthropic API or the
//! subscription (`Connect` or `Disconnect` of the templates `anthropic-api` and
//! `claude-subscription`). No act of this build reads them, so this run removes each such proposal whole: its asking and every
//! response, children first, so a run stopped part way is finished by running it again. A change
//! rests only on its own proposal's heads, so nothing kept rests on a removed object; the run
//! refuses an inbox where something does.
//!
//! The model health (`model-health.json`, beside the heartbeat, box-local) may hold readings of
//! a provider's models; the run keeps only the `claude` CLI's.
//!
//! This module is the one reader of the old forms and goes with the verb once the box has run it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Context as _;
use prodrome_core::event::{EventVocabulary, Hash};
use prodrome_core::literal::{Signature, Table, Value, Vocabulary, parse_literal};
use suzatary_core::secretary::inbox_event::{InboxEvent, InboxVocabulary};
use suzatary_core::store::Prodrome as _;

use crate::instance::Instance;

/// The constructors a removed setting card was stored as, and the values they nest.
const OLD: &[(&str, &[&str])] = &[
    ("ConfigureOrder", &["job", "from", "to"]),
    ("ConfigureReserve", &["from", "to"]),
    ("ConfigureProvider", &["from", "to"]),
    ("DeclaredProvider", &["name", "base_url", "key", "expect"]),
    ("ProviderSetup", &["preset", "workspace", "expect"]),
    ("ClaudeSignIn", &["since"]),
];

/// The acts a removed card was stored as.
const CARDS: [&str; 4] = [
    "ConfigureOrder",
    "ConfigureReserve",
    "ConfigureProvider",
    "ClaudeSignIn",
];

/// The removed templates a Connect or Disconnect card may name.
const TEMPLATES: [&str; 2] = ["anthropic-api", "claude-subscription"];

/// The constructors an asking holds its act in.
const ASKINGS: [&str; 3] = ["Asked", "Noticed", "Offered"];

static INBOX: InboxVocabulary = InboxVocabulary;

/// The inbox's envelopes and events, and the old constructors beside them.
struct Carrying(EventVocabulary<'static, InboxEvent>);

impl Vocabulary for Carrying {
    fn signature(&self, name: &str) -> Option<Signature<'_>> {
        self.0.signature(name).or_else(|| Table(OLD).find(name))
    }
}

/// What a run does to the inbox.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    /// Each proposal removed, by its id.
    pub dropped: BTreeSet<String>,
    /// Its objects, children first, so the store never holds one whose parent is gone.
    pub removed: Vec<Hash>,
}

/// One stored change, read as a literal: the proposal its event names, and the names it rests
/// on.
struct Stored {
    proposal: Option<String>,
    deps: Vec<Hash>,
}

/// Whether `act`, an asking's stored act, is one no act of this build reads.
fn removed_act(act: &Value) -> bool {
    let Value::Call(call) = act else {
        return false;
    };
    match call.name.as_str() {
        name if CARDS.contains(&name) => true,
        "Connect" | "Disconnect" => {
            matches!(call.field("connection"), Some(Value::Call(connection))
                if connection
                    .field("template")
                    .and_then(Value::as_str)
                    .is_some_and(|template| TEMPLATES.contains(&template)))
        }
        _ => false,
    }
}

/// What a run does to `history`, the inbox's objects as stored, each its name and its text.
///
/// # Errors
/// An object that does not read, or a kept object resting on a removed one.
pub fn plan(history: &[(Hash, String)]) -> anyhow::Result<Plan> {
    let words = Carrying(EventVocabulary::new(&INBOX));
    let mut read: BTreeMap<&Hash, Stored> = BTreeMap::new();
    let mut dropped = BTreeSet::new();
    for (name, text) in history {
        let Value::Call(call) =
            parse_literal(text, &words).with_context(|| format!("object {name} does not read"))?
        else {
            anyhow::bail!("object {name} is not a constructor call");
        };
        let event = call.field("event").filter(|_| call.name == "Change");
        let proposal = event
            .and_then(|event| match event {
                Value::Call(event) => event.field("proposal").and_then(Value::as_str),
                _ => None,
            })
            .map(str::to_owned);
        if let (Some(Value::Call(event)), Some(proposal)) = (event, &proposal)
            && ASKINGS.contains(&event.name.as_str())
            && event.field("act").is_some_and(removed_act)
        {
            dropped.insert(proposal.clone());
        }
        let deps = match call.field("deps") {
            Some(Value::Tuple(deps)) => deps
                .iter()
                .map(|dep| {
                    dep.as_str()
                        .and_then(|dep| Hash::new(dep).ok())
                        .with_context(|| format!("object {name} rests on a non-name"))
                })
                .collect::<anyhow::Result<_>>()?,
            _ => Vec::new(),
        };
        read.insert(name, Stored { proposal, deps });
    }
    let gone = |stored: &Stored| {
        stored
            .proposal
            .as_ref()
            .is_some_and(|proposal| dropped.contains(proposal))
    };
    let mut left: BTreeSet<&Hash> = read
        .iter()
        .filter(|(_, stored)| gone(stored))
        .map(|(name, _)| *name)
        .collect();
    if let Some((kept, _)) = read
        .iter()
        .find(|(_, stored)| !gone(stored) && stored.deps.iter().any(|dep| left.contains(dep)))
    {
        anyhow::bail!("object {kept} is kept and rests on an object this run removes");
    }
    // Children first: each pass takes every removed object that no removed object left rests on.
    let mut removed = Vec::with_capacity(left.len());
    while !left.is_empty() {
        let leaves: Vec<&Hash> = left
            .iter()
            .copied()
            .filter(|name| {
                !left
                    .iter()
                    .any(|other| read[*other].deps.iter().any(|dep| dep == *name))
            })
            .collect();
        anyhow::ensure!(!leaves.is_empty(), "the removed objects rest on each other");
        for leaf in leaves {
            left.remove(leaf);
            removed.push(leaf.clone());
        }
    }
    Ok(Plan { dropped, removed })
}

/// Every object of the store at `root`, its name and its text, read off the files and not
/// through the inbox's reader, which leaves out what it cannot read.
fn history(root: &Path) -> anyhow::Result<Vec<(Hash, String)>> {
    let dir = root.join("objects");
    let entries = match std::fs::read_dir(&dir) {
        Err(failed) if failed.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        read => read.with_context(|| format!("reading {}", dir.display()))?,
    };
    let mut history = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".py"))
        else {
            continue;
        };
        let name = Hash::new(name).with_context(|| format!("{} is no object", path.display()))?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        history.push((name, text));
    }
    history.sort();
    Ok(history)
}

/// The inbox at `root` with `plan`'s objects removed, unless `dry`; the caller holds its lock.
///
/// # Errors
/// What [`plan`] refuses, and a file that cannot be removed.
pub fn inbox(root: &Path, dry: bool) -> anyhow::Result<Plan> {
    let plan = plan(&history(root)?)?;
    if !dry {
        let objects = root.join("objects");
        for name in &plan.removed {
            let path = objects.join(format!("{name}.py"));
            suzatary_io::durable::remove(&path)
                .with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(plan)
}

/// The model health's text with only the `claude` CLI's readings kept, and the keys dropped.
///
/// # Errors
/// A text that is not a JSON object.
pub fn health(text: &str) -> anyhow::Result<(String, Vec<String>)> {
    let mut readings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(text).context("the model health is not a JSON object")?;
    let cli = suzatary_io::model::name_of("");
    let dropped: Vec<String> = readings
        .keys()
        .filter(|key| !key.starts_with(&cli))
        .cloned()
        .collect();
    readings.retain(|key, _| key.starts_with(&cli));
    Ok((serde_json::to_string_pretty(&readings)?, dropped))
}

/// `suzatary secretary migrate-models [--dry]`: `dry` writes nothing and says what a run would.
/// Its run order stops the server and the timers first.
///
/// # Errors
/// What [`plan`] refuses, an inbox that does not verify after, and the stores' own.
pub fn run(instance: &Instance, dry: bool) -> anyhow::Result<()> {
    let store = instance.inbox_store();
    let held = store.locked()?;
    let root = store.inner().root().to_path_buf();
    let plan = inbox(&root, dry)?;
    for proposal in &plan.dropped {
        println!("inbox: removed proposal {proposal}");
    }
    println!(
        "inbox: {} proposal(s), {} object(s){}",
        plan.dropped.len(),
        plan.removed.len(),
        if dry { ", nothing written" } else { " removed" }
    );
    drop(held);
    if !dry && !plan.removed.is_empty() {
        let findings = suzatary_core::secretary::inbox::findings(&instance.inbox_store());
        for finding in &findings {
            println!("inbox: {finding}");
        }
        anyhow::ensure!(
            findings.is_empty(),
            "the inbox does not verify after the run"
        );
    }
    let path = instance
        .mail
        .heartbeat
        .with_file_name(suzatary_io::model_health::FILE);
    let text = match std::fs::read_to_string(&path) {
        Err(failed) if failed.kind() == std::io::ErrorKind::NotFound => {
            println!("model health: none at {}", path.display());
            return Ok(());
        }
        read => read.with_context(|| format!("reading {}", path.display()))?,
    };
    let (kept, dropped) = health(&text)?;
    for key in &dropped {
        println!("model health: dropped {key}");
    }
    if !dry && !dropped.is_empty() {
        suzatary_io::durable::replace(&path, kept.as_bytes(), suzatary_io::durable::Mode::Kept)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}
