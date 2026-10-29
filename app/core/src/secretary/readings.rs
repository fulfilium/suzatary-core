//! The box's readings of the secretary's two stores, the inbox and the conversations, as pure
//! functions of their folds: what a route answers and what the tab's module answers from its
//! replicas are these functions applied to the same objects.
//!
//! What lies beyond the two stores (the rendered blocks, the memory's words, a changeset drawn
//! against its chain, the routine the deployment declares) is a [`Dress`], passed in as a value: the box dresses with its renderer, its
//! memory and its chains, and [`Dress::BARE`] dresses with nothing.

use crate::chain_set::ChainSet;

use crate::capability::{AnswerError, Answers, Datetime, Html, Turn};
use crate::reference::{ProposalRef, Reference};
use crate::refusal::{Answered, Refusal};
use crate::secretary::answer::Renderable;
use crate::secretary::attached;
use crate::secretary::attachment_source::AttachmentSource;
use crate::secretary::changes::Changeset;
use crate::secretary::inbox;
use crate::secretary::inbox_reading::{Reading, Words};
use crate::secretary::memory::Fact;
use crate::secretary::responded::{Done, Responded, Tried, Withdrawal};
use crate::secretary::salted_text::TextName;
use crate::secretary::store::{Forward, Proposal, Said, Standing, Stored, Style, stamp};
use crate::secretary::turn_shape::TurnShape;
use crate::wire;

/// One window of a conversation: the turns before `before` (the newest when `None`), at most
/// `limit` of them, narrowed to the rounds touching `about`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    /// An absolute turn index, clamped to the transcript.
    pub before: Option<usize>,
    /// How many turns, already capped.
    pub limit: usize,
    /// The reference the rounds must touch.
    pub about: Option<Reference>,
}

impl Window {
    /// The newest window, full-size, which always holds the round that just happened.
    #[must_use]
    pub fn newest() -> Self {
        Self {
            before: None,
            limit: DEFAULT_TURN_WINDOW,
            about: None,
        }
    }
}

/// The default window size, in turns.
pub const DEFAULT_TURN_WINDOW: usize = 60;

/// How many hits one search answers.
pub const SEARCH_LIMIT: usize = 50;

/// What a reading is dressed with from beyond the two stores.
#[derive(Clone, Copy)]
pub struct Dress<'a> {
    /// Renders a model's blocks.
    pub answers: &'a dyn Answers,
    /// The memory as its route reads it, for a receipt's words; `None` reads every receipt as
    /// unread.
    pub memory: Option<&'a wire::MemoryPayload>,
    /// Draws a changeset against its chain.
    pub draws: &'a dyn Draw,
}

impl Dress<'static> {
    /// Nothing from beyond the stores: every block as its source, every receipt unread, no
    /// changeset drawn.
    pub const BARE: Self = Self {
        answers: &Unrendered,
        memory: None,
        draws: &Undrawn,
    };
}

/// A changeset drawn against the chain it names, and the routine the deployment declares:
/// readings of the todo chains and the instance file, which only the box holds.
pub trait Draw {
    /// The rows, and the box's own refusal when an act would no longer build.
    fn shown(&self, changeset: &Changeset) -> (Vec<wire::ProposedObject>, String);
    /// The routine `[coding]` declares, whose page its token's card opens.
    fn routine(&self) -> Option<&crate::coding::RoutineId>;
    /// The name the deployment stores its model's credential under, which its card words as the
    /// model's sign-in.
    fn model(&self) -> Option<&str>;
    /// The origins the instance file lets `plugin` fetch from now, `None` while it declares no
    /// such plugin.
    fn origins(&self, plugin: &str) -> Option<Vec<String>>;
    /// How the person reaches the app at a terminal, which a step's command is spelled in.
    fn reach(&self) -> &crate::connect::Reach;
    /// The message or file a held value comes from, in the box's words: a message by its
    /// sender, subject and date, a file by its source.
    fn source(&self, handle: &crate::held_value::Handle) -> String;
}

/// Renders nothing, so every block stays its source.
struct Unrendered;

impl Answers for Unrendered {
    fn block(&self, _: &Renderable) -> Result<Html, AnswerError> {
        Err(AnswerError("not rendered here".to_owned()))
    }
}

/// Draws nothing.
struct Undrawn;

impl Draw for Undrawn {
    fn shown(&self, _: &Changeset) -> (Vec<wire::ProposedObject>, String) {
        (Vec::new(), String::new())
    }

    fn routine(&self) -> Option<&crate::coding::RoutineId> {
        None
    }

    fn model(&self) -> Option<&str> {
        None
    }

    fn origins(&self, _: &str) -> Option<Vec<String>> {
        None
    }

    fn reach(&self) -> &crate::connect::Reach {
        &crate::connect::PLAIN
    }

    fn source(&self, _: &crate::held_value::Handle) -> String {
        UNREAD_SOURCE.to_owned()
    }
}

/// What a card says of a held value's message where the reading reads no mail.
pub const UNREAD_SOURCE: &str = "from a message this reading does not look up";

/// The held values `body` carries, each with its message, read with the one parse the tap reads.
fn held_in(body: &str, draws: &dyn Draw) -> wire::HeldIn {
    use crate::held_value::{Reach, Written};
    match Written::parse(body) {
        Ok(written) => wire::HeldIn::Read {
            values: written
                .held()
                .map(|placeholder| wire::HeldFrom {
                    named: placeholder.described(),
                    from: match placeholder.reach() {
                        Reach::Revealable(handle) => draws.source(&handle),
                        Reach::Withheld => {
                            "withheld, from nothing the box can read again".to_owned()
                        }
                    },
                })
                .collect(),
        },
        Err(malformed) => wire::HeldIn::Unreadable {
            why: malformed.to_string(),
        },
    }
}

/// The source of the block named `sha` among a conversation's answers, earlier attempts included:
/// what the page asks the box to render, by name and never by text.
#[must_use]
pub fn block_named(stored: &Stored, sha: &str) -> Option<String> {
    stored.turns.iter().find_map(|said| {
        let text = match &said.turn {
            Turn::Model { text, .. } => text,
            Turn::Erased { words, .. } => match words.first() {
                Some(crate::secretary::words::Words::Kept {
                    kept: crate::secretary::line_words::LineWords::Model { text },
                }) => text,
                _ => return None,
            },
            Turn::Person { .. } | Turn::Tool { .. } | Turn::Act { .. } => return None,
        };
        crate::secretary::answer::blocks(text)
            .into_iter()
            .find(|block| crate::secretary::answer::sha(&block.source) == sha)
            .map(|block| block.source)
    })
}

/// Every conversation, newest first: what the list draws.
#[must_use]
pub fn conversations<S: std::borrow::Borrow<Stored>>(
    stored: &[S],
    inbox: &inbox::Fold,
) -> wire::ConversationsPayload {
    wire::ConversationsPayload {
        conversations: stored.iter().map(|one| row(one.borrow(), inbox)).collect(),
    }
}

/// A remember card's words as `memory` keeps them by the name the card gives them; with no
/// memory, unread.
#[must_use]
pub fn words(text: &TextName, memory: Option<&wire::MemoryPayload>) -> Words {
    match memory {
        None => Words::Unread,
        Some(memory) => Words::of(
            memory
                .cards
                .iter()
                .find(|kept| kept.text == text.as_str())
                .and_then(|kept| Fact::new(kept.fact.as_str()).ok()),
        ),
    }
}

/// Every proposal in the inbox, oldest first, with its state and its value at the set's instant,
/// priced through the chains' `set`; a remember card's headline finds its words in `memory`.
#[must_use]
pub fn inbox(
    fold: &inbox::Fold,
    set: &ChainSet<'_>,
    memory: Option<&wire::MemoryPayload>,
) -> wire::InboxProposalsPayload {
    let now = set.at();
    let mut proposals: Vec<(Datetime, wire::InboxProposal)> = fold
        .objects()
        .map(|object| {
            let state = match (&object.reading, object.reading.shown()) {
                (Reading::Unreadable(why), _) => wire::InboxState::Unreadable {
                    reason: why.to_string(),
                },
                (_, None) => wire::InboxState::Open {},
                (reading, Some(_)) if reading.open() => wire::InboxState::Open {},
                (_, Some(answer)) => match &answer.response {
                    Responded::Dismissed { .. } | Responded::Withdrawn(_) => {
                        wire::InboxState::Cancelled {
                            at: stamp(answer.at),
                        }
                    }
                    Responded::Tried { .. } | Responded::Done(_) | Responded::Checked(_) => {
                        wire::InboxState::Completed {
                            at: stamp(answer.at),
                        }
                    }
                },
            };
            (
                object.at,
                wire::InboxProposal {
                    id: object.id.to_string(),
                    asked: object.asked.asked.clone(),
                    serves: object.asked.price.serves().cloned(),
                    headline: object.asked.proposal.headline(|text| words(text, memory)),
                    by: object
                        .asked
                        .proposer
                        .on_wire(&object.asked.proposal, &object.asked.asked),
                    why: object.asked.reasons.why().to_owned(),
                    state,
                    value: inbox::value(object, set),
                    at: stamp(object.at),
                },
            )
        })
        .collect();
    proposals.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
    wire::InboxProposalsPayload {
        at: stamp(now),
        proposals: proposals.into_iter().map(|(_, row)| row).collect(),
        failed: set.failed(),
    }
}

/// The proposals waiting on the owner at the set's instant, in the Inbox's order (`web/src/inbox/waiting.ts`):
/// ascending in value, an unpriced one after every priced one, ties by id.
#[must_use]
pub fn waiting(fold: &inbox::Fold, set: &ChainSet<'_>) -> Vec<wire::InboxProposal> {
    let mut open: Vec<wire::InboxProposal> = inbox(fold, set, None)
        .proposals
        .into_iter()
        .filter(|row| row.state == wire::InboxState::Open {})
        .collect();
    let urgency = |row: &wire::InboxProposal| {
        row.value
            .and_then(wire::Fulfillment::number)
            .unwrap_or(f64::INFINITY)
    };
    open.sort_by(|a, b| {
        urgency(a)
            .total_cmp(&urgency(b))
            .then_with(|| a.id.cmp(&b.id))
    });
    open
}

/// Every conversation's turns holding `q`, newest first.
///
/// # Errors
/// [`Refusal::bad_request`] for a blank or over-long `q`.
pub fn search<S: std::borrow::Borrow<Stored>>(
    stored: &[S],
    q: &str,
) -> Answered<wire::SearchPayload> {
    let needle = crate::secretary::search::Needle::new(q)
        .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
    let hits = crate::secretary::search::search(
        stored,
        &needle,
        crate::secretary::search::LIST_CONTEXT,
        SEARCH_LIMIT,
    );
    Ok(wire::SearchPayload {
        query: q.trim().to_owned(),
        hits,
    })
}

/// One conversation over one window of its transcript, optionally narrowed by `about`; proposals
/// are filtered by `about` but never windowed. A function of the two stores' readings and what
/// `dress` brings from beyond them.
///
/// A proposal line the inbox's reading cannot place is drawn in `unplaced`, never dropped.
#[must_use]
pub fn conversation(
    stored: &Stored,
    inbox: &inbox::Fold,
    window: &Window,
    dress: &Dress<'_>,
) -> wire::ConversationPayload {
    let lines = stored.lines(inbox);
    let total_turns = stored.turns.len();
    // Not a suffix of `stored.turns`, since a rewind can set a turn aside.
    let queued: std::collections::BTreeSet<usize> = crate::secretary::queue::delta(stored)
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    let spans = crate::secretary::about::round_spans(&stored.turns);
    // With no `about` every span is touched, so the unfiltered path is the same windowing.
    let touched: Vec<bool> = window.about.as_ref().map_or_else(
        || vec![true; spans.len()],
        |reference| {
            spans
                .iter()
                .map(|&span| crate::secretary::about::round_touches(&stored.turns, span, reference))
                .collect()
        },
    );
    let before = window.before.unwrap_or(total_turns).min(total_turns);
    let (selected, first, total) =
        about_window(&spans, &touched, total_turns, before, window.limit);
    wire::ConversationPayload {
        id: stored.id.as_str().to_owned(),
        title: stored.heading().into_owned(),
        model: stored.model.clone(),
        opened: crate::secretary::store::stamp(stored.opened),
        turns: selected
            .iter()
            .map(|&index| {
                turn(
                    dress.answers,
                    &stored.turns[index],
                    Place {
                        index,
                        queued: queued.contains(&index),
                        attempt: if stored.is_live(index) {
                            wire::Attempt::Current
                        } else {
                            wire::Attempt::Earlier
                        },
                    },
                    dress.memory,
                )
            })
            .collect(),
        proposals: lines
            .standings()
            // `owning_span` mirrors `web/src/chat/rounds.ts`'s placement, so this drops exactly what
            // the page would.
            .filter(|standing| {
                spans.is_empty()
                    || touched[crate::secretary::about::owning_span(
                        &spans,
                        standing.asking.after_turn.saturating_sub(1),
                    )]
            })
            .map(|standing| {
                drawn(
                    stored,
                    standing.asking.index,
                    standing,
                    dress.draws,
                    dress.memory,
                )
            })
            .collect(),
        unplaced: unplaced(&lines),
        state: crate::secretary::state(stored),
        first,
        total,
        again: stored.rewind(crate::secretary::store::Rewind::Edit).ok(),
    }
}

/// Every line the inbox's reading cannot place, as the wire carries it.
fn unplaced(lines: &crate::secretary::store::Lines) -> Vec<wire::UnplacedProposal> {
    lines
        .unplaced()
        .map(|unplaced| wire::UnplacedProposal {
            index: unplaced.asking.index,
            why: unplaced.why,
        })
        .collect()
}

fn row(stored: &Stored, fold: &inbox::Fold) -> wire::ConversationRow {
    let lines = stored.lines(fold);
    wire::ConversationRow {
        id: stored.id.as_str().to_owned(),
        title: stored.heading().into_owned(),
        kind: stored.kind,
        opened: crate::secretary::store::stamp(stored.opened),
        last: last_line(stored),
        last_at: crate::secretary::store::stamp(
            stored.turns.last().map_or(stored.opened, |said| said.at),
        ),
        turns: u32::try_from(stored.turns.len()).unwrap_or(u32::MAX),
        standing: lines
            .standings()
            .filter(|standing| standing.object.reading.performs())
            .count(),
        unplaced: unplaced(&lines),
    }
}

/// Proposal `index` of `stored`, drawn by `draws`, with the session its tap started.
pub(crate) fn drawn(
    stored: &Stored,
    index: usize,
    standing: &Standing,
    draws: &dyn Draw,
    memory: Option<&wire::MemoryPayload>,
) -> wire::ProposalRow {
    wire::ProposalRow {
        coding: stored.coding.get(&index).map(session_on_wire),
        announced: announced(stored, index, standing),
        ..proposal(index, standing, draws, memory)
    }
}

/// While the act at `index` runs, the box's last line pointing at it: what the act waits for.
fn announced(stored: &Stored, index: usize, standing: &Standing) -> Option<String> {
    if !standing.object.reading.running() {
        return None;
    }
    let card = Reference::Proposal(ProposalRef {
        conversation: stored.id.clone(),
        index,
    });
    stored
        .live()
        .into_iter()
        .rev()
        .find_map(|(_, said)| match &said.turn {
            Turn::Act { text, references } if references.contains(&card) => Some(text.clone()),
            Turn::Act { .. }
            | Turn::Person { .. }
            | Turn::Model { .. }
            | Turn::Tool { .. }
            | Turn::Erased { .. } => None,
        })
}

/// The list's second line: the last thing said or answered as prose, never a verb's answer.
fn last_line(stored: &Stored) -> String {
    let line = stored
        .live()
        .into_iter()
        .rev()
        .find_map(|(_, said)| match &said.turn {
            Turn::Person { text, .. } => text.lines().map(str::trim).find(|line| !line.is_empty()),
            Turn::Model { text, calls } if calls.is_empty() => {
                crate::secretary::typst_prose_line(text)
                    .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))
            }
            Turn::Model { .. } | Turn::Tool { .. } | Turn::Act { .. } | Turn::Erased { .. } => None,
        })
        .unwrap_or("");
    line.chars().take(120).collect()
}

/// `[max(0, before - limit), before)` of a sequence of length `total`.
pub(crate) fn window(total: usize, before: usize, limit: usize) -> (usize, usize) {
    let end = before.min(total);
    let start = end.saturating_sub(limit);
    (start, end)
}

/// The absolute turn indices one `?about=` window draws, filtering first and then paging over the
/// kept indices.
///
/// Returns `(selected, first, total)`: the indices to draw, the first of them (or `total_turns`),
/// and the paged length.
fn about_window(
    spans: &[(usize, usize)],
    touched: &[bool],
    total_turns: usize,
    before: usize,
    limit: usize,
) -> (Vec<usize>, usize, usize) {
    let sequence: Vec<usize> = spans
        .iter()
        .zip(touched.iter())
        .filter(|&(_, &kept)| kept)
        .flat_map(|(&(start, end), _)| start..=end)
        .collect();
    let before_pos = sequence.partition_point(|&index| index < before.min(total_turns));
    let (start_pos, end_pos) = window(sequence.len(), before_pos, limit);
    let selected = sequence[start_pos..end_pos].to_vec();
    let first = selected.first().copied().unwrap_or(total_turns);
    (selected, first, sequence.len())
}

/// Where a turn stands: its index, and what the queue and the rewinds say of it.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    pub(crate) index: usize,
    pub(crate) queued: bool,
    pub(crate) attempt: wire::Attempt,
}

impl Place {
    /// A live turn nothing waits on.
    #[cfg(test)]
    pub(crate) fn at(index: usize) -> Self {
        Self {
            index,
            queued: false,
            attempt: wire::Attempt::Current,
        }
    }
}

pub(crate) fn turn<A: Answers + ?Sized>(
    answers: &A,
    said: &Said,
    place: Place,
    memory: Option<&wire::MemoryPayload>,
) -> wire::TurnRow {
    let Place {
        index,
        queued,
        attempt,
    } = place;
    let row = |text: &str, who: wire::Speaker| wire::TurnRow {
        index,
        text: text.to_owned(),
        at: crate::secretary::store::stamp(said.at),
        attempt,
        gone: None,
        who,
    };
    match &said.turn {
        // Only a person's words can be queued.
        crate::capability::Turn::Person {
            text,
            references,
            attachments,
        } => row(
            text,
            wire::Speaker::Person {
                references: references.clone(),
                attachments: attachments.iter().map(wire::AttachmentRow::from).collect(),
                queued,
            },
        ),
        // Neither the model speaking nor a verb answering, so drawn as its own kind: the box's
        // own note as the box's, never as the owner's.
        crate::capability::Turn::Act { text, references } => {
            row(text, acted_by(said.by.as_ref(), references.clone()))
        }
        // The calls are not listed, since each gets its own verb turn; each block's `html` is a
        // cached compile.
        crate::capability::Turn::Model { text, .. } => row(
            text,
            wire::Speaker::Model {
                blocks: blocks_of(answers, text),
            },
        ),
        crate::capability::Turn::Tool {
            verb,
            answer,
            outcome,
            files: _,
        } => row(answer, verb_turn(verb, *outcome, memory)),
        crate::capability::Turn::Erased { shape, words } => wire::TurnRow {
            text: crate::secretary::turn_shape::prose(words),
            gone: words
                .first()
                .and_then(crate::secretary::words::Words::gone)
                .map(wire::TurnGone::from),
            ..row("", erased(shape, said.by.as_ref(), memory))
        },
    }
}

/// A verb's turn: its name, and how its call ended, a fact it wrote read from `memory`.
fn verb_turn(
    verb: &crate::secretary::tool_name::ToolName,
    outcome: crate::secretary::outcome::Outcome,
    memory: Option<&wire::MemoryPayload>,
) -> wire::Speaker {
    use crate::secretary::outcome::Outcome;
    wire::Speaker::Verb {
        verb: verb.to_string(),
        outcome: match outcome {
            Outcome::Answered => wire::VerbOutcome::Answered,
            Outcome::Refused => wire::VerbOutcome::Refused,
            Outcome::Remembered { fact } => wire::VerbOutcome::Remembered {
                receipt: receipt(fact, memory),
            },
        },
    }
}

/// Who an act turn is: the owner's act, when the gate stamped it so; the box's line otherwise.
fn acted_by(
    by: Option<&crate::secretary::stamp::Stamp>,
    references: Vec<crate::reference::Reference>,
) -> wire::Speaker {
    if crate::secretary::stamp::Stamp::owners(by) {
        wire::Speaker::Act { references }
    } else {
        wire::Speaker::Box { references }
    }
}

/// Who said an erased turn, read off its shape, which keeps everything but the words.
fn erased(
    shape: &TurnShape,
    by: Option<&crate::secretary::stamp::Stamp>,
    memory: Option<&wire::MemoryPayload>,
) -> wire::Speaker {
    match shape {
        TurnShape::Person { references, .. } => wire::Speaker::Person {
            references: references.clone(),
            attachments: Vec::new(),
            queued: false,
        },
        TurnShape::Model { .. } => wire::Speaker::Model { blocks: Vec::new() },
        // The receipt is the shape's, so a remember answer whose words are gone, by a forget or
        // by the owner's erasure, still names its fact.
        TurnShape::Tool { verb, outcome, .. } => verb_turn(verb, *outcome, memory),
        TurnShape::Act { references } => acted_by(by, references.clone()),
    }
}

/// An answer's blocks, each rendered or not; a block the closed world refuses is its text.
pub(crate) fn blocks_of<A: Answers + ?Sized>(answers: &A, text: &str) -> Vec<wire::BlockRow> {
    crate::secretary::answer::blocks(text)
        .into_iter()
        .map(|block| wire::BlockRow {
            html: answers
                .block(&crate::secretary::answer::Renderable::of(&block.source))
                .ok()
                .map(|html| html.as_str().to_owned()),
            source: block.source,
            plain: block.plain,
            kind: block.kind,
        })
        .collect()
}

/// The receipt of fact `fact`, a `remember` turn's: its words from the memory, so a forgotten
/// fact's receipt says it was forgotten and never what it said.
fn receipt(
    fact: crate::secretary::memory::FactId,
    memory: Option<&wire::MemoryPayload>,
) -> wire::Receipt {
    let Some(memory) = memory else {
        return wire::Receipt::Unread { fact };
    };
    match memory.facts.iter().find(|standing| standing.id == fact) {
        Some(standing) => wire::Receipt::Standing {
            fact,
            text: standing.fact.clone(),
        },
        None if memory.forgotten.contains(&fact) => wire::Receipt::Forgotten { fact },
        None => match memory.merged.iter().find(|merge| merge.fact == fact) {
            Some(merge) => wire::Receipt::Merged {
                fact,
                into: merge.into,
            },
            None => wire::Receipt::Unread { fact },
        },
    }
}

/// A connection to make, as its card reads it: what it writes, its steps spelled as the box
/// reaches the app, and what its plugin may reach now.
fn connect_on_wire(
    connection: &crate::connect::Connection,
    draws: &dyn Draw,
    sentence: String,
) -> wire::ProposedAct {
    wire::ProposedAct::Connect {
        connection: connection.called(),
        writes: connection.writes(),
        steps: connection
            .steps()
            .iter()
            .map(|step| step.sentence(draws.reach()))
            .collect(),
        // The plugin's origins as declared now, or its template's for a first link.
        reach: match connection.template().table {
            crate::connect::Table::Link(fragment) => {
                draws.origins(fragment.plugin.as_str()).unwrap_or_else(|| {
                    fragment
                        .origins
                        .iter()
                        .map(|origin| (*origin).to_owned())
                        .collect()
                })
            }
            _ => Vec::new(),
        },
        sentence,
    }
}

/// A connection to remove, as its card reads it: what goes, the secrets forgotten with it, and
/// what the box keeps as its cache.
fn disconnect_on_wire(
    connection: &crate::connect::Connection,
    sentence: String,
) -> wire::ProposedAct {
    wire::ProposedAct::Disconnect {
        connection: connection.called(),
        writes: connection.writes(),
        forgets: connection
            .secrets()
            .iter()
            .map(|name| crate::settings::Help::label(name))
            .chain(match connection.template().table {
                crate::connect::Table::Account(crate::connect::AccountKind::Microsoft) => {
                    Some("its sign-in to read and send mail".to_owned())
                }
                crate::connect::Table::Account(crate::connect::AccountKind::Google) => {
                    Some("its sign-in with Google".to_owned())
                }
                _ => None,
            })
            .collect(),
        cached: match connection.template().table {
            crate::connect::Table::Account(_) => wire::Cached::Mail,
            _ => wire::Cached::Nothing,
        },
        sentence,
    }
}

/// An act, as the wire carries it, its sentence on every arm; `consent` is the box's path to the
/// consent a running sign-in opened.
#[expect(clippy::too_many_lines, reason = "one arm per kind of act")]
fn act_on_wire(
    act: &crate::secretary::acts::Act,
    consent: Option<String>,
    draws: &dyn Draw,
) -> wire::ProposedAct {
    let routine = draws.routine();
    match act {
        crate::secretary::acts::Act::Auth { account } => wire::ProposedAct::Auth {
            account: account.as_str().to_owned(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::AuthSend { account } => wire::ProposedAct::AuthSend {
            account: account.as_str().to_owned(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::Secret { name, .. } => {
            crate::secretary::card::secret_act(name, routine, draws.model())
        }
        crate::secretary::acts::Act::Web { request, rule } => wire::ProposedAct::Web {
            request: request.kind(),
            target: match request {
                crate::web::WebRequest::Search { query } => query.as_str().to_owned(),
                crate::web::WebRequest::Fetch { url } => url.as_str().to_owned(),
            },
            host: request.host().to_string(),
            always: rule
                .as_ref()
                .map(crate::web::AllowRule::sentence)
                .unwrap_or_default(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::Patch {
            repo,
            visibility,
            base,
            patch,
            message,
        } => wire::ProposedAct::Patch {
            repo: repo.to_string(),
            visibility: *visibility,
            base: base.as_str().to_owned(),
            message: message.clone(),
            files: patch.files(),
            patch: patch.as_str().to_owned(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::DriveRead { path, size } => wire::ProposedAct::Drive {
            path: path.to_string(),
            name: path.name().unwrap_or("/").to_owned(),
            folder: crate::secretary::acts::folder_of(path).to_string(),
            size: size.map(crate::secretary::card::bytes).unwrap_or_default(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::CodingSession { task } => wire::ProposedAct::CodingSession {
            task: task.as_str().to_owned(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::Device(enrolment) => wire::ProposedAct::Device {
            actor: enrolment.actor.as_str().to_owned(),
            key: enrolment.key.as_str().to_owned(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::SignIn { .. } => wire::ProposedAct::SignIn {
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::SignInCode { url } => wire::ProposedAct::SignInCode {
            url: url.as_str().to_owned(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::UnlockStore { .. } => wire::ProposedAct::UnlockStore {
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::Connect { connection } => {
            connect_on_wire(connection, draws, act.sentence())
        }
        crate::secretary::acts::Act::Disconnect { connection } => {
            disconnect_on_wire(connection, act.sentence())
        }
        crate::secretary::acts::Act::Grant { plugin, origin } => wire::ProposedAct::Grant {
            plugin: plugin.to_string(),
            origin: origin.to_string(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::GoogleClient { redirect } => wire::ProposedAct::GoogleClient {
            redirect: redirect.to_string(),
            steps: crate::google::CLIENT_STEPS
                .iter()
                .map(|&step| step.to_owned())
                .collect(),
            sentence: act.sentence(),
        },
        crate::secretary::acts::Act::GoogleSignIn { account, scopes } => {
            wire::ProposedAct::GoogleSignIn {
                account: account.to_string(),
                parts: scopes
                    .iter()
                    .map(|part| part.sentence().to_owned())
                    .collect(),
                consent,
                sentence: act.sentence(),
            }
        }
        crate::secretary::acts::Act::CalendarEvent { account, event } => {
            wire::ProposedAct::CalendarEvent {
                account: account.to_string(),
                event: event.sentence(),
                sentence: act.sentence(),
            }
        }
        crate::secretary::acts::Act::Erase { .. } => wire::ProposedAct::Erase {
            sentence: act.sentence(),
        },
    }
}

/// One attachment as its card shows it.
fn card_file(attached: &attached::Attached) -> wire::CardFile {
    wire::CardFile {
        name: attached.name.clone(),
        media: attached.media.clone(),
        size: crate::secretary::card::bytes(attached.size),
        sha256: attached.sha256.to_string(),
        from: match &attached.source {
            AttachmentSource::Mail { message, .. } => wire::FileFrom::Mail {
                message: message.clone(),
            },
            AttachmentSource::Drive { path } => wire::FileFrom::Drive {
                path: path.to_string(),
            },
            AttachmentSource::Upload { id } => wire::FileFrom::Upload { id: id.to_string() },
            AttachmentSource::Kept { .. } => wire::FileFrom::Kept {},
            AttachmentSource::Repo { repo, path, at } => wire::FileFrom::Repo {
                repo: repo.to_string(),
                path: path.as_str().to_owned(),
                at: at.as_str().to_owned(),
            },
        },
    }
}

/// A proposal's state on the wire and the words beside it, from the response its reading shows,
/// `None` while nothing answered it.
#[must_use]
pub fn state(reading: &Reading) -> (wire::ProposalState, String) {
    if let Reading::Unreadable(why) = reading {
        return (wire::ProposalState::Unreadable, why.to_string());
    }
    match reading.shown().map(|answer| &answer.response) {
        None => (wire::ProposalState::Standing, String::new()),
        Some(Responded::Tried {
            tried: Tried::NotSent { reason },
            ..
        }) => (wire::ProposalState::Failed, reason.clone()),
        Some(Responded::Tried {
            tried: Tried::Running { began, expires_at },
            ..
        }) => (
            wire::ProposalState::Started,
            format!("started at {began}, open until {expires_at}"),
        ),
        Some(Responded::Done(Done::Sent { recipients, .. })) => {
            (wire::ProposalState::Sent, recipients.join(", "))
        }
        // The chain is named by its id alone, which is no word of the owner's, so the line counts.
        Some(Responded::Done(Done::Applied { objects, .. })) => (
            wire::ProposalState::Applied,
            if objects.len() == 1 {
                "1 change".to_owned()
            } else {
                format!("{} changes", objects.len())
            },
        ),
        Some(Responded::Done(Done::Acted { what })) => (wire::ProposalState::Acted, what.clone()),
        Some(Responded::Done(Done::MayHaveGone { reason })) => {
            (wire::ProposalState::Unknown, reason.clone())
        }
        Some(Responded::Checked(check)) => {
            (wire::ProposalState::Checked, check.reason().to_owned())
        }
        Some(Responded::Dismissed { reason }) => (wire::ProposalState::Dismissed, reason.clone()),
        // Never the owner's words: a card replaced by a later one is the secretary's housekeeping.
        Some(Responded::Withdrawn(Withdrawal::Superseded)) => {
            (wire::ProposalState::Superseded, String::new())
        }
        Some(Responded::Withdrawn(Withdrawal::NoLongerNeeded { why })) => {
            (wire::ProposalState::Withdrawn, why.clone())
        }
    }
}

/// A forward's card fields: its own message's, the original named in one line, and the files
/// it sends, each opening the original.
fn forward_on_wire(forward: &Forward) -> wire::ProposedAct {
    let original = &forward.message;
    let file = &original.file;
    let opening = || wire::FileFrom::Mail {
        message: file.id.clone(),
    };
    let attachments = match original.style {
        Style::Inline => file
            .parts
            .iter()
            .map(|part| wire::CardFile {
                name: part.name.clone(),
                media: part.media.clone(),
                size: crate::secretary::card::bytes(part.size),
                sha256: part.sha256.to_string(),
                from: opening(),
            })
            .collect(),
        Style::Attached => vec![wire::CardFile {
            name: crate::secretary::store::EML.to_owned(),
            media: "message/rfc822".to_owned(),
            size: crate::secretary::card::bytes(file.size),
            sha256: file.sha256.to_string(),
            from: opening(),
        }],
    };
    wire::ProposedAct::Forward {
        account: forward.account.clone(),
        to: forward.to.clone(),
        cc: forward.cc.clone(),
        subject: forward.subject.clone(),
        note: forward.note.clone(),
        attachments,
        original: [
            Some(original.subject.clone()),
            Some(format!("from {}", original.from)),
            original.date.clone(),
        ]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", "),
        style: original.style,
        held: file
            .held
            .iter()
            .map(crate::held_value::Held::said)
            .collect(),
    }
}

pub(crate) fn proposal(
    index: usize,
    standing: &Standing,
    draws: &dyn Draw,
    memory: Option<&wire::MemoryPayload>,
) -> wire::ProposalRow {
    let act = match &standing.object.asked.proposal {
        Proposal::Email(draft) => wire::ProposedAct::Email {
            account: draft.account.clone(),
            to: draft.to.clone(),
            cc: draft.cc.clone(),
            subject: draft.subject.clone(),
            body: draft.body.clone(),
            attachments: draft.attachments.iter().map(card_file).collect(),
            held: held_in(&draft.body, draws),
        },
        Proposal::Forward(forward) => forward_on_wire(forward),
        // The diff is derived against the chain as it stands, and only while the proposal stands.
        Proposal::Changes(changeset) => {
            let (changes, problem) = if standing.object.reading.performs() {
                draws.shown(changeset)
            } else {
                (Vec::new(), String::new())
            };
            wire::ProposedAct::Changes {
                chain: changeset.chain().to_owned(),
                changes,
                problem,
            }
        }
        // `Act::sentence` is the one place that knows the imperative.
        // The box's own path, which sends the browser on to the consent the tap opened.
        Proposal::Act(act) => act_on_wire(
            act,
            standing
                .object
                .reading
                .running()
                .then(|| format!("{}{}", crate::google::CONSENT, standing.object.id.as_str())),
            draws,
        ),
        Proposal::Remember { text, kind } => {
            let (words, sentence) = match words(text, memory) {
                Words::Kept(fact) => (
                    wire::RememberWords::Kept {
                        fact: fact.as_str().to_owned(),
                    },
                    format!("Remember, in every conversation from now on: {fact}"),
                ),
                Words::Gone => (
                    wire::RememberWords::Gone {},
                    "Proposed remembering something since forgotten or dismissed; its words \
                     are deleted."
                        .to_owned(),
                ),
                Words::Unread => (
                    wire::RememberWords::Unread {},
                    "Proposed remembering something; its words are not read here.".to_owned(),
                ),
            };
            wire::ProposedAct::Remember {
                words,
                about: *kind,
                sentence,
            }
        }
        Proposal::Decide(draft) => crate::secretary::decide::on_wire(draft),
    };
    let reading = &standing.object.reading;
    let (state, outcome) = state(reading);
    let card = wire::Card {
        by: standing.object.asked.proposer.on_wire(
            &standing.object.asked.proposal,
            &standing.object.asked.asked,
        ),
        ..crate::secretary::card::at(crate::secretary::card::of(&act), reading)
    };
    wire::ProposalRow {
        index,
        card,
        act,
        why: standing.object.asked.reasons.why().to_owned(),
        sources: standing.object.asked.reasons.sources().to_vec(),
        at: crate::secretary::store::stamp(standing.asking.at),
        state,
        outcome,
        tries: u32::try_from(standing.object.tries()).unwrap_or(u32::MAX),
        turn: standing.asking.after_turn,
        coding: None,
        announced: None,
        revises: standing.revises.map(|revised| wire::Revises {
            card: revised.card,
            carried: revised.carried,
        }),
    }
}

/// A session on the wire.
fn session_on_wire(session: &crate::coding::Session) -> wire::CodingSession {
    use crate::coding::{Lines, Session};
    match session {
        Session::Fired { id, url, at } => wire::CodingSession::Fired {
            id: id.to_string(),
            url: url.to_string(),
            at: at.clone(),
        },
        Session::Reported { id, branch, report } => wire::CodingSession::Reported {
            id: id.to_string(),
            branch: branch.to_string(),
            path: report
                .get()
                .map(|report| report.path.as_str().to_owned())
                .unwrap_or_default(),
            report: report.as_ref().map(|report| &report.text).to_string(),
            stat: report
                .get()
                .map(|report| report.stat.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|file| {
                    let (added, removed) = match file.lines {
                        Lines::Text { added, removed } => (Some(added), Some(removed)),
                        Lines::Binary => (None, None),
                    };
                    wire::CodingFile {
                        path: file.path.as_str().to_owned(),
                        added,
                        removed,
                    }
                })
                .collect(),
        },
        Session::Merged { id, sha } => wire::CodingSession::Merged {
            id: id.to_string(),
            sha: sha.as_str().to_owned(),
        },
        Session::Dropped { id, reason } => wire::CodingSession::Dropped {
            id: id.to_string(),
            reason: reason.to_string(),
        },
    }
}
