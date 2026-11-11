//! The engine over the conversations: one task per conversation, nudged whenever a write
//! moves its heads, running the conversation's rules (`secretary::rules`) on each new reading.
//!
//! One task per conversation keeps two rounds from answering the same question; the engine's
//! memo fires each rule once per reading. The lock is held only for a lookup, insert or removal,
//! never across a firing, and a task leaves only under the lock [`Workers::nudge`] takes, so no
//! nudge is lost.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use suzatary_core::conversation::round::Ending;
use suzatary_core::conversation_id::ConversationId;
use suzatary_core::refusal::Refusal;
use suzatary_core::rule::{Engine, Firing};
use suzatary_core::secretary::model::{Halt, Sessions, Turn};
use suzatary_core::secretary::rules::{Conversational, RULES};
use suzatary_core::secretary::store::ConversationState;
use suzatary_core::secretary::{self, transcript};
use suzatary_core::wire;
use tokio::sync::{broadcast, mpsc};

use super::State;

/// How many frames a lagging subscriber may fall behind before it is told to refetch.
const FRAMES: usize = 256;

/// How long an idle worker waits for a nudge before it drops itself.
const IDLE: Duration = Duration::from_secs(30);

/// How long a stop waits for its round to write the stop; a verb already running is not
/// interrupted, so a round can take longer, and then the stop answers with what stands.
const STOP_WAIT: Duration = Duration::from_secs(15);

/// One conversation's channel: a nudge, never a payload, since the next step is reread
/// from the file.
struct Worker {
    nudge: mpsc::Sender<()>,
}

/// One conversation's live stream: the broadcast and the partial reply a round is building.
///
/// The partial is never written to the file, and one mutex guards both it and every
/// subscription so a snapshot and its broadcast position are the same instant.
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
        let cut = suzatary_core::answer::blocks(&self.text[start..]);
        let now = cut.len().saturating_sub(1);
        cut.into_iter()
            .take(now)
            .map(|block| {
                let closed = Closed {
                    sha: suzatary_core::answer::sha(&block.source),
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

    /// The conversation's state changed; never touches the partial.
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

/// Every conversation currently being answered, so a `say` mid-round finds its task.
///
/// Held on [`State`], never a static, because a test binary runs more than one server.
pub struct Workers {
    inner: Mutex<HashMap<ConversationId, Worker>>,
    /// Every conversation's [`Stream`], never removed, since a subscriber may connect
    /// while no worker runs.
    streams: Mutex<HashMap<ConversationId, Arc<Stream>>>,
    /// The halt of each round running now, removed when it ends so a late stop cannot
    /// stop the next round.
    halts: Mutex<HashMap<ConversationId, Halt>>,
    /// Told whenever a round's halt is removed, so a stop answers once its round has ended.
    ended: Condvar,
    /// The conversation's rules and every firing made, shared by every task.
    engine: Mutex<Engine<Conversational>>,
    /// How long an idle worker waits for a nudge; tests shorten it.
    idle: Duration,
}

impl Workers {
    /// An empty table, waiting `IDLE` before an idle worker drops itself.
    #[must_use]
    pub fn new() -> Self {
        Self::with_idle(IDLE)
    }

    /// An empty table with a chosen idle wait, for tests.
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

    /// This conversation's live stream, minted on first use.
    fn stream(&self, id: &ConversationId) -> Arc<Stream> {
        let mut streams = super::locks::held(&self.streams);
        Arc::clone(
            streams
                .entry(id.clone())
                .or_insert_with(|| Arc::new(Stream::new())),
        )
    }

    /// Subscribe to a conversation's live stream, with the frames that rebuild its partial.
    #[must_use]
    pub fn subscribe(
        &self,
        id: &ConversationId,
    ) -> (broadcast::Receiver<wire::Frame>, Vec<wire::Frame>) {
        self.stream(id).subscribe()
    }

    /// A block this round's stream closed, by sha; `None` for one no running round closed.
    #[must_use]
    pub fn closed(&self, id: &ConversationId, sha: &str) -> Option<String> {
        self.stream(id).closed(sha)
    }

    /// Tell the conversation's task its heads moved, starting one if none is running.
    ///
    /// Called after the write, so the nudge carries nothing: the task reads again.
    pub fn nudge(&self, id: &ConversationId, state: Arc<State>) {
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

    /// Stop the round running in this conversation, as `by`, and wait (at most `STOP_WAIT`)
    /// for it to end. A question not yet taken by a round is stopped by its line alone, written
    /// while the halts are held, so no round starts on it in between.
    ///
    /// Blocks: a route calls it off the async workers.
    ///
    /// # Errors
    /// [`Refusal::conflict`] when nothing is running or waiting, so a halt never outlives its
    /// round; the store's failures.
    pub fn stop(
        &self,
        caps: &suzatary_core::capability::Server<'_>,
        id: &ConversationId,
        by: prodrome_core::event::Actor,
    ) -> Result<(), Refusal> {
        let halts = super::locks::held(&self.halts);
        if halts.contains_key(id) {
            self.wait_out(halts, id, by);
            return Ok(());
        }
        secretary::stop_waiting(caps.secretary.conversations, caps.secretary.clock, id, &by)?;
        drop(halts);
        self.announce(id);
        Ok(())
    }

    /// [`Workers::stop`] of a running round alone, so its CLI writes no more to a session an
    /// erasure then deletes.
    pub(crate) fn halt_round(&self, id: &ConversationId, by: prodrome_core::event::Actor) {
        self.wait_out(super::locks::held(&self.halts), id, by);
    }

    fn wait_out(
        &self,
        halts: std::sync::MutexGuard<'_, HashMap<ConversationId, Halt>>,
        id: &ConversationId,
        by: prodrome_core::event::Actor,
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

    /// The round ended: nothing is left to stop, and a stop waiting on it may answer.
    fn unhalt(&self, id: &ConversationId) {
        super::locks::held(&self.halts).remove(id);
        self.ended.notify_all();
    }

    /// Whether a task currently holds this conversation's entry; for tests only.
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

/// The loop over one conversation: perform the first rule due on its reading while one is,
/// else wait for a nudge or the idle timeout.
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

/// One round: the model asked and its calls run, over a session when one is held.
///
/// Errors are logged and end the round; only the session arm streams fragments.
async fn round(id: &ConversationId, state: &Arc<State>) {
    let caps = state.first.caps;
    let held = Arc::clone(state);
    let system = crate::commands::secretary::system_prompt(caps.secretary.manuals);
    let asked = id.clone();
    let logged = id.clone();
    let stream = state.workers.stream(id);
    stream.state(ConversationState::Thinking);
    let for_round = Arc::clone(&stream);
    // Asked per round: only the `claude` subprocess holds a session, and which model
    // answers can change between rounds.
    let result = tokio::task::spawn_blocking(move || {
        // The round arms its halt before it reads what waits. A stop then either finds this halt
        // or wrote its stop line before that read. The halt is armed on this blocking thread
        // because a stop holds the halts' lock across a store write.
        let halt = held.workers.halt(&asked);
        let chains = held.every_chain();
        let settables = held.settables();
        let on_delta = |fragment: &str| for_round.delta(fragment);
        let on_working = |verb: &str| for_round.working(verb);
        let sink = suzatary_core::conversation::round::Sink {
            on_delta: &on_delta,
            on_working: &on_working,
            halt: &halt,
        };
        secretary::answer(
            &caps.secretary,
            || caps.sessions.pick(),
            &held.deployment,
            &settables,
            &chains,
            &asked,
            &system,
            &secretary::surface::Subset::ALL,
            &sink,
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

/// The title job; `true` when a title landed.
async fn titled(id: &ConversationId, state: &Arc<State>) -> bool {
    let caps = state.first.caps;
    let titler = state.titler.clone();
    let asked = id.clone();
    // One pick for the job, which both asks and is named, so the title is credited to the model
    // that read the conversation.
    let ran = tokio::task::spawn_blocking(move || {
        let pick = titler
            .as_deref()
            .map_or_else(|| caps.sessions.pick(), Sessions::pick);
        secretary::title::run(
            caps.secretary.conversations,
            caps.secretary.clock,
            pick.model(),
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
///
/// `is_brief` is judged by what the round's opening turn asked, not by a flag.
fn maybe_push(stream: &Stream, state: &Arc<State>, stored: &secretary::store::Stored) {
    if stream.frames.receiver_count() > 0 {
        return;
    }
    let is_brief = stored
        .turns
        .iter()
        .rev()
        .find_map(|said| match &said.turn {
            Turn::Person { text, .. } => Some(text.as_str() == crate::commands::secretary::BRIEF),
            Turn::Model { .. } | Turn::Tool { .. } | Turn::Act { .. } | Turn::Erased { .. } => None,
        })
        .unwrap_or(false);
    let Some(push) = secretary::answer_push(stored, is_brief) else {
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

/// [`secretary::state`] over a fresh read, mapped to the wire; unreadable reads `idle`.
async fn current_state(id: &ConversationId, state: &Arc<State>) -> ConversationState {
    let caps = state.first.caps;
    let id = id.clone();
    tokio::task::spawn_blocking(move || {
        secretary::conversations::read(caps.secretary.conversations, &id)
            .map_or(ConversationState::Idle, |stored| secretary::state(&stored))
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
            let whole: Vec<String> = suzatary_core::answer::blocks(&answer)
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
