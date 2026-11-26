//! `suzatary decision`: a body is Typst markup through the one door; title, rationale and reason
//! are plain text.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_data::event::Actor;
use prodrome_data::literal::Datetime;
use suzatary_base::capability::Clock;
use suzatary_digest::decisions::about::About;
use suzatary_digest::decisions::brief::{DEFAULT_TOKENS, brief, zoom};
use suzatary_digest::decisions::decision_log::DecisionLog;
use suzatary_digest::decisions::summarizing::{Step, summarize};
use suzatary_io_chains::objects::Objects;
use suzatary_io_local::clock::SystemClock;
use suzatary_todo::decisions::{Area, Decision, DecisionId, Decisions, Draft, Ledger};

use super::store_at::StoreAt;
use crate::instance::Instance;
use crate::interpreters::CompactorModel;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
    #[command(flatten)]
    store: StoreAt,
}

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
        #[arg(long, value_name = "INSTANT", value_parser = super::at::instant)]
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
    /// What a session loads before it plans: the decisions that name the task, whole, and the rest of
    /// the memory as one line a part, within a budget. The same store prints the same bytes.
    Brief {
        /// A path or a word the task names; every decision about it is shown whole. Repeat it.
        #[arg(long, value_name = "PATH_OR_WORD")]
        about: Vec<String>,
        /// The most tokens the brief holds, at about four bytes each.
        #[arg(long, value_name = "TOKENS", default_value_t = DEFAULT_TOKENS)]
        budget: usize,
    },
    /// What lies beneath a part of the brief, given its handle (`first+lines`), down to whole
    /// decisions.
    Zoom {
        /// The handle, as the brief prints it.
        handle: String,
    },
    /// Write the summaries that the brief's view waits for, by the secretary's compactor: one model
    /// call a node, run by the main session after it records, never by the brief.
    Summarize {
        /// Who.
        #[arg(long, default_value = "assistant")]
        actor: String,
        /// When; absent is now.
        #[arg(long, value_name = "INSTANT", value_parser = super::at::instant)]
        at: Option<Datetime>,
    },
    /// Name the parts of the tree a standing decision governs, beside those it already does.
    Govern {
        /// The decision.
        id: String,
        /// A path in the tree, such as `app/tiles`. Repeat it.
        #[arg(long, value_name = "PATH", required = true)]
        area: Vec<String>,
        /// Who.
        #[arg(long, default_value = "assistant")]
        actor: String,
        /// When it was decided; absent is now.
        #[arg(long, value_name = "INSTANT", value_parser = super::at::instant)]
        at: Option<Datetime>,
    },
    /// The decisions as typst's HTML, compiled in the closed world, on stdout or into `--html`.
    View {
        /// Write the HTML here instead.
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
    /// A path in the tree that it governs, such as `app/tiles`. Repeat it.
    #[arg(long, value_name = "PATH")]
    area: Vec<String>,
    /// Who.
    #[arg(long, default_value = "assistant")]
    actor: String,
    /// When it was decided, an ISO date or instant (UTC); absent is now.
    #[arg(long, value_name = "INSTANT", value_parser = super::at::instant)]
    at: Option<Datetime>,
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let root = args.store.or_instance(
        path,
        |dir| Ok(dir.to_owned()),
        |instance| Ok(instance.decisions()),
    )?;
    let store = Objects::open(root);
    let decisions = Decisions { store: &store };
    let typeset = suzatary_io_typeset::typeset::Closed::this_binary();
    match &args.action {
        Action::Record(stated) => {
            let at = stated.at.unwrap_or_else(|| SystemClock.now());
            let name = decisions.decide(&draft(stated)?, at, &actor(&stated.actor)?, &typeset)?;
            println!("decided {} as {}", stated.id, name.as_str());
            governed(&decisions, stated, at)?;
        }
        Action::Revise(stated) => {
            let at = stated.at.unwrap_or_else(|| SystemClock.now());
            let name = decisions.revise(&draft(stated)?, at, &actor(&stated.actor)?, &typeset)?;
            println!("revised {} as {}", stated.id, name.as_str());
            governed(&decisions, stated, at)?;
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
        Action::Brief { about, budget } => print!("{}", briefed(&decisions, about, *budget)?),
        Action::Zoom { handle } => print!("{}", zoomed(&decisions, handle)?),
        Action::Summarize { actor: who, at } => summarized(path, &decisions, who, *at)?,
        Action::Govern {
            id,
            area,
            actor: who,
            at,
        } => {
            let name = decisions.govern(
                &decision(id)?,
                areas(area)?,
                at.unwrap_or_else(|| SystemClock.now()),
                &actor(who)?,
            )?;
            println!("{id} governs {} as {}", area.join(", "), name.as_str());
        }
        Action::View { html } => {
            let called = suzatary_todo::rendering::DECISIONS
                .call(&suzatary_todo::decisions::document(&decisions.ledger()?))?;
            let page = suzatary_io_typeset::render::drawn(&typeset, &called)?;
            match html {
                None => print!("{page}"),
                Some(out) => std::fs::write(out, page)
                    .with_context(|| format!("writing {}", out.display()))?,
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
    if !decision.areas.is_empty() {
        let areas: Vec<&str> = decision.areas.iter().map(Area::as_str).collect();
        said.push(format!("governs {}", areas.join(", ")));
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

/// What a session loads before it plans, for a task that names `about`, within `budget` tokens.
fn briefed(decisions: &Decisions<'_>, about: &[String], budget: usize) -> anyhow::Result<String> {
    let abouts = about
        .iter()
        .map(|word| About::new(word))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(brief(
        &decisions.ledger()?,
        &decisions.summaries()?,
        &abouts,
        budget,
    )?)
}

/// What lies beneath the part of the brief that `handle` names.
fn zoomed(decisions: &Decisions<'_>, handle: &str) -> anyhow::Result<String> {
    zoom(&log_of(decisions)?, handle).ok_or_else(|| {
        anyhow::anyhow!(
            "no part of the memory spans {handle}: a handle is `first+lines`, as the brief shows it"
        )
    })
}

/// The summaries the brief's view waits for, each written as the compactor answers.
fn summarized(
    path: &Path,
    decisions: &Decisions<'_>,
    who: &str,
    at: Option<Datetime>,
) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let model = CompactorModel::new(&instance, instance.secretary.compactors.first().clone());
    let wrote = summarize(
        decisions,
        &model,
        &suzatary_base::call_log::Tracing,
        at.unwrap_or_else(|| SystemClock.now()),
        &actor(who)?,
        |step| match step {
            Step::Wrote { node, words } => println!("summarized {}: {words}", node.as_str()),
            Step::Unfit { node } => println!("no answer fitted {}", node.as_str()),
        },
    )?;
    println!("{wrote} summaries written");
    Ok(())
}

/// The areas a record or a revise names, once its decision stands.
fn governed(decisions: &Decisions<'_>, stated: &Stated, at: Datetime) -> anyhow::Result<()> {
    if stated.area.is_empty() {
        return Ok(());
    }
    let name = decisions.govern(
        &decision(&stated.id)?,
        areas(&stated.area)?,
        at,
        &actor(&stated.actor)?,
    )?;
    println!(
        "{} governs {} as {}",
        stated.id,
        stated.area.join(", "),
        name.as_str()
    );
    Ok(())
}

fn areas(paths: &[String]) -> anyhow::Result<std::collections::BTreeSet<Area>> {
    Ok(paths.iter().map(Area::new).collect::<Result<_, _>>()?)
}

fn log_of(decisions: &Decisions<'_>) -> anyhow::Result<DecisionLog> {
    Ok(DecisionLog::of(
        &decisions.ledger()?,
        &decisions.summaries()?,
    )?)
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

fn decision(id: &str) -> anyhow::Result<DecisionId> {
    Ok(DecisionId::new(id)?)
}

fn actor(name: &str) -> anyhow::Result<Actor> {
    Actor::new(name).map_err(|failed| anyhow::anyhow!("{name}: {failed}"))
}
