use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::health::Reading;
use crate::record::TodoEvent;
use crate::refusal::{Refusal, shown};
use crate::secretary::store::stamp;
use crate::secretary::{Call, SecretarySurface, heartbeat};
use crate::source_key::SourceKey;
use crate::verb::{Chained, Verb};

/// What moved since a day.
pub struct BriefingDelta;

#[derive(Deserialize, JsonSchema)]
pub struct BriefingDeltaArgs {
    since: Option<String>,
}

/// How far back it looks when asked for no day, so a Monday covers the weekend.
const DEFAULT_LOOKBACK_DAYS: i64 = 7;

/// Whether `event` happened on an owner's day before `since`.
fn before(zone: crate::zone::Zone, event: &TodoEvent, since: chrono::NaiveDate) -> bool {
    zone.day(prodrome_core::fpl::instant_of(event.at())) < since
}

fn life_row(todo: &str, life: &prodrome_core::event::Lifecycle) -> Value {
    json!({
        "todo": todo,
        "at": stamp(life.at),
        "by": life.actor.as_str(),
        "note": life.note.as_str(),
    })
}

/// The proposals waiting, in one tagged shape: the Inbox's list and every chain that prices
/// nothing referring into it, or why the inbox did not read, while the rest of the delta stands.
fn waiting(call: &Call<'_>, at: crate::capability::Datetime) -> Value {
    match crate::secretary::inbox::fold(call.caps.inbox) {
        Ok(fold) => {
            let read = crate::secretary::chains_read(call.chains);
            let set = read.set(at);
            json!({
                "ok": crate::secretary::readings::waiting(&fold, &set),
                "unpriced_chains": set.failures().map(ToString::to_string).collect::<Vec<_>>(),
                "cannot_be_read": fold.findings(),
            })
        }
        Err(refused) => json!({ "unreadable": refused.message() }),
    }
}

/// What the model is told the delta is for.
const NOTE: &str = "This is what MOVED. The priced list is the app's job — say what changed, \
                     what became urgent, what is good news, and propose two or three moves. \
                     ALWAYS state triage_last_pass's age too, in one clause, even on an \
                     ordinary day — a quiet chain and a wrapper that has stopped committing \
                     look identical from the delta alone, and this number is the only thing \
                     that tells them apart. Name every account in mail_failing, with its words \
                     and since when, in one clause each; one failing more than a day already \
                     has a proposed todo in the Inbox, so do not propose another. \
                     proposals_waiting is the Inbox's own list, most urgent first, each \
                     priced by the todo it serves: name the first few that wait. Name each \
                     line of cannot_be_read once, as a problem with the inbox's store that \
                     `suzatary verify` reports and no tap can settle. When \
                     compactor_held is not null, say in one clause what holds the digest's \
                     summaries back and how many wait.";

/// What holds the digest's summaries back now, in a sentence; null when nothing does.
fn compactor_held(call: &Call<'_>, at: crate::capability::Datetime) -> Value {
    match crate::secretary::compaction::waiting(&call.caps.memories.digests, at) {
        Ok(held) => held.map_or(Value::Null, |held| json!(held.to_string())),
        Err(refused) => json!({ "unreadable": refused.message() }),
    }
}

impl Verb for BriefingDelta {
    type Surface = SecretarySurface;
    type Args = Chained<BriefingDeltaArgs>;
    type Answer = Value;
    const NAME: &'static str = "briefing_delta";
    const DESCRIPTION: &'static str = "WHAT CHANGED since a day (YYYY-MM-DD; default the last seven days): \
        todos authored, resolved, cancelled, reopened and repriced, and the \
        messages that produced them, and the proposals waiting in the Inbox in \
        its own order: `proposals_waiting` is `{ok: [...], unpriced_chains: \
        [...], cannot_be_read: [...]}` (a chain named there prices nothing that \
        refers into it; a proposal named in the last waits on nothing, since \
        nothing can act on it), or `{unreadable: why}`. This is what a briefing is made of — say what MOVED and \
        what to do next; the priced todo list is the app's, never yours to \
        repeat.";

    fn run(call: &Call<'_>, Chained(args): Chained<BriefingDeltaArgs>) -> Result<Value, Refusal> {
        let at = call.caps.clock.now();
        let now = prodrome_core::fpl::instant_of(at);
        let since = match args.since.as_deref() {
            None => call.deployment.zone.day(now) - chrono::Duration::days(DEFAULT_LOOKBACK_DAYS),
            Some(day) => chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| {
                Refusal::bad_request(format!("since must be YYYY-MM-DD, got {}", shown(day)))
            })?,
        };
        let nodes = call.caps.chain.nodes()?;
        let events = crate::view::events_of(&nodes);

        // Todos that existed before the window, so a revision is not read as a new todo.
        let older: std::collections::BTreeSet<&str> = events
            .iter()
            .filter(|event| before(call.deployment.zone, event, since))
            .map(|event| event.todo().as_str())
            .collect();

        let mut authored = Vec::new();
        let mut revised = Vec::new();
        let mut completed = Vec::new();
        let mut cancelled = Vec::new();
        let mut reopened = Vec::new();
        let mut tended = Vec::new();
        let mut repriced = Vec::new();
        for event in events
            .iter()
            .filter(|event| !before(call.deployment.zone, event, since))
        {
            let todo = event.todo().as_str().to_owned();
            match event {
                TodoEvent::Authored(record) => {
                    let row = json!({
                        "todo": todo,
                        "at": stamp(record.at),
                        "by": record.actor.as_str(),
                        "body": record.payload.body.as_str(),
                        "source": record.payload.source.as_ref().map(|source| json!({
                            "sender": source.sender.as_str(),
                            "subject": source.subject.as_str(),
                            "hash": source.hash.as_str(),
                        })),
                    });
                    if older.contains(todo.as_str()) {
                        revised.push(row);
                    } else {
                        authored.push(row);
                    }
                }
                TodoEvent::Completed(life) => completed.push(life_row(&todo, life)),
                TodoEvent::Cancelled(life) => cancelled.push(life_row(&todo, life)),
                TodoEvent::Reopened(life) => reopened.push(life_row(&todo, life)),
                TodoEvent::Tended(life) => tended.push(life_row(&todo, life)),
                TodoEvent::SpecRevised(spec) => repriced.push(json!({
                    "todo": todo,
                    "at": stamp(spec.at),
                    "by": spec.actor.as_str(),
                })),
                // A bare `Created` says nothing a briefing reader wants.
                TodoEvent::Created(_) => {}
            }
        }
        // The register's reading now, not the window's: an account is failing or it is not.
        let failing: Vec<Value> = call
            .caps
            .health
            .healths()
            .map_err(|failed| Refusal::internal(format!("the sources' health register: {failed}")))?
            .readings()
            .filter_map(|(source, reading)| {
                let (
                    SourceKey::Mail(account),
                    Reading::Failing {
                        at, words, since, ..
                    },
                ) = (source, reading)
                else {
                    return None;
                };
                Some(json!({
                    "account": account.as_str(),
                    "words": words,
                    "since": since.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    "last_tried": at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                }))
            })
            .collect();
        Ok(json!({
            "since": since.to_string(),
            "authored": authored,
            "revised": revised,
            "completed": completed,
            "cancelled": cancelled,
            "reopened": reopened,
            "tended": tended,
            "repriced": repriced,
            // Not from the chain, so it stays truthful when a failing wrapper silences the chain.
            "triage_last_pass": heartbeat::freshness(now, call.caps.corpus.last_triage_pass().as_deref()),
            "mail_failing": failing,
            "proposals_waiting": waiting(call, at),
            "compactor_held": compactor_held(call, at),
            "note": NOTE,
        }))
    }
}
