//! The open list, swept: one secretary round whose only write is `propose_changes` of
//! `complete`/`cancel` on a todo that round showed, over the [`candidates`] the mail offers.

pub mod candidates;

use serde_json::{Map, Value};

use super::changes::Act;
use super::daily::{Daily, daily_round};
use super::store::{ConversationId, Entry, Proposal, Stored};
use super::surface::{Effect, SecretaryCall, SecretaryVerb, Subset};
use super::verbs::Show;
use super::{Bound, MAX_ROUNDS};
use crate::believed::one_outcome;
use crate::capability::{Halt, Secretary, Turn};
use crate::conversation::ask::Ask;
use crate::conversation::round::{Rules, Sink};
use crate::deployment::Deployment;
use crate::refusal::{Refusal, shown as quoted};
use crate::verb::{Chained, Request, Verb};
use candidates::{Candidate, Match, OpenTodo};

/// At most this many candidates per todo, in [`Match`] rank order then newest first.
pub const PER_TODO: usize = 6;

/// At most this many todos per round, freshest evidence first; the rest wait a day.
pub const PER_ROUND: usize = 15;

/// The most characters of one message the round pre-loads; the model can still call `show`.
pub const CLIP_CHARS: usize = 4000;

/// The verbs this round may call: every `Reads` verb plus `propose_changes`, derived from
/// each verb's declared [`Effect`] so a new write cannot join.
pub const SURFACE: Subset = Subset {
    allows: |verb| verb.class().effect == Effect::Reads || verb == SecretaryVerb::ProposeChanges,
    refuses: |called, listed| {
        Refusal::not_found(format!(
            "the sweep may not call {} — its verbs are {listed}",
            quoted(called)
        ))
    },
};

/// The instruction, as a person turn like the review's.
const ASK_HEADER: &str = "Below is every open todo with later mail that might bear on it, and \
                          that mail's own content as `show` would answer it, written out as text. \
                          For each todo: if the mail plainly says it is done or moot, call \
                          `propose_changes` with ONE act — `complete` or `cancel` — naming that \
                          todo, a `why` in a sentence, and `sources` naming at least the \
                          `mail:<hash>` you rested it on. If it does not plainly say so, or \
                          there is nothing here that bears on it, propose nothing for that todo \
                          — the quiet majority stays quiet (CLAUDE.md, \"do not match on age\"). \
                          You may only propose `complete` or `cancel`, never `add`, `update` or \
                          `reprice`, and only for a todo listed below.";

/// The gathered open list after the bounds, so an unbounded turn cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bounded {
    /// The todos laid before the model, freshest evidence first.
    pub todos: Vec<(OpenTodo, Vec<Candidate>)>,
    /// How many open todos had any candidate before [`PER_ROUND`] applied.
    pub considered: usize,
}

impl Bounded {
    /// How many candidates are laid before the model, summed over every todo.
    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.todos.iter().map(|(_, found)| found.len()).sum()
    }
}

/// Applies [`PER_TODO`] and [`PER_ROUND`].
fn bound(mut gathered: Vec<(OpenTodo, Vec<Candidate>)>) -> Bounded {
    let considered = gathered.len();
    for (_, candidates) in &mut gathered {
        candidates.sort_by(|a, b| a.matched.cmp(&b.matched).then(b.at.cmp(&a.at)));
        candidates.truncate(PER_TODO);
    }
    // After the sort each todo's first candidate is its best; `gather` drops todos with none.
    gathered.sort_by(|(_, a), (_, b)| {
        let newest_a = a.first().map(|candidate| candidate.at);
        let newest_b = b.first().map(|candidate| candidate.at);
        newest_b.cmp(&newest_a)
    });
    gathered.truncate(PER_ROUND);
    Bounded {
        todos: gathered,
        considered,
    }
}

/// Clips one loaded message to [`CLIP_CHARS`] with a marker naming what was left off.
fn clip(text: &str) -> String {
    let total = text.chars().count();
    if total <= CLIP_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(CLIP_CHARS).collect();
    let more = total - CLIP_CHARS;
    format!("{head}\n… [clipped, {more} more characters; call show for the rest]")
}

/// What one sweep did.
#[derive(Debug, Clone, PartialEq)]
pub struct Swept {
    /// The conversation as it now stands.
    pub stored: Stored,
    /// How many times the model was asked; zero when nothing open had later mail.
    pub rounds: usize,
    /// The changesets this sweep proposed, each with its index.
    pub proposed: Vec<(usize, super::store::Standing)>,
}

/// Sweeps the first chain's open list in one round; a todo with no later mail proposes nothing.
///
/// # Errors
/// The stores' failures and the model's.
pub fn sweep(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &crate::settings::Settables,
    chains: &[Bound<'_>],
    system: &str,
) -> Result<Swept, Refusal> {
    let bound = chains
        .first()
        .ok_or_else(|| Refusal::internal("a deployment names at least one chain"))?;
    let round = daily_round(caps, deployment.zone, Daily::Sweep)?;
    let id = round.id.clone();
    let gathered = gather(bound, deployment.zone)?;
    if gathered.todos.is_empty() {
        // A day with nothing to sweep opens no conversation and runs no model.
        return Ok(Swept {
            stored: round.idle(caps),
            rounds: 0,
            proposed: Vec::new(),
        });
    }
    let already = round.open(caps)?.proposals.len();

    let text = user_turn(deployment, settables, chains, &id, &gathered);
    super::append(
        caps.conversations,
        &id,
        &Entry::Said {
            at: caps.clock.now(),
            turn: Turn::Person {
                text,
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )?;

    // A `complete`/`cancel` may name only the todos this round showed, not any open one.
    let swept: std::collections::BTreeSet<String> = gathered
        .todos
        .iter()
        .map(|(todo, _)| todo.id.as_str().to_owned())
        .collect();

    let run = |request: Request<SecretaryCall>| {
        if let SecretaryCall::ProposeChanges(Chained(args)) = &request.call {
            refuse_bad_act(&swept, args.acts())?;
        }
        super::run(deployment, settables, chains, &id, request)
    };
    let halt = Halt::new();
    let answered = super::rounded(
        caps,
        &id,
        &Rules {
            ask: Ask::Transcript { model: caps.model },
            system,
            verbs: &SURFACE,
            run: &run,
            bound: MAX_ROUNDS,
            sink: &Sink::quiet(&halt),
            load: crate::conversation::round::LOAD_NOTHING,
        },
    )?;
    if let Some(answered) = &answered {
        answered.ending.ran()?;
    }

    let stored = super::read(caps.conversations, &id)?;
    let proposed = stored
        .lines(&super::inbox::fold(caps.inbox)?)
        // The sweep files into this inbox before it reads, so every line it just filed is placed.
        .into_standings()
        .into_iter()
        .filter(|standing| {
            standing.asking.index >= already
                && matches!(standing.object.asked.proposal, Proposal::Changes(_))
        })
        .map(|standing| (standing.asking.index, standing))
        .collect();
    Ok(Swept {
        stored,
        rounds: answered.map_or(0, |answered| answered.rounds),
        proposed,
    })
}

/// What a sweep would lay before the model, as counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRun {
    /// Each todo in the round, in order, with its candidate count.
    pub todos: Vec<(prodrome_core::event::TodoId, usize)>,
    /// How many open todos had any candidate before [`PER_ROUND`] applied.
    pub considered: usize,
    /// How many todos are in the round.
    pub in_round: usize,
    /// How many candidates the round carries in total.
    pub candidates: usize,
    /// The length in characters of the user turn a real sweep would send, built by `user_turn`.
    pub turn_chars: usize,
}

/// Gathers and bounds exactly as [`sweep`] does, opening no conversation and calling no model.
///
/// # Errors
/// The chain's read failures.
pub fn dry_run(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &crate::settings::Settables,
    chains: &[Bound<'_>],
) -> Result<DryRun, Refusal> {
    let bound_chain = chains
        .first()
        .ok_or_else(|| Refusal::internal("a deployment names at least one chain"))?;
    let gathered = gather(bound_chain, deployment.zone)?;
    let today = deployment
        .zone
        .day(prodrome_core::fpl::instant_of(caps.clock.now()));
    let id = ConversationId::daily(Daily::Sweep, today);
    let turn = user_turn(deployment, settables, chains, &id, &gathered);
    Ok(DryRun {
        todos: gathered
            .todos
            .iter()
            .map(|(todo, found)| (todo.id.clone(), found.len()))
            .collect(),
        considered: gathered.considered,
        in_round: gathered.todos.len(),
        candidates: gathered.candidate_count(),
        turn_chars: turn.chars().count(),
    })
}

/// Every open todo with later mail, paired with that mail and bounded.
fn gather(chain: &Bound<'_>, zone: crate::zone::Zone) -> Result<Bounded, Refusal> {
    let now = chain.caps.clock.now();
    let nodes = chain.caps.chain.nodes()?;
    let records = crate::decoration::records_of(&nodes);
    let events = crate::view::events_of(&nodes);
    let mut open = Vec::new();
    for entry in prodrome_core::view::entries(&nodes, now, &chain.policy.untrusted)
        .map_err(|failed| Refusal::internal(failed.to_string()))?
    {
        if one_outcome(entry.outcome()).is_some() {
            continue;
        }
        let Some(record) = entry.content().first().and_then(|name| records.get(name)) else {
            continue;
        };
        // `created` is a day; a message counts from when that day began where the owner is.
        let created = prodrome_core::fpl::datetime_of(
            zone.start(prodrome_core::fpl::instant_of(record.payload.created).date()),
        )
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
        open.push(OpenTodo {
            id: entry.key.clone(),
            since: since(&events, &entry.key, created, &chain.policy.untrusted),
            source: record.payload.source.clone(),
            body: record.payload.body.clone(),
        });
    }

    let found = candidates::candidates(&open, chain.caps.corpus, now)
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let mut grouped: std::collections::BTreeMap<String, Vec<Candidate>> =
        std::collections::BTreeMap::new();
    for candidate in found {
        grouped
            .entry(candidate.todo.as_str().to_owned())
            .or_default()
            .push(candidate);
    }
    let found: Vec<(OpenTodo, Vec<Candidate>)> = open
        .into_iter()
        .filter_map(|todo| {
            let found = grouped.remove(todo.id.as_str())?;
            (!found.is_empty()).then_some((todo, found))
        })
        .collect();
    Ok(bound(found))
}

/// The floor a todo's evidence must be after: the start of its `created` day, or its latest
/// reopen when that is later. Only a trusted actor's reopen counts: an untrusted one binds
/// nothing, so it cannot hide the mail that closes a todo.
fn since(
    events: &[crate::record::TodoEvent],
    todo: &prodrome_core::event::TodoId,
    created: crate::capability::Datetime,
    untrusted: &prodrome_core::policy::Untrusted,
) -> crate::capability::Datetime {
    events
        .iter()
        .filter(|event| {
            matches!(event, crate::record::TodoEvent::Reopened(_))
                && event.todo() == todo
                && !untrusted.actors().contains(event.actor())
        })
        .map(crate::record::TodoEvent::at)
        .fold(created, std::cmp::max)
}

/// The round's user turn, loading messages through the `show` verb so there is no second reader.
fn user_turn(
    deployment: &Deployment,
    settables: &crate::settings::Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    gathered: &Bounded,
) -> String {
    use std::fmt::Write as _;

    let mut out = String::from(ASK_HEADER);
    out.push_str("\n\n");
    for (todo, candidates) in &gathered.todos {
        let _ = write!(
            out,
            "### todo:{}\nopen since: {}\n\n{}\n\n",
            todo.id.as_str(),
            prodrome_core::fpl::iso(prodrome_core::fpl::instant_of(todo.since)),
            todo.body.as_str(),
        );
        for candidate in candidates {
            let mut args = Map::new();
            args.insert("id".to_owned(), Value::from(candidate.message.as_str()));
            let (loaded, refused) =
                match super::call(deployment, settables, chains, id, Show::NAME, &args) {
                    Ok(super::verb_answer::VerbAnswer { value, .. }) => {
                        (clip(&readable(&value)), false)
                    }
                    Err(refusal) => (refusal.message().to_owned(), true),
                };
            let _ = write!(
                out,
                "- candidate mail:{} at {} ({}{}):\n{}\n\n",
                candidate.message.as_str(),
                prodrome_core::fpl::iso(prodrome_core::fpl::instant_of(candidate.at)),
                match_word(&candidate.matched),
                if refused { ", show refused" } else { "" },
                loaded,
            );
        }
    }
    out
}

/// A verb's answer as text a person reads: each field on a line of its own as `name: value`, a
/// string as itself with its line breaks, each item of a list on a `- ` line, and anything nested
/// indented under its name.
fn readable(value: &Value) -> String {
    let mut out = String::new();
    nested(&mut out, value, 0);
    out.trim_end().to_owned()
}

/// `value` under a name or a list's dash, each of its lines indented by `depth` spaces.
fn nested(out: &mut String, value: &Value, depth: usize) {
    use std::fmt::Write as _;

    let pad = " ".repeat(depth);
    match value {
        Value::Object(fields) => {
            for (name, field) in fields {
                headed(out, &format!("{pad}{name}:"), field, depth);
            }
        }
        Value::Array(items) => {
            for item in items {
                headed(out, &format!("{pad}-"), item, depth);
            }
        }
        Value::String(text) => {
            for line in text.lines() {
                let _ = writeln!(out, "{pad}{line}");
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            let _ = writeln!(out, "{pad}{value}");
        }
    }
}

/// `value` after `head`: on the same line when it is one line or empty, else on the lines below it.
fn headed(out: &mut String, head: &str, value: &Value, depth: usize) {
    use std::fmt::Write as _;

    match value {
        Value::String(text) if !text.contains('\n') => {
            let _ = writeln!(out, "{head} {text}");
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            let _ = writeln!(out, "{head} {value}");
        }
        Value::Array(items) if items.is_empty() => {
            let _ = writeln!(out, "{head} none");
        }
        Value::Object(fields) if fields.is_empty() => {
            let _ = writeln!(out, "{head} none");
        }
        Value::String(_) | Value::Object(_) | Value::Array(_) => {
            let _ = writeln!(out, "{head}");
            nested(out, value, depth + 2);
        }
    }
}

fn match_word(matched: &Match) -> String {
    match matched {
        Match::SameThread => "the same thread".to_owned(),
        Match::SameSender => "the same sender".to_owned(),
        Match::NamesReferent(referent) => format!("names {referent:?}"),
    }
}

/// The sweep's extra rule: every act is `complete` or `cancel`, on a todo in `swept`.
///
/// # Errors
/// [`Refusal::bad_request`] for unreadable `acts`, another act, or a todo not in `swept`.
fn refuse_bad_act(
    swept: &std::collections::BTreeSet<String>,
    acts: Option<&Value>,
) -> Result<(), Refusal> {
    let acts = super::changes::acts_of(acts)?;
    for act in &acts {
        match act {
            Act::Complete { todo, .. } | Act::Cancel { todo, .. } => {
                if !swept.contains(todo.as_str()) {
                    return Err(Refusal::bad_request(format!(
                        "the sweep may only complete or cancel a todo it swept, and {} was not \
                         among the todos shown this round",
                        quoted(todo)
                    )));
                }
            }
            other => {
                return Err(Refusal::bad_request(format!(
                    "the sweep may only propose `complete` or `cancel`, never `{}` (act on {})",
                    other.word(),
                    quoted(other.todo())
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep never writes or settles and proposes only changes, over every verb.
    #[test]
    fn the_sweep_allows_reads_and_propose_changes_and_nothing_else() {
        for &verb in SecretaryVerb::ALL {
            let expected = match verb.class().effect {
                Effect::Reads => true,
                Effect::Proposes => verb.name() == "propose_changes",
                Effect::Writes
                | Effect::Settles
                | Effect::Remembers
                | Effect::Syncs
                | Effect::Pulls
                | Effect::Watches => false,
            };
            assert_eq!((SURFACE.allows)(verb), expected, "{verb}");
        }
        let offered: Vec<&str> = SURFACE.verbs().map(SecretaryVerb::name).collect();
        assert!(offered.contains(&"propose_changes"), "{offered:?}");
        assert!(!offered.contains(&"record"), "{offered:?}");
    }

    /// A trusted reopen moves the floor up to it; an untrusted one, one on another todo, and
    /// one before the created day never move it.
    #[test]
    fn only_a_trusted_reopen_moves_the_floor() {
        let created = dt(2026, 9, 5, 0, 0, 0);
        let untrusted =
            prodrome_core::policy::Untrusted::of([
                prodrome_core::event::Actor::new("triage").expect("a name")
            ]);
        let reopened = |todo: &str, day: u32, who: &str| {
            prodrome_core::event::mk_reopened(todo, dt(2026, 9, day, 9, 0, 0), who, "")
                .expect("a reopen")
        };
        let todo = todo_id("flightcheckin");
        for (events, floor) in [
            (vec![], created),
            (vec![reopened("flightcheckin", 10, "triage")], created),
            (vec![reopened("other", 10, "ada")], created),
            (vec![reopened("flightcheckin", 1, "ada")], created),
            (
                vec![reopened("flightcheckin", 8, "ada")],
                dt(2026, 9, 8, 9, 0, 0),
            ),
            (
                vec![
                    reopened("flightcheckin", 8, "ada"),
                    reopened("flightcheckin", 12, "triage"),
                ],
                dt(2026, 9, 8, 9, 0, 0),
            ),
        ] {
            assert_eq!(
                since(&events, &todo, created, &untrusted),
                floor,
                "{events:?}"
            );
        }
    }

    fn todo_id(id: &str) -> prodrome_core::event::TodoId {
        prodrome_core::event::TodoId::new(id).expect("a valid id")
    }

    fn dt(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> crate::capability::Datetime {
        crate::capability::Datetime::new(year, month, day, hour, minute, second, 0)
            .expect("a real instant")
    }
}
