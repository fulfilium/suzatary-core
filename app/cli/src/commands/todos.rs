//! `suzatary todos open`: the one list of every store the deployment reads, which the briefing
//! and the agent's dedup read; an unconfirmed triage-authored todo is marked where it stands,
//! never moved or sunk.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::bail;
use clap::Subcommand;
use prodrome_core::literal::Datetime;
use suzatary_base::chains::ChainName;
use suzatary_base::reference::GlobalTodo;
use suzatary_plan::listed::{ListRow, Package, Store, Units, Unresolved, Urgency, listed};
use suzatary_plan::wait::{Held, Kept, Missing};
use suzatary_signatures::chain_set::{ChainSet, home};
use suzatary_signatures::chain_set::{NotAMember, Read};
use suzatary_todo::shown::Priority;

use super::at::At;
use super::store_at::StoreAt;
use crate::instance::{Charted, Copied, Instance, Since, Tree};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
    /// Only this store's rows; absent is every store, chains and followed stores alike.
    #[arg(long, global = true, value_name = "NAME")]
    chain: Option<String>,
    #[command(flatten)]
    store: StoreAt,
    #[command(flatten)]
    at: At,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Every open todo, ascending in its reading under the waits — LOW MEANS URGENT.
    Open {
        /// Print the rows as JSON instead of as a list.
        #[arg(long)]
        json: bool,
    },
}

struct Listing {
    read: Read,
    stores: Vec<(ChainName, Kept)>,
    packages: Packages,
    follows: Vec<(ChainName, Copied)>,
}

/// What a row's package is checked against, said once for the whole list.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
enum Packages {
    /// The store that holds the release plan, against the units of the code root's
    /// `context.toml`.
    Checked {
        chain: ChainName,
        #[serde(skip)]
        units: BTreeSet<String>,
    },
    /// Every row's package is as written, for this reason.
    Unchecked { why: String },
}

/// A declared store that is absent is reported and the list succeeds; one that is there and does
/// not read, or the `--chain` store not read, fails the command after the rows that did read.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let read = args.store.or_instance(
        path,
        |dir| {
            if let Some(name) = &args.chain {
                bail!("--chain {name} names a chain of an instance file; --store reads none");
            }
            let (read, name) = super::store_at::by_path(dir)?;
            let packages = packages(
                &crate::instance::Mounts(vec![(name.clone(), dir.to_owned())]),
                None,
            );
            Ok(Listing {
                read,
                stores: vec![(name, Kept::Followed)],
                packages,
                follows: Vec::new(),
            })
        },
        |instance| Ok(of_instance(&instance)),
    )?;
    if let Some(name) = &args.chain
        && !read.stores.iter().any(|(chain, _)| chain.as_str() == name)
    {
        let names: Vec<&str> = read
            .stores
            .iter()
            .map(|(chain, _)| chain.as_str())
            .collect();
        bail!(
            "no store named {name:?}; the list reads {}",
            names.join(", ")
        );
    }
    match args.action {
        Action::Open { json } => open_todos(&read, args.chain.as_deref(), args.at.or_now(), json),
    }
}

/// Every store the instance reads, chains then followed stores, each with whose it is.
pub(crate) fn kept(instance: &Instance) -> Vec<(ChainName, Kept)> {
    let chains = instance.chains.iter().map(|chain| {
        let kept = match chain.charter {
            Charted::Private => Kept::Private,
            Charted::Public(_) => Kept::Public,
        };
        (chain.policy.name.clone(), kept)
    });
    let followed = instance
        .follows
        .iter()
        .map(|followed| (followed.name.clone(), Kept::Followed));
    chains.chain(followed).collect()
}

fn of_instance(instance: &Instance) -> Listing {
    Listing {
        read: super::plan_read(instance),
        stores: kept(instance),
        packages: packages(&instance.mounts(), Some(instance.code())),
        follows: instance
            .follows
            .iter()
            .map(|followed| (followed.name.clone(), followed.copy()))
            .collect(),
    }
}

/// The units of `code`'s `context.toml` for the mount that holds the release plan, or why the
/// packages go unchecked, which fails no reading.
fn packages(mounts: &crate::instance::Mounts, code: Option<&Path>) -> Packages {
    let unchecked = |why: String| Packages::Unchecked { why };
    match mounts.project() {
        Err(why) => unchecked(format!("the release plan does not read: {why:#}")),
        Ok(None) => unchecked("no chain holds a release plan".to_owned()),
        Ok(Some(project)) => match code.map(suzatary_tooling::context::read_plan) {
            Some(Ok(plan)) => Packages::Checked {
                chain: project.chain,
                units: plan.units.into_iter().map(|unit| unit.name).collect(),
            },
            Some(Err(why)) => unchecked(format!("the code root's units do not read: {why:#}")),
            None => unchecked("a store read by its path has no code root".to_owned()),
        },
    }
}

fn open_todos(read: &Listing, only: Option<&str>, at: Datetime, json: bool) -> anyhow::Result<()> {
    let set = read.read.set(at);
    let stores: Vec<Store<'_>> = read
        .stores
        .iter()
        .map(|(chain, kept)| Store {
            chain,
            kept: *kept,
            units: match &read.packages {
                Packages::Checked {
                    chain: project,
                    units,
                } if project == chain => Units::Listed(units),
                Packages::Checked { .. } | Packages::Unchecked { .. } => Units::Unknown,
            },
        })
        .collect();
    let list = listed(&set, &stores);
    let failed = list.failed;
    let unresolved: Vec<Unresolved> = list
        .unresolved
        .into_iter()
        .filter(|wait| only.is_none_or(|name| wait.waiter.chain.as_str() == name))
        .collect();
    let rows: Vec<ListRow> = list
        .rows
        .into_iter()
        .filter(|row| only.is_none_or(|name| row.chain.as_str() == name))
        .collect();
    // An absent store is a followed one no fetch confirmed, which `follows` reports, unless
    // `--chain` names it.
    let named = |why: &NotAMember| only.is_some_and(|name| why.chain().as_str() == name);
    let broken: Vec<&NotAMember> = failed
        .iter()
        .filter(|why| !matches!(why, NotAMember::Absent(_)) || named(why))
        .collect();
    if only.is_some() {
        for why in broken.iter().filter(|why| !named(why)) {
            eprintln!("not read: {why}");
        }
    }
    if json {
        let failed: Vec<_> = broken
            .iter()
            .map(|why| serde_json::json!({ "chain": why.chain(), "why": why.to_string() }))
            .collect();
        let follows: Vec<_> = read
            .follows
            .iter()
            .map(|(chain, copy)| serde_json::json!({ "chain": chain, "copy": copy }))
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "open": rows,
                "failed": failed,
                "follows": follows,
                "unresolved": unresolved,
                "packages": read.packages,
            })
        );
    } else {
        for (chain, copy) in &read.follows {
            eprintln!("{chain}: followed, {}", copied(copy));
        }
        for wait in &unresolved {
            eprintln!("{}", lost(wait));
        }
        if let Packages::Unchecked { why } = &read.packages {
            eprintln!("packages unchecked: {why}");
        }
        for row in rows {
            println!("{}", line(&row));
        }
    }
    match only {
        Some(_) => {
            if let Some(why) = broken.iter().find(|why| named(why)) {
                bail!("{why}");
            }
        }
        None => {
            if !broken.is_empty() {
                let said: Vec<String> = broken.iter().map(ToString::to_string).collect();
                bail!("not every store read:\n{}", said.join("\n"));
            }
        }
    }
    Ok(())
}

/// A followed store's copy, in words.
fn copied(copy: &Copied) -> String {
    let tree = match &copy.tree {
        Tree::Never => "never fetched".to_owned(),
        Tree::Confirmed { commit, at } => format!("fetched {at} at {commit}"),
        Tree::Unsettled { commit, at } => format!(
            "fetched {at} at {commit}, then a fetch stopped while it changed the copy, which is \
             not read until a fetch confirms it"
        ),
    };
    match &copy.since {
        Since::Nothing => tree,
        Since::Refused { at, why } => format!("{tree}; refused since, {at}: {why}"),
    }
}

/// A wait the list could not follow, in words.
fn lost(wait: &Unresolved) -> String {
    format!(
        "{} waits on {}, which moves nothing: {}",
        wait.waiter,
        wait.reference,
        why(&wait.missing)
    )
}

/// Why a reference moves no task held here, in words.
pub(crate) fn why(missing: &Missing) -> String {
    match missing {
        Missing::Unheld => "no store this deployment holds is the one it names".to_owned(),
        Missing::Ambiguous { geneses } => format!(
            "its genesis prefix starts {} held geneses ({}); write more of it",
            geneses.len(),
            geneses.join(", ")
        ),
        Missing::Replicas { stores } => {
            let names: Vec<&str> = stores.iter().map(ChainName::as_str).collect();
            format!(
                "the stores {} begin with its genesis, and not one alone of them is this \
                 deployment's own chain",
                names.join(", ")
            )
        }
        Missing::Private => {
            "it names a task of a private chain, and words anyone may write move none".to_owned()
        }
    }
}

/// The stores of `stores` that `set` read, as a wait reads them.
pub(crate) fn held(set: &ChainSet<'_>, stores: &[(ChainName, Kept)]) -> Held {
    stores.iter().fold(Held::default(), |held, (name, kept)| {
        match set.held(name).map(|store| home(store.dag)) {
            Ok(Ok(Some(prodrome))) => held.with(name.clone(), prodrome, *kept),
            Ok(Ok(None) | Err(_)) | Err(_) => held,
        }
    })
}

/// One row as plain text, which the briefing and the agent's dedup read.
fn line(row: &ListRow) -> String {
    let own = match &row.row.priority {
        Priority::Percent { percent } => percent.clone(),
        Priority::Absent => "∅".to_owned(),
        Priority::Unlinked => "—".to_owned(),
    };
    let (price, raised) = match &row.reading {
        Urgency::Own => (own, String::new()),
        Urgency::Raised { percent, by } => {
            (percent.clone(), format!("  (own {own}; {by} waits on it)"))
        }
    };
    let package = match &row.package {
        Package::Unit(name) | Package::Unchecked(name) => name.clone(),
        Package::Unlisted(name) => format!("{name}?"),
    };
    let mark = if row.row.unconfirmed {
        " (unconfirmed)"
    } else {
        ""
    };
    let name = GlobalTodo {
        genesis: row.name.genesis.shortened(),
        id: row.name.id.clone(),
    };
    format!(
        "{price:>7}  {:<10} {package:<10} {:<40}{mark} {}{raised}",
        row.chain.as_str(),
        name.to_string(),
        row.row.body
    )
}
