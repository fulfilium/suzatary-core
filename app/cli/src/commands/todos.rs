//! `suzatary todos open`: the open list the briefing and the agent's dedup read.
//!
//! An unconfirmed triage-authored todo is marked where it stands, never moved or sunk.

use std::path::Path;

use anyhow::Context;
use clap::Subcommand;
use suzatary_base::capability::Clock;
use suzatary_base::escape::{markup_text, string_text};
use suzatary_core::believed::one_outcome;
use suzatary_core::record::Entry;
use suzatary_io::clock::SystemClock;

use crate::instance::{ChainDir, Instance};

/// Which reading of the todo list.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Which reading.
    #[command(subcommand)]
    action: Action,
    /// WHICH CHAIN to read. Absent is the FIRST
    /// chain the instance file names — `life`, the owner's own todos — which is
    /// what the briefing reads and what this verb has always meant.
    #[arg(long, value_name = "NAME")]
    chain: Option<String>,
}

/// WHICH READING of the todo list. One so far, and a subcommand rather than a
/// bare verb because `todos` is a family and the next reading joins it here.
#[derive(Debug, Subcommand)]
pub enum Action {
    /// Every open todo, ascending in fulfillment — LOW MEANS URGENT.
    Open {
        /// Print the rows as JSON instead of as a list.
        #[arg(long)]
        json: bool,
    },
}

/// `suzatary todos <reading>`.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let chain = instance.chain(args.chain.as_deref())?;
    match args.action {
        Action::Open { json } => open_todos(&instance, chain, json),
    }
}

/// `suzatary todos open`: every open todo, ascending in fulfillment (low is urgent).
fn open_todos(instance: &Instance, chain: &ChainDir, json: bool) -> anyhow::Result<()> {
    let read = super::chains_read(instance);
    let set = read.set(SystemClock.now());
    let nodes = set
        .held(&chain.policy.name)
        .context("reading the chain")?
        .nodes;
    let entries = set
        .entries(&chain.policy.name)
        .context("folding the chain")?;
    let records = suzatary_core::decoration::records_of(nodes);

    // The Inbox's held-batch decisions fold like todos and are dropped, as the web app does.
    let mut open: Vec<&Entry> = entries
        .iter()
        .filter(|entry| one_outcome(entry.outcome()).is_none() && !entry.content().is_empty())
        .filter(|entry| {
            entry
                .content()
                .first()
                .and_then(|name| records.get(name))
                .is_none_or(|record| !record.payload.is_decision())
        })
        .collect();
    // Ascending, ties by id, then every row with no number by id.
    open.sort_by(|left, right| prodrome_core::view::list_order(left, right));

    if json {
        let rows: Vec<serde_json::Value> = open
            .iter()
            .map(|entry| {
                let record = entry.content().first().and_then(|name| records.get(name));
                serde_json::json!({
                    "id": entry.key.as_str(),
                    // "12.34%", "absent" for `∅`, or null where it does not link.
                    "priority": match entry.value() {
                        Ok(Some(value)) => Some(format!("{:.2}%", value * 100.0)),
                        Ok(None) => Some("absent".to_owned()),
                        Err(_) => None,
                    },
                    "kind": record.map(|r| r.payload.kind.as_str()),
                    "category": record.map_or("other", |r| {
                        r.payload.category.as_ref().map_or_else(
                            || suzatary_core::decoration::kind_category(r.payload.kind.as_str()),
                            prodrome_core::event::Name::as_str,
                        )
                    }),
                    "body": record.map(|r| markup_text(r.payload.body.as_str())),
                    "waiting_on": record
                        .map(|r| string_text(r.payload.waiting_on.as_str()))
                        .filter(|text| !text.is_empty()),
                    "authored_by": record.map(|r| r.actor.as_str()),
                    "unconfirmed": entry.confidence.is_provisional(),
                })
            })
            .collect();
        println!("{}", serde_json::json!({ "open": rows }));
        return Ok(());
    }

    for entry in open {
        let record = entry.content().first().and_then(|name| records.get(name));
        let price = match entry.value() {
            Ok(Some(value)) => format!("{:6.2}%", value * 100.0),
            Ok(None) => "      ∅".to_owned(),
            Err(_) => "      —".to_owned(),
        };
        let mark = if entry.confidence.is_provisional() {
            " (unconfirmed)"
        } else {
            ""
        };
        // Plain text: the briefing and the agent's dedup read this row.
        let body = record.map_or_else(String::new, |r| markup_text(r.payload.body.as_str()));
        println!("{price}  {:<24}{mark} {body}", entry.key.as_str());
    }
    Ok(())
}
