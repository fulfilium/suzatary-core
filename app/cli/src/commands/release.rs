//! `suzatary release` — the version the project chain reads, and the release a train cuts.
//!
//! The rule is `suzatary_core::release`'s; this verb reads the chain the plan names, and
//! `scripts/train.sh` writes a tree and a tag only from what it prints.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::{TodoEvent, TodoId};
use prodrome_core::literal::Datetime;
use prodrome_core::term::Term;
use suzatary_core::capability::Clock;
use suzatary_core::escape::markup_text;
use suzatary_core::record::Node;
use suzatary_core::release::{
    Completed, Completes, Cut, Plan, Reading, Released, Version, binding, reopened,
};
use suzatary_core::store::Prodrome;
use suzatary_io::clock::SystemClock;

use super::store_of;
use crate::instance::{ChainDir, Instance};
use suzatary_core::store::Reads as _;

/// Which reading or act.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Which.
    #[command(subcommand)]
    action: Action,
}

/// Every `release` reading and act.
#[derive(Debug, Subcommand)]
pub enum Action {
    /// The `major.minor` the chain reads now (the last completed release's), and each release
    /// with its items.
    Read {
        /// Print the reading as JSON instead of as a list.
        #[arg(long)]
        json: bool,
    },
    /// Attach todos to a release, or detach them, as one revision of its spec, written trusted.
    /// A detached item stays listed, as dropped.
    Items {
        /// The release's todo, one the plan declares.
        release: String,
        /// A todo to attach; once per todo.
        #[arg(long = "add", value_name = "TODO")]
        add: Vec<String>,
        /// A todo to detach; once per todo.
        #[arg(long = "remove", value_name = "TODO")]
        remove: Vec<String>,
        /// Who is rescoping; the chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Refuse, naming them, unless every todo given is on the plan's chain: what a train asks
    /// of each PR's `Completes:` line before it merges the PR. An unknown todo exits 3, any
    /// other failure 1, so a train skips only the PR and stops on the rest.
    Known {
        /// The checkout whose plan names the chain.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
        /// The todos.
        #[arg(required = true)]
        todos: Vec<String>,
    },
    /// Write the next release into a checkout: `app/release.toml` (its version and what it
    /// completed) and the changelog's Unreleased section closed under it. The
    /// version is read as the chain will be once `--completes` are recorded done, which the
    /// train's land does; nothing is written to the chain here.
    Cut {
        /// A todo this release completes, on the plan's chain; once per todo.
        #[arg(long = "completes", value_name = "TODO")]
        completes: Vec<String>,
        /// Who will record them done.
        #[arg(long, default_value = "assistant")]
        actor: String,
        /// The checkout to write.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
        /// The changelog the train started from (master's): an entry its merges added to a
        /// section that one had already released is moved back under Unreleased first.
        #[arg(long, value_name = "FILE")]
        since: Option<PathBuf>,
        /// The last release (origin/master's newest `Release:` trailer); absent before the
        /// first, which is `major.minor.0` of what the chain reads. A version the checkout
        /// declares that is not the last release was never released: its section joins this
        /// release.
        #[arg(long, value_name = "VERSION")]
        last: Option<Version>,
    },
    /// Record done, as `--actor`, each todo the checkout's release completed that the chain does
    /// not yet hold completed, under the chain's write lock: what a land does once origin holds
    /// the release. A todo already done is not written again, so recording twice is recording
    /// once.
    Record {
        /// The release's files (its plan and `app/release.toml`), checked out or extracted.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
        /// `<todo>=<note>`: the note a todo's record carries (the PR that completed it); the
        /// release's version otherwise.
        #[arg(long = "note", value_name = "TODO=NOTE")]
        notes: Vec<String>,
        /// Who records them.
        #[arg(long, default_value = "assistant")]
        actor: String,
    },
    /// Refuse unless the chain, with every todo the checkout says it completed recorded done,
    /// reads at least the checkout's version: what a land asks before it pushes.
    Check {
        /// The checkout to read.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
}

/// `suzatary release <action>`.
///
/// # Errors
/// The instance, the chain, the plan or the tree.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    // A release is cut and checked by the plan of the tree it is written into, so a train
    // that changes the plan is read by its own; the reading serves the compiled one.
    let plan = match &args.action {
        Action::Read { .. } | Action::Items { .. } => {
            Plan::declared().context("the release plan this build carries")?
        }
        Action::Known { tree, .. }
        | Action::Cut { tree, .. }
        | Action::Check { tree, .. }
        | Action::Record { tree, .. } => Plan::parse(&readable(&Files::of(tree).plan)?)?,
    };
    let dir = instance.chain(Some(plan.chain()))?;
    let nodes = store_of(dir).nodes().context("reading the project chain")?;
    let chain = Chain {
        dir,
        nodes: &nodes,
        now: SystemClock.now(),
    };
    match &args.action {
        Action::Read { json } => read(&plan, &chain, *json),
        Action::Known { todos, .. } => {
            if let Err(unknown) = known(&plan, &chain, todos) {
                eprintln!("suzatary: {unknown}");
                std::process::exit(UNKNOWN);
            }
            Ok(())
        }
        Action::Items {
            release,
            add,
            remove,
            actor,
        } => items(
            &instance,
            &plan,
            &chain,
            release,
            add,
            remove,
            actor.as_deref().unwrap_or(dir.owner.as_str()),
        ),
        Action::Cut {
            completes,
            actor,
            tree,
            since,
            last,
        } => cut(
            &plan,
            &chain,
            &Asked {
                completes,
                actor,
                tree,
                since: since.as_deref(),
                last: *last,
            },
        ),
        Action::Check { tree } => check(&plan, &chain, tree),
        Action::Record { tree, notes, actor } => {
            record(&instance, &plan, &chain, tree, notes, actor)
        }
    }
}

/// What `release known` exits with when a todo is not on the chain.
const UNKNOWN: i32 = 3;

/// The plan's chain, read once.
struct Chain<'a> {
    dir: &'a ChainDir,
    nodes: &'a [Node],
    now: Datetime,
}

impl Chain<'_> {
    /// Its binding events.
    fn binding(&self) -> impl Iterator<Item = &suzatary_core::record::TodoEvent> {
        binding(self.nodes, &self.dir.policy.untrusted)
    }

    /// Each todo's spec in force.
    fn current(&self) -> anyhow::Result<BTreeMap<TodoId, Vec<Term>>> {
        Ok(
            suzatary_core::believed::Believed::of(self.nodes, self.now, &self.dir.policy.untrusted)
                .context("folding the project chain")?
                .specs,
        )
    }

    /// A todo's action line as plain text, `None` when the chain holds no record of it.
    fn title(&self, todo: &TodoId) -> Option<String> {
        suzatary_core::todos::current(self.nodes, self.now, todo.as_str())
            .map(|record| markup_text(record.payload.body.as_str()))
    }

    /// Whether `todo` was completed by a binding event.
    fn completed(&self, todo: &TodoId) -> bool {
        self.binding()
            .any(|event| matches!(event, TodoEvent::Completed(e) if e.todo == *todo))
    }
}

fn read(plan: &Plan, chain: &Chain<'_>, json: bool) -> anyhow::Result<()> {
    let reading = Reading::of(plan, chain.binding(), chain.now, &chain.current()?);
    if json {
        let releases: Vec<_> = reading
            .releases
            .iter()
            .map(|r| {
                serde_json::json!({
                    "todo": r.todo.as_str(),
                    "title": chain.title(&r.todo),
                    "version": r.version.to_string(),
                    "lot": r.lot,
                    "place": r.place,
                    "items": r.items.iter().map(|i| serde_json::json!({
                        "todo": i.todo.as_str(),
                        "lot": i.lot,
                        "named": i.named,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "reads": reading.counted.to_string(), "releases": releases })
        );
        return Ok(());
    }
    println!("the chain reads {}", reading.counted);
    for r in &reading.releases {
        println!(
            "{} ({}): {:?}, {:?}",
            r.todo.as_str(),
            r.version,
            r.lot,
            r.place
        );
        for i in &r.items {
            println!("  {} {:?}, {:?}", i.todo.as_str(), i.lot, i.named);
        }
    }
    Ok(())
}

/// `suzatary release items <release> --add … --remove …`: one trusted revision of the release's
/// spec, read and written under the chain's write lock.
fn items(
    instance: &Instance,
    plan: &Plan,
    chain: &Chain<'_>,
    release: &str,
    add: &[String],
    remove: &[String],
    actor: &str,
) -> anyhow::Result<()> {
    if !plan.releases().iter().any(|r| r.todo.as_str() == release) {
        bail!("{release:?} is not a release the plan declares");
    }
    known(plan, chain, add)?;
    if !chain.dir.policy.roster.iter().any(|a| a.as_str() == actor) {
        bail!("{actor:?} is not on the {} chain's roster", plan.chain());
    }
    let writer = Instance::writer_of(
        chain.dir,
        &instance.signers(),
        &prodrome_core::event::Actor::new(actor).map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let _held = writer.locked().context("taking the chain's write lock")?;
    let nodes = writer.nodes().context("reading the project chain")?;
    let Some(parent) = suzatary_core::todos::current(&nodes, chain.now, release) else {
        bail!("no todo {release:?} on the {} chain", plan.chain());
    };
    let functions =
        suzatary_core::believed::Believed::of(&nodes, chain.now, &chain.dir.policy.untrusted)
            .context("folding the project chain")?
            .linkable();
    let event =
        suzatary_core::authoring::rescope(&parent, add, remove, &functions, chain.now, actor)
            .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    writer.append(event).context("appending the revision")?;
    let after = writer.nodes().context("re-reading the project chain")?;
    let current =
        suzatary_core::believed::Believed::of(&after, chain.now, &chain.dir.policy.untrusted)
            .context("folding the project chain")?
            .specs;
    let reading = Reading::of(
        plan,
        binding(&after, &chain.dir.policy.untrusted),
        chain.now,
        &current,
    );
    println!(
        "{}",
        serde_json::json!({
            "release": release,
            "items": reading.releases.iter().find(|r| r.todo.as_str() == release).map(|r| r
                .items
                .iter()
                .map(|i| serde_json::json!({ "todo": i.todo.as_str(), "named": i.named }))
                .collect::<Vec<_>>()),
            "reads": reading.counted.to_string(),
        })
    );
    Ok(())
}

/// `suzatary release record`: the tree's completed todos the chain lacks, recorded done.
fn record(
    instance: &Instance,
    plan: &Plan,
    chain: &Chain<'_>,
    tree: &Path,
    notes: &[String],
    actor: &str,
) -> anyhow::Result<()> {
    let Released { version, completes } = released(&Files::of(tree))?;
    let notes = notes
        .iter()
        .map(|pair| {
            pair.split_once('=')
                .map(|(todo, note)| (todo.to_owned(), note.to_owned()))
                .ok_or_else(|| anyhow::anyhow!("{pair:?} is not <todo>=<note>"))
        })
        .collect::<anyhow::Result<BTreeMap<String, String>>>()?;
    if !chain.dir.policy.roster.iter().any(|a| a.as_str() == actor) {
        bail!("{actor:?} is not on the {} chain's roster", plan.chain());
    }
    let writer = Instance::writer_of(
        chain.dir,
        &instance.signers(),
        &prodrome_core::event::Actor::new(actor).map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let _held = writer.locked().context("taking the chain's write lock")?;
    let objects = writer.nodes().context("reading the project chain")?;
    let held = Chain {
        dir: chain.dir,
        nodes: &objects,
        now: chain.now,
    };
    let mut recorded = Vec::new();
    for todo in &completes {
        if held.completed(todo) {
            continue;
        }
        let note = notes
            .get(todo.as_str())
            .cloned()
            .unwrap_or_else(|| format!("the release {version}"));
        let event = prodrome_core::event::mk_completed(todo.as_str(), chain.now, actor, &note)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        writer
            .append(event)
            .with_context(|| format!("recording {} done", todo.as_str()))?;
        recorded.push(todo.as_str().to_owned());
    }
    println!(
        "{}",
        serde_json::json!({ "version": version.to_string(), "recorded": recorded })
    );
    Ok(())
}

/// The todos named, each one the chain holds, or a refusal naming those it does not.
fn known(plan: &Plan, chain: &Chain<'_>, todos: &[String]) -> anyhow::Result<Vec<TodoId>> {
    let mut held = Vec::new();
    let mut unknown = Vec::new();
    for id in todos {
        match TodoId::new(id.clone()) {
            Ok(todo) if chain.title(&todo).is_some() => held.push(todo),
            Ok(_) | Err(_) => unknown.push(id.as_str()),
        }
    }
    if unknown.is_empty() {
        Ok(held)
    } else {
        bail!(
            "no todo {} on the {} chain",
            unknown.join(", "),
            plan.chain()
        )
    }
}

/// The two files a release writes under `tree`, and the plan it reads there.
struct Files {
    plan: PathBuf,
    changelog: PathBuf,
    released: PathBuf,
}

impl Files {
    fn of(tree: &Path) -> Files {
        Files {
            plan: tree.join("app/core/releases.toml"),
            changelog: tree.join("CHANGELOG.md"),
            released: tree.join("app/release.toml"),
        }
    }
}

/// The release `files` declare.
fn released(files: &Files) -> anyhow::Result<Released> {
    Released::parse(&readable(&files.released)?)
        .with_context(|| format!("reading {}", files.released.display()))
}

fn readable(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// What `release cut` was asked.
struct Asked<'a> {
    completes: &'a [String],
    actor: &'a str,
    tree: &'a Path,
    since: Option<&'a Path>,
    last: Option<Version>,
}

fn cut(plan: &Plan, chain: &Chain<'_>, asked: &Asked<'_>) -> anyhow::Result<()> {
    let todos = known(plan, chain, asked.completes)?;
    let reading = Reading::assuming(
        plan,
        chain.binding(),
        &todos,
        asked.actor,
        chain.now,
        &chain.current()?,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let files = Files::of(asked.tree);
    let existing = readable(&files.released)?;
    let declared = Released::parse(&existing)
        .with_context(|| format!("reading {}", files.released.display()))?
        .version;
    let version = Version::after(asked.last, reading.counted)?;
    let completes = titled(chain, &todos);
    let release = Cut {
        version,
        completes,
        folds: (asked.last != Some(declared)).then_some(declared),
    };
    let day = chrono::NaiveDate::from_ymd_opt(chain.now.year(), chain.now.month(), chain.now.day())
        .context("the clock's day")?;
    let merged = readable(&files.changelog)?;
    let merged = match asked.since {
        Some(base) => reopened(&readable(base)?, &merged),
        None => merged,
    };
    let changelog = release.changelog(&merged, day, plan.chain())?;
    write(&files.changelog, &changelog)?;
    write(&files.released, &release.released().print_over(&existing))?;
    println!("{}", printed(&release));
    Ok(())
}

/// `todos`, each with its title on the chain (its id where the chain holds none): the words a
/// cut lists in the changelog and prints, never what the public release file stores.
fn titled(chain: &Chain<'_>, todos: &[TodoId]) -> Completes {
    Completes::new(todos.iter().map(|todo| {
        Completed {
            todo: todo.clone(),
            title: chain
                .title(todo)
                .unwrap_or_else(|| todo.as_str().to_owned()),
        }
    }))
}

/// What a release says of itself on stdout, which the train reads.
fn printed(release: &Cut) -> serde_json::Value {
    serde_json::json!({
        "version": release.version.to_string(),
        "completes": release.completes.todos().iter().map(|c| serde_json::json!({
            "todo": c.todo.as_str(),
            "title": c.title,
        })).collect::<Vec<_>>(),
    })
}

fn check(plan: &Plan, chain: &Chain<'_>, tree: &Path) -> anyhow::Result<()> {
    let Released {
        version,
        completes: todos,
    } = released(&Files::of(tree))?;
    let reading = Reading::assuming(
        plan,
        chain.binding(),
        &todos,
        "assistant",
        chain.now,
        &chain.current()?,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    // At least, not exactly: a release completed since the cut is the next train's, and the
    // tag stays the cut. Below it, the tree claims more than the chain holds.
    if reading.counted < version.counted() {
        bail!(
            "the {} chain, with {} done, reads {}, below the tree's {version}: the tree claims \
             more than the chain holds, so build the train again",
            plan.chain(),
            if todos.is_empty() {
                "nothing more".to_owned()
            } else {
                todos
                    .iter()
                    .map(TodoId::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            reading.counted
        );
    }
    println!(
        "{}",
        printed(&Cut {
            version,
            completes: titled(chain, &todos),
            folds: None,
        })
    );
    Ok(())
}
