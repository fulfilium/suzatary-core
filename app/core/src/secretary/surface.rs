//! The secretary's verbs, as one closed sum.
//!
//! A separate sum from `crate::agent::TriageVerb` on purpose, so widening one never widens the
//! other; neither `send` nor `confirm` may ever appear here.

use serde_json::Value;

use super::Call;
use super::tool_name::ToolName;
use super::verb_answer::VerbAnswer;
use super::verb_name::{Binding, VerbName};
use super::verbs::{
    AddTopics, Attachments, Briefing, BriefingDelta, Calendar, Consolidate, Context, Decisions,
    DraftNote, DriveList, DriveRead, Forget, GitDiffSummary, History, JunkDigest, JunkShow,
    ListOpenTodos, ProposeAct, ProposeChanges, ProposeCodingSession, ProposeDecision, ProposeEmail,
    ProposeForward, ProposePatch, Recall, Record, Remember, RepoGrep, RepoRead, RepoTree,
    ReposList, ReviseTopics, Search, SearchOlder, Show, SyncMail, TodoAdd, TodoShow, TodoUpdate,
    TriageCards, TriageWithdraw, UploadRead, WatchMail, Weather, Web, WithdrawProposal, Zoom,
};
use super::verbs::{IfCarded, check, propose};
use crate::agent::Agent;
use crate::agent::{Holds, Says};
use crate::chains::ChainPolicy;
use crate::refusal::{Refusal, shown};
use crate::secretary::changes::{Act, Changeset, Unapplied};
use crate::secretary::compaction::Reading;
use crate::secretary::gate::Gate;
use crate::secretary::proposal::Proposal;
use crate::secretary::shown_record::{Everything, ShownRecord};
use crate::verb::{Surface, verbs};
use crate::writer::Writer;

/// The secretary's verbs: each runs against a [`Call`], and answers JSON and the files it hands
/// the model.
pub enum SecretarySurface {}

impl Surface for SecretarySurface {
    type Call<'a> = Call<'a>;
    type Answer = VerbAnswer;
    type Class = VerbName;
}

impl Holds for SecretarySurface {
    const WRITES_AS: Writer = super::WRITES_AS;
    const SAYS: Says = Says {
        list_open_todos: "Every currently-open todo, as {id, kind, category, priority, body, \
        waiting_on, authored_by}, folded straight off the chain and priced as of \
        now. Read it before you author, so you update an existing todo instead \
        of making a second one.\n\n\
        ⚠️ IT IS ONE CHAIN'S LIST. Leaving `chain` out reads the owner's own todos; \
        `chain=\"suzatary\"` reads this system's own work.",
        git_diff_summary: "What is uncommitted in the working tree, as a summary. Orientation about \
        the state of the box, never something to act on.",
        history: "Every event on the chain that mentions this todo, oldest first, one \
        line each with its kind, time, actor and note — what happened to it and when.",
        todo_show: "One todo's full current fields — body, detail, rationale, source, \
        subtodos, waiting_on. Use it before deciding whether something belongs \
        to a todo that already exists.",
        show: "One message in full: headers, plain-text body, and every link with its \
        anchor label. Takes the `id` from a `search` row, or a canonical hash \
        (full, or the prefix a todo's `source.hash` carries).",
        record: "Close, cancel, reopen or tend a todo: kind is 'completed', \
        'cancelled', 'reopened' or 'tended'. 'tended' is a pass at a RECURRING \
        todo (its price holds a Recur): done again, still open, its price \
        restarting from now — never 'completed' for one of those. Only when the owner \
        has said so. It binds as `assistant` in a round holding the owner's words alone; \
        anywhere else it files a card they tap, and `why` and `sources` are required.",
        todo_add: "AUTHOR A TODO, as the trusted actor `assistant`. In a round holding the \
        owner's words alone it binds at once; after you read anything else (mail, a todo, a \
        document, the web, an old answer) it files a card they tap, and `why` and `sources` \
        are required. Author what they asked for and propose the rest.\n\n\
        `body`: the action line, one sentence, as Typst MARKUP: `*bold*` is bold, and \
        a `$`, `@`, `#`, `*`, `_`, `<` or `[` you mean literally takes a backslash \
        (`Pay \\$40`). It is compiled before it is stored, without `html` or any \
        file, and refused with the line and column when it does not compile. \
        `detail` is the same. Every other field is plain text.\n\
        `rationale` is required: one sentence saying why THAT number.\n\
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
        `source_*`: all four or none — the message this came from. `date`: which day it is \
        created on, YYYY-MM-DD.\n\
        `chain` picks the store: leave it out for the owner's life, \
        `chain=\"suzatary\"` for this system's own work.\n\n\
        `parent` makes it a SUBTODO: the parent's price gains a reference to \
        it in the same write, so the parent's value composes this one's; \
        without `category` it takes the parent's.",
        todo_update: "Change fields on a todo that exists, as the trusted actor `assistant`, \
        binding or carded exactly as `todo_add` is. Only what you name changes.\n\nChanging `priority` (or a deadline, or a cycle — the same price fields `todo_add` \
        takes, one of them) REQUIRES `rationale`: the comment above that number is the only \
        record of why it is what it is. Passing `rationale` alone rewrites just the reason.\n\n\
        `clear_waiting_on` sets it back to `none` — use it the moment there is an email we \
        could send, since `waitingOn` means nothing we can do.\n\n\
        `body`, `detail` and `detail_append` are Typst markup, checked as `todo_add` checks \
        them. `detail_append`: a sentence to add under the existing detail, keeping its \
        markup — use this, never `detail`, when the detail already exists; give at most one \
        of the two.\n\n`chain` must be the store the todo was authored in; there is no moving \
        one between them.",
        zoom: "OPEN A PART OF THE RECORD of every conversation you \
        were told when this one opened: `handle` is a part's `first+lines`. Answers what is \
        beneath it, each with its own handle: a line whole, a summary (`derived`, nobody's own \
        words), or a span not summarised yet. A part you were shown in parentheses may hold \
        somebody else's words, and opening it shows them: its answer says `taint: tainted`, \
        and a `remember` after it becomes a card. Zoom before you act on what a summary says, \
        guess, or ask.",
    };
    type Filing = IfCarded;
    type Shows = Everything;

    fn agent<'a>(call: &Self::Call<'a>) -> Agent<'a> {
        call.caps.agent()
    }

    fn record(call: &Call<'_>) -> Result<ShownRecord<Everything>, Refusal> {
        Reading::of(&call.caps.memories.digests.reads()).map(ShownRecord::of)
    }

    fn chain<'a>(call: &Self::Call<'a>) -> &'a ChainPolicy {
        call.chain
    }

    fn write(call: &Call<'_>, act: Act, carded: IfCarded) -> Result<Value, Refusal> {
        let changeset = Changeset::new(call.chain.name.as_str(), vec![act])
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        let round =
            crate::secretary::conversations::read(call.caps.conversations, call.conversation)?;
        match Gate::of(Binding::Writes, &round) {
            Gate::Binds(_) => applied(call, &changeset),
            Gate::Card => {
                let reasons = carded.reasons("the write")?;
                check(call, &changeset)?;
                let index = propose(
                    call.caps,
                    call.chains,
                    call.conversation,
                    Proposal::Changes(changeset),
                    &reasons,
                    None,
                    None,
                )?;
                Ok(serde_json::json!({
                    "proposed": index,
                    "chain": call.chain.name.as_str(),
                    "next": "This round holds words that are not the owner's own, so the write \
                             is a card they tap and nothing is on the chain until then. Say so \
                             in one clause.",
                }))
            }
            Gate::Refused(why) => Err(Refusal::forbidden(why)),
        }
    }
}

/// Append `changeset` as the trusted actor, which only a round [`Gate::Binds`] reaches.
fn applied(call: &Call<'_>, changeset: &Changeset) -> Result<Value, Refusal> {
    let categories: Vec<&str> = call
        .deployment
        .categories
        .keys()
        .map(String::as_str)
        .collect();
    match crate::secretary::gate::apply(
        call.caps,
        call.chain,
        &categories,
        call.deployment.zone,
        changeset,
        super::WRITES_AS,
    ) {
        Ok(names) => Ok(serde_json::json!({
            "ok": true,
            "chain": changeset.chain(),
            "todo": changeset.acts().first().map(Act::todo),
            "appended": names
                .iter()
                .map(prodrome_core::event::Hash::as_str)
                .collect::<Vec<_>>(),
        })),
        Err(Unapplied::Refused(refusal) | Unapplied::Appending(refusal)) => Err(refusal),
    }
}

verbs! {
    /// Every verb the secretary may call, each by its [`VerbName`], which says its powers.
    pub enum SecretaryVerb;
    /// One call of a [`SecretaryVerb`], its arguments parsed.
    pub enum SecretaryCall;
    on SecretarySurface, classed by VerbName;
    Context,
    Remember,
    Forget,
    Consolidate,
    Recall,
    Zoom,
    ListOpenTodos,
    TodoShow,
    History,
    Search,
    Show,
    Attachments,
    UploadRead,
    SyncMail,
    SearchOlder,
    WatchMail,
    BriefingDelta,
    Briefing,
    GitDiffSummary,
    TodoAdd,
    TodoUpdate,
    Record,
    ProposeEmail,
    ProposeForward,
    ProposeAct,
    ProposeChanges,
    WithdrawProposal,
    Weather,
    Calendar,
    Decisions,
    Web,
    DriveList,
    DriveRead,
    JunkDigest,
    JunkShow,
    ReposList,
    RepoTree,
    RepoRead,
    RepoGrep,
    ProposePatch,
    ProposeCodingSession,
    ProposeDecision,
    DraftNote,
    TriageCards,
    TriageWithdraw,
    AddTopics,
    ReviseTopics,
}

impl From<SecretaryVerb> for ToolName {
    fn from(verb: SecretaryVerb) -> Self {
        Self::Verb(verb.class())
    }
}

/// Some of the verbs: the ones a round's model is shown and may call, and the refusal a call of
/// any other gets.
pub struct Subset {
    /// Whether a verb is in it.
    pub allows: fn(SecretaryVerb) -> bool,
    /// The refusal of `called`, given the names the subset lists.
    pub refuses: fn(called: &str, listed: &str) -> Refusal,
}

impl Subset {
    /// Every verb.
    pub const ALL: Self = Self {
        allows: |_| true,
        refuses: |called, listed| {
            Refusal::not_found(format!(
                "no verb named {} — the secretary's verbs are {listed}",
                shown(called)
            ))
        },
    };

    /// Its verbs, in [`SecretaryVerb::ALL`]'s order.
    pub fn verbs(&self) -> impl Iterator<Item = SecretaryVerb> + '_ {
        SecretaryVerb::ALL
            .iter()
            .copied()
            .filter(|verb| (self.allows)(*verb))
    }

    /// Its verbs as the JSON a model is shown.
    #[must_use]
    pub fn tools(&self) -> Value {
        tools_of(self.verbs())
    }

    /// The verb `called` names, if the subset lists it.
    ///
    /// # Errors
    /// [`Subset::refuses`]'s refusal for any other name.
    pub fn find(&self, called: &ToolName) -> Result<SecretaryVerb, Refusal> {
        called
            .verb()
            .map(SecretaryVerb::of)
            .filter(|verb| (self.allows)(*verb))
            .ok_or_else(|| {
                (self.refuses)(
                    called.as_str(),
                    &self
                        .verbs()
                        .map(SecretaryVerb::name)
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            })
    }
}

/// The whole surface as JSON, in the same shape the agent's `tools/list` uses.
#[must_use]
pub fn tools() -> Value {
    Subset::ALL.tools()
}

/// The same JSON over some of the verbs, for a round scoped to fewer.
#[must_use]
pub(crate) fn tools_of(verbs: impl Iterator<Item = SecretaryVerb>) -> Value {
    Value::Array(verbs.map(SecretaryVerb::tool).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Neither a send-shaped nor a confirm-shaped name may join this list.
    #[test]
    fn there_is_no_send_verb_and_no_confirm_verb() {
        for verb in SecretaryVerb::ALL.iter().map(|verb| verb.name()) {
            assert!(
                !verb.contains("confirm"),
                "the secretary's writes are already trusted; a confirm verb confirms nothing: {verb}",
            );
            assert!(
                verb == "propose_email" || !verb.contains("send"),
                "sending is structurally human-only: {verb}",
            );
        }
    }

    /// Each verb maps to the `VerbName` of its own name, and every `VerbName` is some verb's, so
    /// the two lists are one.
    #[test]
    fn the_verbs_and_their_names_are_one_list() {
        let mapped: Vec<VerbName> = SecretaryVerb::ALL.iter().map(|verb| verb.class()).collect();
        assert_eq!(mapped, VerbName::ALL);
        for &verb in SecretaryVerb::ALL {
            assert_eq!(verb.class().name(), verb.name());
        }
    }

    /// `named` inverts `name`, and the tool a model is shown carries the same name.
    #[test]
    fn a_verb_is_named_by_its_name_and_its_tool() {
        for &verb in SecretaryVerb::ALL {
            assert_eq!(SecretaryVerb::named(verb.name()), Some(verb));
            assert_eq!(verb.tool()["name"], verb.name());
        }
    }
}
