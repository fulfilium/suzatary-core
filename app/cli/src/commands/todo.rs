//! `suzatary todo` — author, read and record todos on the chain, trusted.
//!
//! The same authoring functions the agent's MCP verbs call; only the actor differs: a
//! roster name here, `RecordedBy::Triage` there, and that asymmetry lives in the callers.
//!
//! `confirm` is human-only like sending, with no MCP twin: an agent that can confirm its
//! own writes has confirmed nothing.

use std::path::Path;

use anyhow::{Context, bail};
use clap::Subcommand;
use prodrome_core::event::{Actor, TodoId, mk_cancelled, mk_completed, mk_reopened, mk_tended};
use prodrome_core::policy::Untrusted;
use suzatary_core::believed::Preview;
use suzatary_core::capability::Clock;
use suzatary_core::signatures::Signers;
use suzatary_core::store::{Prodrome, Signing};
use suzatary_core::{authoring, todo, view};
use suzatary_io::clock::SystemClock;

use super::store_of;
use crate::instance::{ChainDir, Instance};
use suzatary_core::store::Reads as _;

/// Which `todo` act, and its own arguments.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Which act.
    #[command(subcommand)]
    action: Action,
    /// WHICH CHAIN this act touches (docs/ARCHITECTURE.md §10.2). Absent is the
    /// FIRST chain the instance file names — `life`, the owner's own todos — so
    /// every command written before there were two means what it always did.
    /// `--chain suzatary` writes to this project's own store: the roadmap, and
    /// the issues the system has about itself.
    ///
    /// A todo lives in ONE chain and cannot be moved: the Prodrome does not
    /// rewrite, so this is decided before the write and not afterwards.
    #[arg(long, global = true, value_name = "NAME")]
    chain: Option<String>,
}

/// EVERY `todo` ACT.
#[derive(Debug, Subcommand)]
pub enum Action {
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
        /// WHY this priority, in one sentence. Required, and not decoration.
        #[arg(long)]
        rationale: String,
        /// `todo` (needs a category), `ekaTodo` or `suzateryTodo`.
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
        /// What it is blocked on — only when there is nothing we can do.
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
        /// A new blocker, as plain text.
        #[arg(long)]
        waiting_on: Option<String>,
        /// Set the blocker back to nothing.
        #[arg(long, conflicts_with = "waiting_on")]
        clear_waiting_on: bool,
        /// Who is authoring; the chain's owner when absent.
        #[arg(long)]
        actor: Option<String>,
    },
    /// One todo as the chain holds it: its block, its fields, its authorship.
    Show {
        /// The todo.
        id: String,
    },
    /// Every open todo, ascending in fulfillment — LOW is urgent.
    List,
    /// Record it completed — TRUSTED, so it binds evaluation at once.
    Done(Lifecycle),
    /// Record it cancelled: moot rather than done.
    Cancel(Lifecycle),
    /// Record it open again. Absence from the environment IS openness.
    Reopen(Lifecycle),
    /// Record a pass at a recurring todo — TRUSTED, like `done`, and the todo
    /// stays open: a `Recur` in its price restarts from this instant.
    Tend(Lifecycle),
    /// Undo the LAST event on this todo — its inverse, appended
    /// (`authoring::undo`). The chain never rewrites, so this is one more
    /// event, not a rewound one; history keeps every step.
    Undo(UndoRedo),
    /// Redo the event an undo just undid — recognised STRUCTURALLY, with no
    /// flag and no stored marker (`authoring::redo`).
    Redo(UndoRedo),
}

/// A PRICE, AS FLAGS — every spelling `todo add` and `todo update` take, read
/// into one [`todo::Price`] by `PriceFlags::price`.
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
    /// Where a cycle starts and ends: both or neither, and only beside a cycle.
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
    /// The one price these flags name, or `None` for none; days here, weeks in the
    /// agent's verb, one duration in `Deadline`.
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

/// What `undo` and `redo` take: the todo and who is asking. No `--note` —
/// the note is computed from what is being undone or redone, exactly like
/// `confirm`'s two refusals are one function so the words cannot fork.
#[derive(Debug, clap::Args)]
pub struct UndoRedo {
    /// The todo. It must be an id the chain already mentions.
    id: String,
    /// Who is undoing or redoing. A trusted actor, unlike the agent — which
    /// has no undo verb at all (OPERATOR.md, SECRETARY.md). The chain's owner when absent.
    #[arg(long)]
    actor: Option<String>,
}

/// What the four lifecycle verbs take — the same shape, because they are the
/// same event with four names.
#[derive(Debug, clap::Args)]
pub struct Lifecycle {
    /// The todo. It must be an id the chain already mentions.
    id: String,
    /// A why, recorded on the event.
    #[arg(long, default_value = "")]
    note: String,
    /// Who is recording. A trusted actor, unlike the agent's `record`. The chain's owner when
    /// absent.
    #[arg(long)]
    actor: Option<String>,
}

/// `suzatary todo <act>` — EVERY act, in one place.
pub fn run(path: &Path, args: Args) -> anyhow::Result<()> {
    let instance = &Instance::open(path)?;
    let chain = instance.chain(args.chain.as_deref())?;
    // Resolved once; a key not at hand refuses the write, naming why.
    let keys = &instance.signers();
    let who = |actor: Option<String>| actor.unwrap_or_else(|| chain.owner.as_str().to_owned());
    match args.action {
        Action::Confirm { id, actor, note } => confirm(chain, keys, &id, &who(actor), &note),
        Action::Add {
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
                actor: who(actor),
            },
        ),
        Action::Update {
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
            &who(actor),
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
        Action::Show { id } => show(chain, &id),
        Action::List => list(chain),
        // One event under four names, differing only by constructor.
        Action::Done(event) => lifecycle(chain, keys, "done", mk_completed, &event),
        Action::Cancel(event) => lifecycle(chain, keys, "cancel", mk_cancelled, &event),
        Action::Reopen(event) => lifecycle(chain, keys, "reopen", mk_reopened, &event),
        Action::Tend(event) => lifecycle(chain, keys, "tend", mk_tended, &event),
        Action::Undo(UndoRedo { id, actor }) => {
            undo_redo(chain, keys, Turn::Undo, &id, &who(actor))
        }
        Action::Redo(UndoRedo { id, actor }) => {
            undo_redo(chain, keys, Turn::Redo, &id, &who(actor))
        }
    }
}

/// `suzatary todo confirm <id>` — re-append a provisional record as a trusted
/// actor.
///
/// `--note` is where a confirming human says what they checked.
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
    let Some(before) = suzatary_core::todos::current(&nodes, now, id) else {
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

/// What `suzatary todo add` was told, as a record so no call site can misorder it.
struct Authoring {
    /// The id.
    id: String,
    /// The action line, as Typst markup — admitted by the builder's door.
    body: String,
    /// Why this priority, in one sentence.
    rationale: String,
    /// The block kind.
    kind: String,
    /// The category, or "" for a kind that sets its own.
    category: String,
    /// What it is worth.
    price: todo::Price,
    /// The detail, as Typst markup.
    detail: String,
    /// What it is blocked on, as plain text.
    waiting_on: String,
    /// The todo it is a subtodo of.
    parent: Option<String>,
    /// Who is authoring.
    actor: String,
}

/// The store `actor` writes through, after the roster check: signed by the key that actor
/// owns, so an untrusted actor's write is the agent key's and never the box's.
fn writer(
    chain: &ChainDir,
    signers: &Signers,
    actor: &str,
) -> anyhow::Result<Signing<suzatary_io::objects::Objects<suzatary_core::record::TodoEvent>>> {
    on_the_roster(chain, actor)?;
    let actor = Actor::new(actor).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Instance::writer_of(chain, signers, &actor))
}

/// The roster check every write begins with: an actor this chain does not know is a
/// typo, not a new identity.
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

/// `suzatary todo add <id> --body … --rationale …`.
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
    if suzatary_core::todos::current(preview.nodes(), now, &new.id).is_some() {
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
        fields: suzatary_core::todo::TodoFields {
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
        |refusal: suzatary_core::refusal::Refusal| anyhow::anyhow!("{}", refusal.message());
    let written = match &new.parent {
        None => vec![
            authoring::build_todo(
                &todo,
                suzatary_core::zone::Moment {
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
            let Some(parent) = suzatary_core::todos::current(preview.nodes(), now, parent) else {
                bail!("no todo with id {parent:?} on the chain to be a parent");
            };
            let believed = suzatary_core::believed::Believed::of(
                preview.nodes(),
                now,
                &chain.policy.untrusted,
            )
            .context("folding the chain's functions")?;
            let (child, revision) = authoring::build_subtodo(
                &todo,
                &parent,
                &believed.linkable(),
                suzatary_core::zone::Moment {
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

/// `suzatary todo update <id> --priority … --rationale …`.
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
    let Some(before) = suzatary_core::todos::current(preview.nodes(), now, id) else {
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
        suzatary_core::zone::Moment {
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

/// Render against the chain as it would be, then append, as the MCP verbs do: a block
/// that does not parse back is a write that does not happen.
fn appended(
    store: &impl Prodrome<suzatary_core::record::TodoEvent>,
    untrusted: &Untrusted,
    preview: &mut Preview,
    written: Vec<suzatary_core::record::TodoEvent>,
    now: suzatary_core::capability::Datetime,
) -> anyhow::Result<()> {
    for event in &written {
        preview.ahead(event.clone())?;
    }
    let mut blocks = Vec::with_capacity(written.len());
    for event in &written {
        let suzatary_core::record::TodoEvent::Authored(record) = event else {
            bail!("authoring answers an Authored");
        };
        let block = suzatary_core::todos::render_current(preview.nodes(), now, untrusted, record)
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

/// `suzatary todo show <id>` — one todo as the chain holds it.
fn show(chain: &ChainDir, id: &str) -> anyhow::Result<()> {
    let store = store_of(chain);
    let now = SystemClock.now();
    let nodes = store.nodes().context("reading the chain")?;
    let Some(found) = suzatary_core::todos::current(&nodes, now, id) else {
        bail!("no todo with id {id:?} on the chain");
    };
    let block = suzatary_core::todos::render_current(&nodes, now, &chain.policy.untrusted, &found)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    println!(
        "{}",
        serde_json::json!({
            "todo": id,
            "kind": found.payload.kind.as_str(),
            "authored_by": found.actor.as_str(),
            "block": block,
        })
    );
    Ok(())
}

/// `suzatary todo list` — every open todo, ascending in fulfillment, so what hurts is
/// first.
fn list(chain: &ChainDir) -> anyhow::Result<()> {
    let store = store_of(chain);
    let now = SystemClock.now();
    let nodes = store.nodes().context("reading the chain")?;
    let mut open = suzatary_core::todos::open_todos(&nodes, now, &chain.policy.untrusted)
        .map_err(|refusal| anyhow::anyhow!("{}", refusal.message()))?;
    // A todo with no spec is unpriced, not fulfilled, and sorts last.
    open.sort_by(|a, b| {
        percent_of(a.priority.as_deref())
            .partial_cmp(&percent_of(b.priority.as_deref()))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    println!(
        "{}",
        serde_json::json!(
            open.iter()
                .map(|t| serde_json::json!({
                    "id": t.id,
                    "kind": t.kind,
                    "category": t.category,
                    "priority": t.priority,
                    "body": t.body,
                    "waiting_on": t.waiting_on,
                    "authored_by": t.authored_by,
                }))
                .collect::<Vec<_>>()
        )
    );
    Ok(())
}

/// A lifecycle event's `(todo, at, actor, note)` constructor.
type Build = fn(
    &str,
    prodrome_core::literal::Datetime,
    &str,
    &str,
) -> Result<suzatary_core::record::TodoEvent, prodrome_core::literal::ProdromeError>;

/// `suzatary todo done|cancel|reopen|tend <id>` — a trusted lifecycle event.
///
/// The kinds the agent's `record` writes; the actor differs, so these bind at once while
/// the agent's wait for confirmation. An id no event mentions is a typo, refused.
fn lifecycle(
    chain: &ChainDir,
    keys: &Signers,
    kind: &str,
    build: Build,
    event: &Lifecycle,
) -> anyhow::Result<()> {
    let Lifecycle { id, note, actor } = event;
    let actor = actor.as_deref().unwrap_or(chain.owner.as_str());
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
    // The fold's answer, not this command's guess about what it did.
    let after = store.nodes().context("re-reading the chain")?;
    let todo = TodoId::new(id).map_err(|e| anyhow::anyhow!("{e}"))?;
    let state = suzatary_core::believed::spelled(
        &suzatary_core::believed::Believed::of(&after, now, &chain.policy.untrusted)
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

/// Which way `todo undo|redo` turns a todo's history.
#[derive(Debug, Clone, Copy)]
enum Turn {
    /// The inverse of its last event.
    Undo,
    /// The event an undo just undid, again.
    Redo,
}

impl Turn {
    /// Its verb's name.
    fn as_str(self) -> &'static str {
        match self {
            Turn::Undo => "undo",
            Turn::Redo => "redo",
        }
    }
}

/// `suzatary todo undo|redo <id>` — the inverse of the todo's last event, or the event an
/// undo just undid, appended again; trusted like `done`.
///
/// Prints what was appended and what it reversed, since "it worked" does not say which
/// act happened; refusals are the route's own sentences.
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

/// `"12.34%"` as the number it is, and `f64::INFINITY` for absence.
fn percent_of(shown: Option<&str>) -> f64 {
    shown
        .and_then(|text| text.strip_suffix('%'))
        .and_then(|digits| digits.parse::<f64>().ok())
        .unwrap_or(f64::INFINITY)
}
