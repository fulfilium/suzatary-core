//! Suzatary's record: what this application attaches to a todo beyond the database's `todo`,
//! `at` and `actor`, a decoration of a fulfillment curve. The database reads only two things
//! from it: the spec it prices by and how many checklist items it holds.
//!
//! The stored bytes are frozen here: [`Record::KIND`] and [`Record::FIELDS`] are what an
//! object's name is a hash of, so a field added to a shipped constructor would rename every
//! stored object. Evolution is a new constructor, never a new field; `app/cli/tests/live.rs`
//! is the guard.

use prodrome_core::event::Name;
use prodrome_core::fpl::mk_absent;
use prodrome_core::literal::{Call, Datetime, ProdromeError, Value};
use prodrome_core::payload::{
    Payload, bool_field, datetime_field, expect_call, spec_field, string_field, string_or_empty,
    strings, tuple_field, tuple_or_empty,
};
use prodrome_core::term::{Term, TermF};

use crate::capability::{Rejected, Typeset, parsed};

// Not the Prodrome's `newtype_str!`: its generated methods carry no docs, and
// `missing_docs` cannot be allowed from outside a `macro_rules!` item.

/// Typst markup source, the inside of a `[..]` block; a separate type from [`StringSource`]
/// because `]` is structure in one and `"` in the other.
///
/// Its constructors are its doors: [`MarkupSource::checked`] for a writer's markup, whoever
/// the writer, [`MarkupSource::escaped`] for text that is data, and
/// [`MarkupSource::from_chain`] for bytes already admitted when they were stored.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        if let Some(reason) = crate::tripwire::suspicious(&document) {
            return Err(Unadmitted::Suspicious(reason));
        }
        Ok(Self(text.to_owned()))
    }

    /// Text that is DATA, never meant as markup (a subject, a quoted line, a sentence the box
    /// composed): every structural character backslashed, so it reads as itself.
    #[must_use]
    pub fn escaped(prose: &str) -> Self {
        Self(crate::escape::escape_markup(prose).as_str().to_owned())
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StringSource(String);

impl StringSource {
    /// Text already in this language: from `crate::escape` or read back off the chain.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Who it came from, as the header read.
    pub sender: StringSource,
    /// Its subject.
    pub subject: StringSource,
    /// The day it was sent.
    pub date: Datetime,
    /// The canonical message hash, the one `/api/mail/message/<hash>` resolves.
    pub hash: StringSource,
    /// The thread it belongs to, `""` when unrecorded.
    pub thread_id: StringSource,
    /// Its `Message-ID`, `""` when unrecorded.
    pub message_id: StringSource,
}

/// A comment run attached to one field of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Which field it is about.
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
/// the database owns.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// Which constructor renders it, one of `decoration::KINDS`.
    pub kind: Name,
    /// The day the todo was authored, a block's `created:`.
    pub created: Datetime,
    /// The todo itself, in typst markup source.
    pub body: MarkupSource,
    /// What it is worth as a function of time: FPL's `Absent` when the chain prices it not at
    /// all, the one spelling of absence. Stored `spec=None`, the bytes every such record already
    /// holds; an explicit `Absent()` is refused at the parse, so the two cannot both exist.
    pub spec: Term,
    /// Why the price is what it is: the `//` run above `priority:`, the only place that
    /// reasoning exists.
    pub rationale: Vec<String>,
    /// Which section it renders under, or `None`. The vocabulary is
    /// `instance.toml`'s `[categories]`.
    pub category: Option<Name>,
    /// Who or what is blocking it, `""` when nothing is.
    pub waiting_on: StringSource,
    /// The long form, in typst markup source.
    pub detail: MarkupSource,
    /// The message it came from, when it came from one.
    pub source: Option<Source>,
    /// The checklist; its length is read by the database.
    pub subtodos: Vec<SubTodo>,
    /// The comment runs a block carries, each on the site it explains.
    pub notes: Vec<Note>,
    /// A note about this write: why the record was appended.
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
pub fn mk_note(on: &str, lines: Vec<String>) -> Result<Note, ProdromeError> {
    if lines.is_empty() {
        return Err(ProdromeError::invalid(
            "Note.lines must be nonempty — an empty note is not a note",
        ));
    }
    Ok(Note {
        on: NoteSite::parse(on)?,
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
pub fn mk_record(
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
        waiting_on: StringSource::new(waiting_on),
        detail,
        source,
        subtodos,
        notes,
        note: note.to_owned(),
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
    prodrome_core::event::mk_record(
        todo,
        at,
        actor,
        mk_record(
            kind, created, body, spec, rationale, category, waiting_on, detail, source, subtodos,
            notes, note,
        )?,
    )
}

// --- the literal round trip --------------------------------------------------

fn text(value: impl Into<String>) -> Value {
    Value::Str(value.into())
}

fn optional_text(value: Option<&Name>) -> Value {
    Value::Str(
        value
            .map(|name| name.as_str().to_owned())
            .unwrap_or_default(),
    )
}

fn tuple_of<T>(items: &[T], each: impl Fn(&T) -> Value) -> Value {
    Value::Tuple(items.iter().map(each).collect())
}

fn field(name: &str, value: Value) -> (String, Value) {
    (name.to_owned(), value)
}

impl Source {
    /// `Source(sender, subject, date, hash, thread_id, message_id)`, in `VOCABULARY`'s order.
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::call(
            "Source",
            vec![
                field("sender", text(self.sender.as_str())),
                field("subject", text(self.subject.as_str())),
                field("date", Value::Datetime(self.date)),
                field("hash", text(self.hash.as_str())),
                field("thread_id", text(self.thread_id.as_str())),
                field("message_id", text(self.message_id.as_str())),
            ],
        )
    }
}

impl Note {
    /// `Note(on, lines)`.
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::call(
            "Note",
            vec![
                field("on", text(self.on.as_str())),
                field("lines", tuple_of(&self.lines, |line| text(line.clone()))),
            ],
        )
    }
}

impl SubTodo {
    /// `SubTodo(body, done)`.
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::call(
            "SubTodo",
            vec![
                field("body", text(self.body.clone())),
                field("done", Value::Bool(self.done)),
            ],
        )
    }
}

fn source_from_value(value: &Value) -> Result<Source, ProdromeError> {
    let call = expect_call(value, "Source")?;
    Ok(mk_source(
        &string_field(call, "sender")?,
        &string_field(call, "subject")?,
        datetime_field(call, "date")?,
        &string_or_empty(call, "hash")?,
        &string_or_empty(call, "thread_id")?,
        &string_or_empty(call, "message_id")?,
    ))
}

fn note_from_value(value: &Value) -> Result<Note, ProdromeError> {
    let call = expect_call(value, "Note")?;
    mk_note(
        &string_field(call, "on")?,
        strings(tuple_field(call, "lines")?, "Note.lines")?,
    )
}

fn subtodo_from_value(value: &Value) -> Result<SubTodo, ProdromeError> {
    let call = expect_call(value, "SubTodo")?;
    mk_subtodo(&string_field(call, "body")?, bool_field(call, "done")?)
}

impl Record {
    /// Whether this record is a decision: something waiting on the owner, which belongs to the
    /// Inbox and never to a list of his todos.
    #[must_use]
    pub fn is_decision(&self) -> bool {
        self.kind.as_str() == "decision"
    }
}

impl Payload for Record {
    /// `Authored`, not `Record`: the constructor's name is in the bytes of every stored object.
    const KIND: &'static str = "Authored";

    const FIELDS: &'static [&'static str] = &[
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
    ];

    const VOCABULARY: &'static [(&'static str, &'static [&'static str])] = &[
        (
            "Source",
            &[
                "sender",
                "subject",
                "date",
                "hash",
                "thread_id",
                "message_id",
            ],
        ),
        ("Note", &["on", "lines"]),
        ("SubTodo", &["body", "done"]),
    ];

    fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("kind", text(self.kind.as_str())),
            ("created", Value::Datetime(self.created)),
            ("body", text(self.body.as_str())),
            (
                "spec",
                if is_absent(&self.spec) {
                    Value::None
                } else {
                    self.spec.to_value()
                },
            ),
            (
                "rationale",
                tuple_of(&self.rationale, |line| text(line.clone())),
            ),
            ("category", optional_text(self.category.as_ref())),
            ("waiting_on", text(self.waiting_on.as_str())),
            ("detail", text(self.detail.as_str())),
            (
                "source",
                self.source.as_ref().map_or(Value::None, Source::to_value),
            ),
            ("subtodos", tuple_of(&self.subtodos, SubTodo::to_value)),
            ("notes", tuple_of(&self.notes, Note::to_value)),
            ("note", text(self.note.clone())),
        ]
    }

    fn from_fields(call: &Call) -> Result<Record, ProdromeError> {
        mk_record(
            &string_field(call, "kind")?,
            datetime_field(call, "created")?,
            MarkupSource::from_chain(string_field(call, "body")?),
            match spec_field(call, "spec")? {
                None => mk_absent(),
                Some(term) if is_absent(&term) => {
                    return Err(ProdromeError::invalid(
                        "Authored.spec spells absence `None`, never `Absent()`",
                    ));
                }
                Some(term) => term,
            },
            strings(tuple_or_empty(call, "rationale")?, "Authored.rationale")?,
            &string_or_empty(call, "category")?,
            &string_or_empty(call, "waiting_on")?,
            MarkupSource::from_chain(string_or_empty(call, "detail")?),
            match call.field("source") {
                None | Some(Value::None) => None,
                Some(other) => Some(source_from_value(other)?),
            },
            tuple_or_empty(call, "subtodos")?
                .iter()
                .map(subtodo_from_value)
                .collect::<Result<Vec<_>, _>>()?,
            tuple_or_empty(call, "notes")?
                .iter()
                .map(note_from_value)
                .collect::<Result<Vec<_>, _>>()?,
            &string_or_empty(call, "note")?,
        )
    }

    /// The database prices a record with no spec by its head, so `Absent` is none here.
    fn spec(&self) -> Option<&Term> {
        (!is_absent(&self.spec)).then_some(&self.spec)
    }

    fn checklist_len(&self) -> usize {
        self.subtodos.len()
    }
}

// --- the parameter, applied once ---------------------------------------------
//
// The application has one record shape, so these aliases apply it once, under the
// database's own names: the import line is where a reader sees which payload a type is over.

/// A stored content record: the database's three fields around a [`Record`].
pub type Authored = prodrome_core::event::Authored<Record>;

/// One event of any kind, over this application's payload.
pub type TodoEvent = prodrome_core::event::TodoEvent<Record>;

/// The label of the `Genesis` a chain's store begins with: a chain is a prodrome of todos.
pub const GENESIS: &str = "todos";

/// The schema a chain is opened at: what every print of one parses at.
pub type TodoVocabulary = prodrome_core::todo::TodoVocabulary<Record>;

/// One stored object, over the same.
pub type Envelope = prodrome_core::event::Envelope<TodoEvent>;

/// One object as the register folds read it.
pub type Node = prodrome_core::registers::Node<TodoEvent>;

/// The register fold's state.
pub type Folded = prodrome_core::registers::Folded<TodoEvent>;

/// The chain on disk, parsed against one closed vocabulary.
pub type EventStore = prodrome_core::store::EventStore<TodoEvent>;

/// One todo as the view reads it at a moment.
pub type Entry = prodrome_core::view::Entry<TodoEvent>;

/// `SubTodo(body, done)`, a checklist line in a stored act.
pub const SUBTODO: prodrome_core::literal::codec::Constructor = ("SubTodo", &["body", "done"]);

impl prodrome_core::literal::codec::Literal for SubTodo {
    fn to_value(&self) -> Value {
        prodrome_core::literal::codec::record(
            SUBTODO,
            vec![self.body.to_value(), self.done.to_value()],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = prodrome_core::literal::codec::Fields::of(value, SUBTODO)?;
        Ok(SubTodo {
            body: fields.get("body")?,
            done: fields.get("done")?,
        })
    }
}

/// `Source(sender, subject, date, hash, thread_id, message_id)`, the message a stored act names.
pub const SOURCE: prodrome_core::literal::codec::Constructor = (
    "Source",
    &[
        "sender",
        "subject",
        "date",
        "hash",
        "thread_id",
        "message_id",
    ],
);

impl prodrome_core::literal::codec::Literal for Source {
    fn to_value(&self) -> Value {
        Source::to_value(self)
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        source_from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome_core::payload::record_signature;

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
        use prodrome_core::event::{canonical, parse_event};
        use prodrome_core::payload::Payload as _;

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
        let prodrome_core::event::TodoEvent::Authored(record) = &back else {
            panic!("an Authored");
        };
        assert!(is_absent(&record.payload.spec));
        assert_eq!(record.payload.spec(), None);
        assert_eq!(back, event);

        let absent = prodrome_core::literal::print_literal(&mk_absent().to_value());
        let spelled = printed.replace("spec=None", &format!("spec={absent}"));
        assert!(parse_event::<TodoEvent>(&TodoVocabulary::default(), &printed).is_ok());
        assert!(
            parse_event::<TodoEvent>(&TodoVocabulary::default(), &spelled).is_err(),
            "{spelled}"
        );
    }

    #[test]
    fn fields_answers_with_fields_in_order() {
        let at = Datetime::new(2026, 9, 9, 7, 3, 0, 0).expect("a real instant");
        let record = mk_record(
            "todo",
            at,
            MarkupSource::escaped("body"),
            prodrome_core::fpl::mk_absent(),
            vec![],
            "",
            "",
            MarkupSource::empty(),
            None,
            vec![],
            vec![],
            "",
        )
        .expect("valid");
        let named: Vec<&str> = record.fields().into_iter().map(|(name, _)| name).collect();
        assert_eq!(named, Record::FIELDS);
    }

    /// The parser's whitelist is the database's names union this payload's, so a reused name
    /// would be unreachable rather than in force.
    #[test]
    fn the_payloads_names_do_not_collide_with_the_databases() {
        let core: Vec<&str> = prodrome_core::term::schema::signatures()
            .names()
            .chain(
                prodrome_core::event::ENVELOPE_SIGNATURES
                    .iter()
                    .chain(prodrome_core::todo::TODO_SIGNATURES)
                    .map(|(name, _)| *name),
            )
            .collect();
        for (name, _) in Record::VOCABULARY {
            assert!(
                !core.contains(name),
                "{name} is one of the database's names"
            );
        }
        assert!(!core.contains(&Record::KIND));
    }
}
