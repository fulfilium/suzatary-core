use super::{Clock, Corpus, Mailbox, Manuals, Render, Repository};
use crate::record::TodoEvent;
use crate::secretary::recollection::Recollection;
use crate::store::Reads;

/// What the triage agent may use: it reads attacker-controlled mail, so nothing here
/// sends, notifies, names a peer or performs a card, and its `chain` is only read.
/// Its writes are cards, filed through the call's [`crate::secretary::filed::TriageCards`].
///
/// ```
/// fn write(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.chain; }
/// ```
///
/// No field reaches the owner's phone, a peer's login, the model, or the inbox a person
/// taps:
///
/// ```compile_fail,E0609
/// fn push(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.notify; }
/// ```
/// ```compile_fail,E0609
/// fn peer(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.whois; }
/// ```
/// ```compile_fail,E0609
/// fn file(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.inbox; }
/// ```
/// ```compile_fail,E0609
/// fn fetch(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.web; }
/// ```
///
/// Its chain is read and never written: neither `append` nor `accept` is reachable from it.
///
/// ```compile_fail,E0599
/// fn write(agent: suzatary_core::capability::Agent<'_>, event: suzatary_core::record::TodoEvent) {
///     let _ = agent.chain.append(event);
/// }
/// ```
/// ```compile_fail,E0599
/// fn join(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.chain.accept(&[]); }
/// ```
///
/// It reads the secretary's memory and has no way to write it: the memory is a
/// [`Recollection`], whose stores are private and whose every method reads.
///
/// ```compile_fail,E0616
/// fn reach(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.memory.memories; }
/// ```
/// ```compile_fail,E0599
/// fn remember(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.memory.hold(); }
/// ```
///
/// Those stores are read halves ([`Reads`], [`crate::store::Gets`]), so not even code inside
/// [`Recollection`] can append a fact or a summary, or keep or delete their words:
///
/// ```compile_fail,E0599
/// use suzatary_core::secretary::memories::ReadMemories;
/// use suzatary_core::secretary::memory_event::MemoryEvent;
/// fn remember(memory: ReadMemories<'_>, event: MemoryEvent) { let _ = memory.history.append(event); }
/// ```
/// ```compile_fail,E0599
/// use suzatary_core::secretary::memories::ReadMemories;
/// use suzatary_core::secretary::salted_text::TextName;
/// fn forget(memory: ReadMemories<'_>, name: &TextName) { let _ = memory.texts.remove(name); }
/// ```
/// ```compile_fail,E0599
/// use suzatary_core::secretary::memories::ReadMemories;
/// use suzatary_core::secretary::digest_event::DigestEvent;
/// fn summarize(memory: ReadMemories<'_>, event: DigestEvent) {
///     let _ = memory.digests.history.append(event);
/// }
/// ```
///
/// Its record of the conversations is the owner's part alone: no line of the whole record, a
/// verb's answer or the secretary's, is reachable from it.
///
/// ```compile_fail,E0599
/// fn lines(agent: suzatary_core::capability::Agent<'_>) {
///     let _ = agent.memory.record().map(|record| record.entries().len());
/// }
/// ```
/// ```compile_fail,E0599
/// fn reading(agent: suzatary_core::capability::Agent<'_>) { let _ = agent.memory.reading(); }
/// ```
///
/// A caller may swap one of its effects, and may not add one:
///
/// ```
/// use suzatary_core::capability::{Agent, Clock};
/// fn swap<'a>(agent: Agent<'a>, clock: &'a dyn Clock) -> Agent<'a> {
///     Agent { clock, ..agent }
/// }
/// ```
/// ```compile_fail,E0560
/// use suzatary_core::capability::{Agent, Notify};
/// fn widen<'a>(agent: Agent<'a>, push: &'a dyn Notify) -> Agent<'a> {
///     Agent { notify: push, ..agent }
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Agent<'a> {
    /// The chain, which it reads: a [`Reads`], so no write is reachable from it.
    pub chain: &'a dyn Reads<TodoEvent>,
    /// The triage queue and its dedup state.
    pub corpus: &'a dyn Corpus,
    /// The mail index, read-only.
    pub mail: &'a dyn Mailbox,
    /// The repository's gates.
    pub repository: &'a dyn Repository,
    /// The manuals it is handed.
    pub manuals: &'a dyn Manuals,
    /// Typst, for confirmed markup.
    pub render: &'a dyn Render,
    /// The instant every write is stamped with.
    pub clock: &'a dyn Clock,
    /// The secretary's memory, read-only.
    pub memory: Recollection<'a>,
}
