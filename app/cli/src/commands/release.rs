//! `suzatary release`: a cut and a check read a store by path (`--store`), so each is a function
//! of the code tree and that commit, and `scripts/train.sh` writes a tree and a tag only from what
//! this verb prints.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::{TodoEvent, TodoId};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use prodrome_core::term::Term;
use suzatary_base::capability::Clock;
use suzatary_base::escape::markup_text;
use suzatary_io_local::clock::SystemClock;
use suzatary_record::record::Node;
use suzatary_release::release::{
    Completed, Completes, Cut, Fragment, Plan, Reading, Released, Version, binding,
};
use suzatary_store::store::Prodrome;

use super::{chain_at, store_of};
use crate::instance::{ChainDir, Instance, project};
use suzatary_signatures::believed::Believed;
use suzatary_store::store::Reads as _;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// The `major.minor` the store reads now (the last completed release's), and each release
    /// with its items.
    Read {
        /// Print the reading as JSON instead of as a list.
        #[arg(long)]
        json: bool,
        /// A store with its `releases.toml`, read by path; absent is the plan store mounted.
        #[arg(long, value_name = "DIR")]
        store: Option<PathBuf>,
    },
    /// Attach todos to a release, or detach them, as one revision of its spec written on the
    /// mounted plan store. A detached item stays listed, as dropped.
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
    /// Print where each todo is found, `roadmap <id>` for one the store at `--store` holds and
    /// `upkeep <id>` for one only the instance's `[upkeep]` chain holds: what a train asks of each
    /// PR's `Completes:` line before it merges the PR. A todo found in neither exits 3, any other
    /// failure 1, so a train skips only the PR and stops on the rest.
    Known {
        /// The roadmap store, written out at the commit the train reads.
        #[arg(long, value_name = "DIR")]
        store: PathBuf,
        /// The todos.
        #[arg(required = true)]
        todos: Vec<String>,
    },
    /// Write the next release into a checkout: `app/release.toml` and the changelog's Unreleased
    /// section closed under it, `changelog.d/` folded in and removed. The version is the store's
    /// once `--completes` are done (a land's work); nothing is written to the store.
    Cut {
        /// A todo this release completes, held by the store; once per todo.
        #[arg(long = "completes", value_name = "TODO")]
        completes: Vec<String>,
        /// Who will record them done.
        #[arg(long, default_value = "assistant")]
        actor: String,
        /// The checkout to write.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
        /// The roadmap store, written out at the commit the release names in `Tasks:`.
        #[arg(long, value_name = "DIR")]
        store: PathBuf,
        /// The last release (origin/master's newest `Release:` trailer); absent before the
        /// first. A version the checkout declares past it was never released: its section joins.
        #[arg(long, value_name = "VERSION")]
        last: Option<Version>,
    },
    /// Refuse unless the store, with every todo the checkout says it completed done, reads at
    /// least the checkout's version: what a land asks before it pushes.
    Check {
        /// The checkout to read.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
        /// The roadmap store, written out at its head.
        #[arg(long, value_name = "DIR")]
        store: PathBuf,
    },
    /// Record done, under each store's write lock, each todo given that is not done: on the
    /// mounted plan store when it holds the todo, else on the `[upkeep]` chain. A land's write,
    /// idempotent; it prints what it wrote where.
    Complete {
        /// The todos.
        #[arg(required = true)]
        todos: Vec<String>,
        /// The release that completes them: a roadmap completion's whole note.
        #[arg(long, value_name = "VERSION")]
        release: Version,
        /// `<todo>=<note>`: the note an upkeep completion carries (the PR that completed it);
        /// never written to the roadmap, which names nothing of the private repository.
        #[arg(long = "note", value_name = "TODO=NOTE")]
        notes: Vec<String>,
        /// Who records them.
        #[arg(long, default_value = "assistant")]
        actor: String,
    },
    /// Print the directory of the mounted plan store, the clone a land commits to; nothing where
    /// no mount holds a plan, so a deployment without one cuts no release.
    Store,
    /// Read the checkout's `changelog.d/` as a cut does: the merge gate's `changelog` step.
    Fragments {
        /// The checkout to read.
        #[arg(long, default_value = ".")]
        tree: PathBuf,
    },
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let now = SystemClock.now();
    match &args.action {
        Action::Fragments { tree } => read_fragments(tree),
        Action::Read { json, store } => {
            let source = match store {
                Some(dir) => Source::at(dir, now)?,
                None => Source::mounted(&Instance::open(path)?, now)?,
            };
            read(&source, *json)
        }
        Action::Known { store, todos } => {
            let source = Source::at(store, now)?;
            let instance = Instance::open(path)?;
            match found(&source, &upkeep(&instance)?, todos) {
                Ok(places) => {
                    for (todo, place) in places {
                        println!("{} {}", place.word(), todo.as_str());
                    }
                    Ok(())
                }
                Err(unknown) => {
                    eprintln!("suzatary: {unknown}");
                    std::process::exit(UNKNOWN);
                }
            }
        }
        Action::Items {
            release,
            add,
            remove,
            actor,
        } => {
            let instance = Instance::open(path)?;
            items(&instance, now, release, add, remove, actor.as_deref())
        }
        Action::Cut {
            completes,
            actor,
            tree,
            store,
            last,
        } => cut(
            &Source::at(store, now)?,
            &Asked {
                completes,
                actor,
                tree,
                last: *last,
            },
        ),
        Action::Check { tree, store } => check(&Source::at(store, now)?, tree),
        Action::Complete {
            todos,
            release,
            notes,
            actor,
        } => complete(
            &Instance::open(path)?,
            now,
            todos,
            &Noted {
                release: *release,
                notes,
            },
            actor,
        ),
        Action::Store => {
            if let Some(project) = Instance::open(path)?.mounts().project()? {
                println!("{}", project.store.display());
            }
            Ok(())
        }
    }
}

/// What `release known` exits with when a todo is found nowhere.
const UNKNOWN: i32 = 3;

/// A store read once for its releases: its objects, whom it does not believe, and its plan.
struct Source {
    nodes: std::sync::Arc<[Node]>,
    untrusted: Untrusted,
    plan: Plan,
    now: Datetime,
}

impl Source {
    /// The store at `dir` with the plan beside it, believing every writer, as `--store` reads.
    fn at(dir: &Path, now: Datetime) -> anyhow::Result<Source> {
        let plan = project::beside(dir)?
            .ok_or_else(|| anyhow::anyhow!("{} holds no {}", dir.display(), project::PLAN))?;
        let untrusted = Untrusted::none();
        let nodes = chain_at(dir.to_owned(), untrusted.clone())
            .nodes()
            .with_context(|| format!("reading the store at {}", dir.display()))?;
        Ok(Source {
            nodes,
            untrusted,
            plan,
            now,
        })
    }

    /// The plan store the instance mounts, under its chain's trust policy.
    fn mounted(instance: &Instance, now: Datetime) -> anyhow::Result<Source> {
        let (dir, plan) = mounted(instance)?;
        Ok(Source {
            nodes: store_of(dir).nodes().context("reading the plan store")?,
            untrusted: dir.policy.untrusted.clone(),
            plan,
            now,
        })
    }

    fn binding(&self) -> impl Iterator<Item = &suzatary_record::record::TodoEvent> {
        binding(&self.nodes, &self.untrusted)
    }

    /// Each todo's spec in force.
    fn current(&self) -> anyhow::Result<BTreeMap<TodoId, Vec<Term>>> {
        Ok(Believed::of(&self.nodes, self.now, &self.untrusted)
            .context("folding the store")?
            .specs)
    }

    /// A todo's action line as plain text, `None` when the store holds no record of it.
    fn title(&self, todo: &TodoId) -> Option<String> {
        title(&self.nodes, self.now, todo)
    }
}

fn title(nodes: &[Node], now: Datetime, todo: &TodoId) -> Option<String> {
    suzatary_todo::todos::current(nodes, now, todo.as_str())
        .map(|record| markup_text(record.payload.body.as_str()))
}

fn completed(nodes: &[Node], untrusted: &Untrusted, todo: &TodoId) -> bool {
    binding(nodes, untrusted)
        .any(|event| matches!(event, TodoEvent::Completed(e) if e.todo == *todo))
}

fn mounted(instance: &Instance) -> anyhow::Result<(&ChainDir, Plan)> {
    let project = instance.mounts().project()?.ok_or_else(|| {
        anyhow::anyhow!(
            "no chain of {} holds a {} beside its store, so no store counts releases",
            instance.path.display(),
            project::PLAN
        )
    })?;
    Ok((instance.chain(Some(project.chain.as_str()))?, project.plan))
}

/// Where a `Completes:` todo is found: the place answers, never a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The roadmap holds it: a release counts it, and the land completes it there.
    Roadmap,
    /// Only the deployment's upkeep chain holds it: a private task, completed there.
    Upkeep,
}

impl Place {
    fn word(self) -> &'static str {
        match self {
            Place::Roadmap => "roadmap",
            Place::Upkeep => "upkeep",
        }
    }
}

/// The `[upkeep]` chain's objects, none where the instance names no upkeep chain.
fn upkeep(instance: &Instance) -> anyhow::Result<std::sync::Arc<[Node]>> {
    match &instance.deployment.upkeep {
        Some(name) => store_of(instance.chain(Some(name.as_str()))?)
            .nodes()
            .context("reading the upkeep chain"),
        None => Ok(std::sync::Arc::from(Vec::new())),
    }
}

/// Each todo with where it is found, the roadmap first; a refusal names those found nowhere.
fn found(
    roadmap: &Source,
    upkeep: &[Node],
    todos: &[String],
) -> anyhow::Result<Vec<(TodoId, Place)>> {
    let mut places = Vec::new();
    let mut unknown = Vec::new();
    for id in todos {
        let Ok(todo) = TodoId::new(id.clone()) else {
            unknown.push(id.as_str());
            continue;
        };
        if roadmap.title(&todo).is_some() {
            places.push((todo, Place::Roadmap));
        } else if title(upkeep, roadmap.now, &todo).is_some() {
            places.push((todo, Place::Upkeep));
        } else {
            unknown.push(id.as_str());
        }
    }
    if unknown.is_empty() {
        Ok(places)
    } else {
        bail!(
            "no todo {} on the roadmap or the upkeep chain",
            unknown.join(", ")
        )
    }
}

/// The todos named, each one the store holds, or a refusal naming those it does not.
fn held(source: &Source, todos: &[String]) -> anyhow::Result<Vec<TodoId>> {
    let mut held = Vec::new();
    let mut unknown = Vec::new();
    for id in todos {
        match TodoId::new(id.clone()) {
            Ok(todo) if source.title(&todo).is_some() => held.push(todo),
            Ok(_) | Err(_) => unknown.push(id.as_str()),
        }
    }
    if unknown.is_empty() {
        Ok(held)
    } else {
        bail!("no todo {} in the roadmap store", unknown.join(", "))
    }
}

fn read(source: &Source, json: bool) -> anyhow::Result<()> {
    let reading = Reading::of(
        &source.plan,
        source.binding(),
        source.now,
        &source.current()?,
    );
    if json {
        let releases: Vec<_> = reading
            .releases
            .iter()
            .map(|r| {
                serde_json::json!({
                    "todo": r.todo.as_str(),
                    "title": source.title(&r.todo),
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
    println!("the store reads {}", reading.counted);
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

/// The store `dir` written through from the terminal, and the name `actor` writes under there.
fn writer(
    instance: &Instance,
    dir: &ChainDir,
    actor: Option<&str>,
) -> anyhow::Result<(
    suzatary_store::store::Signing<
        suzatary_io::objects::Objects<suzatary_record::record::TodoEvent>,
    >,
    String,
)> {
    let actor = super::todo::named(dir, actor)?;
    if !dir.policy.roster.iter().any(|a| a.as_str() == actor) {
        bail!("{actor:?} is not on the {} chain's roster", dir.policy.name);
    }
    let writer = Instance::writer_of(
        dir,
        &instance.signers(),
        &prodrome_core::event::Actor::new(actor.as_str()).map_err(|e| anyhow::anyhow!("{e}"))?,
        crate::commands::door::Opened::AtTerminal,
    );
    Ok((writer, actor))
}

/// One trusted revision of the release's spec, read and written under the plan store's write lock.
fn items(
    instance: &Instance,
    now: Datetime,
    release: &str,
    add: &[String],
    remove: &[String],
    actor: Option<&str>,
) -> anyhow::Result<()> {
    let source = Source::mounted(instance, now)?;
    let plan = &source.plan;
    if !plan.releases().iter().any(|r| r.todo.as_str() == release) {
        bail!("{release:?} is not a release the plan declares");
    }
    held(&source, add)?;
    let (dir, _) = mounted(instance)?;
    let (writer, actor) = writer(instance, dir, actor)?;
    let actor = actor.as_str();
    let _held = writer.locked().context("taking the store's write lock")?;
    let nodes = writer.nodes().context("reading the plan store")?;
    let Some(parent) = suzatary_todo::todos::current(&nodes, now, release) else {
        bail!("no todo {release:?} in the plan store");
    };
    let functions = Believed::of(&nodes, now, &dir.policy.untrusted)
        .context("folding the plan store")?
        .linkable();
    let event = suzatary_todo::authoring::rescope(&parent, add, remove, &functions, now, actor)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    writer.append(event).context("appending the revision")?;
    let after = writer.nodes().context("re-reading the plan store")?;
    let current = Believed::of(&after, now, &dir.policy.untrusted)
        .context("folding the plan store")?
        .specs;
    let reading = Reading::of(plan, binding(&after, &dir.policy.untrusted), now, &current);
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

/// Each todo found and not done, recorded done where it is found; every todo is placed before
/// anything is written, so one found nowhere writes nothing.
fn complete(
    instance: &Instance,
    now: Datetime,
    todos: &[String],
    noted: &Noted<'_>,
    actor: &str,
) -> anyhow::Result<()> {
    let by_todo = noted
        .notes
        .iter()
        .map(|pair| {
            pair.split_once('=')
                .map(|(todo, note)| (todo.to_owned(), note.to_owned()))
                .ok_or_else(|| anyhow::anyhow!("{pair:?} is not <todo>=<note>"))
        })
        .collect::<anyhow::Result<BTreeMap<String, String>>>()?;
    let roadmap = Source::mounted(instance, now)?;
    let places = found(&roadmap, &upkeep(instance)?, todos)?;
    let mut written: BTreeMap<&str, Vec<String>> =
        BTreeMap::from([("roadmap", Vec::new()), ("upkeep", Vec::new())]);
    for place in [Place::Roadmap, Place::Upkeep] {
        let todos: Vec<&TodoId> = places
            .iter()
            .filter(|(_, at)| *at == place)
            .map(|(todo, _)| todo)
            .collect();
        if todos.is_empty() {
            continue;
        }
        let dir = match place {
            Place::Roadmap => mounted(instance)?.0,
            Place::Upkeep => instance.chain(
                instance
                    .deployment
                    .upkeep
                    .as_ref()
                    .map(suzatary_base::chains::ChainName::as_str),
            )?,
        };
        let (writer, actor) = writer(instance, dir, Some(actor))?;
        let _held = writer.locked().context("taking the store's write lock")?;
        let held = writer.nodes().context("reading the store")?;
        for todo in todos {
            if completed(&held, &dir.policy.untrusted, todo) {
                continue;
            }
            // A public store's event names nothing of the private repository, as it names only
            // public actors (`ChainPolicy::named`): the roadmap's note is the release alone.
            let note = match place {
                Place::Roadmap => format!("the release {}", noted.release),
                Place::Upkeep => by_todo.get(todo.as_str()).cloned().unwrap_or_default(),
            };
            let event = prodrome_core::event::mk_completed(todo.as_str(), now, &actor, &note)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            writer
                .append(event)
                .with_context(|| format!("recording {} done", todo.as_str()))?;
            written
                .entry(place.word())
                .or_default()
                .push(todo.as_str().to_owned());
        }
    }
    println!("{}", serde_json::json!(written));
    Ok(())
}

/// What `release complete` writes as each completion's note.
struct Noted<'a> {
    release: Version,
    notes: &'a [String],
}

/// The two files a release writes under `tree`, and the fragments it folds.
struct Files {
    changelog: PathBuf,
    fragments: PathBuf,
    released: PathBuf,
}

impl Files {
    fn of(tree: &Path) -> Files {
        Files {
            changelog: tree.join("CHANGELOG.md"),
            fragments: tree.join("changelog.d"),
            released: tree.join("app/release.toml"),
        }
    }
}

fn released(files: &Files) -> anyhow::Result<Released> {
    Released::parse(&readable(&files.released)?)
        .with_context(|| format!("reading {}", files.released.display()))
}

/// The fragments in `dir` (`changelog.d/`), folded in the order of their names, and their paths.
fn fragments(dir: &Path) -> anyhow::Result<(Vec<PathBuf>, Fragment)> {
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        listed => listed?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()?,
    };
    paths.sort();
    let mut folded = Fragment::default();
    for path in &paths {
        anyhow::ensure!(
            path.extension().is_some_and(|e| e == "md"),
            "{} is no `.md` fragment",
            path.display()
        );
        let fragment =
            Fragment::parse(&readable(path)?).with_context(|| path.display().to_string())?;
        folded = folded.append(fragment);
    }
    Ok((paths, folded))
}

fn read_fragments(tree: &Path) -> anyhow::Result<()> {
    let (paths, _) = fragments(&Files::of(tree).fragments)?;
    println!("{} fragment(s) read as a cut reads them", paths.len());
    Ok(())
}

fn readable(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

struct Asked<'a> {
    completes: &'a [String],
    actor: &'a str,
    tree: &'a Path,
    last: Option<Version>,
}

fn cut(source: &Source, asked: &Asked<'_>) -> anyhow::Result<()> {
    let todos = held(source, asked.completes)?;
    let reading = Reading::assuming(
        &source.plan,
        source.binding(),
        &todos,
        asked.actor,
        source.now,
        &source.current()?,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let files = Files::of(asked.tree);
    let existing = readable(&files.released)?;
    let declared = Released::parse(&existing)
        .with_context(|| format!("reading {}", files.released.display()))?
        .version;
    let version = Version::after(asked.last, reading.counted)?;
    let completes = titled(source, &todos);
    let release = Cut {
        version,
        completes,
        folds: (asked.last != Some(declared)).then_some(declared),
    };
    let now = source.now;
    let day = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), now.day())
        .context("the clock's day")?;
    let (paths, folded) = fragments(&files.fragments)?;
    let changelog = release.changelog(&readable(&files.changelog)?, &folded, day)?;
    write(&files.changelog, &changelog)?;
    for path in &paths {
        std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
    }
    write(&files.released, &release.released().print_over(&existing))?;
    println!("{}", printed(&release));
    Ok(())
}

/// `todos`, each with its title in the store (its id where the store holds none): the words a
/// cut lists in the changelog and prints, never what the public release file stores.
fn titled(source: &Source, todos: &[TodoId]) -> Completes {
    Completes::new(todos.iter().map(|todo| {
        Completed {
            todo: todo.clone(),
            title: source
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

fn check(source: &Source, tree: &Path) -> anyhow::Result<()> {
    let Released {
        version,
        completes: todos,
    } = released(&Files::of(tree))?;
    let reading = Reading::assuming(
        &source.plan,
        source.binding(),
        &todos,
        "assistant",
        source.now,
        &source.current()?,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    // At least, not exactly: a release completed since the cut is the next train's, and the
    // tag stays the cut. Below it, the tree claims more than the store holds.
    if reading.counted < version.counted() {
        bail!(
            "the roadmap, with {} done, reads {}, below the tree's {version}: the tree claims \
             more than the store holds, so build the train again",
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
            completes: titled(source, &todos),
            folds: None,
        })
    );
    Ok(())
}
