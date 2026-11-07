//! A record as the agent reads it: `render_todo` and [`render_current`] give the block that
//! shows it, through the one block renderer, and [`open_todos`] is the dedup reference.
//!
//! Absence is openness: "open" is the todos with an open world, so there is no open record to
//! keep in sync. `untrusted` is required so a provisional triage event cannot close or reprice a
//! todo; content is folded from every actor, and `authored_by` says whose text it is.

use crate::believed::{Believed, one_outcome, records};
use crate::record::{Authored, Node};
use prodrome_core::event::TodoId;
use prodrome_core::fpl::Outcome;
use prodrome_core::fpl::{self};
use prodrome_core::literal::Datetime;
use prodrome_core::payload::Payload as _;
use prodrome_core::policy::Untrusted;
use prodrome_core::term::Term;

use crate::decoration::{DecorationError, block, kind_category, render_verified, shown_of};
use crate::escape::markup_text;
use crate::refusal::Refusal;

// One record as a block.

/// [`block::raise_block`]'s refusal is the caller's request being wrong.
fn refusal_of(DecorationError(message): &DecorationError) -> Refusal {
    Refusal::bad_request(message.clone())
}

/// One record as verified block text, through [`shown_of`] and [`block::raise_block`].
pub(crate) fn render_todo(
    a: &Authored,
    state: Option<Outcome>,
    price: Option<f64>,
) -> Result<String, Refusal> {
    let row = shown_of(a, price, state);
    let (call, leading) = block::raise_block(&row).map_err(|e| refusal_of(&e))?;
    render_verified(&call, &leading)
}

/// `a` as its block shows it now, priced against the chain's history with its
/// lifecycle; what write verbs hand back instead of an echo of their arguments.
pub fn render_current(
    nodes: &[Node],
    at: Datetime,
    untrusted: &Untrusted,
    a: &Authored,
) -> Result<String, Refusal> {
    let believed = Believed::of(nodes, at, untrusted)
        .map_err(|e| Refusal::internal(format!("the chain's specs could not be folded: {e}")))?;
    render_todo(
        a,
        one_outcome(&believed.outcome(&a.todo)),
        believed.value(&a.todo, at),
    )
}

/// The record one id shows, or `None`.
#[must_use]
pub fn current(nodes: &[Node], at: Datetime, todo: &str) -> Option<Authored> {
    let id = TodoId::new(todo).ok()?;
    records(nodes, at).remove(&id)?.into_iter().next()
}

// The open list.

/// One open todo as the agent sees it, the dedup reference row.
///
/// `body` and `waiting_on` are plain text, since this row is read, not rendered; `spec` is the
/// latest revision and `priority` its value at the query moment, never a compiled number.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenTodo {
    /// The handle every other verb takes.
    pub id: String,
    /// The block kind.
    pub kind: String,
    /// The section it lands in.
    pub category: String,
    /// The evaluated fulfillment as `"12.34%"`, or `None` where there is no
    /// number: a record with no spec (`∅`), or one that does not link.
    pub priority: Option<String>,
    /// The action line, as plain text.
    pub body: String,
    /// The effective spec.
    pub spec: Option<Term>,
    /// What it is blocked on, as plain text.
    pub waiting_on: Option<String>,
    /// Whose text this is.
    pub authored_by: String,
}

/// Every todo the chain holds content for and confirmed history has not closed, as of `at`,
/// ordered by id.
pub fn open_todos(
    nodes: &[Node],
    at: Datetime,
    untrusted: &Untrusted,
) -> Result<Vec<OpenTodo>, Refusal> {
    let believed = Believed::of(nodes, at, untrusted)
        .map_err(|e| Refusal::internal(format!("the chain's specs could not be folded: {e}")))?;
    // One link table for the whole list rather than one per todo.
    let linkable = believed.linkable();
    let now = fpl::instant_of(at);
    Ok(believed
        .content
        .keys()
        .filter(|todo| believed.is_open(todo))
        .filter_map(|todo| Some((todo, believed.record(todo)?)))
        .map(|(todo, a)| OpenTodo {
            id: todo.as_str().to_owned(),
            kind: a.payload.kind.as_str().to_owned(),
            category: a
                .payload
                .category
                .as_ref()
                .map(|c| c.as_str().to_owned())
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| kind_category(a.payload.kind.as_str()).to_owned()),
            priority: believed
                .functions
                .get(todo)
                .and_then(|term| fpl::link(term, &linkable).ok())
                .and_then(|closed| fpl::fulfillment(&closed, now, &believed.env))
                .map(|value| format!("{:.2}%", value * 100.0)),
            body: markup_text(a.payload.body.as_str()),
            spec: believed
                .specs
                .get(todo)
                .and_then(|specs| specs.first())
                .cloned()
                .or_else(|| a.payload.spec().cloned()),
            waiting_on: if a.payload.waiting_on.as_str().is_empty() {
                None
            } else {
                Some(crate::view::string_text(a.payload.waiting_on.as_str()))
            },
            authored_by: a.actor.as_str().to_owned(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use crate::record::{TodoEvent, mk_authored, mk_source};
    use prodrome_core::event::Actor;

    use super::*;

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 30, 0, 0).expect("a real instant")
    }

    fn midnight(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 0, 0, 0, 0).expect("a real instant")
    }

    fn a_record(actor: &str) -> Authored {
        match mk_authored(
            "filething",
            at(7),
            actor,
            "todo",
            midnight(7),
            crate::record::MarkupSource::from_chain(r"File the \*permit\* renewal"),
            fpl::mk_flat(0.35).expect("a term"),
            vec!["Missing it is not recoverable.".to_owned()],
            "work",
            "",
            crate::record::MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("a record")
        {
            TodoEvent::Authored(record) => *record,
            _ => unreachable!("mk_authored answers an Authored"),
        }
    }

    fn untrusted() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    #[test]
    fn a_record_raises_to_its_block() {
        let text = render_todo(&a_record("triage"), None, Some(0.35)).expect("renders");
        assert!(text.contains("id: \"filething\""), "{text}");
        assert!(
            text.contains("created: datetime(day: 7, month: 9, year: 2026)"),
            "{text}"
        );
        assert!(text.contains("category: category.work"), "{text}");
        assert!(text.contains("// Missing it is not recoverable."), "{text}");
        assert!(text.contains("priority: 35.00%"), "{text}");
        assert!(text.contains("waitingOn: none"), "{text}");
        assert!(text.contains("authoredBy: \"triage\""), "{text}");
        assert!(text.ends_with(r"[File the \*permit\* renewal]"), "{text}");
    }

    /// A source out of a hostile email is still one argument: the fields are escaped on the way
    /// in, so the block parses back to itself.
    #[test]
    fn a_source_field_cannot_alter_the_block() {
        let mut record = a_record("triage");
        record.payload.source = Some(mk_source(
            crate::escape::escape_string("evil\") name").as_str(),
            crate::escape::escape_string("re: \\ hello").as_str(),
            midnight(6),
            "abc123",
            "",
            "",
        ));
        let text = render_todo(&record, None, Some(0.35)).expect("renders");
        assert!(text.contains("source: ("), "{text}");
        assert!(
            text.contains("datetime(day: 6, month: 9, year: 2026)"),
            "{text}"
        );
    }

    /// A provisional close closes nothing: the list reads the trusted fold.
    #[test]
    fn a_triage_completion_leaves_the_todo_open() {
        let events = vec![
            TodoEvent::Authored(Box::new(a_record("ada"))),
            prodrome_core::event::mk_completed("filething", at(8), "triage", "").expect("an event"),
        ];
        let open = open_todos(
            crate::believed::Preview::chain(&events).nodes(),
            at(9),
            &untrusted(),
        )
        .expect("folds");
        assert_eq!(open.len(), 1, "a triage completion is provisional");
    }
}
