//! The triage agent's verbs, as one closed sum.
//!
//! The agent re-binds these names every run with no negotiation, so a name leaving [`TriageVerb`] is a
//! major version and one joining it a minor; `app/core/tests/mcp-surface.json` pins [`tools`]
//! byte for byte, and `confirm` and `send` are pinned never to appear.

use serde_json::Value;

use super::verbs::{
    Context, Evaluate, HistoryArgs, Mark, New, NoiseAdd, Recall, ShowArgs, Sync, VerifyChain,
};
use super::{Call, Holds, Says};
use crate::authoring::Writer;
use crate::capability::Agent;
use crate::chains::ChainPolicy;
use crate::reference::TodoRef;
use crate::refusal::Refusal;
use crate::secretary::changes::{self, Act, Building, Changeset};
use crate::secretary::filed::Carding;
use crate::secretary::shown_record::{OwnersRecord, OwnersWords};
use crate::secretary::verbs::Reasoned;
use crate::verb::{Chained, Surface, verbs};
use crate::zone::Moment;
use prodrome_core::event::TodoId;
use serde_json::json;

/// The triage agent's verbs: each runs against a [`Call`] and answers JSON.
pub enum TriageSurface {}

impl Surface for TriageSurface {
    type Call<'a> = Call<'a>;
    type Answer = Value;
    type Class = ();
}

impl Holds for TriageSurface {
    const WRITES_AS: Writer = super::WRITES_AS;
    const SAYS: Says = Says {
        list_open_todos: "Every currently-open todo, as {id, kind, category, priority, body, \
        waiting_on, authored_by}. Use this to avoid creating a duplicate — if a \
        message belongs to a topic already tracked here, link it to that todo's \
        id via `mark ... --todo-id <id>` instead of authoring a new one. \
        Complete view: it is the chain's own fold, not a scan of recent files, \
        and the priority is evaluated as of right now rather than whenever a \
        document was last compiled.\n\n\
        ⚠️ IT IS ONE CHAIN'S LIST. Dedup against the chain you are about to \
        write to: leaving `chain` out reads the owner's own todos, and \
        `chain=\"suzatary\"` reads this system's own work. Checking one and \
        writing to the other is how a duplicate gets made.",
        git_diff_summary: "A short --stat summary of uncommitted changes, for a final sanity check \
        before ending your session. You do not commit or push yourself — that \
        happens afterward, deterministically, only if every store verifies.",
        history: "The full lifecycle event stream for one todo from the chain, oldest \
        first, as readable text (one line per event: kind, at, actor, note). An \
        unknown id is not an error — it just has no history yet. `chain` picks \
        the store the todo lives in.",
        todo_show: "One todo as the chain holds it: its rendered block, its fields, and who \
        last authored it when. Use it to check what you just wrote, or to see a \
        todo's detail and source before deciding whether a new message belongs to \
        it. `chain` picks the store it lives in.",
        show: "Full message headers + plain-text body. Takes either the `id` field from \
        `new`/`search`, or a canonical `hash` — full, or a unique prefix such as \
        the one stored in a todo's `source.hash`. A body longer than one answer \
        comes back as its first part, with a line naming how many parts there are \
        — pass `part` (2, 3, …) for the rest. Never guess at what a cut part left \
        out; ask for the next one. `links` and every header travel whole on every \
        part.",
        record: "Propose a lifecycle event for a todo. kind is 'completed', 'cancelled', \
        'reopened', or 'tended' (no 'created' — todo authoring lives in the \
        content verbs; no spec authoring). 'tended' is a pass at a RECURRING \
        todo (its price holds a Recur): it was done again and stays open, so \
        its price restarts from now. Never 'completed' for one of those.\n\n\
        Nothing is written to the chain: this files a card in the owner's Inbox, joining \
        any open card about the same todo, and he taps it or answers no. `why` and \
        `sources` are REQUIRED, as on `todo_add`; `sources` names the message that shows \
        it happened. `todo` must be an id the chain already knows, or one an open card \
        of yours adds; an unknown id is rejected as a likely typo.\n\n\
        `chain` is WHICH STORE the todo lives in and must match the one it was \
        authored in — leave it out for the owner's own list, pass \
        `chain=\"suzatary\"` for this system's work.",
        todo_add: "Propose a new todo. Nothing is written to the chain: this files a CARD in the owner's \
        Inbox, which he taps to make the todo or answers no. A later `todo_update` or `record` \
        on the same todo joins the same card, so the Inbox holds one card for it.\n\n\
        `why`: REQUIRED — what the card is for, in a sentence the owner reads first.\n\
        `sources`: REQUIRED — what it rests on, comma-separated references, usually \
        `mail:<hash>` of the message.\n\
        `todo_id`: 3-64 lowercase letters/digits, starting with a letter. It is \
        the handle `mark --todo-id` and `record` use, so choose something you can \
        reproduce (e.g. `permitfiling`).\n\
        `body`: the action line, one sentence, as Typst MARKUP: `*bold*` is bold, and \
        a `$`, `@`, `#`, `*`, `_`, `<` or `[` you mean literally takes a backslash \
        (`Pay \\$40`). It is compiled before it is stored, without `html` or any \
        file, and refused with the line and column when it does not compile. \
        `detail` is the same. Every other field is plain text.\n\
        `rationale`: REQUIRED — WHY this priority, in one sentence. It is \
        recorded as the comment above the number and is the only place the \
        reasoning exists.\n\
        `kind`: 'todo' (needs `category`), 'ekaTodo' or 'suzateryTodo' (which set \
        their own category — do not pass one).\n\
        `priority`: a whole percentage like '65%'. It is a FULFILLMENT: LOW means \
        urgent, HIGH means satisfied or someday. Give exactly one of `priority`, \
        `deadline_end_date`, `recur_every_days` or `periodic_every_days`.\n\
        `deadline_*`: use when the todo decays toward a date rather than sitting \
        at a fixed number (`deadline_start_priority` defaults to 60%, \
        `deadline_end_priority` to 5%). A deadline bounds WHEN, not the stakes: \
        something merely moot afterwards (a form, a small recoverable cost) still \
        floors around 60% on its last day.\n\
        `recur_every_days`: a chore done again and again (vinegar the \
        toothbrush every two months). After every pass (`record` kind \
        'tended') the fulfillment runs from `cycle_from_priority` to \
        `cycle_to_priority` over that many days, then holds; before the first \
        pass it keeps the price it had. `periodic_every_days` is the same \
        cycle on the calendar, whatever is done (rent). Both need both ends.\n\
        `waiting_on`: only when there is genuinely nothing we can do. One \
        unblocking email we could send means leave it unset.\n\
        `source_*`: all four or none — the message this came from.\n\
        `date`: which day the todo is created on, YYYY-MM-DD, default the day it is made.\n\
        `parent`: the id of the todo this is a SUBTODO of. The parent's price \
        gains a reference to this one in the same write, so its value composes \
        this one's; without `category` this takes the parent's.\n\
        `chain`: WHICH LIST it goes in, and this is a decision, not a detail. \
        Leave it out for the owner's own todos — mail, deadlines, people, money, \
        travel. Pass `chain=\"suzatary\"` for this system's own work: a gate \
        that failed, a test that is flaky, a sync that broke, a verb that needs \
        fixing. The test is who the todo is FOR. A todo lives in ONE chain and \
        cannot be moved afterwards, because the Prodrome does not rewrite.",
        todo_update: "Propose changed fields on a todo that already exists. Nothing is written to the \
        chain: this files a card in the owner's Inbox, joining any open card about the same \
        todo, and he taps it or answers no. `why` and `sources` are REQUIRED, as on \
        `todo_add`. Only the fields you pass change.\n\nChanging `priority` (or a deadline, or a cycle — the same price fields `todo_add` \
        takes, one of them) REQUIRES `rationale`: the comment above that number is the only \
        record of why it is what it is. Passing `rationale` alone rewrites just the reason.\n\n\
        `clear_waiting_on` sets it back to `none` — use it the moment there is an email we \
        could send, since `waitingOn` means nothing we can do.\n\n\
        `body`, `detail` and `detail_append` are Typst markup, checked as `todo_add` checks \
        them. `detail_append`: a sentence to add under the existing detail, keeping its \
        markup — use this, never `detail`, when the detail already exists; give at most one \
        of the two.\n\n`chain` must be the store the todo was authored in; there is no moving \
        one between them.",
        zoom: "OPEN A PART OF THE SECRETARY'S RECORD, read-only: \
        `handle` is a part's `first+lines`, as `context`'s `memory` or `recall` shows it. \
        Answers what is beneath it, each with its own handle: the owner's own line whole, a \
        summary of nothing but their lines (`derived`, nobody's own words), or a span not \
        summarised yet. Anything else (the secretary's answers, a verb's answer, the box's \
        notes) stays shown by its kinds alone, however far you open it. Zoom before you file \
        on what a summary says.",
    };
    type Filing = Reasoned;
    type Shows = OwnersWords;

    fn agent<'a>(call: &Self::Call<'a>) -> Agent<'a> {
        *call.caps
    }

    fn record(call: &Call<'_>) -> Result<OwnersRecord, Refusal> {
        call.caps.memory.record()
    }

    fn chain<'a>(call: &Self::Call<'a>) -> &'a ChainPolicy {
        call.chain
    }

    fn write(call: &Call<'_>, act: Act, filing: Reasoned) -> Result<Value, Refusal> {
        let reasons = filing.reasons()?;
        let at = call.caps.clock.now();
        let nodes = call.caps.chain.nodes()?;
        // A card about a todo the chain holds is priced as that todo is.
        let serves = crate::todos::current(&nodes, at, act.todo())
            .map(|_| TodoId::new(act.todo()))
            .transpose()
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?
            .map(|id| TodoRef {
                chain: call.chain.name.clone(),
                id,
            });
        let changeset = Changeset::new(call.chain.name.as_str(), vec![act])
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        let categories: Vec<&str> = call
            .deployment
            .categories
            .keys()
            .map(String::as_str)
            .collect();
        // Built as the tap will build it, so a card the tap would refuse is refused now.
        let building = Building {
            now: Moment {
                at,
                zone: call.deployment.zone,
            },
            by: crate::secretary::WRITES_AS,
            categories: &categories,
            untrusted: &call.chain.untrusted,
            typeset: call.caps.render,
        };
        let check = |changeset: &Changeset| changes::checked(changeset, call.caps.chain, &building);
        let carded = call.cards.file(at, changeset, reasons, serves, &check)?;
        Ok(json!({
            "filed": carded.card.to_string(),
            "acts": carded.acts,
            "next": match carded.how {
                Carding::New => {
                    "Filed as a card in the owner's Inbox. Nothing is on the chain until he taps \
                     it; a later todo_update or record on this todo joins this card."
                }
                Carding::Joined => {
                    "Joined the open card about this todo: the owner sees one card with every act \
                     in order. Nothing is on the chain until he taps it."
                }
                Carding::Already => {
                    "The open card about this todo already asks for this: nothing new was filed."
                }
            },
        }))
    }
}

type GitDiffSummary = super::verbs::GitDiffSummary<TriageSurface>;
type History = super::verbs::History<TriageSurface, Chained<HistoryArgs>>;
type ListOpenTodos = super::verbs::ListOpenTodos<TriageSurface>;
type Record = super::verbs::Record<TriageSurface>;
type TodoAdd = super::verbs::TodoAdd<TriageSurface>;
type TodoUpdate = super::verbs::TodoUpdate<TriageSurface>;
type Search = super::verbs::Search<TriageSurface>;
type Show = super::verbs::Show<TriageSurface, ShowArgs>;
type TodoShow = super::verbs::TodoShow<TriageSurface>;
type Zoom = super::verbs::Zoom<TriageSurface>;

verbs! {
    /// Every verb the triage agent may call, and nothing else.
    pub enum TriageVerb;
    /// One call of a [`TriageVerb`], its arguments parsed.
    pub enum TriageCall;
    on TriageSurface;
    Sync: (),
    New: (),
    Search: (),
    Show: (),
    Mark: (),
    NoiseAdd: (),
    ListOpenTodos: (),
    GitDiffSummary: (),
    Record: (),
    History: (),
    Evaluate: (),
    VerifyChain: (),
    Context: (),
    TodoAdd: (),
    TodoUpdate: (),
    TodoShow: (),
    Recall: (),
    Zoom: (),
}

/// The whole surface as JSON in the MCP `tools/list` shape, which both the server and the
/// snapshot read.
#[must_use]
pub fn tools() -> Value {
    Value::Array(TriageVerb::ALL.iter().map(|verb| verb.tool()).collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// The surface is a closed list, and `confirm` and `send` may never join it.
    #[test]
    fn there_is_no_confirm_verb_and_no_send_verb() {
        for verb in TriageVerb::ALL.iter().map(|verb| verb.name()) {
            assert!(
                !verb.contains("confirm"),
                "an agent that can confirm its own writes has confirmed nothing: {verb}",
            );
            assert!(
                !verb.contains("send") && !verb.contains("reply"),
                "sending is structurally human-only: {verb}",
            );
        }
    }

    /// `record` is the only mutator over the chain's lifecycle.
    #[test]
    fn the_chain_has_exactly_one_lifecycle_mutator() {
        let names: BTreeSet<&str> = TriageVerb::ALL.iter().map(|verb| verb.name()).collect();
        assert!(names.contains("record"));
        for forbidden in ["append", "seal", "delete", "amend"] {
            assert!(!names.contains(forbidden), "{forbidden}");
        }
    }
}
