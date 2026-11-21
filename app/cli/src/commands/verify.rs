//! `suzatary verify`: the stores' fsck.
//!
//! One line per problem and a non-zero exit when there is one: `scripts/safe-push.sh` aborts on
//! it before committing, since a bad object in a Merkle log must not reach the remote.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::bail;
use suzatary_base::chains::ChainName;
use suzatary_stores::stores::{Database, Kind};

use super::store_at::StoreAt;
use super::store_of;
use crate::instance::Instance;
use crate::interpreters::{file_store, upload_store};
use prodrome_core::event::Hash;
use suzatary_io::objects::Objects;
use suzatary_io::texts::Texts;
use suzatary_lines::first_read;
use suzatary_lines::secretary::prune;
use suzatary_store::store::{Prodrome, Reads as _};

/// What `suzatary verify` checks.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Check instead each line this directory holds, `<name>.py` as the last commit held it,
    /// against the lines' store: the commit held the whole its name hashes, the store keeps that
    /// whole's husk, and no key opens a word of it, now or later.
    #[arg(long, conflicts_with = "store")]
    husked: Option<std::path::PathBuf>,
    #[command(flatten)]
    store: StoreAt,
}

/// Fsck the todo store `--store` names, or every store the data push stages, or check the lines
/// husked since the last commit.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    args.store.or_instance(
        path,
        |dir| {
            let store = super::chain_at(dir.to_owned(), prodrome_core::policy::Untrusted::none());
            let what = Kind::Chain.name().to_owned();
            reported(
                &[(what.clone(), store.verify())],
                &BTreeMap::from([(what, dir.to_owned())]),
            )
        },
        |instance| match &args.husked {
            Some(was) => husked(&instance, was),
            None => every_store(&instance),
        },
    )
}

/// The plugins' stores are cache tier, never pushed, so they are not checked.
fn every_store(instance: &Instance) -> anyhow::Result<()> {
    let mut findings: Vec<(String, Vec<String>)> = Vec::new();
    let stores: Vec<_> = instance
        .chains
        .iter()
        .map(|chain| (chain.policy.name.clone(), store_of(chain)))
        .collect();
    let inbox = instance.inbox_store();
    let conversations = instance.conversation_stores();
    let memory = instance.memory_store();
    let memory_texts = Texts::open(instance.secretary.memory_texts());
    let digests = instance.digest_store();
    let digest_texts = Texts::open(instance.secretary.digest_texts());
    let decisions = Objects::open(instance.decisions());
    let topics = instance.secretary.topics.open();
    let asks = Objects::open(instance.asks());
    let triage = suzatary_io::state::State::open(instance.mail.state.clone());
    let triage_local = suzatary_io::state::Local::open(instance.local.mail());
    let devices = suzatary_io::devices::Devices::open(instance.paths.devices.clone());
    let uploads = upload_store(instance);
    let kept = file_store(instance);
    let chains: BTreeMap<ChainName, &dyn Prodrome<_>> = stores
        .iter()
        .map(|(name, store)| (name.clone(), store as &dyn Prodrome<_>))
        .collect();
    let database = Database {
        chains,
        inbox: &inbox,
        conversations: &conversations.lines,
        line_keys: &conversations.keys,
        memory: &memory,
        memory_texts: &memory_texts,
        digests: &digests,
        digest_texts: &digest_texts,
        decisions: &decisions,
        topics: &topics.memberships,
        topic_aliases: &topics.aliases,
        asks: &asks,
        plugins: BTreeMap::new(),
        triage: &triage,
        triage_local: &triage_local,
        devices: &devices,
        uploads: &uploads,
        kept: &kept,
    };
    let requests =
        suzatary_io::requests::Requests::open(instance.requests(), instance.paths.answers.clone());
    let stray = first_read::first("stores", || {
        findings.extend(database.verify());
        prune::stray(conversations.conversations())
    })
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    if let Some((_, lines)) = findings.iter_mut().find(|(what, _)| what == "lines") {
        lines.extend(stray);
    }
    findings.push(("requests".to_owned(), requests.verify()));

    let roots: BTreeMap<String, PathBuf> = Kind::ALL
        .iter()
        .flat_map(|kind| instance.directories(*kind))
        .collect();
    reported(&findings, &roots)
}

/// Each store's findings, and an OK line naming its root for a store with none; an error when
/// there is a finding.
fn reported(
    findings: &[(String, Vec<String>)],
    roots: &BTreeMap<String, PathBuf>,
) -> anyhow::Result<()> {
    let mut total = 0;
    for (what, problems) in findings {
        total += problems.len();
        if problems.is_empty() {
            match roots.get(what) {
                Some(root) => println!("{what} OK — {}", root.display()),
                None => println!("{what} OK"),
            }
        }
        for problem in problems {
            eprintln!("{what}: {problem}");
        }
    }
    if total == 0 {
        return Ok(());
    }
    bail!("{total} problem(s) across the stores checked")
}

/// `verify --husked <was>`.
fn husked(instance: &Instance, was: &Path) -> anyhow::Result<()> {
    let stores = instance.conversation_stores();
    let objects = stores.lines.inner().root().join("objects");
    let mut committed = Vec::new();
    for file in std::fs::read_dir(was)? {
        let file = file?.path();
        let name = file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        committed.push((Hash::new(name)?, std::fs::read(&file)?));
    }
    let now = |name: &Hash| std::fs::read(objects.join(format!("{}.py", name.as_str()))).ok();
    let findings = first_read::first("husked lines", || {
        prune::admitted(stores.conversations(), &committed, now)
    })
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    for finding in &findings {
        println!("husked FAIL — {finding}");
    }
    if !findings.is_empty() {
        bail!("{} line(s) are not the husks of dead lines", findings.len());
    }
    println!("husked OK — {} line(s)", committed.len());
    Ok(())
}
