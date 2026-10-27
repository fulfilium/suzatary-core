//! `suzatary decision` — record, revise, withdraw, list and view the recorded decisions.
//!
//! A decision's body is Typst markup through the one door, as a todo's is; its title, rationale
//! and reason are plain text. The store is `decisions/` in the data tree unless `--store` names
//! another, read and written with no instance file: a scratch copy, or the project's memory,
//! `memory/` in the code repository.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use suzatary_core::capability::{Clock, Typeset};
use suzatary_core::decisions::{Decision, DecisionId, Decisions, Draft, Ledger};
use suzatary_io::clock::SystemClock;
use suzatary_io::objects::Objects;

use crate::instance::Instance;

/// Which `decision` act, and its own arguments.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Which act.
    #[command(subcommand)]
    action: Action,
    /// The store to read and write; absent is `decisions/` in the data tree.
    #[arg(long, global = true, value_name = "DIR")]
    store: Option<PathBuf>,
}

/// EVERY `decision` act.
#[derive(Debug, Subcommand)]
pub enum Action {
    /// Record a new decision.
    Record(Stated),
    /// Restate a standing decision; its links join those it already made.
    Revise(Stated),
    /// Withdraw a standing decision: it never stands again.
    Withdraw {
        /// The decision.
        id: String,
        /// Why it no longer holds.
        #[arg(long)]
        reason: String,
        /// Who.
        #[arg(long, default_value = "assistant")]
        actor: String,
        /// When it was withdrawn; absent is now.
        #[arg(long, value_name = "INSTANT", value_parser = instant)]
        at: Option<Datetime>,
    },
    /// Every decision, newest lineage first, each above what it supersedes.
    List {
        /// Only the decisions whose title, body or rationale holds every one of these words, in
        /// any case.
        #[arg(long, value_name = "WORDS")]
        grep: Option<String>,
    },
    /// One decision whole.
    Show {
        /// The decision.
        id: String,
    },
    /// The decisions as a Typst document on stdout, or compiled to HTML with `--html`.
    View {
        /// Write typst's HTML, compiled in the closed world, here.
        #[arg(long, value_name = "FILE")]
        html: Option<PathBuf>,
    },
}

/// What a `record` or a `revise` says.
#[derive(Debug, clap::Args)]
pub struct Stated {
    /// The id, 3-64 lowercase letters and digits starting with a letter.
    id: String,
    /// One line naming what was decided.
    #[arg(long)]
    title: String,
    /// What was decided, as Typst markup, checked in the closed world before it is stored.
    #[arg(long)]
    body: String,
    /// Why, in plain text.
    #[arg(long)]
    rationale: String,
    /// The decisions this is the newer word on, by id, comma-separated.
    #[arg(long, default_value = "")]
    supersedes: String,
    /// Who.
    #[arg(long, default_value = "assistant")]
    actor: String,
    /// When it was decided, an ISO date or instant (UTC); absent is now.
    #[arg(long, value_name = "INSTANT", value_parser = instant)]
    at: Option<Datetime>,
}

/// Run one act.
///
/// # Errors
/// The instance file, the store, or the act's refusal.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let root = match &args.store {
        Some(root) => root.clone(),
        None => Instance::open(path)?.decisions(),
    };
    let store = Objects::open(root);
    let decisions = Decisions { store: &store };
    let typeset = suzatary_io::typeset::Closed::this_binary();
    match &args.action {
        Action::Record(stated) => {
            let name = decisions.decide(
                &draft(stated)?,
                stated.at.unwrap_or_else(|| SystemClock.now()),
                &actor(&stated.actor)?,
                &typeset,
            )?;
            println!("decided {} as {}", stated.id, name.as_str());
        }
        Action::Revise(stated) => {
            let name = decisions.revise(
                &draft(stated)?,
                stated.at.unwrap_or_else(|| SystemClock.now()),
                &actor(&stated.actor)?,
                &typeset,
            )?;
            println!("revised {} as {}", stated.id, name.as_str());
        }
        Action::Withdraw {
            id,
            reason,
            actor: who,
            at,
        } => {
            let name = decisions.withdraw(
                &decision(id)?,
                reason,
                at.unwrap_or_else(|| SystemClock.now()),
                &actor(who)?,
            )?;
            println!("withdrew {id} as {}", name.as_str());
        }
        Action::List { grep } => {
            let words = grep.as_deref().map_or_else(Vec::new, words);
            print!("{}", listing(&decisions.ledger()?, &words));
        }
        Action::Show { id } => {
            let ledger = decisions.ledger()?;
            let id = decision(id)?;
            let Some(found) = ledger.get(&id) else {
                bail!("no decision is named {id}");
            };
            let statement = found.statement();
            println!("{id}: {}", statement.title());
            println!("{}", line(&ledger, &id));
            println!(
                "\n{}\n\nWhy: {}",
                statement.body().as_str(),
                statement.rationale()
            );
        }
        Action::View { html } => {
            let source = suzatary_core::decisions::document(&decisions.ledger()?);
            match html {
                None => print!("{source}"),
                Some(out) => {
                    let page = typeset
                        .closed(&source)
                        .map_err(|rejected| anyhow::anyhow!("{}", rejected.said(&source)))?;
                    if let Some(reason) = suzatary_core::tripwire::suspicious(&page) {
                        bail!("typst's HTML for the decisions carried {reason}; not written");
                    }
                    std::fs::write(out, page)
                        .with_context(|| format!("writing {}", out.display()))?;
                }
            }
        }
    }
    Ok(())
}

/// One line per decision whose title, body or rationale holds every word, in the lineages' order, a
/// lineage's first flush and the rest indented beneath it.
fn listing(ledger: &Ledger, words: &[String]) -> String {
    let mut out = String::new();
    for lineage in ledger.lineages() {
        for (at, id) in lineage.into_iter().enumerate() {
            let Some(statement) = ledger.get(id).map(Decision::statement) else {
                continue;
            };
            let said = format!(
                "{} {} {}",
                statement.title(),
                statement.body().as_str(),
                statement.rationale()
            )
            .to_lowercase();
            if !words.iter().all(|word| said.contains(word.as_str())) {
                continue;
            }
            let indent = if at == 0 { "" } else { "  " };
            let title = statement.title();
            let _ = writeln!(out, "{indent}{id}  {title}\n{indent}  {}", line(ledger, id));
        }
    }
    out
}

/// `--grep`'s words, lowercased, so a match ignores case.
fn words(grep: &str) -> Vec<String> {
    grep.split_whitespace().map(str::to_lowercase).collect()
}

/// Where `id` stands, in one line.
fn line(ledger: &Ledger, id: &DecisionId) -> String {
    let Some(decision) = ledger.get(id) else {
        return String::new();
    };
    let mut said = vec![format!("decided {}", decision.decided.isoformat())];
    if let Some(withdrawn) = &decision.withdrawn {
        said.push(format!("withdrawn: {}", withdrawn.reason));
    }
    let by = ledger.superseded_by(id);
    if !by.is_empty() {
        said.push(format!("superseded by {}", joined(by)));
    }
    if !decision.supersedes.is_empty() {
        said.push(format!("supersedes {}", joined(&decision.supersedes)));
    }
    if ledger.in_force(id) {
        said.push("in force".to_owned());
    }
    if decision.is_conflict() {
        said.push(format!(
            "{} statements in conflict",
            decision.statements().len()
        ));
    }
    said.join("; ")
}

fn joined<'a>(ids: impl IntoIterator<Item = &'a DecisionId>) -> String {
    ids.into_iter()
        .map(DecisionId::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn draft(stated: &Stated) -> anyhow::Result<Draft> {
    Ok(Draft::new(
        &stated.id,
        &stated.title,
        &stated.body,
        &stated.rationale,
        stated.supersedes.split(','),
    )?)
}

/// `--at`'s instant, parsed once where it enters.
fn instant(text: &str) -> Result<Datetime, String> {
    prodrome_core::fpl::parse_iso(text)
        .map_err(|refused| refused.to_string())
        .and_then(|at| prodrome_core::fpl::datetime_of(at).map_err(|refused| refused.to_string()))
}

fn decision(id: &str) -> anyhow::Result<DecisionId> {
    Ok(DecisionId::new(id)?)
}

fn actor(name: &str) -> anyhow::Result<Actor> {
    Actor::new(name).map_err(|failed| anyhow::anyhow!("{name}: {failed}"))
}
