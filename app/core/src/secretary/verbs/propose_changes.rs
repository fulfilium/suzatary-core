use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::Refusal;
use crate::secretary::changes::{self, Changeset};
use crate::secretary::store::Proposal;
use crate::secretary::verbs::FilingArgs;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::{Chained, Verb};

/// Propose chain acts, which only a tap applies.
pub struct ProposeChanges;

#[derive(Deserialize, JsonSchema)]
pub struct ProposeChangesArgs {
    #[schemars(required, with = "Vec<Value>")]
    acts: Option<Value>,
    #[serde(flatten)]
    filing: FilingArgs,
}

impl ProposeChangesArgs {
    /// The acts as the model sent them, which the sweep checks before the verb runs.
    pub(crate) fn acts(&self) -> Option<&Value> {
        self.acts.as_ref()
    }
}

impl Verb for ProposeChanges {
    type Surface = SecretarySurface;
    type Args = Chained<ProposeChangesArgs>;
    type Answer = Value;
    const NAME: &'static str = "propose_changes";
    const DESCRIPTION: &'static str = "PROPOSE A CHANGESET — chain acts you want made and are NOT SURE OF. \
        Nothing is appended: the owner sees the acts as a DIFF, row by row with \
        before and after, and confirms, denies with a reason, or replies.\n\n\
        WRITE DIRECTLY (todo_add / todo_update / record) for routine, evidenced \
        work. PROPOSE A CHANGESET for anything that changes a real commitment of \
        theirs — a new todo at or under 45, any change to a todo THEY wrote, anything \
        about identity documents, money, legal or medical, a cancellation, a \
        repricing across rows. When in doubt, propose. SECRETARY.md has the \
        rule.\n\n\
        `acts` is an ARRAY of objects, each with an `act` of add, update, \
        complete, cancel, tend or reprice and a `todo` naming the id — `todo` \
        on EVERY act, not `todo_id`. An `add` takes `body` and `rationale` \
        and a price (`priority` as '45%', the `deadline_*` fields, or a cycle \
        — `recur_every_days` or `periodic_every_days` with \
        `cycle_from_priority` and `cycle_to_priority`, exactly as on todo_add), \
        plus `kind`, `category`, `detail`, `waiting_on`; an `update` takes only \
        what changes; `complete`, `cancel` and `tend` (a pass at a recurring \
        todo, which stays open) take an optional `note`; a `reprice` takes one \
        price in any of those spellings and `rationale` — making a chore \
        recurring is a reprice. The acts happen IN ORDER, so a \
        changeset may add a todo and then complete it.\n\n\
        Every act is built against the chain the moment you propose it, so an \
        id already taken or a todo that is not there is refused NOW. `why` and \
        `sources` are REQUIRED and are what they read first, exactly as on \
        propose_email. `chain` picks the store, as everywhere else.\n\n\
        ⚠️ `serves` and `revises` are the same two arguments `propose_email` \
        takes, and mean the same thing.";

    fn run(call: &Call<'_>, Chained(args): Chained<ProposeChangesArgs>) -> Result<Value, Refusal> {
        let reasons = args.filing.reasons()?;
        let changeset = Changeset::new(
            call.chain.name.as_str(),
            changes::acts_of(args.acts.as_ref())?,
        )
        .map_err(|failed| Refusal::bad_request(failed.to_string()))?;

        // Built against the chain only as a check, so one the chain would refuse is refused
        // while it is fixable; the tap is the act.
        let known: Vec<&str> = call
            .deployment
            .categories
            .keys()
            .map(String::as_str)
            .collect();
        changes::checked(
            &changeset,
            call.caps.chain,
            &changes::Building {
                now: crate::zone::Moment {
                    at: call.caps.clock.now(),
                    zone: call.deployment.zone,
                },
                by: crate::secretary::WRITES_AS,
                categories: &known,
                untrusted: &call.chain.untrusted,
                typeset: call.caps.render,
            },
        )?;

        let index = args
            .filing
            .file(call, Proposal::Changes(changeset.clone()), &reasons)?;
        Ok(json!({
            "proposed": index,
            "chain": call.chain.name.as_str(),
            "acts": changeset.acts().len(),
            "why": reasons.why(),
            "next": "The owner sees this as a DIFF — every row with its before and after — and \
                     confirms it, denies it with a reason, or replies. Nothing is on the chain \
                     until they confirm. Do not propose the same changeset twice; if they deny \
                     one, read the reason and propose the corrected one or ask.",
        }))
    }
}
