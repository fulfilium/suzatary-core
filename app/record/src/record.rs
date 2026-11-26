//! Suzatary's record: what this application attaches to a todo beyond the database's `todo`,
//! `at` and `actor`, a decoration of a fulfillment curve. The database reads only two things
//! from it: the spec it prices by and how many checklist items it holds.
//!
//! The stored bytes are frozen here: the name and fields of [`Record`]'s
//! [`codec::Record::CONSTRUCTOR`] are what an object's name is a hash of, so a field added to a
//! shipped constructor would rename every stored object. Evolution is a new constructor, never a
//! new field; `app/cli/tests/live.rs` is the guard.

use prodrome_data::event::Name;
use prodrome_data::literal::codec::{self, Constructor, Literal};
use prodrome_data::literal::{Datetime, ProdromeError, Value};
use prodrome_term::fpl::mk_absent;
use prodrome_term::term::{Term, TermF};
use prodrome_todo::payload::Payload;

use suzatary_base::capability::{Rejected, Typeset, parsed};

// Not the Prodrome's `newtype_str!`: its generated methods carry no docs, and
// `missing_docs` cannot be allowed from outside a `macro_rules!` item.

/// Typst markup source, the inside of a `[..]` block; a separate type from [`StringSource`]
/// because `]` is structure in one and `"` in the other.
///
/// Its constructors are its doors: [`MarkupSource::checked`] for a writer's markup, whoever
/// the writer, [`MarkupSource::escaped`] for text that is data, and
/// [`MarkupSource::from_chain`] for bytes already admitted when they were stored. Its `Default`
/// is [`MarkupSource::empty`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MarkupSource(String);

impl MarkupSource {
    /// A writer's markup, as written, admitted only if typst's parser finds no error in it, it
    /// stands inside a `[..]` block exactly as it stands alone (the document and the record
    /// embed it there), it compiles in the closed world, and its render passes the tripwire.
    ///
    /// The closed world is the containment: its library has no `html` and no `plugin`, and it
    /// holds no file, package or clock, so what any writer's text can produce is typst's own
    /// elements and nothing else. The writer is never asked who it is.
    pub fn checked(text: &str, typeset: &dyn Typeset) -> Result<Self, Unadmitted> {
        parsed(text).map_err(Unadmitted::Typst)?;
        if !embeds(text) {
            return Err(Unadmitted::Unembeddable);
        }
        let document = typeset.closed(text).map_err(Unadmitted::Typst)?;
        if let Some(reason) = suzatary_base::tripwire::suspicious(&document) {
            return Err(Unadmitted::Suspicious(reason));
        }
        Ok(Self(text.to_owned()))
    }

    /// Text that is DATA, never meant as markup (a subject, a quoted line, a sentence the box
    /// composed): every structural character backslashed, so it reads as itself.
    #[must_use]
    pub fn escaped(prose: &str) -> Self {
        Self(
            suzatary_base::escape::escape_markup(prose)
                .as_str()
                .to_owned(),
        )
    }

    /// Markup read back from where it is stored (an object's literal), admitted
    /// by a door when it was written. Never a writer's text: that is what the doors are for.
    #[must_use]
    pub fn from_chain(stored: impl Into<String>) -> Self {
        Self(stored.into())
    }

    /// No markup at all: an absent detail.
    #[must_use]
    pub fn empty() -> Self {
        Self(String::new())
    }

    /// The source, for the printer and for typst.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The source, owned.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for MarkupSource {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Why a writer's markup was not admitted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unadmitted {
    /// Typst's parser or the closed world refused it.
    #[error("{0}")]
    Typst(Rejected),
    /// It parses alone but not inside `[..]`: a line comment at its end would swallow the
    /// bracket that closes it.
    #[error(
        "it does not stand inside a [..] block as it stands alone (does it end in a // comment?)"
    )]
    Unembeddable,
    /// Its render carries what the tripwire refuses.
    #[error("its render carries {0}, which this box never draws")]
    Suspicious(&'static str),
}

impl Unadmitted {
    /// The refusal in words, positions read against the text that was refused.
    #[must_use]
    pub fn said(&self, text: &str) -> String {
        match self {
            Unadmitted::Typst(rejected) => rejected.said(text),
            other => other.to_string(),
        }
    }
}

/// `[text]` parses as exactly one content block holding exactly `text`.
fn embeds(text: &str) -> bool {
    use typst_syntax::SyntaxKind;
    let block = typst_syntax::parse_code(&format!("[{text}]"));
    let mut kids = block.children();
    match (kids.next(), kids.next()) {
        (Some(only), None) => {
            !block.diagnosis().errors
                && only.kind() == SyntaxKind::ContentBlock
                && only
                    .children()
                    .find(|kid| kid.kind() == SyntaxKind::Markup)
                    .is_some_and(|markup| markup.full_text() == text)
        }
        _ => false,
    }
}

/// Typst string source, the inside of a `".."` literal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StringSource(String);

impl StringSource {
    /// Text already in this language: from `suzatary_base::escape` or read back off the chain.
    #[must_use]
    pub fn new(text: impl Into<String>) -> StringSource {
        StringSource(text.into())
    }

    /// The source, for the printer and for typst.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The source, owned.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for StringSource {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Where a [`Note`] may attach: a field of the record, or the block as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoteSite {
    /// The `//` run above the whole block.
    Block,
    /// The run after the last argument: a closure note.
    Tail,
    /// About `id:`.
    Todo,
    /// About `created:`.
    Created,
    /// About `category:`.
    Category,
    /// About `waitingOn:`.
    WaitingOn,
    /// About `source:`.
    Source,
    /// About `detail:`.
    Detail,
    /// About the checklist.
    Subtodos,
}

impl AsRef<str> for NoteSite {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl NoteSite {
    /// Every site, in declared order.
    pub const ALL: [NoteSite; 9] = [
        NoteSite::Block,
        NoteSite::Tail,
        NoteSite::Todo,
        NoteSite::Created,
        NoteSite::Category,
        NoteSite::WaitingOn,
        NoteSite::Source,
        NoteSite::Detail,
        NoteSite::Subtodos,
    ];

    /// The stored spelling a `Note.on` field holds.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NoteSite::Block => "block",
            NoteSite::Tail => "tail",
            NoteSite::Todo => "todo",
            NoteSite::Created => "created",
            NoteSite::Category => "category",
            NoteSite::WaitingOn => "waiting_on",
            NoteSite::Source => "source",
            NoteSite::Detail => "detail",
            NoteSite::Subtodos => "subtodos",
        }
    }

    /// The parse boundary: a stored literal arrives as a bare string.
    pub fn parse(text: &str) -> Result<NoteSite, ProdromeError> {
        NoteSite::ALL
            .into_iter()
            .find(|site| site.as_str() == text)
            .ok_or_else(|| {
                let mut known: Vec<&str> = NoteSite::ALL.iter().map(|site| site.as_str()).collect();
                known.sort_unstable();
                ProdromeError::invalid(format!("Note.on must be one of {known:?}, got {text:?}"))
            })
    }
}

/// The message a todo came from. `date` is a day; the three identifiers are `""` when the
/// writer did not record them.
#[derive(Debug, Clone, PartialEq, Eq, Literal)]
#[literal(crate = prodrome_data)]
pub struct Source {
    /// Who it came from, as the header read.
    #[literal(with = stored_text)]
    pub sender: StringSource,
    /// Its subject.
    #[literal(with = stored_text)]
    pub subject: StringSource,
    /// The day it was sent.
    pub date: Datetime,
    /// The canonical message hash, the one `/api/mail/message/<hash>` resolves.
    #[literal(with = stored_text, default)]
    pub hash: StringSource,
    /// The thread it belongs to, `""` when unrecorded.
    #[literal(with = stored_text, default)]
    pub thread_id: StringSource,
    /// Its `Message-ID`, `""` when unrecorded.
    #[literal(with = stored_text, default)]
    pub message_id: StringSource,
}

/// A comment run attached to one field of a record.
#[derive(Debug, Clone, PartialEq, Eq, Literal)]
#[literal(crate = prodrome_data, new = mk_note)]
pub struct Note {
    /// Which field it is about.
    #[literal(with = codec::text_field)]
    pub on: NoteSite,
    /// Its lines, nonempty by construction.
    pub lines: Vec<String>,
}

/// A checklist item inside a todo: no id, no history of its own.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SubTodo {
    /// What it says.
    pub body: String,
    /// Whether it is ticked.
    pub done: bool,
}

/// The payload: everything Suzatary attaches to a todo, beyond the `todo`, `at` and `actor`
/// the database owns. Stored as `Authored`, not `Record`: the constructor's name is in the bytes
/// of every stored object.
#[derive(Debug, Clone, PartialEq, Literal)]
#[literal(crate = prodrome_data, rename = "Authored", new = mk_record)]
pub struct Record {
    /// Which constructor renders it, one of `decoration::KINDS`.
    #[literal(with = codec::text_field)]
    pub kind: Name,
    /// The day the todo was authored, a block's `created:`.
    pub created: Datetime,
    /// The todo itself, in typst markup source.
    #[literal(with = stored_markup)]
    pub body: MarkupSource,
    /// What it is worth as a function of time: FPL's `Absent` when the chain prices it not at
    /// all, the one spelling of absence. Stored `spec=None`, the bytes every such record already
    /// holds; an explicit `Absent()` is refused at the parse, so the two cannot both exist.
    #[literal(with = stored_spec)]
    pub spec: Term,
    /// Why the price is what it is: the `//` run above `priority:`, the only place that
    /// reasoning exists.
    #[literal(default)]
    pub rationale: Vec<String>,
    /// Which section it renders under, or `None`. The vocabulary is
    /// `instance.toml`'s `[categories]`.
    #[literal(with = codec::blank_field, default)]
    pub category: Option<Name>,
    /// Who or what is blocking it, `""` when nothing is.
    #[literal(with = stored_text, default)]
    pub waiting_on: StringSource,
    /// The long form, in typst markup source.
    #[literal(with = stored_markup, default)]
    pub detail: MarkupSource,
    /// The message it came from, when it came from one.
    #[literal(default)]
    pub source: Option<Source>,
    /// The checklist; its length is read by the database.
    #[literal(with = authored_subtodos, default)]
    pub subtodos: Vec<SubTodo>,
    /// The comment runs a block carries, each on the site it explains.
    #[literal(default)]
    pub notes: Vec<Note>,
    /// A note about this write: why the record was appended.
    #[literal(default)]
    pub note: String,
}

// --- smart constructors ------------------------------------------------------

/// The message a todo came from. Total: every field is opaque text or a day.
#[must_use]
pub fn mk_source(
    sender: &str,
    subject: &str,
    date: Datetime,
    hash: &str,
    thread_id: &str,
    message_id: &str,
) -> Source {
    Source {
        sender: StringSource::new(sender),
        subject: StringSource::new(subject),
        date,
        hash: StringSource::new(hash),
        thread_id: StringSource::new(thread_id),
        message_id: StringSource::new(message_id),
    }
}

/// One comment run. Refuses an unknown site and an empty run.
#[allow(
    clippy::needless_pass_by_value,
    reason = "the derived read passes what it read"
)]
pub(crate) fn mk_note(on: String, lines: Vec<String>) -> Result<Note, ProdromeError> {
    if lines.is_empty() {
        return Err(ProdromeError::invalid(
            "Note.lines must be nonempty — an empty note is not a note",
        ));
    }
    Ok(Note {
        on: NoteSite::parse(&on)?,
        lines,
    })
}

/// One checklist item. Refuses a blank body: a nameless item would still move the parent's
/// price, a number nobody could explain.
pub fn mk_subtodo(body: &str, done: bool) -> Result<SubTodo, ProdromeError> {
    if body.trim().is_empty() {
        return Err(ProdromeError::invalid("SubTodo.body cannot be empty"));
    }
    Ok(SubTodo {
        body: body.to_owned(),
        done,
    })
}

/// Every field of this payload, in declared order. Long on purpose: the field set is frozen,
/// and a builder that let one be forgotten could print a different object under the same name.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mk_record(
    kind: String,
    created: Datetime,
    body: MarkupSource,
    spec: Term,
    rationale: Vec<String>,
    category: String,
    waiting_on: StringSource,
    detail: MarkupSource,
    source: Option<Source>,
    subtodos: Vec<SubTodo>,
    notes: Vec<Note>,
    note: String,
) -> Result<Record, ProdromeError> {
    if body.as_str().trim().is_empty() {
        return Err(ProdromeError::invalid("Authored.body cannot be empty"));
    }
    Ok(Record {
        kind: Name::new("Authored.kind", kind)?,
        created,
        body,
        spec,
        rationale,
        category: Name::optional("Authored.category", category)?,
        waiting_on,
        detail,
        source,
        subtodos,
        notes,
        note,
    })
}

/// Is `spec` FPL's `Absent`, a record that prices its todo not at all?
#[must_use]
pub fn is_absent(spec: &Term) -> bool {
    matches!(spec.out(), TermF::Absent)
}

/// The whole record event, the database's three fields and this payload's twelve: the
/// constructor every content write goes through.
#[allow(clippy::too_many_arguments)]
pub fn mk_authored(
    todo: &str,
    at: Datetime,
    actor: &str,
    kind: &str,
    created: Datetime,
    body: MarkupSource,
    spec: Term,
    rationale: Vec<String>,
    category: &str,
    waiting_on: &str,
    detail: MarkupSource,
    source: Option<Source>,
    subtodos: Vec<SubTodo>,
    notes: Vec<Note>,
    note: &str,
) -> Result<TodoEvent, ProdromeError> {
    prodrome_todo::event::mk_record(
        todo,
        at,
        actor,
        mk_record(
            kind.to_owned(),
            created,
            body,
            spec,
            rationale,
            category.to_owned(),
            StringSource::new(waiting_on),
            detail,
            source,
            subtodos,
            notes,
            note.to_owned(),
        )?,
    )
}

// --- the literal round trip --------------------------------------------------

/// A field of [`StringSource`]: its text.
mod stored_text {
    use super::{Literal as _, ProdromeError, StringSource, Value};

    pub(super) use prodrome_data::literal::codec::text_field::to_value;

    pub(super) fn from_field(
        value: &Value,
        context: &dyn std::fmt::Display,
    ) -> Result<StringSource, ProdromeError> {
        String::from_field(value, context).map(StringSource::new)
    }
}

/// A field of [`MarkupSource`]: its text, read back through the door for stored bytes.
mod stored_markup {
    use super::{Literal as _, MarkupSource, ProdromeError, Value};

    pub(super) use prodrome_data::literal::codec::text_field::to_value;

    pub(super) fn from_field(
        value: &Value,
        context: &dyn std::fmt::Display,
    ) -> Result<MarkupSource, ProdromeError> {
        Ok(MarkupSource::from_chain(String::from_field(
            value, context,
        )?))
    }
}

/// `Authored.spec`: `Absent` is stored `None`, and an `Absent()` written out is refused.
mod stored_spec {
    use super::{Literal as _, ProdromeError, Term, Value, is_absent, mk_absent};

    pub(super) fn to_value(spec: &Term) -> Value {
        if is_absent(spec) {
            Value::None
        } else {
            spec.to_value()
        }
    }

    pub(super) fn from_field(
        value: &Value,
        context: &dyn std::fmt::Display,
    ) -> Result<Term, ProdromeError> {
        match Option::<Term>::from_field(value, context)? {
            None => Ok(mk_absent()),
            Some(term) if is_absent(&term) => Err(ProdromeError::invalid(
                "Authored.spec spells absence `None`, never `Absent()`",
            )),
            Some(term) => Ok(term),
        }
    }
}

/// `Authored.subtodos`: each item through [`mk_subtodo`], which refuses a blank body. An inbox
/// act reads [`SubTodo`]'s own literal, which does not, so the two reads stay apart.
mod authored_subtodos {
    use super::{Literal, ProdromeError, SUBTODO, SubTodo, Value, codec, mk_subtodo};

    pub(super) fn to_value(items: &[SubTodo]) -> Value {
        Value::Tuple(items.iter().map(SubTodo::to_value).collect())
    }

    pub(super) fn from_field(
        value: &Value,
        context: &dyn std::fmt::Display,
    ) -> Result<Vec<SubTodo>, ProdromeError> {
        codec::tuple(value, context)?
            .iter()
            .map(|item| {
                let fields = codec::Fields::of(item, SUBTODO)?;
                mk_subtodo(&fields.get::<String>("body")?, fields.get("done")?)
            })
            .collect()
    }
}

impl Record {
    /// Whether this record is a decision: something waiting on the owner, which belongs to the
    /// Inbox and never to a list of his todos.
    #[must_use]
    pub fn is_decision(&self) -> bool {
        self.kind.as_str() == "decision"
    }

    /// Every field held as markup, by its stored name: what a renderer evaluates in the closed
    /// world. A field added to [`Record`] fails to compile here until it is placed.
    #[must_use]
    pub fn markup(&self) -> [(&'static str, &MarkupSource); 2] {
        let Record {
            kind: _,
            created: _,
            body,
            spec: _,
            rationale: _,
            category: _,
            waiting_on: _,
            detail,
            source: _,
            subtodos: _,
            notes: _,
            note: _,
        } = self;
        [("body", body), ("detail", detail)]
    }
}

impl Payload for Record {
    const VOCABULARY: &'static [Constructor] = &[
        <Source as codec::Record>::CONSTRUCTOR,
        <Note as codec::Record>::CONSTRUCTOR,
        SUBTODO,
    ];

    /// The database prices a record with no spec by its head, so `Absent` is none here.
    fn spec(&self) -> Option<&Term> {
        (!is_absent(&self.spec)).then_some(&self.spec)
    }

    fn checklist_len(&self) -> usize {
        self.subtodos.len()
    }
}

// --- the parameter, applied once ---------------------------------------------

/// A stored content record: the database's three fields around a [`Record`].
pub type Authored = prodrome_todo::event::Authored<Record>;

/// One event of any kind, over this application's payload.
pub type TodoEvent = prodrome_todo::event::TodoEvent<Record>;

/// The label of the `Genesis` a chain's store begins with: a chain is a prodrome of todos.
pub const GENESIS: &str = "todos";

/// The schema a chain is opened at: what every print of one parses at.
pub type TodoVocabulary = prodrome_todo::todo::TodoVocabulary<Record>;

/// One stored object, over the same.
pub type Envelope = prodrome_data::event::Envelope<TodoEvent>;

/// One object as the register folds read it.
pub type Node = prodrome_dag::registers::Node<TodoEvent>;

/// The register fold's state.
pub type Folded = prodrome_dag::registers::Folded<TodoEvent>;

/// The chain on disk, parsed against one closed vocabulary.
pub type EventStore = prodrome_store::store::EventStore<TodoEvent>;

/// One todo as the view reads it at a moment.
pub type Entry = prodrome_reading::view::Entry<TodoEvent>;

/// `SubTodo(body, done)`, a checklist line in a stored act.
// By hand: an inbox act reads it through `Fields`, whose refusals the derive does not write.
pub const SUBTODO: prodrome_data::literal::codec::Constructor = ("SubTodo", &["body", "done"]);

impl prodrome_data::literal::codec::Literal for SubTodo {
    fn to_value(&self) -> Value {
        prodrome_data::literal::codec::record(
            SUBTODO,
            vec![self.body.to_value(), self.done.to_value()],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = prodrome_data::literal::codec::Fields::of(value, SUBTODO)?;
        Ok(SubTodo {
            body: fields.get("body")?,
            done: fields.get("done")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome_todo::payload::record_signature;

    /// The frozen stored shape: `Authored(todo, at, actor, <the twelve>)`, in exactly that order.
    #[test]
    fn the_record_signature_is_the_databases_three_then_this_payloads_twelve() {
        assert_eq!(
            record_signature::<Record>(),
            vec![
                "todo",
                "at",
                "actor",
                "kind",
                "created",
                "body",
                "spec",
                "rationale",
                "category",
                "waiting_on",
                "detail",
                "source",
                "subtodos",
                "notes",
                "note",
            ]
        );
    }

    /// Absence has one spelling: `Absent` in memory, `spec=None` on disk, which every unpriced
    /// record already holds, so no stored byte moves; the database reads it as a record with no
    /// spec, as it always did. `Absent()` written out is refused, so it cannot be a second.
    #[test]
    fn an_unpriced_record_is_absent_in_memory_and_none_on_disk() {
        use prodrome_data::event::{canonical, parse_event};
        use prodrome_todo::payload::Payload as _;

        let at = Datetime::new(2026, 9, 9, 7, 3, 0, 0).expect("a real instant");
        let event = mk_authored(
            "unpriced",
            at,
            "ada",
            "todo",
            at,
            MarkupSource::escaped("body"),
            mk_absent(),
            Vec::new(),
            "",
            "",
            MarkupSource::empty(),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("a record");
        let printed = canonical(&event);
        assert!(printed.contains("spec=None"), "{printed}");
        let back: TodoEvent =
            parse_event(&TodoVocabulary::default(), &printed).expect("it reads back");
        let prodrome_todo::event::TodoEvent::Authored(record) = &back else {
            panic!("an Authored");
        };
        assert!(is_absent(&record.payload.spec));
        assert_eq!(record.payload.spec(), None);
        assert_eq!(back, event);

        let absent = prodrome_data::literal::print_literal(&mk_absent().to_value());
        let spelled = printed.replace("spec=None", &format!("spec={absent}"));
        assert!(parse_event::<TodoEvent>(&TodoVocabulary::default(), &printed).is_ok());
        assert!(
            parse_event::<TodoEvent>(&TodoVocabulary::default(), &spelled).is_err(),
            "{spelled}"
        );
    }

    /// Every constructor this crate stores, with its fields in print order.
    #[test]
    fn the_stored_names_are_pinned() {
        use crate::todo::price;
        let mut stored = vec![<Record as codec::Record>::CONSTRUCTOR];
        stored.extend(Record::VOCABULARY);
        stored.extend([
            <Source as codec::Record>::CONSTRUCTOR,
            SUBTODO,
            price::FLAT_PRICE,
            price::DEADLINE_PRICE,
            price::RECUR_PRICE,
            price::PERIODIC_PRICE,
        ]);
        prodrome_data::literal::codec::assert_pinned(
            &stored,
            "
            Authored(kind, created, body, spec, rationale, category, waiting_on, detail, source, subtodos, notes, note)
            Source(sender, subject, date, hash, thread_id, message_id)
            Note(on, lines)
            SubTodo(body, done)
            Source(sender, subject, date, hash, thread_id, message_id)
            SubTodo(body, done)
            FlatPrice(percent)
            DeadlinePrice(end_date, start_percent, end_percent, lead_up_days, start_date)
            RecurPrice(days, from_percent, to_percent)
            PeriodicPrice(days, from_percent, to_percent)
            ",
        );
    }

    /// The parser's whitelist is the database's names union this payload's, so a reused name
    /// would be unreachable rather than in force.
    #[test]
    fn the_payloads_names_do_not_collide_with_the_databases() {
        let core: Vec<&str> = prodrome_term::term::schema::signatures()
            .names()
            .chain(
                prodrome_data::event::ENVELOPE_SIGNATURES
                    .iter()
                    .chain(prodrome_todo::todo::TODO_SIGNATURES)
                    .map(|(name, _)| *name),
            )
            .collect();
        for (name, _) in Record::VOCABULARY {
            assert!(
                !core.contains(name),
                "{name} is one of the database's names"
            );
        }
        assert!(!core.contains(&<Record as codec::Record>::CONSTRUCTOR.0));
    }
}
