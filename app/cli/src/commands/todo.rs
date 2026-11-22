//! `suzatary todo`; `confirm` is human-only like sending, with no MCP twin: an agent that can
//! confirm its own writes has confirmed nothing.

use std::path::Path;

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::{Actor, TodoId, mk_cancelled, mk_completed, mk_reopened, mk_tended};
use prodrome_core::policy::Untrusted;
use suzatary_base::capability::Clock;
use suzatary_io::clock::SystemClock;
use suzatary_record::todo;
use suzatary_signatures::believed::Preview;
use suzatary_signatures::signatures::Signers;
use suzatary_store::store::{Prodrome, Signing};
use suzatary_todo::{authoring, view};

use super::store_at::StoreAt;
use crate::instance::{ChainDir, Instance};
use suzatary_base::chains::ChainName;
use suzatary_base::reference::{GenesisPrefix, GlobalTodo, Reference};
use suzatary_plan::wait::Audience;
use suzatary_signatures::chain_set::Read;
use suzatary_store::store::Reads as _;

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
    /// The chain this act touches; absent is the instance file's first. A todo lives in one
    /// chain and cannot be moved, since the Prodrome does not rewrite.
    #[arg(long, global = true, value_name = "NAME")]
    chain: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// One todo as the chain holds it: its block, its fields, its authorship, and its name in
    /// every store (`todo:+<genesis>/<id>`).
    Show {
        /// The todo: its id in `--chain`, `todo:<chain>/<id>`, or `todo:+<genesis>/<id>`, which
        /// names it in whichever store held here begins with that genesis.
        id: String,
        #[command(flatten)]
        store: StoreAt,
        #[command(flatten)]
        at: super::at::At,
    },
    #[command(flatten)]
    Act(Box<Act>),
}

#[derive(Debug, Subcommand)]
pub enum Act {
    /// Confirm a triage-authored todo: re-append its content, trusted.
    ///
    /// HUMAN-ONLY, structurally, exactly like sending — there is no MCP twin
    /// and there must never be one.
    Confirm {
        /// The todo.
        id: String,
        /// Who is confirming; the chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
        /// What they checked.
        #[arg(long, default_value = "")]
        note: String,
    },
    /// Author a new todo from fields — the same builder the agent's verb uses.
    Add {
        /// The id, 3-64 lowercase letters and digits starting with a letter.
        id: String,
        /// The action line, as Typst markup, checked in the closed world before it is stored.
        #[arg(long)]
        body: String,
        /// Why this priority, in one sentence.
        #[arg(long)]
        rationale: String,
        /// `todo`, which takes a category, or `ekaTodo`, which sets its own.
        #[arg(long, default_value = "todo")]
        kind: String,
        /// One of this deployment's categories.
        #[arg(long, default_value = "")]
        category: String,
        /// What it is worth — exactly one spelling.
        #[command(flatten)]
        price: PriceFlags,
        /// The optional second paragraph, as Typst markup.
        #[arg(long, default_value = "")]
        detail: String,
        /// What it is blocked on — only when there is nothing we can do. A todo it waits on is
        /// `todo:<chain>/<id>`, or `todo:+<genesis>/<id>`, the one form a public chain takes.
        #[arg(long, default_value = "")]
        waiting_on: String,
        /// The todo this is a subtodo of: its spec gains a reference to this
        /// one, in the same act.
        #[arg(long)]
        parent: Option<String>,
        /// Who is authoring; the chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Change fields on a todo that already exists.
    Update {
        /// The todo.
        id: String,
        /// A new reason. REQUIRED whenever the price moves.
        #[arg(long)]
        rationale: Option<String>,
        /// A new price, in at most one spelling.
        #[command(flatten)]
        price: PriceFlags,
        /// A new action line, as Typst markup.
        #[arg(long)]
        body: Option<String>,
        /// A new detail, as Typst markup. Empty clears it. Exclusive with
        /// `--detail-append`.
        #[arg(long)]
        detail: Option<String>,
        /// A sentence to add under the existing detail, keeping its markup —
        /// use this, never `--detail`, when the detail already exists.
        #[arg(long)]
        detail_append: Option<String>,
        /// A new blocker, as plain text; a todo in it as `--waiting-on` on `add` names one.
        #[arg(long)]
        waiting_on: Option<String>,
        /// Set the blocker back to nothing.
        #[arg(long, conflicts_with = "waiting_on")]
        clear_waiting_on: bool,
        /// Who is authoring; the chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// Record it completed — TRUSTED, so it binds evaluation at once.
    Done(Lifecycle),
    /// Record it cancelled: moot rather than done.
    Cancel(Lifecycle),
    /// Record it open again. Absence from the environment IS openness.
    Reopen(Lifecycle),
    /// Record a pass at a recurring todo — TRUSTED, like `done`, and the todo
    /// stays open: a `Recur` in its price restarts from this instant.
    Tend(Lifecycle),
    /// Undo the last event on this todo by appending its inverse (`authoring::undo`).
    Undo(UndoRedo),
    /// Redo the event an undo just undid (`authoring::redo`).
    Redo(UndoRedo),
}

#[derive(Debug, clap::Args)]
pub struct PriceFlags {
    /// A flat fulfillment as a whole percentage. LOW is urgent.
    #[arg(long)]
    priority: Option<u32>,
    /// A deadline to decay toward, YYYY-MM-DD.
    #[arg(long)]
    deadline: Option<String>,
    /// The fulfillment at the start of the lead-up.
    #[arg(long, default_value_t = 60)]
    deadline_start: u32,
    /// The fulfillment at the deadline.
    #[arg(long, default_value_t = 5)]
    deadline_end: u32,
    /// How long the decay runs, in days.
    #[arg(long)]
    lead_up_days: Option<i64>,
    /// A chore done again and again: after every pass (`todo tend`) the
    /// fulfillment runs from `--from` to `--to` over this many days, then
    /// holds. Until the first pass the todo keeps the price it had.
    #[arg(long, value_name = "DAYS", group = "cycle", requires = "Ends")]
    recur_days: Option<u32>,
    /// A cycle on the calendar: from now on, every this many days the
    /// fulfillment starts again at `--from` and runs to `--to`, whatever
    /// is done.
    #[arg(long, value_name = "DAYS", group = "cycle", requires = "Ends")]
    periodic_days: Option<u32>,
    #[command(flatten)]
    ends: Option<Ends>,
}

/// A cycle's two ends, which clap admits together or not at all, and only with a cycle's days.
#[derive(Debug, Clone, Copy, clap::Args)]
#[group(requires = "cycle")]
pub struct Ends {
    /// Where a cycle starts, as a whole percentage.
    #[arg(long, value_name = "PERCENT", required = false, requires = "to")]
    from: u32,
    /// Where a cycle ends and stays, as a whole percentage.
    #[arg(long, value_name = "PERCENT", required = false, requires = "from")]
    to: u32,
}

impl PriceFlags {
    fn price(self) -> anyhow::Result<Option<todo::Price>> {
        let deadline = match self.deadline {
            None => None,
            Some(end) => Some(todo::Price::Deadline(todo::Deadline {
                end_date: chrono::NaiveDate::parse_from_str(&end, "%Y-%m-%d").map_err(|e| {
                    anyhow::anyhow!("--deadline must be YYYY-MM-DD, got {end:?} ({e})")
                })?,
                start_percent: self.deadline_start,
                end_percent: self.deadline_end,
                lead_up: self.lead_up_days.map(chrono::Duration::days),
                start_date: None,
            })),
        };
        // Clap admits a cycle's days only with its ends, so `ends` is there whenever days are.
        let cycle = |days: u32| {
            self.ends.map(|Ends { from, to }| todo::Cycle {
                days,
                from_percent: from,
                to_percent: to,
            })
        };
        let recur = self.recur_days.and_then(cycle);
        let periodic = self.periodic_days.and_then(cycle);
        let named: Vec<todo::Price> = [
            self.priority.map(|percent| todo::Price::Flat { percent }),
            deadline,
            recur.map(todo::Price::Recur),
            periodic.map(todo::Price::Periodic),
        ]
        .into_iter()
        .flatten()
        .collect();
        match <[todo::Price; 1]>::try_from(named) {
            Ok([one]) => Ok(Some(one)),
            Err(named) if named.is_empty() => Ok(None),
            Err(_) => bail!(
                "give one of --priority, --deadline, --recur-days or --periodic-days: \
                 a todo holds one price"
            ),
        }
    }
}

/// What `undo` and `redo` take; the note is computed from what is undone or redone.
#[derive(Debug, clap::Args)]
pub struct UndoRedo {
    /// The todo. It must be an id the chain already mentions.
    id: String,
    /// Who is undoing or redoing, a trusted actor; the chain's owner when absent.
    #[arg(long)]
    actor: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct Lifecycle {
    /// The todo. It must be an id the chain already mentions.
    id: String,
    /// A why, recorded on the event.
    #[arg(long, default_value = "")]
    note: String,
    /// Who is recording, a trusted actor; the chain's owner when absent.
    #[arg(long)]
    actor: Option<String>,
}

pub fn run(path: &Path, args: Args) -> anyhow::Result<()> {
    match args.action {
        Action::Show { id, store, at } => {
            let at = at.or_now();
            match shown_as(&id, args.chain.as_deref())? {
                Named::Here(chain, id) => show(&store.todos(path, chain.as_deref())?, at, &id),
                Named::Global(global) => {
                    let (read, stores) = store.every(path)?;
                    let held = super::todos::held(&read.set(at), &stores);
                    let todo =
                        suzatary_plan::wait::by_genesis(Audience::Deployment, &global, &held)
                            .map_err(|missing| {
                                anyhow::anyhow!("{global}: {}", super::todos::why(&missing))
                            })?;
                    show(&(read, todo.chain), at, todo.id.as_str())
                }
            }
        }
        Action::Act(act) => act_on(path, args.chain.as_deref(), *act),
    }
}

fn act_on(path: &Path, chain: Option<&str>, act: Act) -> anyhow::Result<()> {
    let instance = &Instance::open(path)?;
    let chain = instance.chain(chain)?;
    let keys = &instance.signers();
    let who = |actor: Option<String>| named(chain, actor.as_deref());
    match act {
        Act::Confirm { id, actor, note } => confirm(chain, keys, &id, &who(actor)?, &note),
        Act::Add {
            id,
            body,
            rationale,
            kind,
            category,
            price,
            detail,
            waiting_on,
            parent,
            actor,
        } => add(
            instance,
            chain,
            keys,
            &Authoring {
                id,
                body,
                rationale,
                kind,
                category,
                price: price.price()?.context(
                    "a todo needs a price: give --priority, --deadline, --recur-days \
                     or --periodic-days",
                )?,
                detail,
                waiting_on,
                parent,
                actor: who(actor)?,
            },
        ),
        Act::Update {
            id,
            rationale,
            price,
            body,
            detail,
            detail_append,
            waiting_on,
            clear_waiting_on,
            actor,
        } => update(
            instance,
            chain,
            keys,
            &id,
            &who(actor)?,
            // No category, checklist or deferral: those want this deployment's vocabulary, a
            // ticked line or a date picker, which are the app's edit form.
            &todo::TodoPatch {
                rationale: rationale.into(),
                price: price.price()?.into(),
                body: body.into(),
                detail: detail.into(),
                detail_append: detail_append.into(),
                waiting_on: waiting_on.into(),
                clear_waiting_on,
                ..todo::TodoPatch::default()
            },
        ),
        Act::Done(event) => lifecycle(chain, keys, "done", mk_completed, &event),
        Act::Cancel(event) => lifecycle(chain, keys, "cancel", mk_cancelled, &event),
        Act::Reopen(event) => lifecycle(chain, keys, "reopen", mk_reopened, &event),
        Act::Tend(event) => lifecycle(chain, keys, "tend", mk_tended, &event),
        Act::Undo(UndoRedo { id, actor }) => undo_redo(chain, keys, Turn::Undo, &id, &who(actor)?),
        Act::Redo(UndoRedo { id, actor }) => undo_redo(chain, keys, Turn::Redo, &id, &who(actor)?),
    }
}

fn confirm(
    chain: &ChainDir,
    keys: &Signers,
    id: &str,
    actor: &str,
    note: &str,
) -> anyhow::Result<()> {
    if !chain
        .policy
        .roster
        .iter()
        .any(|known| known.as_str() == actor)
    {
        let roster: Vec<&str> = chain.policy.roster.iter().map(Actor::as_str).collect();
        bail!("actor must be one of {roster:?}, got {actor:?}");
    }
    let store = writer(chain, keys, actor)?;
    let clock = SystemClock;
    let now = clock.now();

    // Under the write lock from read to append, so the record is not re-authored between.
    let _held = store.locked().context("taking the chain's write lock")?;
    let nodes = store.nodes().context("reading the chain")?;
    TodoId::new(id).map_err(|e| anyhow::anyhow!("{id:?} is not a todo id: {e}"))?;
    let Some(before) = suzatary_todo::todos::current(&nodes, now, id) else {
        bail!("unknown todo id {id:?} — the chain holds no content for it");
    };
    let from = before.actor.as_str().to_owned();
    let confirmed = authoring::confirm_todo(&before, now, actor, &chain.policy.untrusted, note)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    let digest = store.append(confirmed).context("appending")?;
    println!(
        "{}",
        serde_json::json!({
            "tip": digest.as_str(),
            "todo": id,
            "confirmed_from": from,
            "actor": actor,
        })
    );
    Ok(())
}

struct Authoring {
    id: String,
    /// Typst markup, admitted by the builder's door.
    body: String,
    rationale: String,
    kind: String,
    /// "" for a kind that sets its own.
    category: String,
    price: todo::Price,
    /// Typst markup.
    detail: String,
    /// Plain text.
    waiting_on: String,
    parent: Option<String>,
    actor: String,
}

/// The name `actor` (the chain's owner when absent) writes under on `chain`: on a public store its
/// public name, and refused where it has none (`ChainPolicy::named`).
pub(crate) fn named(chain: &ChainDir, actor: Option<&str>) -> anyhow::Result<String> {
    let actor =
        Actor::new(actor.unwrap_or(chain.owner.as_str())).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(chain.policy.named(&actor)?.as_str().to_owned())
}

/// The store `actor` writes through, after the roster check, signed by that actor's key: an
/// untrusted actor's write is the agent key's, never the box's.
fn writer(
    chain: &ChainDir,
    signers: &Signers,
    actor: &str,
) -> anyhow::Result<Signing<suzatary_io::objects::Objects<suzatary_record::record::TodoEvent>>> {
    on_the_roster(chain, actor)?;
    let actor = Actor::new(actor).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Instance::writer_of(
        chain,
        signers,
        &actor,
        crate::commands::door::Opened::AtTerminal,
    ))
}

/// Refuses an actor this chain does not know: a typo, not a new identity.
fn on_the_roster(chain: &ChainDir, actor: &str) -> anyhow::Result<()> {
    if chain
        .policy
        .roster
        .iter()
        .any(|known| known.as_str() == actor)
    {
        return Ok(());
    }
    let roster: Vec<&str> = chain.policy.roster.iter().map(Actor::as_str).collect();
    bail!("actor must be one of {roster:?}, got {actor:?}")
}

fn add(
    instance: &Instance,
    chain: &ChainDir,
    keys: &Signers,
    new: &Authoring,
) -> anyhow::Result<()> {
    let store = writer(chain, keys, &new.actor)?;
    let now = SystemClock.now();

    // Under the write lock from check to append, so the id is not taken between.
    let _held = store.locked().context("taking the chain's write lock")?;
    let mut preview =
        Preview::of(store.objects().context("reading the chain")?).context("reading the chain")?;
    if suzatary_todo::todos::current(preview.nodes(), now, &new.id).is_some() {
        bail!(
            "todo {:?} already exists — `todo update` it, or choose another id",
            new.id
        );
    }
    let known: Vec<&str> = instance
        .deployment
        .categories
        .keys()
        .map(String::as_str)
        .collect();
    let todo = authoring::NewTodo {
        todo: new.id.clone(),
        fields: suzatary_record::todo::TodoFields {
            body: new.body.clone(),
            rationale: new.rationale.clone(),
            kind: new.kind.clone(),
            category: new.category.clone(),
            price: new.price.clone(),
            detail: new.detail.clone(),
            waiting_on: new.waiting_on.clone(),
        },
        source: None,
        subtodos: Vec::new(),
    };
    let refused =
        |refusal: suzatary_base::refusal::Refusal| anyhow::anyhow!("{}", refusal.message());
    let written = match &new.parent {
        None => vec![
            authoring::build_todo(
                &todo,
                suzatary_base::zone::Moment {
                    at: now,
                    zone: instance.persona.zone,
                },
                &new.actor,
                &known,
                &suzatary_io::typeset::Closed::this_binary(),
            )
            .map_err(refused)?,
        ],
        Some(parent) => {
            let Some(parent) = suzatary_todo::todos::current(preview.nodes(), now, parent) else {
                bail!("no todo with id {parent:?} on the chain to be a parent");
            };
            let believed = suzatary_signatures::believed::Believed::of(
                preview.nodes(),
                now,
                &chain.policy.untrusted,
            )
            .context("folding the chain's functions")?;
            let (child, revision) = authoring::build_subtodo(
                &todo,
                &parent,
                &believed.linkable(),
                suzatary_base::zone::Moment {
                    at: now,
                    zone: instance.persona.zone,
                },
                &new.actor,
                &known,
                &suzatary_io::typeset::Closed::this_binary(),
            )
            .map_err(refused)?;
            vec![child, revision]
        }
    };
    appended(&store, &chain.policy.untrusted, &mut preview, written, now)
}

fn update(
    instance: &Instance,
    chain: &ChainDir,
    keys: &Signers,
    id: &str,
    actor: &str,
    edit: &todo::TodoPatch,
) -> anyhow::Result<()> {
    let store = writer(chain, keys, actor)?;
    let now = SystemClock.now();
    let _held = store.locked().context("taking the chain's write lock")?;
    let mut preview =
        Preview::of(store.objects().context("reading the chain")?).context("reading the chain")?;
    let Some(before) = suzatary_todo::todos::current(preview.nodes(), now, id) else {
        bail!("no todo with id {id:?} on the chain");
    };
    let known: Vec<&str> = instance
        .deployment
        .categories
        .keys()
        .map(String::as_str)
        .collect();
    let event = authoring::update_todo(
        &before,
        edit,
        suzatary_base::zone::Moment {
            at: now,
            zone: instance.persona.zone,
        },
        actor,
        &known,
        "",
        &suzatary_io::typeset::Closed::this_binary(),
    )
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    appended(
        &store,
        &chain.policy.untrusted,
        &mut preview,
        vec![event],
        now,
    )
}

/// Render against the chain as it would be, then append: a block that does not render is not
/// written.
fn appended(
    store: &impl Prodrome<suzatary_record::record::TodoEvent>,
    untrusted: &Untrusted,
    preview: &mut Preview,
    written: Vec<suzatary_record::record::TodoEvent>,
    now: suzatary_base::capability::Datetime,
) -> anyhow::Result<()> {
    for event in &written {
        preview.ahead(event.clone())?;
    }
    let mut blocks = Vec::with_capacity(written.len());
    for event in &written {
        let suzatary_record::record::TodoEvent::Authored(record) = event else {
            bail!("authoring answers an Authored");
        };
        let block = suzatary_todo::todos::render_current(preview.nodes(), now, untrusted, record)
            .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
        blocks.push((
            record.todo.as_str().to_owned(),
            record.actor.as_str().to_owned(),
            block,
        ));
    }
    for ((todo, actor, block), event) in blocks.into_iter().zip(written) {
        let digest = store.append(event).context("appending")?;
        println!(
            "{}",
            serde_json::json!({
                "tip": digest.as_str(),
                "todo": todo,
                "actor": actor,
                "block": block,
            })
        );
    }
    Ok(())
}

fn show(
    (read, name): &(Read, ChainName),
    at: prodrome_core::literal::Datetime,
    id: &str,
) -> anyhow::Result<()> {
    let set = read.set(at);
    let held = set.held(name).context("reading the chain")?;
    let by_hash = held
        .dag
        .objects()
        .iter()
        .map(|(name, envelope)| (name.clone(), envelope))
        .collect();
    let home = suzatary_signatures::chain_set::home(held.dag).unwrap_or(None);
    let entries = set.entries(name).context("folding the chain")?;
    let Some(entry) = entries.iter().find(|entry| entry.key.as_str() == id) else {
        bail!("no todo with id {id:?} on the chain");
    };
    let Some(shown) = suzatary_todo::shown::shown(entry, &by_hash)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?
    else {
        bail!("no todo with id {id:?} on the chain");
    };
    let prodrome = entry
        .genesis
        .clone()
        .or(home)
        .context("a store that holds a todo holds its prodrome")?;
    let printed = Printed {
        name: GlobalTodo {
            genesis: GenesisPrefix::whole(&prodrome),
            id: entry.key.clone(),
        }
        .to_string(),
        shown,
    };
    println!("{}", serde_json::to_string(&printed)?);
    Ok(())
}

/// `todo show`'s answer: the todo's name in every store beside what the chain holds of it.
#[derive(serde::Serialize)]
struct Printed<T> {
    name: String,
    #[serde(flatten)]
    shown: T,
}

enum Named {
    /// An id in a chain of this deployment, `--chain`'s or a local reference's.
    Here(Option<String>, String),
    Global(GlobalTodo),
}

fn shown_as(id: &str, chain: Option<&str>) -> anyhow::Result<Named> {
    if !id.contains(':') {
        return Ok(Named::Here(chain.map(str::to_owned), id.to_owned()));
    }
    match id.parse::<Reference>()? {
        Reference::Todo(local) => match chain {
            Some(named) if named != local.chain.as_str() => {
                bail!(
                    "{local} names the chain {}, and --chain names {named}",
                    local.chain
                )
            }
            _ => Ok(Named::Here(
                Some(local.chain.to_string()),
                local.id.as_str().to_owned(),
            )),
        },
        Reference::Global(global) => Ok(Named::Global(global)),
        other => bail!("{other} is not a todo"),
    }
}

/// A lifecycle event's `(todo, at, actor, note)` constructor.
type Build =
    fn(
        &str,
        prodrome_core::literal::Datetime,
        &str,
        &str,
    ) -> Result<suzatary_record::record::TodoEvent, prodrome_core::literal::ProdromeError>;

/// A trusted lifecycle event, binding at once; an id no event mentions is refused as a typo.
fn lifecycle(
    chain: &ChainDir,
    keys: &Signers,
    kind: &str,
    build: Build,
    event: &Lifecycle,
) -> anyhow::Result<()> {
    let Lifecycle { id, note, actor } = event;
    let actor = &named(chain, actor.as_deref())?;
    let store = writer(chain, keys, actor)?;
    let now = SystemClock.now();
    let _held = store.locked().context("taking the chain's write lock")?;
    let nodes = store.nodes().context("reading the chain")?;
    if !view::events_of(&nodes)
        .iter()
        .any(|e| e.todo().as_str() == id)
    {
        bail!("unknown todo id {id:?} — no event in the chain mentions it");
    }
    let event = build(id, now, actor, note).map_err(|e| anyhow::anyhow!("{e}"))?;
    let digest = store.append(event).context("appending")?;
    let after = store.nodes().context("re-reading the chain")?;
    let todo = TodoId::new(id).map_err(|e| anyhow::anyhow!("{e}"))?;
    let state = suzatary_signatures::believed::spelled(
        &suzatary_signatures::believed::Believed::of(&after, now, &chain.policy.untrusted)
            .context("folding the chain")?
            .outcome(&todo),
    );
    println!(
        "{}",
        serde_json::json!({
            "tip": digest.as_str(),
            "todo": id,
            "action": kind,
            "actor": actor,
            "state": state,
        })
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Turn {
    Undo,
    Redo,
}

impl Turn {
    fn as_str(self) -> &'static str {
        match self {
            Turn::Undo => "undo",
            Turn::Redo => "redo",
        }
    }
}

fn undo_redo(
    chain: &ChainDir,
    keys: &Signers,
    turn: Turn,
    id: &str,
    actor: &str,
) -> anyhow::Result<()> {
    let store = writer(chain, keys, actor)?;
    let now = SystemClock.now();
    let _held = store.locked().context("taking the chain's write lock")?;
    let nodes = store.nodes().context("reading the chain")?;
    let stream = view::stream_of(&nodes, id);
    if stream.is_empty() {
        bail!("unknown todo id {id:?} — no event in the chain mentions it");
    }
    let reversed = stream.last().map(|(hash, _)| hash.as_str().to_owned());
    let event = match turn {
        Turn::Undo => authoring::undo(&stream, id, now, actor, &chain.policy.untrusted),
        Turn::Redo => authoring::redo(&stream, id, now, actor, &chain.policy.untrusted),
    }
    .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    let appended = event.kind_name().to_owned();
    let digest = store.append(event).context("appending")?;
    println!(
        "{}",
        serde_json::json!({
            "tip": digest.as_str(),
            "todo": id,
            "action": turn.as_str(),
            "actor": actor,
            "appended": appended,
            "reversed": reversed,
        })
    );
    Ok(())
}
