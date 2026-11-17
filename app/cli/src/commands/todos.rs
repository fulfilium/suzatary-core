//! `suzatary todos open`: the open list the briefing and the agent's dedup read.
//!
//! An unconfirmed triage-authored todo is marked where it stands, never moved or sunk.

use std::path::Path;

use anyhow::Context;
use clap::Subcommand;
use prodrome_core::literal::Datetime;
use suzatary_todo::shown::{Priority, open_rows};

use super::at::At;
use super::store_at::StoreAt;
use suzatary_base::chains::ChainName;
use suzatary_signatures::chain_set::Read;

/// Which reading of the todo list.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Which reading.
    #[command(subcommand)]
    action: Action,
    /// The chain to read; absent is the instance file's first, which the briefing reads.
    #[arg(long, value_name = "NAME")]
    chain: Option<String>,
    #[command(flatten)]
    store: StoreAt,
    #[command(flatten)]
    at: At,
}

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
    let read = args.store.todos(path, args.chain.as_deref())?;
    match args.action {
        Action::Open { json } => open_todos(read, args.at.or_now(), json),
    }
}

/// `suzatary todos open`: every open todo, ascending in fulfillment (low is urgent).
fn open_todos((read, name): (Read, ChainName), at: Datetime, json: bool) -> anyhow::Result<()> {
    let set = read.set(at);
    let nodes = set.held(&name).context("reading the chain")?.nodes;
    let entries = set.entries(&name).context("folding the chain")?;
    let rows = open_rows(&entries, &suzatary_record::decoration::records_of(nodes));
    if json {
        println!("{}", serde_json::json!({ "open": rows }));
        return Ok(());
    }
    for row in rows {
        let price = match &row.priority {
            Priority::Percent { percent } => format!("{percent:>7}"),
            Priority::Absent => "      ∅".to_owned(),
            Priority::Unlinked => "      —".to_owned(),
        };
        let mark = if row.unconfirmed {
            " (unconfirmed)"
        } else {
            ""
        };
        // Plain text: the briefing and the agent's dedup read this row.
        println!("{price}  {:<24}{mark} {}", row.id, row.body);
    }
    Ok(())
}
