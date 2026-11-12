//! Building `Authored` records from typed fields, so no model ever operates an editor inside a
//! file it is trusted not to wreck. Pure: it builds a record, and the caller appends it.
//!
//! A todo's body and detail are markup, admitted through [`MarkupSource::checked`] whoever writes
//! them: the closed world it compiles in, not the writer, is what keeps a text to typst's own
//! elements. A blocker stays a string, escaped as data.
//!
//! `build_todo`, `update_todo` and `confirm_todo` are one function each for every caller: what
//! differs between callers is the actor only, and a second spelling of a refusal is a second
//! policy waiting to disagree.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use chrono::{Duration, NaiveDate};
use prodrome_core::event::{Hash, mk_cancelled, mk_completed, mk_reopened, mk_spec_revised};
use prodrome_core::fpl::{self};
use prodrome_core::payload::Payload as _;
use prodrome_core::policy::{Policy, Untrusted};
use prodrome_core::term::{Term, TermF};
use regex::Regex;
use suzatary_record::record::{
    Authored, MarkupSource, Record, Source, SubTodo, TodoEvent, is_absent, mk_authored, mk_source,
};

use suzatary_base::capability::{Datetime, Typeset};
use suzatary_base::escape::escape_string;
use suzatary_base::reference::Reference;
use suzatary_base::refusal::{Refusal, shown};
use suzatary_base::zone::Moment;
use suzatary_record::todo::price::ratio;
use suzatary_record::todo::{Patch, TodoFields, TodoPatch};

/// A todo id, as authoring spells one: narrower than the chain's `[a-z0-9_-]+`, because ids
/// are referenced by hand forever.
static ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A[a-z][a-z0-9]{2,63}\z").expect("a literal pattern"));

/// How long a derived id may be.
pub(crate) const ID_CHARS: usize = 24;

/// A todo id from its action line: its words, lowercased, run together and cut, since the body is
/// the only handle a person can reproduce from what they see.
pub fn slug(body: &str) -> Result<String, Refusal> {
    let joined: String = body
        .to_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let trimmed = joined.trim_start_matches(|c: char| c.is_ascii_digit());
    if trimmed.chars().count() < 3 {
        return Err(Refusal::bad_request(format!(
            "cannot make an id from {} — give one explicitly as `id`",
            shown(body)
        )));
    }
    Ok(trimmed.chars().take(ID_CHARS).collect())
}

/// `base`, or the first `base<n>` the chain has never mentioned. Suffixing beats refusing: two
/// todos about the same thing is a normal week, and no id is used twice, a cancelled todo's
/// included.
pub fn fresh_id(base: &str, known: &std::collections::BTreeSet<String>) -> Result<String, Refusal> {
    if !known.contains(base) {
        return Ok(base.to_owned());
    }
    for n in 2..100u32 {
        let suffix = n.to_string();
        let head: String = base
            .chars()
            .take(ID_CHARS.saturating_sub(suffix.len()))
            .collect();
        let candidate = format!("{head}{suffix}");
        if !known.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(Refusal::bad_request(format!(
        "every id from {} to {base}99 is taken — give one explicitly as `id`",
        shown(base)
    )))
}

use suzatary_record::decoration::KINDS;
use suzatary_record::decoration::takes_category;

/// The recurrence in force in `term` at `now`, in words ("every 60 days after a pass, from
/// 100% to 30%"). The inverse of [`Price::term`](suzatary_record::todo::Price::term)'s cyclic arms; `None` where nothing recurs or
/// the body is not a ramp `Cycle::ramp` wrote.
#[must_use]
pub fn recurrence_words(term: &Term, now: fpl::Instant) -> Option<String> {
    match term.out() {
        TermF::Recur { term: body, .. } => {
            ramp_words(body).map(|(days, ends)| format!("every {days} days after a pass, {ends}"))
        }
        TermF::Periodic {
            period, term: body, ..
        } => ramp_words(body)
            .filter(|(days, _)| Duration::days(*days) == *period)
            .map(|(days, ends)| format!("every {days} days on the calendar, {ends}")),
        TermF::Piecewise(schedule) => recurrence_words(schedule.at(now), now),
        other => other
            .children()
            .into_iter()
            .find_map(|child| recurrence_words(child, now)),
    }
}

/// A two-point ramp's length in days and its two ends in words.
fn ramp_words(body: &Term) -> Option<(i64, String)> {
    let TermF::Curve { points } = body.out() else {
        return None;
    };
    let [start, end] = points.as_slice() else {
        return None;
    };
    Some((
        (end.at - start.at).num_days(),
        format!(
            "from {:.0}% to {:.0}%",
            start.value * 100.0,
            end.value * 100.0
        ),
    ))
}

/// A new todo: its id, its fields, and where it came from.
#[derive(Debug, Clone)]
pub struct NewTodo {
    /// The id, checked against `ID`.
    pub todo: String,
    /// What it says and what it is worth.
    pub fields: TodoFields,
    /// The message it came from.
    pub source: Option<Source>,
    /// The checklist.
    pub subtodos: Vec<SubTodo>,
}

/// The rationale as comment lines of about 72 columns. Required: it is the only record of why
/// a number is what it is.
pub(crate) fn wrap(rationale: &str) -> Result<Vec<String>, Refusal> {
    if rationale.trim().is_empty() {
        return Err(Refusal::bad_request(
            "rationale is required: say WHY this priority, in one sentence. It is \
             recorded as the comment above it and is the only record of the reasoning.",
        ));
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in rationale.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > 72 {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else if current.is_empty() {
            current.push_str(word);
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    Ok(lines)
}

/// A field's markup through the one door, or the refusal naming the field and where typst
/// stopped. Every writer's markup comes here: a todo's, and a recorded decision's body.
///
/// # Errors
/// [`Refusal::bad_request`] for markup the door does not admit.
pub fn admitted(field: &str, text: &str, typeset: &dyn Typeset) -> Result<MarkupSource, Refusal> {
    MarkupSource::checked(text, typeset).map_err(|refused| {
        Refusal::bad_request(format!(
            "{field} is typst markup, and this is not markup this box renders: {}",
            refused.said(text)
        ))
    })
}

/// The checks that make a todo referenceable and well-kinded.
fn category(kind: &str, todo: &str, named: &str, known: &[&str]) -> Result<String, Refusal> {
    if !KINDS.contains(&kind) {
        return Err(Refusal::bad_request(format!(
            "kind must be one of {:?}, got {}",
            sorted(KINDS),
            shown(kind)
        )));
    }
    if !ID.is_match(todo) {
        return Err(Refusal::bad_request(format!(
            "todo_id must be 3-64 lowercase letters/digits starting with a letter, \
             got {} — ids are referenced by mark and record, so no punctuation, \
             spaces or capitals",
            shown(todo)
        )));
    }
    category_named(kind, named, known)
}

/// Just the category half of [`category`]: an existing todo's kind and id are not rechecked,
/// or edits to todos created before [`ID`] tightened would be refused.
fn category_named(kind: &str, named: &str, known: &[&str]) -> Result<String, Refusal> {
    if named.is_empty() {
        return Ok(String::new());
    }
    if !takes_category(kind) {
        return Err(Refusal::bad_request(format!(
            "{kind} sets its own category; omit category"
        )));
    }
    if !known.contains(&named) {
        return Err(Refusal::bad_request(format!(
            "category must be one of {:?}, got {}",
            sorted(known),
            shown(named)
        )));
    }
    Ok(named.to_owned())
}

fn sorted(names: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
    out.sort();
    out
}

/// A new todo's `Authored` record, created on the day of `at`, the file it renders into.
pub fn build_todo(
    new: &NewTodo,
    now: Moment,
    actor: &str,
    categories: &[&str],
    typeset: &dyn Typeset,
) -> Result<TodoEvent, Refusal> {
    let fields = &new.fields;
    if fields.body.trim().is_empty() {
        return Err(Refusal::bad_request(
            "body is the action line and cannot be empty",
        ));
    }
    let category = category(&fields.kind, &new.todo, &fields.category, categories)?;
    let created = created_on(now)?;
    let body = admitted("body", &fields.body, typeset)?;
    let waiting_on = if fields.waiting_on.is_empty() {
        String::new()
    } else {
        escape_string(&fields.waiting_on).as_str().to_owned()
    };
    let detail = if fields.detail.trim().is_empty() {
        MarkupSource::empty()
    } else {
        admitted("detail", &fields.detail, typeset)?
    };
    mk_authored(
        &new.todo,
        now.at,
        actor,
        &fields.kind,
        created,
        body,
        fields.price.term(&new.todo, now, None)?,
        wrap(&fields.rationale)?,
        &category,
        &waiting_on,
        detail,
        new.source.clone(),
        new.subtodos.clone(),
        Vec::new(),
        "",
    )
    .map_err(|e| Refusal::bad_request(e.to_string()))
}

/// What a held batch supplies to author the decision it asks for.
#[derive(Debug, Clone)]
pub struct NewDecision {
    /// The one sentence a person decides on.
    pub body: String,
    /// Why it matters, in one sentence; becomes `rationale`.
    pub why: String,
    /// What performs it, printed on `detail`'s first line so [`decision_reference`] can read it
    /// back: a `batch:` (or, on older records, a `proposal:`), never a todo.
    pub performs: Reference,
    /// The full text a person reads before deciding, as prose after the reference line.
    pub detail: String,
    /// A fulfillment, 0..100, priced against the same table as every writer; `None` is 55.
    pub priority: Option<u32>,
    /// One of this deployment's categories, or "" for none.
    pub category: String,
    /// The message it came from, when it came from one. "" is absent.
    pub source: String,
}

/// Fields → a decision record: the dullest cousin of [`build_todo`], with the same escaping,
/// the same midnight `created`, flat pricing, and an id derived by [`slug`] then [`fresh_id`]
/// since a held batch supplies none. Nothing but the tap unblocks one, so `waiting_on` is
/// always absent, and it has no checklist.
///
/// # Errors
/// [`Refusal::bad_request`] for an empty `body` or `detail`, an empty `why`
/// (wrapped through `wrap`, which is `rationale`'s own refusal), a category
/// this deployment does not know, an out-of-range `priority`, or an id this
/// chain has already mentioned ninety-nine times over.
pub fn build_decision(
    new: &NewDecision,
    now: Moment,
    actor: &str,
    categories: &[&str],
    known: &BTreeSet<String>,
) -> Result<TodoEvent, Refusal> {
    if new.body.trim().is_empty() {
        return Err(Refusal::bad_request(
            "body is the one sentence a person decides on and cannot be empty",
        ));
    }
    if new.detail.trim().is_empty() {
        return Err(Refusal::bad_request(
            "detail is what a person reads before deciding and cannot be empty",
        ));
    }
    let category = category_named("decision", &new.category, categories)?;
    let created = created_on(now)?;
    let id = fresh_id(&slug(&new.body)?, known)?;
    let body = MarkupSource::escaped(&new.body);
    // The reference is the first line: its text form holds no special character, so escaping
    // the block leaves it byte-identical and `decision_reference` reads it off the markup.
    let whole = format!("{}\n\n{}", new.performs, new.detail);
    let detail = MarkupSource::escaped(&whole);
    let spec = fpl::mk_flat(ratio(new.priority.unwrap_or(55), "priority")?)
        .map_err(|e| Refusal::bad_request(e.to_string()))?;
    let source = if new.source.is_empty() {
        None
    } else {
        // Only the hash is known, so the rest of `Source` is "" (absent), and `date` is the
        // decision's own `created` rather than an invented one.
        Some(mk_source("", "", created, &new.source, "", ""))
    };
    mk_authored(
        &id,
        now.at,
        actor,
        "decision",
        created,
        body,
        spec,
        wrap(&new.why)?,
        &category,
        "",
        detail,
        source,
        Vec::new(),
        Vec::new(),
        "",
    )
    .map_err(|e| Refusal::bad_request(e.to_string()))
}

/// What a decision performs, parsed off [`Record::detail`]'s first line and never trusted: an
/// unparseable line is `None`, drawn as text with no controls.
#[must_use]
pub fn decision_reference(record: &Record) -> Option<Reference> {
    record.detail.as_str().lines().next()?.parse().ok()
}

/// The day a todo is created on, the owner's, stored as that day's midnight: `created` is a day.
fn created_on(now: Moment) -> Result<Datetime, Refusal> {
    fpl::datetime_of(now.today().and_time(chrono::NaiveTime::MIN))
        .map_err(|e| Refusal::bad_request(e.to_string()))
}

/// A price and the reason for it, which move together or not at all.
type Priced = (Term, Vec<String>);

/// The new price and its reason, if the change moves either. A new price needs a fresh rationale:
/// the old sentence beside a new number reads as reviewed reasoning and is not.
fn repricing(
    before: &Authored,
    own: &Term,
    edit: &TodoPatch,
    now: Moment,
) -> Result<Option<Priced>, Refusal> {
    if let Patch::Set(to) = edit.defer_to {
        if !edit.price.is_keep() {
            return Err(Refusal::bad_request(
                "give at most one of priority or defer_to — a deferral carries no \
                 number of its own, so a fresh price and a deferral cannot both \
                 land on one edit",
            ));
        }
        if is_absent(own) {
            return Err(Refusal::bad_request(format!(
                "{} has no priority to defer",
                shown(before.todo.as_str())
            )));
        }
        let deferred = defer(own, to, now)?;
        return Ok(Some((deferred, wrap(&format!("deferred to {to}"))?)));
    }
    let Patch::Set(price) = &edit.price else {
        let Patch::Set(rationale) = &edit.rationale else {
            return Ok(None);
        };
        if is_absent(own) {
            return Err(Refusal::bad_request(format!(
                "{} has no priority to annotate",
                shown(before.todo.as_str())
            )));
        }
        return Ok(Some((own.clone(), wrap(rationale)?)));
    };
    let Some(rationale) = edit.rationale.set().filter(|r| !r.trim().is_empty()) else {
        return Err(Refusal::bad_request(
            "changing priority requires a rationale: the comment above the number \
             is the only record of why, and leaving the old one in place beside a \
             new number misrepresents it as reviewed",
        ));
    };
    Ok(Some((
        price.term(before.todo.as_str(), now, (!is_absent(own)).then_some(own))?,
        wrap(rationale)?,
    )))
}

/// A deferral, as a curve and never a flag: `Piecewise(Flat(1.0), [(to, term)])` with `term`
/// unshifted, because a deadline does not move when work on it is postponed (so never a
/// `Shift`, and never a fresh `Decay`). Deferring again replaces the outer deferral rather than
/// nesting one.
///
/// # Errors
/// `to` is not strictly after `today`, or `to` is past a `Decay` this term carries: postponing
/// past the deadline is a reprice, not a deferral.
pub fn defer(term: &Term, to: NaiveDate, now: Moment) -> Result<Term, Refusal> {
    let today = now.today();
    if to <= today {
        return Err(Refusal::bad_request(format!(
            "defer to a date after today ({today}), got {to}"
        )));
    }
    let to_instant = now.zone.start(to);
    if let Some(deadline) = decay_end_dates(term).into_iter().min()
        && to_instant > deadline
    {
        return Err(Refusal::bad_request(format!(
            "{to} is past this todo's deadline ({}) — postponing past the \
             deadline is a reprice, not a deferral",
            now.zone.day(deadline - Duration::nanoseconds(1))
        )));
    }
    let inner = already_deferred(term)
        .cloned()
        .unwrap_or_else(|| term.clone());
    let head = fpl::mk_flat(1.0).map_err(|e| Refusal::bad_request(e.to_string()))?;
    fpl::mk_piecewise(head, vec![(to_instant, inner)])
        .map_err(|e| Refusal::bad_request(e.to_string()))
}

/// Every `Decay`'s `end_date` inside `term`: the deadlines a deferral must not cross.
fn decay_end_dates(term: &Term) -> Vec<fpl::Instant> {
    let mut out = if let TermF::Decay { end_date, .. } = term.out() {
        vec![*end_date]
    } else {
        Vec::new()
    };
    for child in term.out().children() {
        out.extend(decay_end_dates(child));
    }
    out
}

/// The deferred term, if `term` is exactly one deferral's shape (`Flat(1.0)` head, one piece),
/// so [`defer`] replaces it instead of nesting.
fn already_deferred(term: &Term) -> Option<&Term> {
    let TermF::Piecewise(schedule) = term.out() else {
        return None;
    };
    let [(_, inner)] = schedule.knots() else {
        return None;
    };
    let TermF::Flat { value } = schedule.head().out() else {
        return None;
    };
    if (value - 1.0).abs() < 1e-9 {
        Some(inner)
    } else {
        None
    }
}

/// A whole new record for the same todo with the patch's fields changed, built through
/// `mk_authored` again so a change cannot make a record the constructor would refuse. Prose is
/// escaped as on the way in, and a category checked against `categories` as [`build_todo`] does.
///
/// `note` is the write's own why (`Record.note`); only `POST /api/author` passes a person's.
///
/// # Errors
/// Nothing in `edit` differs from `before`; a reprice with no fresh rationale;
/// a body blanked to nothing; a category this deployment does not know.
pub fn update_todo(
    before: &Authored,
    edit: &TodoPatch,
    now: Moment,
    actor: &str,
    categories: &[&str],
    note: &str,
    typeset: &dyn Typeset,
) -> Result<TodoEvent, Refusal> {
    if edit.is_empty() {
        return Err(Refusal::bad_request(
            "an edit must change something — nothing here differs from what is on the chain",
        ));
    }
    if !edit.detail.is_keep() && !edit.detail_append.is_keep() {
        return Err(Refusal::bad_request(
            "give at most one of detail or detail_append — a replace and an \
             append cannot both land on one edit",
        ));
    }
    let composed = Composed::of(&before.payload.spec);
    let (spec, rationale) = match repricing(before, &composed.own, edit, now)? {
        Some((own, rationale)) => (Composed { own, ..composed }.spec(), rationale),
        None => (
            before.payload.spec.clone(),
            before.payload.rationale.clone(),
        ),
    };
    let body = match &edit.body {
        Patch::Keep => before.payload.body.clone(),
        Patch::Set(text) if text.trim().is_empty() => {
            return Err(Refusal::bad_request(
                "body is the action line and cannot be empty",
            ));
        }
        Patch::Set(text) => admitted("body", text, typeset)?,
    };
    let detail = match (&edit.detail, &edit.detail_append) {
        (Patch::Set(text), _) if text.trim().is_empty() => MarkupSource::empty(),
        (Patch::Set(text), _) => admitted("detail", text, typeset)?,
        (Patch::Keep, Patch::Set(text)) if text.trim().is_empty() => {
            return Err(Refusal::bad_request(
                "an edit must change something — nothing here differs from what is on the chain",
            ));
        }
        (Patch::Keep, Patch::Set(text)) => {
            let existing = before.payload.detail.as_str();
            if existing.trim().is_empty() {
                admitted("detail_append", text, typeset)?
            } else {
                admitted("detail", &format!("{existing}\n\n{text}"), typeset)?
            }
        }
        (Patch::Keep, Patch::Keep) => before.payload.detail.clone(),
    };
    let waiting_on = if edit.clear_waiting_on {
        String::new()
    } else {
        match &edit.waiting_on {
            Patch::Keep => before.payload.waiting_on.as_str().to_owned(),
            Patch::Set(text) => escape_string(text).as_str().to_owned(),
        }
    };
    let category = match &edit.category {
        Patch::Keep => before
            .payload
            .category
            .as_ref()
            .map_or_else(String::new, |name| name.as_str().to_owned()),
        Patch::Set(named) => category_named(before.payload.kind.as_str(), named, categories)?,
    };
    let subtodos = edit
        .subtodos
        .set()
        .unwrap_or(&before.payload.subtodos)
        .clone();
    mk_authored(
        before.todo.as_str(),
        now.at,
        actor,
        before.payload.kind.as_str(),
        before.payload.created,
        body,
        spec,
        rationale,
        &category,
        &waiting_on,
        detail,
        before.payload.source.clone(),
        subtodos,
        before.payload.notes.clone(),
        note,
    )
    .map_err(|e| Refusal::bad_request(e.to_string()))
}

/// A spec as a parent reads it: `Conj[own, Ref(c1), …, Ref(cn)]` at p = −4, `own` FPL's
/// `Absent` for a parent that is only its subtodos and left out of the conjunction then; any
/// other shape is all `own`. An edit reprices `own`; the subtodo act adds to `children`.
#[derive(Debug, Clone, PartialEq)]
pub struct Composed {
    /// The todo's own term.
    pub own: Term,
    /// Its subtodos, in the order they were added.
    pub children: Vec<String>,
}

impl Composed {
    /// Read a spec. `Composed::of(s).spec() == s` for every `s`.
    #[must_use]
    pub fn of(spec: &Term) -> Composed {
        let whole = Composed {
            own: spec.clone(),
            children: Vec::new(),
        };
        let TermF::Conj { terms, p } = spec.out() else {
            return whole;
        };
        let first_ref = terms
            .iter()
            .position(|t| matches!(t.out(), TermF::Ref { .. }))
            .unwrap_or(terms.len());
        let (own, refs) = terms.split_at(first_ref);
        let children: Option<Vec<String>> = refs
            .iter()
            .map(|t| match t.out() {
                TermF::Ref {
                    store: None,
                    entity,
                } => Some(entity.clone()),
                _ => None,
            })
            .collect();
        match (own, children) {
            ([] | [_], Some(children))
                if !children.is_empty() && (p - fpl::PRIORITY_POWER).abs() < f64::EPSILON =>
            {
                Composed {
                    own: own.first().cloned().unwrap_or_else(fpl::mk_absent),
                    children,
                }
            }
            _ => whole,
        }
    }

    /// The spec it reads.
    #[must_use]
    pub fn spec(self) -> Term {
        if self.children.is_empty() {
            return self.own;
        }
        let refs = self.children.into_iter().map(|entity| {
            Term::new(TermF::Ref {
                store: None,
                entity,
            })
        });
        let own = (!is_absent(&self.own)).then_some(self.own);
        Term::new(TermF::Conj {
            terms: own.into_iter().chain(refs).collect(),
            p: fpl::PRIORITY_POWER,
        })
    }
}

/// A subtodo, authored as one act: the child, and the parent's revision whose
/// spec gains `Ref(child)`. `functions` are every todo's function as
/// `fold::link_specs` spells them; a parent the child already reaches is
/// refused, since the new reference would close a loop. A child with no
/// category takes the parent's, so it groups where the parent does.
pub fn build_subtodo(
    new: &NewTodo,
    parent: &Authored,
    functions: &BTreeMap<String, Term>,
    now: Moment,
    actor: &str,
    categories: &[&str],
    typeset: &dyn Typeset,
) -> Result<(TodoEvent, TodoEvent), Refusal> {
    let mut inherited = new.clone();
    if let Some(category) = &parent.payload.category
        && new.fields.category.is_empty()
        && takes_category(&new.fields.kind)
    {
        category.as_str().clone_into(&mut inherited.fields.category);
    }
    let child = build_todo(&inherited, now, actor, categories, typeset)?;
    let mut functions = functions.clone();
    functions.insert(
        new.todo.clone(),
        new.fields.price.term(&new.todo, now, None)?,
    );
    let revision = add_subtodo(parent, &new.todo, &functions, now.at, actor)?;
    Ok((child, revision))
}

/// The parent's half of [`build_subtodo`]: the same record with `Ref(child)`
/// added to its spec, refused when `child` already reaches it.
pub(crate) fn add_subtodo(
    parent: &Authored,
    child: &str,
    functions: &BTreeMap<String, Term>,
    at: Datetime,
    actor: &str,
) -> Result<TodoEvent, Refusal> {
    let parent_id = parent.todo.as_str();
    if let Some(mut loop_) = reaching(functions, child, parent_id, &mut BTreeSet::new()) {
        loop_.insert(0, parent_id.to_owned());
        return Err(Refusal::conflict(format!(
            "{} cannot be a subtodo of {}: the references would loop, {}",
            shown(child),
            shown(parent_id),
            loop_.join(" → ")
        )));
    }
    let mut composed = Composed::of(&parent.payload.spec);
    if composed.children.iter().any(|c| c == child) {
        return Err(Refusal::conflict(format!(
            "{} is already a subtodo of {}",
            shown(child),
            shown(parent_id)
        )));
    }
    composed.children.push(child.to_owned());
    respecified(
        parent,
        composed.spec(),
        at,
        actor,
        &format!("subtodo {child}"),
    )
}

/// `parent` with `add` attached as subtodos and `remove` detached, as one revision of its
/// spec: how a release is rescoped. Each added todo must not already be a subtodo nor reach the
/// parent; each removed one must be a subtodo; a change of nothing is refused. Detaching is a
/// revision, never a rewrite: what an earlier spec named stays named in the history.
///
/// # Errors
/// A refusal naming the todo it is about.
pub fn rescope(
    parent: &Authored,
    add: &[String],
    remove: &[String],
    functions: &BTreeMap<String, Term>,
    at: Datetime,
    actor: &str,
) -> Result<TodoEvent, Refusal> {
    let parent_id = parent.todo.as_str();
    if add.is_empty() && remove.is_empty() {
        return Err(Refusal::bad_request(format!(
            "rescoping {} names nothing to add or remove",
            shown(parent_id)
        )));
    }
    let mut composed = Composed::of(&parent.payload.spec);
    for child in remove {
        let Some(at) = composed.children.iter().position(|c| c == child) else {
            return Err(Refusal::conflict(format!(
                "{} is not a subtodo of {}",
                shown(child),
                shown(parent_id)
            )));
        };
        composed.children.remove(at);
    }
    for child in add {
        if composed.children.iter().any(|c| c == child) {
            return Err(Refusal::conflict(format!(
                "{} is already a subtodo of {}",
                shown(child),
                shown(parent_id)
            )));
        }
        if let Some(mut loop_) = reaching(functions, child, parent_id, &mut BTreeSet::new()) {
            loop_.insert(0, parent_id.to_owned());
            return Err(Refusal::conflict(format!(
                "{} cannot be a subtodo of {}: the references would loop, {}",
                shown(child),
                shown(parent_id),
                loop_.join(" → ")
            )));
        }
        composed.children.push(child.clone());
    }
    let said: Vec<String> = add
        .iter()
        .map(|c| format!("+{c}"))
        .chain(remove.iter().map(|c| format!("-{c}")))
        .collect();
    respecified(
        parent,
        composed.spec(),
        at,
        actor,
        &format!("items {}", said.join(" ")),
    )
}

/// A path of references from `from` to `to`, both included, if there is one.
fn reaching(
    functions: &BTreeMap<String, Term>,
    from: &str,
    to: &str,
    seen: &mut BTreeSet<String>,
) -> Option<Vec<String>> {
    if from == to {
        return Some(vec![to.to_owned()]);
    }
    if !seen.insert(from.to_owned()) {
        return None;
    }
    let function = functions.get(from)?;
    suzatary_base::tree::refs(function)
        .into_iter()
        .find_map(|next| {
            let mut path = reaching(functions, next, to, seen)?;
            path.insert(0, from.to_owned());
            Some(path)
        })
}

/// A confirmation: the latest provisional record re-appended, same content, by a trusted actor.
/// No new event kind or flag, since the authorship projection reads the latest writer; the
/// provisional record stays on the chain.
///
/// The two refusals are the policy: content a trusted actor already wrote has nothing in
/// doubt, and a provisional actor confirming is an agent agreeing with itself.
pub fn confirm_todo(
    before: &Authored,
    at: Datetime,
    actor: &str,
    untrusted: &Untrusted,
    note: &str,
) -> Result<TodoEvent, Refusal> {
    let is_untrusted = |name: &str| untrusted.actors().iter().any(|a| a.as_str() == name);
    if !is_untrusted(before.actor.as_str()) {
        return Err(Refusal::conflict(format!(
            "{} was last authored by {}, a trusted actor — there is nothing to confirm",
            shown(before.todo.as_str()),
            shown(before.actor.as_str())
        )));
    }
    if is_untrusted(actor) {
        return Err(Refusal::conflict(format!(
            "cannot confirm as {} — a provisional actor cannot confirm its own content",
            shown(actor)
        )));
    }
    revise(before, at, actor, note)
}

/// A new `Authored` for the same todo with only `at`, `actor` and `note` changed, built through
/// `mk_authored` again so a re-append cannot produce a record the constructor would refuse.
pub fn revise(
    before: &Authored,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent, Refusal> {
    respecified(before, before.payload.spec.clone(), at, actor, note)
}

/// [`revise`] with a new spec.
fn respecified(
    before: &Authored,
    spec: Term,
    at: Datetime,
    actor: &str,
    note: &str,
) -> Result<TodoEvent, Refusal> {
    mk_authored(
        before.todo.as_str(),
        at,
        actor,
        before.payload.kind.as_str(),
        before.payload.created,
        before.payload.body.clone(),
        spec,
        before.payload.rationale.clone(),
        before
            .payload
            .category
            .as_ref()
            .map_or("", prodrome_core::event::Name::as_str),
        before.payload.waiting_on.as_str(),
        before.payload.detail.clone(),
        before.payload.source.clone(),
        before.payload.subtodos.clone(),
        before.payload.notes.clone(),
        note,
    )
    .map_err(|e| Refusal::bad_request(e.to_string()))
}

// Undo and redo: the chain never rewrites, so both are appends.
//
// Undoing the last event on a todo appends its inverse; the folds are functions of the event
// set, and an undo is one more member of it. Undo is an involution over a todo's own stream,
// so `redo` is `undo` run again after a structural check (no stored marker) that the last
// event was itself an undo: its own inverse equals the event before it.

/// The refusal when the stream has no event to invert.
fn nothing_to_undo(todo: &str) -> Refusal {
    Refusal::conflict(format!(
        "nothing to undo for {} — its stream carries no event to invert",
        shown(todo)
    ))
}

/// `Created` cannot be undone: the id would dangle with no record at all. A
/// cancel is the honest act for a todo that should not have stood.
fn created_cannot_be_undone(todo: &str) -> Refusal {
    Refusal::conflict(format!(
        "Created cannot be undone: undoing {}'s Created would leave the id \
         dangling with no record — cancel it instead if it should not stand",
        shown(todo)
    ))
}

/// `Tended` cannot be undone: the tendings are a grow-only set.
fn tended_cannot_be_undone(todo: &str) -> Refusal {
    Refusal::conflict(format!(
        "Tended cannot be undone: {}'s passes are a grow-only set, and a pass \
         recorded stays recorded",
        shown(todo)
    ))
}

/// `redo`'s structural check failed: the last event was not itself produced
/// by an undo, so there is nothing to redo.
fn not_an_undo(todo: &str) -> Refusal {
    Refusal::conflict(format!(
        "the last act on {} is not an undo — there is nothing to redo",
        shown(todo)
    ))
}

/// What an undo or a redo reverses: the object it names, which its note spells as
/// `undo of <hash>` or `redo of <hash>`. Printed by [`Reversal::note`] alone and read back by
/// `Reversal::of_note` alone, so the note has one spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reversal {
    /// The inverse of the object named.
    Undo {
        /// The object it inverts.
        of: Hash,
    },
    /// The object an undo undid, again: the undo named.
    Redo {
        /// The undo it reverses.
        of: Hash,
    },
}

impl Reversal {
    const UNDO: &'static str = "undo of ";
    const REDO: &'static str = "redo of ";

    /// The note the appended event carries.
    #[must_use]
    pub fn note(&self) -> String {
        match self {
            Self::Undo { of } => format!("{}{}", Self::UNDO, of.as_str()),
            Self::Redo { of } => format!("{}{}", Self::REDO, of.as_str()),
        }
    }

    /// The reversal a note names, or `None` for an ordinary event's note.
    #[must_use]
    pub(crate) fn of_note(note: &str) -> Option<Self> {
        if let Some(of) = note.strip_prefix(Self::UNDO) {
            return Hash::new(of).ok().map(|of| Self::Undo { of });
        }
        let of = note.strip_prefix(Self::REDO)?;
        Hash::new(of).ok().map(|of| Self::Redo { of })
    }
}

/// Which lifecycle outcome a `Reopened` cleared, read off the stream since a `Reopened` does
/// not name it.
enum Resolution {
    Completed,
    Cancelled,
}

/// The nearest `Completed` or `Cancelled` before a `Reopened`.
fn resolution_before(prefix: &[(Hash, TodoEvent)]) -> Option<Resolution> {
    prefix.iter().rev().find_map(|(_, event)| match event {
        TodoEvent::Completed(_) => Some(Resolution::Completed),
        TodoEvent::Cancelled(_) => Some(Resolution::Cancelled),
        _ => None,
    })
}

/// The spec over a todo's own causal prefix. An `Authored` spec counts from any actor, as the
/// spec register reads it; a `SpecRevised` only where the policy says it binds.
fn spec_before(prefix: &[(Hash, TodoEvent)], untrusted: &Untrusted) -> Option<Term> {
    let mut spec = None;
    for (name, event) in prefix {
        match event {
            TodoEvent::Authored(record) => {
                if let Some(term) = record.payload.spec() {
                    spec = Some(term.clone());
                }
            }
            TodoEvent::SpecRevised(revised) if Policy::standing(untrusted, name, event).binds() => {
                spec = Some(revised.spec.clone());
            }
            TodoEvent::Created(_)
            | TodoEvent::Completed(_)
            | TodoEvent::Cancelled(_)
            | TodoEvent::Reopened(_)
            | TodoEvent::Tended(_)
            | TodoEvent::SpecRevised(_) => {}
        }
    }
    spec
}

/// The content, read over the same kind of prefix.
fn authored_before(prefix: &[(Hash, TodoEvent)]) -> Option<Authored> {
    prefix.iter().rev().find_map(|(_, event)| match event {
        TodoEvent::Authored(record) => Some((**record).clone()),
        _ => None,
    })
}

/// Two content records say the same thing: every field but `note`, which is the write's own
/// reason and never part of what it means.
#[must_use]
pub(crate) fn same_content(a: &Record, b: &Record) -> bool {
    a.kind == b.kind
        && a.created == b.created
        && a.body == b.body
        && a.spec == b.spec
        && a.rationale == b.rationale
        && a.category == b.category
        && a.waiting_on == b.waiting_on
        && a.detail == b.detail
        && a.source == b.source
        && a.subtodos == b.subtodos
        && a.notes == b.notes
}

/// Two events are the same act: same kind and content, ignoring `todo`, `at`, `actor` and
/// `note`.
#[must_use]
fn same_act(a: &TodoEvent, b: &TodoEvent) -> bool {
    match (a, b) {
        (TodoEvent::Completed(_), TodoEvent::Completed(_))
        | (TodoEvent::Cancelled(_), TodoEvent::Cancelled(_))
        | (TodoEvent::Reopened(_), TodoEvent::Reopened(_)) => true,
        (TodoEvent::SpecRevised(x), TodoEvent::SpecRevised(y)) => x.spec == y.spec,
        (TodoEvent::Authored(x), TodoEvent::Authored(y)) => same_content(&x.payload, &y.payload),
        _ => false,
    }
}

/// Replace an event's `note` field.
fn with_note(event: TodoEvent, note: String) -> TodoEvent {
    match event {
        TodoEvent::Created(mut e) => {
            e.note = note;
            TodoEvent::Created(e)
        }
        TodoEvent::Completed(mut e) => {
            e.note = note;
            TodoEvent::Completed(e)
        }
        TodoEvent::Cancelled(mut e) => {
            e.note = note;
            TodoEvent::Cancelled(e)
        }
        TodoEvent::Reopened(mut e) => {
            e.note = note;
            TodoEvent::Reopened(e)
        }
        TodoEvent::Tended(mut e) => {
            e.note = note;
            TodoEvent::Tended(e)
        }
        TodoEvent::SpecRevised(mut e) => {
            e.note = note;
            TodoEvent::SpecRevised(e)
        }
        TodoEvent::Authored(mut e) => {
            e.payload.note = note;
            TodoEvent::Authored(e)
        }
    }
}

/// The inverse of the stream's last event, with an empty note: `undo`'s core, and how
/// [`redo`] recognises its target.
fn inverse_of_last(
    stream: &[(Hash, TodoEvent)],
    todo: &str,
    at: Datetime,
    actor: &str,
    untrusted: &Untrusted,
) -> Result<TodoEvent, Refusal> {
    let Some((_, last)) = stream.last() else {
        return Err(nothing_to_undo(todo));
    };
    let prefix = &stream[..stream.len() - 1];
    match last {
        TodoEvent::Created(_) => Err(created_cannot_be_undone(todo)),
        TodoEvent::Tended(_) => Err(tended_cannot_be_undone(todo)),
        TodoEvent::Completed(_) | TodoEvent::Cancelled(_) => {
            mk_reopened(todo, at, actor, "").map_err(|e| Refusal::bad_request(e.to_string()))
        }
        TodoEvent::Reopened(_) => {
            match resolution_before(prefix) {
                Some(Resolution::Completed) => mk_completed(todo, at, actor, "")
                    .map_err(|e| Refusal::bad_request(e.to_string())),
                Some(Resolution::Cancelled) => mk_cancelled(todo, at, actor, "")
                    .map_err(|e| Refusal::bad_request(e.to_string())),
                None => Err(nothing_to_undo(todo)),
            }
        }
        TodoEvent::SpecRevised(_) => match spec_before(prefix, untrusted) {
            Some(spec) => mk_spec_revised(todo, at, actor, spec, "")
                .map_err(|e| Refusal::bad_request(e.to_string())),
            None => Err(nothing_to_undo(todo)),
        },
        TodoEvent::Authored(_) => match authored_before(prefix) {
            Some(previous) => revise(&previous, at, actor, ""),
            None => Err(nothing_to_undo(todo)),
        },
    }
}

/// Undo: the inverse of the last event on this todo, appended.
///
/// `stream` is this todo's own events with their hashes, in causal order, never filtered by
/// a moment: undo reads what the chain holds, not what an instant believes. `untrusted` says
/// which earlier revision binds when undoing a `SpecRevised`.
///
/// The refusals ([`Refusal::conflict`], since the chain refuses a well-formed request): nothing
/// to invert, a `Created` or `Tended` last, or no earlier state to restore.
pub fn undo(
    stream: &[(Hash, TodoEvent)],
    todo: &str,
    at: Datetime,
    actor: &str,
    untrusted: &Untrusted,
) -> Result<TodoEvent, Refusal> {
    let hash = stream
        .last()
        .ok_or_else(|| nothing_to_undo(todo))?
        .0
        .clone();
    let inverse = inverse_of_last(stream, todo, at, actor, untrusted)?;
    Ok(with_note(inverse, Reversal::Undo { of: hash }.note()))
}

/// Redo: the undone event, appended again.
///
/// Recognised structurally, with no stored marker: the last event is a redo target exactly
/// when its own inverse is the same act as the event before it, and since undo is an
/// involution that inverse is the event to re-append.
pub fn redo(
    stream: &[(Hash, TodoEvent)],
    todo: &str,
    at: Datetime,
    actor: &str,
    untrusted: &Untrusted,
) -> Result<TodoEvent, Refusal> {
    if stream.len() < 2 {
        return Err(not_an_undo(todo));
    }
    let hash = stream[stream.len() - 1].0.clone();
    let candidate = inverse_of_last(stream, todo, at, actor, untrusted)
        .map_err(|_refused| not_an_undo(todo))?;
    let (_, before_last) = &stream[stream.len() - 2];
    if !same_act(&candidate, before_last) {
        return Err(not_an_undo(todo));
    }
    Ok(with_note(candidate, Reversal::Redo { of: hash }.note()))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;

    /// Noon UTC on `day`, with days read in UTC.
    fn on(day: NaiveDate) -> Moment {
        Moment {
            at: fpl::datetime_of(
                day.and_time(chrono::NaiveTime::from_hms_opt(12, 0, 0).expect("noon")),
            )
            .expect("an instant"),
            zone: suzatary_base::zone::Zone::UTC,
        }
    }
    use prodrome_core::event::Actor;
    use suzatary_base::capability::Parses;

    use super::*;
    use suzatary_record::todo::{Deadline, Price};

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 30, 0, 0).expect("a real instant")
    }

    fn untrusted() -> Untrusted {
        Untrusted::of([Actor::new("triage").expect("valid")])
    }

    fn a_todo() -> NewTodo {
        NewTodo {
            todo: "filething".to_owned(),
            fields: TodoFields {
                body: "File the *permit* renewal".to_owned(),
                rationale: "Missing it is not recoverable.".to_owned(),
                kind: "todo".to_owned(),
                category: "work".to_owned(),
                price: Price::Flat { percent: 35 },
                detail: String::new(),
                waiting_on: String::new(),
            },
            source: None,
            subtodos: Vec::new(),
        }
    }

    fn a_todo_at(price: Price) -> NewTodo {
        let mut todo = a_todo();
        todo.fields.price = price;
        todo
    }

    fn authored_of(event: TodoEvent) -> Authored {
        match event {
            TodoEvent::Authored(record) => *record,
            other => panic!("expected an Authored, got {}", other.kind_name()),
        }
    }

    /// A body is markup, stored as written once the door admits it.
    #[test]
    fn a_body_is_stored_as_the_markup_its_writer_wrote() {
        let record = authored_of(
            build_todo(
                &a_todo(),
                suzatary_base::zone::Moment {
                    at: at(7),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &["work", "home"],
                &Parses,
            )
            .expect("a record"),
        );
        assert_eq!(record.payload.body.as_str(), "File the *permit* renewal");
        // `created` is the DAY of `at`, which is the file it renders into.
        assert_eq!(
            fpl::iso(fpl::instant_of(record.payload.created)),
            "2026-09-07T00:00:00"
        );
    }

    /// A closed world whose every render carries a script: what the tripwire is for.
    struct Scripted;
    impl Typeset for Scripted {
        fn closed(&self, _: &str) -> Result<String, suzatary_base::capability::Rejected> {
            Ok("<body><script>x()</script></body>".to_owned())
        }
    }

    fn adding(body: &str, actor: &str, typeset: &dyn Typeset) -> Result<TodoEvent, Refusal> {
        let mut todo = a_todo();
        body.clone_into(&mut todo.fields.body);
        build_todo(
            &todo,
            suzatary_base::zone::Moment {
                at: at(7),
                zone: suzatary_base::zone::Zone::UTC,
            },
            actor,
            &["work"],
            typeset,
        )
    }

    /// The door refuses what does not parse, what would not stand inside `[..]`, and a render
    /// the tripwire flags, and says where typst stopped.
    #[test]
    fn the_door_refuses_what_does_not_parse_embed_or_render_cleanly() {
        let stray = adding("a ] b", "ada", &Parses).expect_err("a stray bracket");
        assert!(
            stray
                .message()
                .starts_with("body is typst markup, and this is not markup this box renders: 1:3: unexpected closing bracket"),
            "{}",
            stray.message()
        );
        let swallowed = adding("Call ada // later", "ada", &Parses).expect_err("a comment");
        assert!(
            swallowed.message().contains("[..] block"),
            "{}",
            swallowed.message()
        );
        let scripted = adding("Renew it", "ada", &Scripted).expect_err("a script");
        assert!(
            scripted.message().contains("an element typst never writes"),
            "{}",
            scripted.message()
        );
    }

    /// Who writes is never asked: the triage agent's text and the owner's meet one door.
    #[test]
    fn every_writer_meets_the_same_door() {
        for body in ["Renew *passport*", "a ] b", "Call ada // later"] {
            let mine = adding(body, "ada", &Parses).map(|e| authored_of(e).payload.body);
            let theirs = adding(body, "triage", &Parses).map(|e| authored_of(e).payload.body);
            assert_eq!(
                mine.map_err(|r| r.message().to_owned()),
                theirs.map_err(|r| r.message().to_owned()),
                "{body}"
            );
        }
    }

    /// An appended detail is checked as the whole detail it makes.
    #[test]
    fn an_appended_detail_is_checked_whole() {
        let before = authored_of(adding("Renew it", "ada", &Parses).expect("a record"));
        let edit = |text: &str| TodoPatch {
            detail_append: Patch::Set(text.to_owned()),
            ..TodoPatch::default()
        };
        let once = authored_of(
            update_todo(
                &before,
                &edit("First *note*"),
                suzatary_base::zone::Moment {
                    at: at(8),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &[],
                "",
                &Parses,
            )
            .expect("appends"),
        );
        let twice = authored_of(
            update_todo(
                &once,
                &edit("Second"),
                suzatary_base::zone::Moment {
                    at: at(9),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &[],
                "",
                &Parses,
            )
            .expect("appends"),
        );
        assert_eq!(twice.payload.detail.as_str(), "First *note*\n\nSecond");
        assert!(
            update_todo(
                &once,
                &edit("x ]"),
                suzatary_base::zone::Moment {
                    at: at(9),
                    zone: suzatary_base::zone::Zone::UTC
                },
                "ada",
                &[],
                "",
                &Parses
            )
            .is_err()
        );
    }

    /// Confirmation is a trusted re-append of the same content, with a new actor and instant.
    #[test]
    fn confirming_re_appends_the_same_content_under_the_trusted_actor() {
        let theirs = authored_of(
            build_todo(
                &a_todo(),
                suzatary_base::zone::Moment {
                    at: at(7),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "triage",
                &["work"],
                &Parses,
            )
            .expect("a record"),
        );
        let mine = authored_of(
            confirm_todo(&theirs, at(8), "ada", &untrusted(), "checked the deadline")
                .expect("confirms"),
        );
        assert_eq!(mine.actor.as_str(), "ada");
        assert_eq!(mine.payload.body.as_str(), theirs.payload.body.as_str());
        assert_eq!(mine.payload.spec, theirs.payload.spec);
        assert_eq!(mine.payload.rationale, theirs.payload.rationale);
        assert_eq!(mine.payload.created, theirs.payload.created);
        assert_eq!(mine.payload.note, "checked the deadline");
        assert_ne!(mine.at, theirs.at);
    }

    /// The two refusals are the policy.
    #[test]
    fn there_is_nothing_to_confirm_and_nobody_confirms_themselves() {
        let mine = authored_of(
            build_todo(
                &a_todo(),
                suzatary_base::zone::Moment {
                    at: at(7),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &["work"],
                &Parses,
            )
            .expect("a record"),
        );
        assert_eq!(
            confirm_todo(&mine, at(8), "ada", &untrusted(), "")
                .unwrap_err()
                .message(),
            "'filething' was last authored by 'ada', a trusted actor — there is nothing to confirm"
        );
        let theirs = authored_of(
            build_todo(
                &a_todo(),
                suzatary_base::zone::Moment {
                    at: at(7),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "triage",
                &["work"],
                &Parses,
            )
            .expect("a record"),
        );
        assert_eq!(
            confirm_todo(&theirs, at(8), "triage", &untrusted(), "")
                .unwrap_err()
                .message(),
            "cannot confirm as 'triage' — a provisional actor cannot confirm its own content"
        );
    }

    /// 21:00 in New York on 2026-10-06 is 01:00 UTC on the 7th: every day the box reads is
    /// still the 6th, so tomorrow may be deferred to, the todo is created on the 6th and a
    /// deadline on Friday is reached when Friday ends in New York.
    #[test]
    fn at_nine_in_the_evening_in_new_york_the_days_are_new_yorks() {
        let new_york = suzatary_base::zone::Zone::named("America/New_York").expect("a zone");
        let late = fpl::datetime_of(
            NaiveDate::from_ymd_opt(2026, 10, 7)
                .and_then(|d| d.and_hms_opt(1, 0, 0))
                .expect("an instant"),
        )
        .expect("in range");
        let now = Moment {
            at: late,
            zone: new_york,
        };
        let utc = Moment {
            at: late,
            zone: suzatary_base::zone::Zone::UTC,
        };
        let day = |d| NaiveDate::from_ymd_opt(2026, 10, d).expect("a day");
        let flat = fpl::mk_flat(0.35).expect("valid");

        assert_eq!(now.today(), day(6));
        let deferred = defer(&flat, day(7), now).expect("tomorrow, in New York");
        let TermF::Piecewise(schedule) = deferred.out() else {
            panic!("a deferral is a piecewise")
        };
        assert_eq!(
            schedule.knots().first().map(|piece| piece.0),
            NaiveDate::from_ymd_opt(2026, 10, 7).and_then(|d| d.and_hms_opt(4, 0, 0)),
            "held at 100% until the 7th begins in New York"
        );
        assert!(
            defer(&flat, day(7), utc).is_err(),
            "in UTC the 7th is already today"
        );
        assert!(
            defer(&flat, day(6), now).is_err(),
            "and in New York the 6th is today"
        );

        let record =
            authored_of(build_todo(&a_todo(), now, "ada", &["work"], &Parses).expect("a record"));
        assert_eq!(fpl::instant_of(record.payload.created).date(), day(6));

        let mut due = a_todo();
        due.fields.price = Price::Deadline(Deadline {
            end_date: day(9),
            start_percent: 60,
            end_percent: 5,
            lead_up: Some(Duration::days(7)),
            start_date: None,
        });
        let record =
            authored_of(build_todo(&due, now, "ada", &["work"], &Parses).expect("a record"));
        let TermF::Decay { end_date, .. } = record.payload.spec().expect("a spec").out() else {
            panic!("a deadline is a decay")
        };
        assert_eq!(
            Some(*end_date),
            NaiveDate::from_ymd_opt(2026, 10, 10).and_then(|d| d.and_hms_opt(4, 0, 0)),
            "Friday the 9th ends at midnight EDT"
        );
    }

    // --- undo and redo ---------------------------------------------------

    fn hash_of(i: usize) -> Hash {
        Hash::new(format!("{i:064x}")).expect("64 lowercase hex")
    }

    fn created(id: &str, day: u32) -> TodoEvent {
        prodrome_core::event::mk_created(id, at(day), "ada", "", "").expect("valid")
    }

    fn completed(id: &str, day: u32) -> TodoEvent {
        mk_completed(id, at(day), "ada", "").expect("valid")
    }

    fn cancelled(id: &str, day: u32) -> TodoEvent {
        mk_cancelled(id, at(day), "ada", "").expect("valid")
    }

    #[test]
    fn a_pass_cannot_be_undone_because_the_tendings_only_grow() {
        let pass = prodrome_core::event::mk_tended("alpha", at(2), "ada", "").expect("valid");
        let stream = [(hash_of(0), created("alpha", 1)), (hash_of(1), pass)];
        let refusal = undo(&stream, "alpha", at(3), "ada", &untrusted()).unwrap_err();
        assert!(
            refusal.message().starts_with("Tended cannot be undone"),
            "{}",
            refusal.message()
        );
    }

    /// A reversal's note reads back as the same reversal; any other note, an ordinary one or one
    /// whose tail is not a hash, reads as none.
    #[test]
    fn a_reversal_prints_and_reads_one_note() {
        for reversal in [
            Reversal::Undo { of: hash_of(1) },
            Reversal::Redo { of: hash_of(2) },
        ] {
            assert_eq!(Reversal::of_note(&reversal.note()), Some(reversal));
        }
        for note in [
            "",
            "undo of",
            "undo of not-a-hash",
            "moved to Friday",
            "Undo of x",
        ] {
            assert_eq!(Reversal::of_note(note), None, "{note:?}");
        }
    }

    #[test]
    fn undoing_a_reprice_restores_the_earlier_spec() {
        let first = fpl::mk_flat(0.4).expect("valid");
        let second = fpl::mk_flat(0.2).expect("valid");
        let stream = [
            (
                hash_of(0),
                mk_authored(
                    "alpha",
                    at(1),
                    "ada",
                    "todo",
                    at(1),
                    MarkupSource::escaped("body"),
                    first.clone(),
                    vec![],
                    "",
                    "",
                    MarkupSource::empty(),
                    None,
                    vec![],
                    vec![],
                    "",
                )
                .expect("valid"),
            ),
            (
                hash_of(1),
                mk_spec_revised("alpha", at(2), "ada", second, "").expect("valid"),
            ),
        ];
        let appended = undo(&stream, "alpha", at(3), "ada", &untrusted()).expect("invertible");
        let TodoEvent::SpecRevised(revised) = &appended else {
            panic!("expected a SpecRevised, got {}", appended.kind_name())
        };
        assert_eq!(revised.spec, first);
    }

    // --- detail_append --------------------------------------------------

    /// A todo authored directly with real typst markup in `detail`.
    fn hand_authored_detail(detail: &str) -> Authored {
        authored_of(
            mk_authored(
                "filething",
                at(1),
                "ada",
                "todo",
                at(1),
                MarkupSource::escaped("body"),
                fpl::mk_flat(0.5).unwrap(),
                vec![],
                "",
                "",
                MarkupSource::from_chain(detail),
                None,
                vec![],
                vec![],
                "",
            )
            .expect("valid"),
        )
    }

    /// Appending carries stored markup through byte for byte, never re-escaped, followed by
    /// the escaped new paragraph after a blank line.
    #[test]
    fn appending_a_paragraph_keeps_existing_markup_byte_for_byte() {
        let markup = "*strong* and _emph_ and #quote[Rena's words]";
        let before = hand_authored_detail(markup);
        let after = authored_of(
            update_todo(
                &before,
                &TodoPatch {
                    detail_append: Patch::Set("Confirmed on 2026-09-11.".to_owned()),
                    ..TodoPatch::default()
                },
                suzatary_base::zone::Moment {
                    at: at(2),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &["work"],
                "",
                &Parses,
            )
            .expect("a valid append"),
        );
        assert_eq!(
            after.payload.detail.as_str(),
            format!("{markup}\n\nConfirmed on 2026-09-11.")
        );
    }

    fn reopened_event(id: &str, day: u32) -> TodoEvent {
        mk_reopened(id, at(day), "ada", "").expect("valid")
    }

    // --- the properties, over random streams ------------------------------

    /// One step of a todo's history. `ResolveThenReopen` is one step, so no generated
    /// `Reopened` is groundless: that would test a refusal, not the restoration law.
    #[derive(Debug, Clone)]
    enum Step {
        Author(Option<f64>),
        Reprice(f64),
        Complete,
        Cancel,
        ResolveThenReopen(bool),
    }

    fn step() -> impl proptest::strategy::Strategy<Value = Step> {
        use proptest::prelude::*;
        prop_oneof![
            proptest::option::of(0.0f64..1.0).prop_map(Step::Author),
            (0.0f64..1.0).prop_map(Step::Reprice),
            Just(Step::Complete),
            Just(Step::Cancel),
            any::<bool>().prop_map(Step::ResolveThenReopen),
        ]
    }

    /// [`step`] without `Reprice` and with every `Author` spec `None`: the spec register reads
    /// any `Authored` spec, so undoing a content-only edit could expose an older spec. These
    /// properties stay about content and lifecycle; reprice undo has its own test.
    fn step_no_reprice() -> impl proptest::strategy::Strategy<Value = Step> {
        use proptest::prelude::*;
        prop_oneof![
            Just(Step::Author(None)),
            Just(Step::Complete),
            Just(Step::Cancel),
            any::<bool>().prop_map(Step::ResolveThenReopen),
        ]
    }

    /// A generated sequence with a `Complete`/`Cancel` on an already-resolved todo turned into
    /// `Author(None)`: undo maps a resolution to `Reopened` by kind alone, so a second
    /// resolution has no earlier reading for these properties to restore.
    fn normalize(steps: Vec<Step>) -> Vec<Step> {
        let mut resolved = false;
        steps
            .into_iter()
            .map(|step| match step {
                Step::Complete | Step::Cancel if resolved => Step::Author(None),
                Step::Complete | Step::Cancel => {
                    resolved = true;
                    step
                }
                Step::ResolveThenReopen(_) => {
                    resolved = false;
                    step
                }
                other @ (Step::Author(_) | Step::Reprice(_)) => other,
            })
            .collect()
    }

    /// The seed plus every step, as events with synthetic hashes.
    fn stream_of(id: &str, steps: &[Step]) -> Vec<(Hash, TodoEvent)> {
        let mut out: Vec<(Hash, TodoEvent)> = Vec::new();
        let push = |event: TodoEvent, out: &mut Vec<(Hash, TodoEvent)>| {
            out.push((hash_of(out.len()), event));
        };
        push(
            mk_authored(
                id,
                at(1),
                "ada",
                "todo",
                at(1),
                MarkupSource::escaped("seed body"),
                fpl::mk_flat(0.5).unwrap(),
                vec![],
                "",
                "",
                MarkupSource::empty(),
                None,
                vec![],
                vec![],
                "",
            )
            .expect("valid"),
            &mut out,
        );
        let mut day = 2u32;
        for stepped in steps {
            match stepped {
                Step::Author(spec) => push(
                    mk_authored(
                        id,
                        at(day),
                        "ada",
                        "todo",
                        at(1),
                        MarkupSource::escaped(&format!("revised body {day}")),
                        spec.map_or_else(fpl::mk_absent, |v| fpl::mk_flat(v).unwrap()),
                        vec![],
                        "",
                        "",
                        MarkupSource::empty(),
                        None,
                        vec![],
                        vec![],
                        "",
                    )
                    .expect("valid"),
                    &mut out,
                ),
                Step::Reprice(value) => push(
                    mk_spec_revised(id, at(day), "ada", fpl::mk_flat(*value).unwrap(), "")
                        .expect("valid"),
                    &mut out,
                ),
                Step::Complete => push(completed(id, day), &mut out),
                Step::Cancel => push(cancelled(id, day), &mut out),
                Step::ResolveThenReopen(as_completed) => {
                    if *as_completed {
                        push(completed(id, day), &mut out);
                    } else {
                        push(cancelled(id, day), &mut out);
                    }
                    day += 1;
                    push(reopened_event(id, day), &mut out);
                }
            }
            day += 1;
        }
        out
    }

    fn events_of(stream: &[(Hash, TodoEvent)]) -> Vec<TodoEvent> {
        stream.iter().map(|(_, event)| event.clone()).collect()
    }

    /// What the folds say about this one todo: its outcome's kind (a fresh append never
    /// claims the same instant), its spec, and its content without `note`.
    #[derive(Debug, PartialEq)]
    struct Reading {
        state: &'static str,
        spec: Option<Term>,
        content: Option<Record>,
    }

    fn reading_of(events: &[TodoEvent], t: Datetime, policy: &Untrusted, id: &str) -> Reading {
        let todo = prodrome_core::event::TodoId::new(id).expect("valid");
        let believed = crate::believed::Believed::of(
            crate::believed::Preview::chain(events).nodes(),
            t,
            policy,
        )
        .expect("folds");
        let state = believed
            .env
            .outcomes
            .get(todo.as_str())
            .and_then(crate::believed::one_outcome)
            .map_or("open", |binding| binding.kind());
        let spec = believed
            .specs
            .get(&todo)
            .and_then(|specs| specs.first())
            .cloned();
        // `note` is blanked, as `same_content` ignores it, since `Reading` uses derived equality.
        let content = believed.record(&todo).map(|record| {
            let mut payload = record.payload.clone();
            payload.note = String::new();
            payload
        });
        Reading {
            state,
            spec,
            content,
        }
    }

    /// An instant strictly after anything [`stream_of`] can produce (day ≤ 13); `at(u32)` is a
    /// day of the month and cannot express it.
    fn later(n: u32) -> Datetime {
        let base = NaiveDate::from_ymd_opt(2026, 9, 30)
            .expect("a real day")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            + Duration::hours(i64::from(n));
        fpl::datetime_of(base).expect("a real instant")
    }

    fn far() -> Datetime {
        later(250)
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(200))]

        /// Undo∘act restores the fold's reading for that todo, at a moment after everything.
        #[test]
        fn undo_restores_the_folds_reading(steps in proptest::collection::vec(step_no_reprice(), 1..6)) {
            let id = "alpha";
            let stream = stream_of(id, &normalize(steps));
            let before = reading_of(&events_of(&stream[..stream.len() - 1]), far(), &untrusted(), id);

            let appended = undo(&stream, id, later(200), "ada", &untrusted())
                .expect("the generator only ever produces an invertible last event");
            let mut after_stream = stream.clone();
            after_stream.push((hash_of(after_stream.len()), appended));
            let after = reading_of(&events_of(&after_stream), far(), &untrusted(), id);

            proptest::prop_assert_eq!(before, after);
        }

        /// Redo∘undo restores the reading right after the original last event.
        #[test]
        fn redo_after_undo_restores_the_folds_reading(steps in proptest::collection::vec(step_no_reprice(), 1..6)) {
            let id = "alpha";
            let stream = stream_of(id, &normalize(steps));
            let original = reading_of(&events_of(&stream), far(), &untrusted(), id);

            let undone = undo(&stream, id, later(200), "ada", &untrusted()).expect("invertible");
            let mut after_undo = stream.clone();
            after_undo.push((hash_of(after_undo.len()), undone));

            let redone = redo(&after_undo, id, later(210), "ada", &untrusted())
                .expect("the undo just appended is structurally recognised");
            let mut after_redo = after_undo.clone();
            after_redo.push((hash_of(after_redo.len()), redone));
            let restored = reading_of(&events_of(&after_redo), far(), &untrusted(), id);

            proptest::prop_assert_eq!(original, restored);
        }

        /// Undo of undo is the same act `redo` would append.
        #[test]
        fn undo_of_undo_is_redo(steps in proptest::collection::vec(step(), 1..6)) {
            let id = "alpha";
            let stream = stream_of(id, &steps);
            let undone = undo(&stream, id, later(200), "ada", &untrusted()).expect("invertible");
            let mut after_undo = stream.clone();
            after_undo.push((hash_of(after_undo.len()), undone));

            let undo_of_undo = undo(&after_undo, id, later(210), "ada", &untrusted())
                .expect("undo is an involution over its own history");
            let redo_of_it = redo(&after_undo, id, later(220), "ada", &untrusted())
                .expect("structurally recognised");

            proptest::prop_assert!(same_act(&undo_of_undo, &redo_of_it));
        }
    }

    // --- `defer`: a curve, never a flag ---------------------------------------

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a real day")
    }

    /// A deferred term is in `mk_piecewise`'s normal form: it round-trips through the literal
    /// printer and parser.
    #[test]
    fn a_deferred_term_round_trips_through_the_literal_printer() {
        let decay = fpl::mk_decay(
            0.6,
            0.05,
            NaiveDateTime::from(day("2026-10-01")),
            Duration::weeks(1),
            None,
        )
        .expect("a valid decay");
        let deferred = defer(&decay, day("2026-09-20"), on(day("2026-09-10"))).expect("defers");
        let printed = fpl::print_term(&deferred);
        let reparsed = fpl::parse_term(&printed).expect("prints as valid FPL");
        assert_eq!(reparsed, deferred);
    }

    // --- build_decision ------------------------------------------------------

    fn performs() -> Reference {
        Reference::Proposal(suzatary_base::reference::ProposalRef {
            conversation: suzatary_base::conversation_id::ConversationId::new("2026-09-17-1")
                .expect("a slug"),
            index: 3,
        })
    }

    fn a_decision() -> NewDecision {
        NewDecision {
            body: "Send Re: the deed to Harrow".to_owned(),
            why: "He asked for a status update yesterday.".to_owned(),
            performs: performs(),
            detail: "To: counsel@example.test\n\nHere is where things stand.".to_owned(),
            priority: None,
            category: String::new(),
            source: String::new(),
        }
    }

    fn record_of(event: TodoEvent) -> Record {
        authored_of(event).payload
    }

    /// The reference round-trips off `detail`'s first line.
    #[test]
    fn the_reference_round_trips_off_the_first_line() {
        let record = record_of(
            build_decision(
                &a_decision(),
                suzatary_base::zone::Moment {
                    at: at(17),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "assistant",
                &[],
                &BTreeSet::new(),
            )
            .expect("a record"),
        );
        assert_eq!(decision_reference(&record), Some(performs()));
    }

    /// A decision's prose is escaped exactly as a todo's.
    #[test]
    fn prose_is_escaped_on_the_way_in() {
        let mut structural = a_decision();
        structural.body = "Apply *3* changes #now".to_owned();
        let record = record_of(
            build_decision(
                &structural,
                suzatary_base::zone::Moment {
                    at: at(17),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "assistant",
                &[],
                &BTreeSet::new(),
            )
            .expect("a record"),
        );
        // Escaping neutralises structure without dropping characters.
        assert_eq!(record.body.as_str(), r"Apply \*3\* changes \#now");
    }

    // --- subtodos ------------------------------------------------------------

    fn child(id: &str) -> NewTodo {
        let mut child = a_todo_at(Price::Flat { percent: 50 });
        id.clone_into(&mut child.todo);
        child.fields.category = String::new();
        child
    }

    fn reference(todo: &str) -> Term {
        fpl::mk_ref(todo.to_owned()).expect("a ref")
    }

    /// The parent, then each named subtodo added to it in turn.
    fn parent_with(children: &[&str]) -> Authored {
        let mut parent = authored_of(
            build_todo(
                &a_todo(),
                suzatary_base::zone::Moment {
                    at: at(7),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &["work"],
                &Parses,
            )
            .expect("a parent"),
        );
        for id in children {
            let (_, revision) = build_subtodo(
                &child(id),
                &parent,
                &BTreeMap::new(),
                suzatary_base::zone::Moment {
                    at: at(8),
                    zone: suzatary_base::zone::Zone::UTC,
                },
                "ada",
                &["work"],
                &Parses,
            )
            .expect("a subtodo");
            parent = authored_of(revision);
        }
        parent
    }

    #[test]
    fn a_spec_reads_back_as_itself() {
        let own = fpl::mk_flat(0.4).expect("a flat");
        let conj = |terms: Vec<Term>| fpl::mk_conj(terms, fpl::PRIORITY_POWER).expect("a conj");
        for spec in [
            fpl::mk_absent(),
            own.clone(),
            conj(vec![own.clone(), reference("aaa")]),
            conj(vec![reference("aaa"), reference("bbb")]),
            conj(vec![own.clone(), own.clone(), reference("aaa")]),
            conj(vec![reference("aaa"), own.clone()]),
            fpl::mk_conj(vec![own.clone(), reference("aaa")], 0.0).expect("a conj"),
        ] {
            assert_eq!(Composed::of(&spec).spec(), spec);
        }
    }

    #[test]
    fn a_parent_that_would_close_a_loop_is_refused() {
        let parent = parent_with(&[]);
        let functions = BTreeMap::from([
            ("scanforms".to_owned(), reference("printer")),
            ("printer".to_owned(), reference("filething")),
        ]);
        let refused =
            add_subtodo(&parent, "scanforms", &functions, at(8), "ada").expect_err("a loop");
        assert!(
            refused
                .message()
                .contains("filething → scanforms → printer → filething"),
            "{}",
            refused.message()
        );
        let own =
            add_subtodo(&parent, "filething", &BTreeMap::new(), at(8), "ada").expect_err("itself");
        assert!(own.message().contains("loop"), "{}", own.message());
        let twice = parent_with(&["scanforms"]);
        assert!(add_subtodo(&twice, "scanforms", &BTreeMap::new(), at(8), "ada").is_err());
    }

    /// Suffixing beats refusing, and no id is ever reused.
    #[test]
    fn a_taken_id_takes_the_next_free_suffix() {
        let mut known: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        assert_eq!(fresh_id("water", &known).expect("free"), "water");
        known.insert("water".to_owned());
        assert_eq!(fresh_id("water", &known).expect("free"), "water2");
        known.insert("water2".to_owned());
        assert_eq!(fresh_id("water", &known).expect("free"), "water3");
        // The suffix eats into the cut, so the id never grows past its cap.
        let long = "a".repeat(ID_CHARS);
        known.insert(long.clone());
        let next = fresh_id(&long, &known).expect("free");
        assert_eq!(next.chars().count(), ID_CHARS);
        assert!(next.ends_with('2'));
    }
}
