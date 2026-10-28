//! The verbs that only look; the ones that append are [`super::writes`].

use crate::believed::Believed;
use crate::record::TodoEvent;
use prodrome_core::fpl;
use prodrome_core::payload::Payload as _;
use serde_json::{Value, json};

use super::events_of;
use crate::capability::{Agent, Manual, Manuals};
use crate::chains::ChainPolicy;
use crate::deployment::Deployment;
use crate::refusal::{Refusal, shown};
use crate::secretary::memory::Memory;
use crate::secretary::shown_record::OwnersRecord;
use crate::store::Reads;
use crate::todos;

pub(crate) fn list_open_todos(caps: &Agent<'_>, chain: &ChainPolicy) -> Result<Value, Refusal> {
    let open = todos::open_todos(&caps.chain.nodes()?, caps.clock.now(), &chain.untrusted)?;
    Ok(Value::Array(
        open.iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "kind": t.kind,
                    "category": t.category,
                    "priority": t.priority,
                    "body": t.body,
                    // In `evaluate`'s input grammar, so a row can be handed straight back.
                    "spec": t.spec.as_ref().map_or(Value::Null, crate::term_json::to_json),
                    "waiting_on": t.waiting_on,
                    "authored_by": t.authored_by,
                })
            })
            .collect(),
    ))
}

/// The most `context` spends on the manual and the vocabularies, in serialized bytes. The whole
/// answer is one budget, this and `[agent] memory_bytes`, filled in order: the manual and the
/// vocabularies, the facts, then the record. An answer the harness will not show inline is read
/// by nobody. With the default `memory_bytes` of 16 KiB the answer is at most 40 KiB: at most
/// 40,960 tokens, since no token is shorter than one byte, and about 10,000 tokens of English.
pub const CONTEXT_CAP: usize = 24 * 1024;

/// What the answer leaves unspent so `OPERATOR.md` can grow without a cut: a fifth of the cap,
/// weighed against the real manual by `app/cli/tests/context_headroom.rs`.
pub const CONTEXT_HEADROOM: usize = CONTEXT_CAP / 5;

/// What the category vocabulary may spend, so the manual's room is a constant.
const CATEGORIES_BUDGET: usize = 1024;

/// The bytes a value costs as the agent is sent it, pretty-printed (`commands::mcp`), measured by
/// the encoder itself. A string costs the same printed either way; an object's layout does not.
pub(crate) fn wire_len<T: serde::Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_string_pretty(value).map_or(usize::MAX, |wire| wire.len())
}

/// `text` cut at a line boundary to cost at most `budget` bytes, with a last line saying
/// so; empty when even that notice does not fit.
fn within(text: &str, budget: usize, file: &str) -> String {
    if wire_len(text) <= budget {
        return text.to_owned();
    }
    let notice = format!(
        "\n\n(… cut here: {file} no longer fits this verb's {CONTEXT_CAP}-byte answer. \
         The whole file is in the repository, and the agent reads what is above.)"
    );
    if wire_len(&notice) > budget {
        return String::new();
    }
    // The quotes are paid once, by the notice; escaping is per character, so lines add.
    let mut kept = String::new();
    let mut spent = wire_len(&notice);
    for line in text.split_inclusive('\n') {
        let cost = wire_len(line) - 2;
        if spent + cost > budget {
            break;
        }
        spent += cost;
        kept.push_str(line);
    }
    kept + &notice
}

/// The entries that fit `budget` bytes, with a last entry counting the rest.
fn listed(items: &[String], budget: usize) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    let mut spent = 0usize;
    for (at, item) in items.iter().enumerate() {
        // `+ 1` for the comma this entry costs inside the array.
        let cost = wire_len(item) + 1;
        if spent + cost > budget {
            kept.push(format!(
                "(… and {} more, past this verb's budget)",
                items.len() - at
            ));
            return kept;
        }
        spent += cost;
        kept.push(item.clone());
    }
    kept
}

/// `OPERATOR.md` and the authoring vocabularies, under [`CONTEXT_CAP`]: the secretary's
/// `context`. `CLAUDE.md` stays out: it is already the session's memory.
pub fn manual(manuals: &dyn Manuals, deployment: &Deployment) -> Value {
    manual_within(manuals, deployment, CONTEXT_CAP)
}

/// The agent's `context`: [`manual`] within what leaves room for the memory's key, then the
/// secretary's memory (the facts in `memory`, then the record read in `record`) in what is left
/// of `CONTEXT_CAP + memory_bytes`.
pub fn context(
    manuals: &dyn Manuals,
    deployment: &Deployment,
    memory: Result<&Memory, &Refusal>,
    record: Result<&OwnersRecord, &Refusal>,
    memory_bytes: usize,
) -> Value {
    // What the `memory` key costs beside its string, which pays for its own quotes and escapes,
    // measured on an object laid out as the answer is; the manual leaves the string room to say
    // at least that nothing fits.
    let key = wire_len(&json!({"a": 0, "memory": ""})) - wire_len(&json!({"a": 0})) - 2;
    let least = wire_len(super::remembered::TOO_BIG);
    let mut answer = manual_within(manuals, deployment, CONTEXT_CAP - key - least);
    let room = (CONTEXT_CAP + memory_bytes).saturating_sub(wire_len(&answer) + key);
    answer["memory"] = Value::String(super::remembered::remembered(memory, record, room));
    answer
}

/// [`manual`] within `cap` bytes.
fn manual_within(manuals: &dyn Manuals, deployment: &Deployment, cap: usize) -> Value {
    let read = |which: Manual| {
        manuals
            .document(which)
            .unwrap_or_else(|| format!("({} is not present)", which.file_name()))
    };
    let vocabulary: Vec<String> = deployment.categories.keys().cloned().collect();
    let categories = listed(&vocabulary, CATEGORIES_BUDGET);
    let mut kinds = serde_json::Map::new();
    kinds.insert("todo".into(), json!("general; REQUIRES category"));
    for kind in crate::decoration::CATEGORY_KINDS {
        kinds.insert(
            (*kind).into(),
            json!(
                "work for the category its name begins with; sets its own category, do not pass one"
            ),
        );
    }
    kinds.insert(
        "suzateryTodo".into(),
        json!("this system's own upkeep; sets its own category"),
    );
    let shape = |operator: String| {
        json!({
            "constitution": "Not carried here: CLAUDE.md is already your project memory.",
            "operator_manual": operator,
            "authoring": {
                "kinds": kinds,
                "categories": categories,
                "priority": "a whole percentage like '65%'. It is a FULFILLMENT: LOW \
                             is urgent, HIGH is satisfied or someday. Sort ascending.",
                "rationale": "required on every priority; it becomes the comment \
                              above the number and is the only record of the reason.",
                "note": "You do not edit files. todo_add and todo_update take \
                         fields. `body`, `detail` and `detail_append` are Typst \
                         MARKUP, checked before anything is stored: write `*bold*`, \
                         `_emph_`, lists, `#link(\"https://...\")[text]`, and \
                         backslash a character you mean literally (`\\$`, `\\@`, \
                         `\\#`, `\\*`). A refusal names the line and column. Every \
                         other field is plain text, escaped for you.",
            },
        })
    };
    let fixed = wire_len(&shape(String::new()));
    let room = cap.saturating_sub(fixed);
    shape(within(&read(Manual::Operator), room, "OPERATOR.md"))
}

pub(crate) fn todo_show(
    caps: &Agent<'_>,
    chain: &ChainPolicy,
    todo_id: &str,
) -> Result<Value, Refusal> {
    let nodes = caps.chain.nodes()?;
    let now = caps.clock.now();
    let Some(found) = todos::current(&nodes, now, todo_id) else {
        return Ok(json!({
            "ok": false,
            "error": format!("no todo with id {} on the chain", shown(todo_id)),
        }));
    };
    let block = todos::render_current(&nodes, now, &chain.untrusted, &found)?;
    Ok(json!({
        "ok": true,
        "todo_id": todo_id,
        "block": block,
        "fields": {
            "kind": found.payload.kind.as_str(),
            "category": found
                .payload.category
                .as_ref()
                .map(prodrome_core::event::Name::as_str)
                .filter(|c| !c.is_empty()),
            "created": format!(
                "{:04}-{:02}-{:02}",
                found.payload.created.year(),
                found.payload.created.month(),
                found.payload.created.day()
            ),
            "body": found.payload.body.as_str(),
            "detail": Some(found.payload.detail.as_str()).filter(|d| !d.is_empty()),
            "waiting_on": Some(found.payload.waiting_on.as_str()).filter(|w| !w.is_empty()),
            "spec": found.payload.spec().map_or(Value::Null, crate::term_json::to_json),
            "rationale": found.payload.rationale,
            "source": found.payload.source.as_ref().map_or(Value::Null, |s| json!({
                "sender": s.sender.as_str(),
                "subject": s.subject.as_str(),
                "date": fpl::iso(fpl::instant_of(s.date)),
                "hash": s.hash.as_str(),
                "thread_id": s.thread_id.as_str(),
                "message_id": s.message_id.as_str(),
            })),
        },
        "authored": {
            "at": fpl::iso(fpl::instant_of(found.at)),
            "actor": found.actor.as_str(),
        },
    }))
}

pub(crate) fn history(chain: &dyn Reads<TodoEvent>, todo: &str) -> Result<Value, Refusal> {
    let events = events_of(chain)?;
    let lines: Vec<String> = events
        .iter()
        .filter(|e| e.todo().as_str() == todo)
        .map(|e| {
            let note = note_of(e);
            format!(
                "{}  {}  {}{}",
                e.kind_name().to_lowercase(),
                fpl::iso(fpl::instant_of(e.at())),
                e.actor().as_str(),
                if note.is_empty() {
                    String::new()
                } else {
                    format!("  — {note}")
                }
            )
        })
        .collect();
    Ok(Value::String(if lines.is_empty() {
        format!("(no events for todo {})", shown(todo))
    } else {
        lines.join("\n")
    }))
}

fn note_of(event: &TodoEvent) -> &str {
    match event {
        TodoEvent::Created(e) => &e.note,
        TodoEvent::Completed(e)
        | TodoEvent::Cancelled(e)
        | TodoEvent::Reopened(e)
        | TodoEvent::Tended(e) => &e.note,
        TodoEvent::SpecRevised(e) => &e.note,
        TodoEvent::Authored(e) => &e.payload.note,
    }
}

pub(crate) fn evaluate(
    caps: &Agent<'_>,
    chain: &ChainPolicy,
    spec: Option<&Value>,
    at_iso: Option<&str>,
) -> Result<Value, Refusal> {
    let Some(spec) = spec else {
        return Ok(json!({"error": "could not parse spec or timestamp: spec_json is required"}));
    };
    let term = match crate::term_json::from_json(spec) {
        Ok(term) => term,
        Err(failed) => {
            return Ok(json!({
                "error": format!("could not parse spec or timestamp: {failed}"),
            }));
        }
    };
    let at = match at_iso {
        None => caps.clock.now(),
        Some(text) => match fpl::parse_iso(text).and_then(|t| {
            fpl::datetime_of(t).map_err(|e| prodrome_core::fpl::FplError(e.to_string()))
        }) {
            Ok(at) => at,
            Err(failed) => {
                return Ok(json!({
                    "error": format!("could not parse spec or timestamp: {failed}"),
                }));
            }
        },
    };
    let believed = Believed::of(&caps.chain.nodes()?, at, &chain.untrusted)
        .map_err(|e| Refusal::internal(format!("the chain's specs could not be folded: {e}")))?;
    let env = &believed.env;
    let now = fpl::instant_of(at);
    let term = match fpl::link(&term, &believed.linkable()) {
        Ok(closed) => closed,
        Err(failed) => return Ok(json!({"error": format!("the spec does not link: {failed}")})),
    };
    Ok(json!({
        "at": fpl::iso(now),
        "value": crate::wire::Fulfillment::of_value(fpl::fulfillment(&term, now, env)),
        "explain": crate::term_json::explain(&term, now, env),
    }))
}

pub(crate) fn verify_chain(chain: &dyn Reads<TodoEvent>) -> String {
    let problems = chain.verify();
    if problems.is_empty() {
        "healthy".to_owned()
    } else {
        problems.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use prodrome_core::policy::Untrusted;

    use super::*;

    struct Library {
        operator: String,
    }

    impl Manuals for Library {
        fn document(&self, which: Manual) -> Option<String> {
            Some(match which {
                Manual::Constitution => "the constitution".to_owned(),
                Manual::Operator => self.operator.clone(),
                Manual::Secretary => "the secretary's brief".to_owned(),
            })
        }
    }

    fn deployment(categories: usize) -> Deployment {
        Deployment {
            viewers: BTreeSet::new(),
            actors: BTreeMap::new(),
            chains: crate::chains::Chains::of(vec![crate::chains::ChainPolicy {
                name: crate::chains::ChainName::new("life").expect("a slug"),
                untrusted: Untrusted::of([]),
                roster: BTreeSet::new(),
                root: crate::cast::keys::root(),
            }])
            .expect("one chain"),
            categories: (0..categories)
                .map(|n| (format!("category{n:04}"), u32::try_from(n).expect("small")))
                .collect(),
            app_url: "https://example.invalid/".to_owned(),
            coding: None,
            zone: crate::zone::Zone::UTC,
        }
    }

    /// `context` over an empty memory and a record that did not read, spending nothing beyond
    /// the manual's cap.
    fn bare(manuals: &dyn Manuals, deployment: &Deployment) -> Value {
        let memory = Memory {
            facts: Vec::new(),
            forgotten: Vec::new(),
            version: 0,
        };
        let unread = Refusal::not_found("no record here".to_owned());
        context(manuals, deployment, Ok(&memory), Err(&unread), 0)
    }

    fn paragraphs(lines: usize) -> String {
        use std::fmt::Write as _;

        (0..lines).fold(String::new(), |mut out, n| {
            let _ = writeln!(out, "Line {n} of a document that keeps going, and going.");
            out
        })
    }

    #[test]
    fn the_answer_fits_the_cap_however_large_the_repository_grows() {
        let library = Library {
            operator: paragraphs(20_000),
        };
        let answer = bare(&library, &deployment(1_000));
        let size = wire_len(&answer);
        assert!(size <= CONTEXT_CAP, "{size} bytes is over the cap");

        let text = serde_json::to_string(&answer).expect("the answer serialises");
        assert!(text.contains("cut here: OPERATOR.md"), "{size}");
        assert!(text.contains("more, past this verb's budget"), "{size}");

        let operator = answer["operator_manual"].as_str().expect("a document");
        assert!(
            operator.starts_with("Line 0 of a document"),
            "{operator:.120}"
        );
        let head = operator.split("\n\n(… cut here").next().expect("a head");
        assert!(head.ends_with("going, and going.\n"), "{head:.120}");
    }

    #[test]
    fn an_ordinary_repository_is_carried_whole_and_names_the_constitution() {
        let library = Library {
            operator: paragraphs(120),
        };
        let answer = bare(&library, &deployment(8));
        assert!(wire_len(&answer) <= CONTEXT_CAP);
        assert_eq!(answer["operator_manual"], json!(library.operator));
        assert_eq!(answer["authoring"]["categories"][0], json!("category0000"));
        let constitution = answer["constitution"].as_str().expect("a sentence");
        assert!(constitution.contains("CLAUDE.md"), "{constitution}");
        assert!(constitution.contains("project memory"), "{constitution}");
    }
}
