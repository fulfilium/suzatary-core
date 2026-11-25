//! The engine over the conversations: one task per conversation, nudged whenever a write
//! moves its heads, running the conversation's rules (`suzatary_topics::secretary::rules`) on each new reading.
//!
//! One task per conversation keeps two rounds from answering the same question; the engine's
//! memo fires each rule once per reading. The lock is held only for a lookup, insert or removal,
//! never across a firing, and a task leaves only under the lock [`Workers::nudge`] takes, so no
//! nudge is lost.

use prodrome_data::event::{Actor, Hash};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use suzatary_api::wire;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::{Halt, Picker, Turn};
use suzatary_conversations::secretary::store::ConversationState;
use suzatary_lines::secretary::transcript;
use suzatary_secretary::conversation::round::Ending;
use suzatary_secretary::secretary;
use suzatary_topics::secretary::rules::{Conversational, RULES};
use suzatary_triage::rule::{Engine, Firing};
use tokio::sync::{broadcast, mpsc};

use super::State;

/// How many frames a lagging subscriber may fall behind before it is told to refetch.
const FRAMES: usize = 256;

/// How long an idle worker waits for a nudge before it drops itself.
const IDLE: Duration = Duration::from_secs(30);

/// How long an erasure waits for the round it stops to end; a verb already running is not
/// interrupted, so a round can take longer, and then the erasure goes on.
const STOP_WAIT: Duration = Duration::from_secs(15);

/// One conversation's channel: a nudge, never a payload, since the next step is reread
/// from the file.
struct Worker {
    nudge: mpsc::Sender<()>,
}

/// One conversation's live stream: the partial is never written to the file, and one mutex
/// guards both it and every subscription so a snapshot and its broadcast position are the
/// same instant.
struct Stream {
    partial: Mutex<Partial>,
    frames: broadcast::Sender<wire::Frame>,
}

/// A round's reply as it streams: the text so far, and the blocks closed off its front.
#[derive(Default)]
struct Partial {
    text: String,
    closed: Vec<Closed>,
}

/// One closed block, kept for its render by the sha the page names it by.
struct Closed {
    sha: String,
    source: String,
}

impl Partial {
    /// Append a fragment, and the frames for every block it closed.
    fn push(&mut self, fragment: &str) -> Vec<wire::Frame> {
        self.text.push_str(fragment);
        let start: usize = self.closed.iter().map(|block| block.source.len()).sum();
        let cut = suzatary_base::answer::blocks(&self.text[start..]);
        let now = cut.len().saturating_sub(1);
        cut.into_iter()
            .take(now)
            .map(|block| {
                let closed = Closed {
                    sha: suzatary_base::answer::sha(&block.source),
                    source: block.source,
                };
                let frame = closed.frame(self.closed.len());
                self.closed.push(closed);
                frame
            })
            .collect()
    }

    /// Everything a subscriber arriving now needs: the text, then its closed blocks.
    fn replay(&self) -> Vec<wire::Frame> {
        (!self.text.is_empty())
            .then(|| wire::Frame::Delta {
                text: self.text.clone(),
            })
            .into_iter()
            .chain(
                self.closed
                    .iter()
                    .enumerate()
                    .map(|(n, closed)| closed.frame(n)),
            )
            .collect()
    }
}

impl Closed {
    fn frame(&self, n: usize) -> wire::Frame {
        wire::Frame::Block {
            n,
            sha: self.sha.clone(),
            source: self.source.clone(),
        }
    }
}

impl Stream {
    fn new() -> Self {
        let (frames, _receiver) = broadcast::channel(FRAMES);
        Self {
            partial: Mutex::new(Partial::default()),
            frames,
        }
    }

    /// Never touches the partial.
    fn state(&self, state: ConversationState) {
        let _ = self.frames.send(wire::Frame::State { state });
    }

    /// A fragment arrived: append it to the partial and broadcast it, as one locked step.
    fn delta(&self, fragment: &str) {
        let mut partial = super::locks::held(&self.partial);
        let closed = partial.push(fragment);
        let _ = self.frames.send(wire::Frame::Delta {
            text: fragment.to_owned(),
        });
        for frame in closed {
            let _ = self.frames.send(frame);
        }
    }

    /// A verb is about to run: a status broadcast, never part of the partial.
    fn working(&self, verb: &str) {
        let _ = self.frames.send(wire::Frame::Working {
            verb: verb.to_owned(),
        });
    }

    /// A round ended: clear the partial and broadcast the append, as one locked step.
    fn turn(&self) {
        let mut partial = super::locks::held(&self.partial);
        *partial = Partial::default();
        let _ = self.frames.send(wire::Frame::Turn);
    }

    /// Subscribe, with the frames that rebuild the partial as it stood at that instant.
    fn subscribe(&self) -> (broadcast::Receiver<wire::Frame>, Vec<wire::Frame>) {
        let partial = super::locks::held(&self.partial);
        let receiver = self.frames.subscribe();
        (receiver, partial.replay())
    }

    /// A closed block's Typst, by the sha a `block` frame named it.
    fn closed(&self, sha: &str) -> Option<String> {
        super::locks::held(&self.partial)
            .closed
            .iter()
            .find(|closed| closed.sha == sha)
            .map(|closed| closed.source.clone())
    }
}

/// The rounds a stop ends, as its conversation read after the take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ended {
    /// Those of these runs.
    Runs(std::collections::BTreeSet<suzatary_conversations::secretary::store::RunId>),
    /// The conversation did not read: the round running, whatever its run.
    Unread,
}

impl Ended {
    #[must_use]
    pub(crate) fn of<E>(read: &Result<Arc<transcript::Transcript>, E>, stop: &Hash) -> Self {
        match read {
            Ok(read) => Ended::Runs(read.ended_by(stop)),
            Err(_) => Ended::Unread,
        }
    }
}

/// Every conversation currently being answered; held on [`State`], never a static, because a
/// test binary runs more than one server.
pub struct Workers {
    inner: Mutex<HashMap<ConversationId, Worker>>,
    /// Every conversation's [`Stream`], never removed, since a subscriber may connect
    /// while no worker runs.
    streams: Mutex<HashMap<ConversationId, Arc<Stream>>>,
    /// The halt of each round running now, removed when it ends so a late stop cannot
    /// stop the next round.
    halts: Mutex<HashMap<ConversationId, Halt>>,
    /// Told whenever a round's halt is removed, so an erasure goes on once its round has ended.
    ended: Condvar,
    /// Every firing made, shared by every task.
    engine: Mutex<Engine<Conversational>>,
    idle: Duration,
}

impl Workers {
    #[must_use]
    pub fn new() -> Self {
        Self::with_idle(IDLE)
    }

    #[must_use]
    pub fn with_idle(idle: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            halts: Mutex::new(HashMap::new()),
            ended: Condvar::new(),
            engine: Mutex::new(Engine::new(RULES.to_vec())),
            idle,
        }
    }

    fn stream(&self, id: &ConversationId) -> Arc<Stream> {
        let mut streams = super::locks::held(&self.streams);
        Arc::clone(
            streams
                .entry(id.clone())
                .or_insert_with(|| Arc::new(Stream::new())),
        )
    }

    #[must_use]
    pub(crate) fn subscribe(
        &self,
        id: &ConversationId,
    ) -> (broadcast::Receiver<wire::Frame>, Vec<wire::Frame>) {
        self.stream(id).subscribe()
    }

    /// A block this round's stream closed, by sha; `None` for one no running round closed.
    #[must_use]
    pub(crate) fn closed(&self, id: &ConversationId, sha: &str) -> Option<String> {
        self.stream(id).closed(sha)
    }

    /// Tell the conversation's task its heads moved, starting one if none is running; called
    /// after the write, so the nudge carries nothing: the task reads again.
    pub(crate) fn nudge(&self, id: &ConversationId, state: Arc<State>) {
        let mut inner = super::locks::held(&self.inner);
        if let Some(worker) = inner.get(id) {
            // Full is fine: a nudge already stands and its round rereads the queue.
            let _ = worker.nudge.try_send(());
            return;
        }
        let (tx, rx) = mpsc::channel(1);
        inner.insert(id.clone(), Worker { nudge: tx });
        drop(inner);
        tokio::spawn(run(id.clone(), state, rx, self.idle));
    }

    /// Something was appended outside any round (an act's own turns): tell open pages
    /// to refetch.
    pub fn announce(&self, id: &ConversationId) {
        self.stream(id).turn();
    }

    /// A tab's stop was taken: the round running is told to stop where the transcript reads it as
    /// one the stop ends; the stop's line has closed that round already, so nothing waits for it,
    /// and a round begun since the take runs on.
    pub(crate) fn stopped(state: Arc<State>, id: &ConversationId, stop: Hash, by: Actor) {
        let id = id.clone();
        tokio::task::spawn_blocking(move || {
            let read = transcript::read(state.first.caps.secretary.conversations, &id);
            if let Err(failed) = &read {
                tracing::warn!(
                    "secretary: {id}'s stop was taken and its conversation did not read, so the round running is stopped: {failed}"
                );
            }
            state.workers.halt_ended(&id, &Ended::of(&read, &stop), by);
        });
    }

    /// The halt of the round running in `id` pulled, as `by`, where `ended` names its run, or
    /// whatever runs where the conversation did not read: the person asked to stop.
    pub(crate) fn halt_ended(&self, id: &ConversationId, ended: &Ended, by: Actor) {
        if let Some(halt) = super::locks::held(&self.halts).get(id) {
            let ends = match ended {
                Ended::Runs(runs) => halt.run().is_some_and(|run| runs.contains(run)),
                Ended::Unread => true,
            };
            if ends {
                halt.stop(by);
            }
        }
    }

    /// The running round stopped, waiting (at most `STOP_WAIT`) for it to end, so its model
    /// process is gone before an erasure deletes what the round held.
    pub(crate) fn halt_round(&self, id: &ConversationId, by: Actor) {
        self.wait_out(super::locks::held(&self.halts), id, by);
    }

    fn wait_out(
        &self,
        halts: std::sync::MutexGuard<'_, HashMap<ConversationId, Halt>>,
        id: &ConversationId,
        by: Actor,
    ) {
        let Some(halt) = halts.get(id) else {
            return;
        };
        halt.stop(by);
        // A round started since has a fresh halt nobody pulled, so it is not waited for.
        let (_halts, waited) = self
            .ended
            .wait_timeout_while(halts, STOP_WAIT, |halts| {
                halts.get(id).is_some_and(Halt::halted)
            })
            .unwrap_or_else(PoisonError::into_inner);
        if waited.timed_out() {
            tracing::warn!("secretary: {id}'s stopped round is still running a verb");
        }
    }

    fn halt(&self, id: &ConversationId) -> Halt {
        let halt = Halt::new();
        super::locks::held(&self.halts).insert(id.clone(), halt.clone());
        halt
    }

    /// The round ended, so a stop waiting on it may answer.
    fn unhalt(&self, id: &ConversationId) {
        super::locks::held(&self.halts).remove(id);
        self.ended.notify_all();
    }

    #[doc(hidden)]
    #[must_use]
    pub fn is_running(&self, id: &ConversationId) -> bool {
        super::locks::held(&self.inner).contains_key(id)
    }
}

impl Default for Workers {
    fn default() -> Self {
        Self::new()
    }
}

async fn run(
    id: ConversationId,
    state: Arc<State>,
    mut nudged: mpsc::Receiver<()>,
    idle: Duration,
) {
    loop {
        // Drain a stale nudge before reading, never after: a write that lands between the
        // two would lose its nudge and wait out the idle timeout.
        let _ = nudged.try_recv();
        if let Some((rule, firing)) = due(&id, &state).await {
            perform(rule, &firing, &id, &state).await;
            continue;
        }
        match tokio::time::timeout(idle, nudged.recv()).await {
            Ok(Some(())) => {}
            // The sender lives only in this task's map entry, so the entry is already gone.
            Ok(None) => return,
            Err(_elapsed) => {
                let mut inner = super::locks::held(&state.workers.inner);
                if nudged.try_recv().is_ok() {
                    // A nudge slipped in after the timeout; loop so it is still read.
                    drop(inner);
                    continue;
                }
                inner.remove(&id);
                return;
            }
        }
    }
}

/// The first rule due on the conversation's reading now, marked as fired; a conversation that
/// does not read fires nothing this time, and says why.
async fn due(id: &ConversationId, state: &Arc<State>) -> Option<(Conversational, Firing)> {
    let caps = state.first.caps;
    let read = id.clone();
    let reading = match tokio::task::spawn_blocking(move || {
        transcript::read(caps.secretary.conversations, &read)
    })
    .await
    {
        Ok(Ok(reading)) => reading,
        Ok(Err(failed)) => {
            tracing::warn!("secretary: {id} did not read, so no rule fired: {failed}");
            return None;
        }
        Err(join_error) => {
            tracing::warn!("secretary: reading {id} did not complete: {join_error}");
            return None;
        }
    };
    super::locks::held(&state.workers.engine).due(id, &reading)
}

/// What each rule performs; its outcome is the line it writes, which the compactor is told of.
async fn perform(rule: Conversational, firing: &Firing, id: &ConversationId, state: &Arc<State>) {
    tracing::info!("secretary: {firing}");
    match rule {
        Conversational::Opening => {
            if noted(id, state).await {
                state.workers.stream(id).turn();
            }
        }
        Conversational::Answer => round(id, state).await,
        Conversational::Title => {
            if titled(id, state).await {
                state.workers.stream(id).turn();
            }
        }
    }
    if let Some(compactor) = &state.compactor {
        compactor.nudge();
    }
}

/// One round: the model asked, its reply streamed, and its calls run.
async fn round(id: &ConversationId, state: &Arc<State>) {
    let caps = state.first.caps;
    let held = Arc::clone(state);
    let system = suzatary_secretary::secretary::system_prompt(caps.secretary.manuals);
    let asked = id.clone();
    let logged = id.clone();
    let stream = state.workers.stream(id);
    stream.state(ConversationState::Thinking);
    let for_round = Arc::clone(&stream);
    let retry = state.retry();
    // Asked per round: which model answers can change between rounds.
    let result = tokio::task::spawn_blocking(move || {
        // The round arms its halt before it reads what waits. A stop then either finds this halt
        // or wrote its stop line before that read. The halt is armed on this blocking thread
        // because a stop holds the halts' lock across a store write.
        let halt = held.workers.halt(&asked);
        let chains = held.every_chain();
        let settables = held.settables();
        let on_delta = |fragment: &str| for_round.delta(fragment);
        let on_working = |verb: &str| for_round.working(verb);
        let sink = suzatary_secretary::conversation::round::Sink {
            on_delta: &on_delta,
            on_working: &on_working,
            halt: &halt,
        };
        secretary::answer(
            &caps.secretary,
            || caps.models.pick(),
            &held.deployment,
            &settables,
            &chains,
            &asked,
            &system,
            &secretary::surface::Subset::ALL,
            &sink,
            retry,
            secretary::Holder::Task,
        )
    })
    .await;
    state.workers.unhalt(id);
    // Regardless of outcome, since a round that stops short still writes the box's turn.
    stream.turn();
    match result {
        Ok(Ok(Some(answered))) => {
            match &answered.ending {
                Ending::Answered => {}
                Ending::Ended { why } => {
                    tracing::warn!(
                        "secretary: {logged} ended without an answer: {}",
                        why.said()
                    );
                }
            }
            // The box's turn is pushed like an answer: the owner learns the round stopped.
            maybe_push(&stream, state, &answered.stored);
        }
        // Nothing waited, so no round ran.
        Ok(Ok(None)) => {}
        Ok(Err(refusal)) => {
            tracing::warn!(
                "secretary: {logged} did not finish, and the box could not say so: {}",
                refusal.message()
            );
        }
        Err(_join_error) => {
            tracing::warn!(
                "secretary: {logged}'s round did not finish (the task did not complete)"
            );
        }
    }
    stream.state(current_state(id, state).await);
}

/// A set-up conversation's opening note: the template's steps the box still owes, read now.
async fn noted(id: &ConversationId, state: &Arc<State>) -> bool {
    let caps = state.first.caps;
    let settables = state.settables();
    let asked = id.clone();
    let noted = tokio::task::spawn_blocking(move || {
        let client =
            suzatary_topics::secretary::set_up::client_kept(caps.secretary.secrets, &settables);
        suzatary_topics::secretary::set_up::open(
            caps.secretary.conversations,
            &asked,
            caps.secretary.clock.now(),
            |template| template.template().opening(client),
        )
    })
    .await;
    match noted {
        Ok(Ok(noted)) => noted,
        Ok(Err(failed)) => {
            tracing::warn!("secretary: {id}'s opening was not written: {failed}");
            false
        }
        Err(_join_error) => {
            tracing::warn!("secretary: {id}'s opening did not complete");
            false
        }
    }
}

async fn titled(id: &ConversationId, state: &Arc<State>) -> bool {
    let caps = state.first.caps;
    let titler = state.titler.clone();
    let asked = id.clone();
    // One pick for the job, which both asks and is named, so the title is credited to the model
    // that read the conversation.
    let ran = tokio::task::spawn_blocking(move || {
        let pick = titler
            .as_deref()
            .map_or_else(|| caps.models.pick(), Picker::pick);
        suzatary_topics::secretary::title::run(
            caps.secretary.conversations,
            caps.secretary.clock,
            pick.as_ref(),
            &asked,
        )
    })
    .await;
    match ran {
        Ok(Ok(named)) => named.is_some(),
        Ok(Err(refusal)) => {
            tracing::warn!("secretary: {id} was not titled: {}", refusal.message());
            false
        }
        Err(_join_error) => {
            tracing::warn!("secretary: {id}'s title job did not complete");
            false
        }
    }
}

/// Push once, when nobody is watching: a live subscriber means the answer is on screen.
fn maybe_push(
    stream: &Stream,
    state: &Arc<State>,
    stored: &suzatary_conversations::secretary::store::Stored,
) {
    if stream.frames.receiver_count() > 0 {
        return;
    }
    let is_brief = stored
        .turns
        .iter()
        .rev()
        .find_map(|said| match &said.turn {
            Turn::Person { text, .. } => {
                Some(text.as_str() == suzatary_secretary::secretary::briefs::BRIEF)
            }
            Turn::Model { .. } | Turn::Tool { .. } | Turn::Act { .. } | Turn::Erased { .. } => None,
        })
        .unwrap_or(false);
    let Some(push) = suzatary_cards::notify::answer_push(stored, is_brief) else {
        return;
    };
    let caps = state.first.caps;
    // Not waited on, so a store that keeps the push waiting never holds the worker; never fatal,
    // since a phone with no push registered is a log line, not an outage.
    tokio::task::spawn_blocking(move || {
        if let Err(error) = caps.notify.push(&push) {
            tracing::warn!("secretary: nothing pushed: {error}");
        }
    });
}

/// The claims' state over a fresh read; unreadable reads `idle`.
async fn current_state(id: &ConversationId, state: &Arc<State>) -> ConversationState {
    let caps = state.first.caps;
    let id = id.clone();
    tokio::task::spawn_blocking(move || {
        transcript::read(caps.secretary.conversations, &id)
            .map_or(ConversationState::Idle, |read| read.state())
    })
    .await
    .unwrap_or(ConversationState::Idle)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::Partial;

    /// Pieces of an answer, the kinds a block can be and the kinds that span a blank line.
    const PIECES: &[&str] = &[
        "Plain words.",
        "- an item",
        "+ a numbered item",
        "/ Term: its description",
        "= A heading",
        "*strong* and _emphasis_",
        "$x + 1$",
        "$ x\n\n= y $",
        "```\ncode\n\nmore code\n```",
        "```rust\nfn f() {}\n```",
        "#quote[a\n\nb]",
        "a `raw` word",
        "> not a quote in typst",
        "*unclosed",
        "-",
    ];
    const BETWEEN: &[&str] = &["\n\n", "\n", " ", "\n\n\n", ""];

    /// The stream closes blocks as later ones begin, so the law below is about something.
    #[test]
    fn a_stream_closes_each_block_once_the_next_begins() {
        let mut partial = Partial::default();
        for fragment in ["Plain words.\n", "\n- an item\n\n", "= A heading"] {
            partial.push(fragment);
        }
        let closed: Vec<&str> = partial.closed.iter().map(|c| c.source.as_str()).collect();
        assert_eq!(closed, ["Plain words.\n\n", "- an item\n\n"]);
    }

    proptest! {
        /// Every block the stream closes, and so renders and keeps, is a block of the whole
        /// answer: an erasure finds the blocks of erased words by splitting the whole text
        /// again, so a streamed block the whole split never names would outlive the erasure.
        #[test]
        fn every_block_the_stream_closes_is_a_block_of_the_whole_answer(
            parts in prop::collection::vec((0..PIECES.len(), 0..BETWEEN.len()), 1..8),
            cuts in prop::collection::vec(any::<prop::sample::Index>(), 0..12),
        ) {
            let answer: String = parts.iter().flat_map(|&(p, b)| [PIECES[p], BETWEEN[b]]).collect();
            let bounds: Vec<usize> = (0..=answer.len()).filter(|&i| answer.is_char_boundary(i)).collect();
            let mut at: Vec<usize> = cuts.iter().map(|i| bounds[i.index(bounds.len())]).collect();
            at.extend([0, answer.len()]);
            at.sort_unstable();
            at.dedup();
            let mut partial = Partial::default();
            for pair in at.windows(2) {
                partial.push(&answer[pair[0]..pair[1]]);
            }
            let whole: Vec<String> = suzatary_base::answer::blocks(&answer)
                .into_iter()
                .map(|block| block.source)
                .collect();
            for closed in &partial.closed {
                prop_assert!(
                    whole.contains(&closed.source),
                    "streamed {:?} is no block of {:?}, whose blocks are {:?}",
                    closed.source,
                    answer,
                    whole
                );
            }
        }
    }
}

#[cfg(test)]
mod halts {
    use super::*;
    use suzatary_conversations::secretary::store::RunId;

    /// A stop pulls the halt of the round it ends, never that of a round begun after its take,
    /// and the round running when its conversation does not read (`Ended::of` an unread one).
    #[test]
    fn a_stop_halts_the_round_it_ends_and_none_begun_after() {
        let id = ConversationId::new("2026-09-30-1").expect("an id");
        let ada = Actor::new("ada").expect("an actor");
        let (before, after) = (RunId::of([1; 16]), RunId::of([2; 16]));
        let ended = Ended::Runs(std::collections::BTreeSet::from([before.clone()]));
        for (run, read, halts) in [
            (before.clone(), ended.clone(), true),
            (after.clone(), ended.clone(), false),
            (
                after,
                Ended::of(
                    &Err::<Arc<transcript::Transcript>, _>("unread"),
                    &Hash::new("0".repeat(64)).expect("a name"),
                ),
                true,
            ),
        ] {
            let workers = Workers::new();
            let halt = workers.halt(&id);
            halt.claims(run.clone());
            workers.halt_ended(&id, &read, ada.clone());
            assert_eq!(halt.halted(), halts, "{run} under {read:?}");
        }
    }
}
