//! `suzatary verify` — the stores' fsck.
//!
//! Every object rehashed, every parent present, every failure the old health registers hold
//! carried into the sources' one, one line per problem and a non-zero exit when there is one: `scripts/safe-push.sh` aborts on it before committing, since a bad
//! object in a Merkle log is history that must not reach the remote.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::bail;
use suzatary_core::chains::ChainName;
use suzatary_core::store::{Database, Kind};

use super::store_of;
use crate::commands::serve::interpreters::{file_store, upload_store};
use crate::instance::Instance;

/// `suzatary verify` — fsck every store the data push stages.
///
/// The database's own fsck, [`Database::verify`], over every prodrome it holds, and beside it
/// the request records, which the database does not hold. The plugins' stores are the cache
/// tier, never pushed and rebuilt by syncing, so a damaged one is no reason to hold the data
/// back.
pub fn run(path: &Path) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let mut findings: Vec<(String, Vec<String>)> = Vec::new();
    let stores: Vec<_> = instance
        .chains
        .iter()
        .map(|chain| (chain.policy.name.clone(), store_of(chain)))
        .collect();
    let inbox = instance.inbox_store();
    let conversations = instance.conversation_stores();
    let memory = instance.memory_store();
    let memory_texts = suzatary_io::texts::Texts::open(instance.secretary.memory_texts());
    let digests = instance.digest_store();
    let digest_texts = suzatary_io::texts::Texts::open(instance.secretary.digest_texts());
    let decisions = suzatary_io::objects::Objects::open(instance.decisions());
    let topics = instance.secretary.topics.open();
    let triage = suzatary_io::state::State::open(instance.mail.state.clone());
    let triage_local = suzatary_io::state::Local::open(instance.local.mail());
    let devices = suzatary_io::devices::Devices::open(instance.paths.devices.clone());
    let uploads = upload_store(&instance);
    let kept = file_store(&instance);
    let chains: BTreeMap<ChainName, &dyn suzatary_core::store::Prodrome<_>> = stores
        .iter()
        .map(|(name, store)| {
            (
                name.clone(),
                store as &dyn suzatary_core::store::Prodrome<_>,
            )
        })
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
        plugins: BTreeMap::new(),
        triage: &triage,
        triage_local: &triage_local,
        devices: &devices,
        uploads: &uploads,
        kept: &kept,
    };
    let requests =
        suzatary_io::requests::Requests::open(instance.requests(), instance.paths.answers.clone());
    findings.extend(database.verify());
    findings.push(("requests".to_owned(), requests.verify()));
    findings.push((
        "source health".to_owned(),
        super::sources::uncarried(&instance)?
            .iter()
            .map(|key| {
                format!(
                    "{key} fails in an old health register and not in this one: run `suzatary \
                     sources migrate-health` before the deploy"
                )
            })
            .collect(),
    ));

    // Where each store lives, by the name its findings carry, so an OK line says which
    // directory was read.
    let mut roots: BTreeMap<String, PathBuf> = Kind::ALL
        .iter()
        .flat_map(|kind| instance.directories(*kind))
        .collect();
    roots.insert(
        "source health".to_owned(),
        (instance.mail.heartbeat).with_file_name(suzatary_io::source_health::FILE),
    );

    let mut total = 0;
    for (what, problems) in &findings {
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
    bail!("{total} problem(s) across the stores the data push stages")
}
