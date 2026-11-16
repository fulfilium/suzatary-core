//! The carried list: the public name of each actor whose words are carried, and for each open
//! todo of the source, by its id, whether `prodrome carry` takes it to the public store and in
//! what words, or leaves it on the source, with a one-line reason. The main session writes it
//! and the owner reads it, so it is a box-local file, never a tracked one. It is TOML:
//!
//! ```toml
//! [actors]                            # a source actor = the name it is published under
//! assistant = "assistant"
//!
//! [todos.coldstart]
//! carry = "Any store starts slowly, so this is the project's work."
//! as = "coldread"                     # a new id, where the old one does not pass door 1
//! body = "A cold start reads every object again."
//! detail = "Keep the reading across restarts."
//! rationale = "It costs every restart."
//! waiting_on = ""                     # "" clears it
//! category = "project"                # "" clears it
//! subtodos = [{ body = "Measure it", done = false }]
//! notes = []                          # each { on = "<site>", lines = ["…"] }
//!
//! [todos.boxdisk]
//! stay = "The box's own disk."
//! ```
//!
//! Every free-text field a carried todo publishes can be restated here. What the list cannot
//! change holds no free text: the kind is one of a closed set, `created` and each `at` are
//! instants, and the price is numbers and instants whose references the carry renames through
//! this list.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::{Actor, Name, TodoId};
use suzatary_base::capability::Typeset;
use suzatary_record::record::{MarkupSource, Note, NoteSite, StringSource, SubTodo};

/// The public actors, and each todo the list names with what becomes of it. A table is keyed by
/// its todo, and TOML refuses a key twice, so no todo is both carried and staying.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CarriedList {
    /// Each source actor whose words may be carried, and the name it is published under.
    pub(crate) actors: BTreeMap<Actor, Actor>,
    pub(crate) todos: BTreeMap<TodoId, Decision>,
}

/// What the list says of one todo.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// Carried to the public store.
    Carry(Restated),
    /// Left on the source: it is the deployment's own upkeep.
    Stay {
        /// Why, in one line.
        reason: String,
    },
}

/// One field of a carried record: as the source holds it, or as the list restates it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Field<T> {
    Kept,
    Restated(T),
}

/// A carried todo, and each field the list restates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Restated {
    /// Why, in one line.
    pub(crate) reason: String,
    /// Its id on the public store.
    pub(crate) to: Option<TodoId>,
    pub(crate) body: Field<MarkupSource>,
    pub(crate) detail: Field<MarkupSource>,
    pub(crate) rationale: Field<Vec<String>>,
    pub(crate) waiting_on: Field<StringSource>,
    pub(crate) category: Field<Option<Name>>,
    pub(crate) subtodos: Field<Vec<SubTodo>>,
    pub(crate) notes: Field<Vec<Note>>,
}

/// The file as written, before [`CarriedList::parse`] reads it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenList {
    #[serde(default)]
    actors: BTreeMap<String, String>,
    #[serde(default)]
    todos: BTreeMap<String, Written>,
}

/// One table as written.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    carry: Option<String>,
    stay: Option<String>,
    #[serde(rename = "as")]
    to: Option<String>,
    body: Option<String>,
    detail: Option<String>,
    rationale: Option<String>,
    waiting_on: Option<String>,
    category: Option<String>,
    subtodos: Option<Vec<SubTodo>>,
    notes: Option<Vec<WrittenNote>>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenNote {
    on: String,
    lines: Vec<String>,
}

impl<T> Field<T> {
    /// The restated value, else `kept`.
    pub(crate) fn or(self, kept: T) -> T {
        match self {
            Field::Kept => kept,
            Field::Restated(value) => value,
        }
    }

    pub(crate) fn is_restated(&self) -> bool {
        matches!(self, Field::Restated(_))
    }

    fn of<W>(
        written: Option<W>,
        read: impl FnOnce(W) -> Result<T, String>,
    ) -> Result<Field<T>, String> {
        written.map_or(Ok(Field::Kept), |w| read(w).map(Field::Restated))
    }
}

impl Restated {
    /// A completed release the list does not name: carried with every field as it stands.
    pub(crate) fn kept() -> Restated {
        Restated {
            reason: String::new(),
            to: None,
            body: Field::Kept,
            detail: Field::Kept,
            rationale: Field::Kept,
            waiting_on: Field::Kept,
            category: Field::Kept,
            subtodos: Field::Kept,
            notes: Field::Kept,
        }
    }

    /// Whether any word of the record is the list's rather than the source's.
    pub(crate) fn restates(&self) -> bool {
        self.body.is_restated()
            || self.detail.is_restated()
            || self.rationale.is_restated()
            || self.waiting_on.is_restated()
            || self.category.is_restated()
            || self.subtodos.is_restated()
            || self.notes.is_restated()
    }
}

impl CarriedList {
    /// The public actors' names, door 1's writers on the destination.
    pub(crate) fn public_actors(&self) -> BTreeSet<Actor> {
        self.actors.values().cloned().collect()
    }

    /// The list `text` holds, its restated markup checked by `typeset`.
    ///
    /// # Errors
    /// Not TOML; an actor, a key or an `as` that is not a name or a todo id; a table with both
    /// or neither of `carry` and `stay`, or a staying one that restates a field; a reason that is
    /// blank or longer than one line; a blank body or checklist item; an unknown note site or an
    /// empty note; markup the closed world refuses.
    pub(crate) fn parse(text: &str, typeset: &dyn Typeset) -> anyhow::Result<CarriedList> {
        let written: WrittenList = toml::from_str(text)
            .map_err(|e| anyhow::anyhow!("the carried list does not read: {}", e.message()))?;
        let actors = written
            .actors
            .into_iter()
            .map(|(from, to)| {
                let actor = |name: String| {
                    Actor::new(name)
                        .map_err(|e| anyhow::anyhow!("the carried list's [actors]: {e}"))
                };
                Ok((actor(from)?, actor(to)?))
            })
            .collect::<anyhow::Result<_>>()?;
        let todos = written
            .todos
            .into_iter()
            .map(|(id, written)| {
                let wrong = |why: &str| anyhow::anyhow!("the carried list's [todos.{id}] {why}");
                let todo = TodoId::new(id.clone()).map_err(|e| wrong(&e.to_string()))?;
                let decision = decision(written, typeset).map_err(|why| wrong(&why))?;
                Ok((todo, decision))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(CarriedList { actors, todos })
    }
}

fn decision(written: Written, typeset: &dyn Typeset) -> Result<Decision, String> {
    match written {
        Written {
            carry: Some(reason),
            stay: None,
            to,
            body,
            detail,
            rationale,
            waiting_on,
            category,
            subtodos,
            notes,
        } => Ok(Decision::Carry(Restated {
            reason: one_line(reason)?,
            to: to
                .map(TodoId::new)
                .transpose()
                .map_err(|e| format!("as: {e}"))?,
            body: Field::of(body, |text| match text.trim() {
                "" => Err("restates its body as nothing".to_owned()),
                _ => markup("body", &text, typeset),
            })?,
            detail: Field::of(detail, |text| markup("detail", &text, typeset))?,
            rationale: Field::of(rationale, |text| {
                Ok(text.lines().map(str::to_owned).collect())
            })?,
            waiting_on: Field::of(waiting_on, |text| Ok(StringSource::new(text)))?,
            category: Field::of(category, |text| {
                Name::optional("category", text).map_err(|e| e.to_string())
            })?,
            subtodos: Field::of(subtodos, |items| {
                items
                    .into_iter()
                    .map(|item| {
                        suzatary_record::record::mk_subtodo(&item.body, item.done)
                            .map_err(|e| e.to_string())
                    })
                    .collect()
            })?,
            notes: Field::of(notes, |notes| {
                notes
                    .into_iter()
                    .map(|WrittenNote { on, lines }| match lines.as_slice() {
                        [] => Err("restates a note with no lines".to_owned()),
                        _ => Ok(Note {
                            on: NoteSite::parse(&on).map_err(|e| e.to_string())?,
                            lines,
                        }),
                    })
                    .collect()
            })?,
        })),
        Written {
            carry: None,
            stay: Some(reason),
            to: None,
            body: None,
            detail: None,
            rationale: None,
            waiting_on: None,
            category: None,
            subtodos: None,
            notes: None,
        } => Ok(Decision::Stay {
            reason: one_line(reason)?,
        }),
        Written {
            carry: None,
            stay: Some(_),
            ..
        } => Err("stays, and a todo that stays restates nothing".to_owned()),
        Written {
            carry: Some(_),
            stay: Some(_),
            ..
        }
        | Written {
            carry: None,
            stay: None,
            ..
        } => Err("says neither carry nor stay, or both".to_owned()),
    }
}

/// A reason as one nonblank line.
fn one_line(reason: String) -> Result<String, String> {
    if reason.trim().is_empty() || reason.contains('\n') {
        return Err("gives a reason that is blank or longer than one line".to_owned());
    }
    Ok(reason)
}

/// Restated markup, through the one markup door.
fn markup(field: &str, text: &str, typeset: &dyn Typeset) -> Result<MarkupSource, String> {
    suzatary_todo::authoring::admitted(field, text, typeset)
        .map_err(|refusal| refusal.message().to_owned())
}
