//! `suzatary coding`: a coding session's two effects on this box.
//!
//! The fire is reached only by the owner's tap (`Coder` is in no effect record); the
//! poll reads `claude/` branches for a report and never merges, builds or promotes one.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use suzatary_core::capability::Clock as _;
use suzatary_core::coding::{Fired, RoutineId, Task};
use suzatary_core::secretary::coding::Advanced;
use suzatary_core::secretary::responded::{Done, Performed, Tried};
use suzatary_core::secretary::store::ConversationId;
use suzatary_io::conversations::ConversationStores;
use suzatary_io::routine::{Endpoint, FireError};
use suzatary_io::secrets::Resolves;

use crate::instance::Instance;

/// How often the server polls while any session is fired or reported.
pub const EVERY: Duration = Duration::from_mins(10);

/// `suzatary coding`.
#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    command: Verb,
}

#[derive(Debug, clap::Subcommand)]
enum Verb {
    /// Read the `claude/` branches once and move every session a report or merge speaks for.
    Poll {
        /// Move nothing: print each waiting session, each branch seen with its reports and the
        /// sessions they name, and how the poll reads each session.
        #[arg(long)]
        explain: bool,
    },
}

/// Where a tap's fire goes.
pub enum Firing {
    /// The instance declares no `[coding]`: a tap is refused in words.
    Undeclared,
    /// The routine's fire endpoint at `base`, its token resolved through `secrets` at each
    /// fire; a test points `base` at a loopback server.
    Endpoint {
        /// Where the endpoint is.
        base: String,
        /// Which routine.
        routine: RoutineId,
        /// Where the token is read from: secretspec.
        secrets: Arc<dyn Resolves + Send + Sync>,
    },
}

impl Firing {
    /// This instance's routine at the real endpoint, or none.
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        instance
            .deployment
            .coding
            .as_ref()
            .map_or(Self::Undeclared, |coding| Self::Endpoint {
                base: suzatary_io::routine::API.to_owned(),
                routine: coding.routine.clone(),
                secrets: Arc::new(instance.secrets()),
            })
    }

    fn fire(&self, task: &Task) -> Result<Fired, Performed> {
        match self {
            Self::Undeclared => Err(Performed::Tried(Tried::NotSent {
                reason: "this deployment declares no [coding] routine; nothing was started"
                    .to_owned(),
            })),
            Self::Endpoint {
                base,
                routine,
                secrets,
            } => Endpoint::resolve_over(
                base.clone(),
                routine.clone(),
                secrets.as_ref(),
                suzatary_io::http::Ureq::new(),
            )
            .and_then(|endpoint| endpoint.fire(task))
            .map_err(|failed| failed_fire(&failed)),
        }
    }
}

/// Classify a failed fire by whether a session may still have started: a 4xx-shaped
/// refusal started none, while a lost connection, a 5xx or an unreadable 2xx may have.
fn failed_fire(failed: &FireError) -> Performed {
    let reason = failed.to_string();
    match failed {
        FireError::NoToken
        | FireError::Locked(_)
        | FireError::Unauthorized(_)
        | FireError::Throttled(_)
        | FireError::Refused { status: ..500, .. } => Performed::Tried(Tried::NotSent { reason }),
        FireError::Unreachable(_) | FireError::Refused { .. } | FireError::Malformed(_) => {
            Performed::Done(Done::MayHaveGone { reason })
        }
    }
}

/// What a tapped coding session is started through, and where its lines land.
pub struct Coder {
    /// The routine.
    pub firing: Firing,
    /// The conversations a session's lines are written into: the server's own handle.
    pub conversations: crate::commands::secretary::ActingConversations,
}

impl Coder {
    /// Fire the routine with `task` and nothing else, write the session's first line,
    /// and answer the outcome the tap records; a refusal is an outcome in words.
    #[must_use]
    pub fn start(&self, id: &ConversationId, index: usize, task: &Task) -> Performed {
        let fired = match self.firing.fire(task) {
            Ok(fired) => fired,
            Err(failed) => return failed,
        };
        let url = fired.url.to_string();
        match suzatary_core::secretary::coding::fired(
            self.conversations.conversations(),
            id,
            index,
            fired,
            suzatary_io::clock::SystemClock.now(),
        ) {
            Ok(_) => Performed::Done(Done::Acted {
                what: format!("started a coding session: {url}"),
            }),
            // The session is running either way, so the outcome still names it.
            Err(refused) => Performed::Done(Done::MayHaveGone {
                reason: format!(
                    "the session started at {url}, but its line was not written: {}",
                    refused.message()
                ),
            }),
        }
    }
}

/// One poll: nothing is fetched while no session waits; otherwise the `claude/`
/// branches are read once and every session they speak for moves.
///
/// # Errors
/// The mirror's or the store's own failures.
pub fn poll(
    conversations: &ConversationStores,
    mirrors: &suzatary_io::repos::Mirrors,
    repo: &suzatary_core::repos::RepoName,
) -> anyhow::Result<Vec<Advanced>> {
    let waiting = waiting(conversations)?;
    if waiting.is_empty() {
        tracing::debug!("coding poll: no session waits, so nothing is fetched");
        return Ok(Vec::new());
    }
    let seen = seen(mirrors, repo)?;
    for line in suzatary_core::secretary::coding::explained(&waiting, &seen) {
        tracing::debug!("coding poll: {line}");
    }
    suzatary_core::secretary::coding::advance(
        conversations.conversations(),
        &seen,
        suzatary_io::clock::SystemClock.now(),
    )
    .map_err(|refused| anyhow::anyhow!("{}", refused.message()))
}

/// Every session still waiting on a branch.
fn waiting(conversations: &ConversationStores) -> anyhow::Result<Vec<Advanced>> {
    suzatary_core::secretary::coding::waiting(conversations.conversations())
        .map_err(|refused| anyhow::anyhow!("{}", refused.message()))
}

/// Every `claude/` branch of `repo`, as the mirror holds it after a fetch.
fn seen(
    mirrors: &suzatary_io::repos::Mirrors,
    repo: &suzatary_core::repos::RepoName,
) -> anyhow::Result<Vec<suzatary_core::coding::Seen>> {
    mirrors
        .seen(repo)
        .map_err(|failed| anyhow::anyhow!("reading {repo}: {failed}"))
}

/// What `suzatary coding poll --explain` prints. It moves nothing; it fetches the mirror as a
/// poll would, and reads the branches even while no session waits.
///
/// # Errors
/// The mirror's or the store's own failures.
pub fn explain(
    conversations: &ConversationStores,
    mirrors: &suzatary_io::repos::Mirrors,
    repo: &suzatary_core::repos::RepoName,
) -> anyhow::Result<Vec<String>> {
    Ok(suzatary_core::secretary::coding::explained(
        &waiting(conversations)?,
        &seen(mirrors, repo)?,
    ))
}

/// The server's poll, every [`EVERY`], one [`poll_once`] a tick.
pub async fn polling(
    conversations: crate::commands::secretary::ActingConversations,
    mirrors: Arc<suzatary_io::repos::Mirrors>,
    repo: suzatary_core::repos::RepoName,
    workers: Arc<crate::commands::serve::Workers>,
) {
    let mut every = tokio::time::interval(EVERY);
    loop {
        every.tick().await;
        poll_once(conversations, &mirrors, &repo, &workers).await;
    }
}

/// One tick of the server's poll, off the async threads: each session moved wakes its
/// conversation's page, and a poll that fails or panics is logged and answers how many
/// sessions moved (none), so the server and the next tick go on.
pub async fn poll_once(
    conversations: crate::commands::secretary::ActingConversations,
    mirrors: &Arc<suzatary_io::repos::Mirrors>,
    repo: &suzatary_core::repos::RepoName,
    workers: &crate::commands::serve::Workers,
) -> usize {
    let (mirrors, repo) = (Arc::clone(mirrors), repo.clone());
    match tokio::task::spawn_blocking(move || poll(conversations, &mirrors, &repo)).await {
        Ok(Ok(moved)) => {
            for advanced in &moved {
                tracing::info!(
                    conversation = %advanced.conversation,
                    proposal = advanced.index,
                    state = advanced.session.word(),
                    "coding session moved"
                );
                workers.announce(&advanced.conversation);
            }
            moved.len()
        }
        Ok(Err(failed)) => {
            tracing::warn!(
                "coding poll: {}",
                suzatary_core::excerpt::Excerpt::of(&format!("{failed:#}"))
            );
            0
        }
        Err(panicked) => {
            tracing::warn!("coding poll: {panicked}");
            0
        }
    }
}

/// Spawn the server's poll when `[coding]` names a routine, reading the server's own `conversations`.
pub fn spawn_polling(
    instance: &Instance,
    conversations: crate::commands::secretary::ActingConversations,
    workers: &Arc<crate::commands::serve::Workers>,
) {
    if let Some(coding) = &instance.deployment.coding {
        tokio::spawn(polling(
            conversations,
            crate::commands::serve::interpreters::mirrors(instance),
            coding.repo.clone(),
            Arc::clone(workers),
        ));
    }
}

/// `suzatary coding poll`.
///
/// # Errors
/// No `[coding]` in the instance, or the poll's own failures.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let Verb::Poll {
        explain: explaining,
    } = args.command;
    let coding =
        instance.deployment.coding.clone().ok_or_else(|| {
            anyhow::anyhow!("{}: no [coding] routine is declared", path.display())
        })?;
    let (conversations, mirrors) = (
        instance.conversation_stores(),
        crate::commands::serve::interpreters::mirrors(&instance),
    );
    if explaining {
        for line in explain(&conversations, &mirrors, &coding.repo)? {
            println!("{line}");
        }
        return Ok(());
    }
    let moved = poll(&conversations, &mirrors, &coding.repo)?;
    if moved.is_empty() {
        println!("no session moved; `suzatary coding poll --explain` says why");
    }
    for advanced in moved {
        println!(
            "{} proposal {}: {}",
            advanced.conversation,
            advanced.index,
            advanced.session.word()
        );
    }
    Ok(())
}
