//! The conversations' schema since their words were locked: one line of one conversation, its
//! shape in the clear and each slot of its words locked under a key of its own.
//!
//! A line is `Kind(conversation, at, actor, held, carried, <shape fields>, words)`: the head every
//! line has, the shape of its kind ([`LineShape`]), and `words`, one `Scrubbed(key, box, rules)`
//! per slot: the key's name, the ciphertext in base64, and the redaction rules the words passed
//! ([`ScrubbedUnder`]). A slot locked before lines recorded their rules is `Locked(key, box)`.
//! Each slot's associated data is the line's own print without its words, with the slot's number
//! and its rules (`LineSlot`), so a ciphertext moved to another slot, line or store does not
//! open, and a slot cannot claim other rules. Every line writes its conversation's one register,
//! the transcript, so a line rests on the lines its writer held and the lines are causally
//! ordered. Nothing in an object is words: the redaction door is applied to the words in `seal`,
//! before they are locked. Opening a slot locked under the current rules only parses it; a slot
//! locked under any other rules is scrubbed again as it opens (`SealedSlot::open`).

use std::collections::BTreeSet;

use base64::Engine as _;
use prodrome_core::event::{Actor, Hash};
use prodrome_core::literal::{Datetime, ProdromeError, Table, Value, Vocabulary, print_literal};
use prodrome_core::schema::Schema;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::capability::Entropy;
use crate::redact::Rules;
use crate::secretary::carried::Carried;
use crate::secretary::erasure::Erasure;
use crate::secretary::line_shape::{self, LineShape};
use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::{FreshKey, KeyName, Locked, Shape, TextKey};
use crate::secretary::scrubbed_under::ScrubbedUnder;
use crate::secretary::slots::Slots;
use crate::secretary::store::{ConversationId, Entry};
use crate::secretary::words::{Gone, Words};
use prodrome_core::fold::sole::{One, Only, Whole};
use prodrome_core::literal::codec::{Constructor, Fields, Literal, no_arm, record};

/// One line: which conversation, when, who wrote it, how many lines its writer held, where it
/// came from, its shape, and its words locked.
///
/// `held` tells a line said twice from a line written twice: the Prodrome writes nothing for an
/// event it already holds, and the same words said twice in one second are two lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEvent {
    conversation: ConversationId,
    at: Datetime,
    actor: Actor,
    held: u32,
    carried: Carried,
    shape: LineShape,
    words: Vec<SealedSlot>,
}

/// One slot's words as stored: locked, and the rules they passed before they were locked.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SealedSlot {
    locked: Locked<LineSlot>,
    under: ScrubbedUnder,
}

impl SealedSlot {
    /// The words, opened by `key` for `binding`: as locked when they passed the current rules,
    /// and scrubbed under the current rules now when they passed any other, so no reader sees
    /// what the current rules catch; withheld when they no longer pass the door at all. `None`
    /// for a key that does not open them. A reader that keeps what it opens
    /// (`secretary::opened`, the tab's held lines) scrubs a stale slot once, and no reader opens
    /// a seal a reseal supersedes ([`crate::secretary::transcript::Through`]).
    fn open(&self, binding: &LineSlot, key: &TextKey) -> Option<Words<LineWords>> {
        let words = self.locked.open(binding, key)?;
        Some(if self.under.is_current(()) {
            Words::kept(words)
        } else {
            words
                .scrubbed()
                .map_or_else(|_| Gone::Withheld.into(), Words::kept)
        })
    }
}

/// One slot of one line, as its words are bound: the line's print without its words, the slot's
/// number, and the rules the slot's words passed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LineSlot(Vec<u8>);

impl Shape for LineSlot {
    type Words = LineWords;
    const DOMAIN: &'static [u8] = b"suzatary lines v1";
    fn print(&self) -> Vec<u8> {
        self.0.clone()
    }
}

/// An entry that cannot be a stored line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotALine {
    /// Some of its words are already gone: a line is written with its words.
    #[error("a line is written with its words, and some of these are gone")]
    Gone,
    /// Its words do not pass the redaction door whole.
    #[error("its words do not pass the redaction door: {0}")]
    Unscrubbable(String),
    /// The generator gave no key.
    #[error("no key for its words: {0}")]
    NoKey(String),
}

impl LineEvent {
    /// Line `entry` of `conversation`, written here by `actor` holding `held` lines, each slot of
    /// its words through the redaction door and locked under a key drawn from `entropy`; the keys
    /// that open them are the only thing to keep.
    ///
    /// # Errors
    /// [`NotALine`] for words already gone, words the door refuses, or no key.
    pub fn write(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Entry,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let at = entry.at();
        let (shape, words) = line_shape::split(entry);
        let words = words.ok_or(NotALine::Gone)?;
        let mut line = Self {
            conversation,
            at,
            actor,
            held,
            carried: Carried::Written,
            shape,
            words: Vec::new(),
        };
        let mut keys = Vec::with_capacity(words.len());
        for (slot, words) in words.iter().enumerate() {
            let (sealed, opens) = seal(&line, slot, words, entropy)?;
            line.words.push(sealed);
            keys.push(opens);
        }
        Ok((line, keys))
    }

    /// The words of slot `of` of a line of `conversation`, sealed again from the seal locked
    /// under `from`: `words` through the redaction door and locked under a key drawn from
    /// `entropy`, as a line written here by `actor` holding `held` lines at `at`; answers the key
    /// that opens it.
    ///
    /// # Errors
    /// [`NotALine`] for words the door refuses, or no key.
    #[expect(
        clippy::too_many_arguments,
        reason = "a line's head, the slot, and its words"
    )]
    pub fn reseal(
        conversation: ConversationId,
        at: Datetime,
        actor: Actor,
        held: u32,
        of: KeyName,
        from: KeyName,
        words: &LineWords,
        entropy: &dyn Entropy,
    ) -> Result<(Self, TextKey), NotALine> {
        let mut line = Self {
            conversation,
            at,
            actor,
            held,
            carried: Carried::Written,
            shape: LineShape::Resealed { of, from },
            words: Vec::new(),
        };
        let (sealed, opens) = seal(&line, 0, words, entropy)?;
        line.words.push(sealed);
        Ok((line, opens))
    }

    /// Line `entry` locked as scrubbed `under` rules other than the current ones, its words not
    /// scrubbed: a line scrubbed under rules that missed what is in it, for the tests of reading
    /// and of the migration, and the fixture's stale store.
    ///
    /// # Errors
    /// [`NotALine`] for words already gone, or no key.
    #[cfg(any(test, feature = "nothing", feature = "fixture"))]
    pub fn written_under(
        under: &ScrubbedUnder,
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Entry,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let (shape, words) = line_shape::split(entry);
        let words = words.ok_or(NotALine::Gone)?;
        let mut line = Self {
            conversation,
            at: entry.at(),
            actor,
            held,
            carried: Carried::Written,
            shape,
            words: Vec::new(),
        };
        let mut keys = Vec::with_capacity(words.len());
        for (slot, words) in words.iter().enumerate() {
            let key = FreshKey::draw(entropy).map_err(NotALine::NoKey)?;
            let (locked, opens) = Locked::lock(&line.slot(slot, under), words, key);
            line.words.push(SealedSlot {
                locked,
                under: under.clone(),
            });
            keys.push(opens);
        }
        Ok((line, keys))
    }

    /// An erasure of `keys` in `conversation`, at `at`, written by `actor` holding `held` lines.
    #[must_use]
    pub fn erasure(
        conversation: ConversationId,
        at: Datetime,
        actor: Actor,
        held: u32,
        keys: BTreeSet<KeyName>,
        erasure: Erasure,
    ) -> Self {
        Self {
            conversation,
            at,
            actor,
            held,
            carried: Carried::Written,
            shape: LineShape::Erased { keys, erasure },
            words: Vec::new(),
        }
    }

    /// The conversation.
    #[must_use]
    pub fn conversation(&self) -> &ConversationId {
        &self.conversation
    }

    /// When it was written, by its writer's clock.
    #[must_use]
    pub fn at(&self) -> Datetime {
        self.at
    }

    /// How many lines its writer held.
    #[must_use]
    pub fn held(&self) -> u32 {
        self.held
    }

    /// Where it came from.
    #[must_use]
    pub fn carried(&self) -> &Carried {
        &self.carried
    }

    /// What the line stands for, read from the store as `name`: the object it carries, or
    /// itself.
    #[must_use]
    pub fn identity<'a>(&'a self, name: &'a Hash) -> &'a Hash {
        match &self.carried {
            Carried::From(old) => old,
            Carried::Written => name,
        }
    }

    /// Its shape.
    #[must_use]
    pub fn shape(&self) -> &LineShape {
        &self.shape
    }

    /// The name of each slot's key, in slot order.
    pub fn keys(&self) -> impl Iterator<Item = &KeyName> {
        self.words.iter().map(|sealed| sealed.locked.key())
    }

    /// The rules each slot's words passed, in slot order.
    pub fn scrubbed_under(&self) -> impl Iterator<Item = &ScrubbedUnder> {
        self.words.iter().map(|sealed| &sealed.under)
    }

    /// Slot `slot`'s words, opened by `key`; `None` for a slot the line does not hold or a key
    /// that does not open it.
    #[must_use]
    pub fn open(&self, slot: usize, key: &TextKey) -> Option<LineWords> {
        self.read(slot, key).and_then(Words::into_kept)
    }

    /// Slot `slot`'s words, opened by `key` and read under the current rules (`SealedSlot`'s
    /// door): kept, or withheld when they no longer pass it; `None` for a slot the line does not
    /// hold or a key that does not open it.
    #[must_use]
    pub fn read(&self, slot: usize, key: &TextKey) -> Option<Words<LineWords>> {
        let sealed = self.words.get(slot)?;
        sealed.open(&self.slot(slot, &sealed.under), key)
    }

    /// Slot `slot`'s words as they were locked, under whatever rules: only for sealing them again
    /// through the door ([`LineEvent::reseal`]), never for a reader.
    pub(crate) fn unscrubbed(&self, slot: usize, key: &TextKey) -> Option<LineWords> {
        let sealed = self.words.get(slot)?;
        sealed.locked.open(&self.slot(slot, &sealed.under), key)
    }

    /// Slot `slot`'s words as `slots` reads them by its key's name, opened with this line's
    /// binding for that slot; missing for a slot the line does not hold.
    #[must_use]
    pub fn slot_words(&self, slot: usize, slots: &impl Slots) -> Words<LineWords> {
        let Some(sealed) = self.words.get(slot) else {
            return Gone::Missing.into();
        };
        let binding = self.slot(slot, &sealed.under);
        slots.words(
            sealed.locked.key(),
            &binding.0,
            sealed.locked.sealed(),
            &|key| sealed.open(&binding, key),
        )
    }

    /// The line as read: each slot read by `slots` by its key's name, opened with this line's
    /// binding for that slot.
    #[must_use]
    pub fn entry(&self, slots: &impl Slots) -> Entry {
        // A reseal's words are read where the slot it seals stands, so it opens nothing here.
        let words = if matches!(self.shape, LineShape::Resealed { .. }) {
            Vec::new()
        } else {
            (0..self.words.len())
                .map(|slot| self.slot_words(slot, slots))
                .collect()
        };
        line_shape::join(&self.conversation, self.shape.clone(), self.at, words)
    }

    /// Slot `slot`'s binding: this line's print without its words, the slot's number, and the
    /// rules its words passed, which an unversioned slot's binding does not name.
    fn slot(&self, slot: usize, under: &ScrubbedUnder) -> LineSlot {
        let ((name, fields), mut values) = self.bare();
        let rules: &[&str] = match under {
            ScrubbedUnder::Version(rules) => {
                values.extend([slot.to_value(), rules.to_value()]);
                &["slot", "rules"]
            }
            ScrubbedUnder::Unversioned => {
                values.push(slot.to_value());
                &["slot"]
            }
        };
        // The binding is the bare line with `slot` in place of `words`: no stored constructor.
        let named: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|field| *field != "words")
            .chain(rules.iter().copied())
            .collect();
        LineSlot(
            print_literal(&Value::call(
                name,
                named.into_iter().map(str::to_owned).zip(values).collect(),
            ))
            .into_bytes(),
        )
    }

    fn head(&self) -> Vec<Value> {
        vec![
            self.conversation.to_value(),
            self.at.to_value(),
            self.actor.to_value(),
            self.held.to_value(),
            self.carried.to_value(),
        ]
    }

    /// The line's print without its words: its head and its shape, under its signature.
    fn bare(&self) -> (Constructor, Vec<Value>) {
        let mut values = self.head();
        values.extend(shape_fields(&self.shape));
        (signature(&self.shape), values)
    }
}

/// Slot `slot` of `line`: `words` through the redaction door, then locked under a key drawn from
/// `entropy`, recording the rules they passed. The one place a line's words are scrubbed.
fn seal(
    line: &LineEvent,
    slot: usize,
    words: &LineWords,
    entropy: &dyn Entropy,
) -> Result<(SealedSlot, TextKey), NotALine> {
    let words = words
        .scrubbed()
        .map_err(|failed| NotALine::Unscrubbable(failed.to_string()))?;
    let under = ScrubbedUnder::Version(Rules::current(()).clone());
    let key = FreshKey::draw(entropy).map_err(NotALine::NoKey)?;
    let (locked, opens) = Locked::lock(&line.slot(slot, &under), &words, key);
    Ok((SealedSlot { locked, under }, opens))
}

/// The fields a shape prints after the head, in signature order.
fn shape_fields(shape: &LineShape) -> Vec<Value> {
    match shape {
        LineShape::Opened { model, kind } => vec![model.to_value(), kind.to_value()],
        LineShape::Titled { by } => vec![json(by)],
        LineShape::Said { turn, by } => vec![json(turn), by.to_value()],
        LineShape::Proposed { object, revises } => vec![object.to_value(), revises.to_value()],
        LineShape::Reviewed {
            chain,
            batch,
            decision,
        } => vec![
            chain.to_value(),
            batch.to_value(),
            decision.word().to_owned().to_value(),
            decision.objects().to_vec().to_value(),
        ],
        LineShape::ReviewFailed { standing } => vec![standing.to_value()],
        LineShape::Stopped { by } => vec![by.to_value()],
        LineShape::Rewound { to, by } => vec![to.to_value(), by.to_value()],
        LineShape::MemorySeen { version } => vec![version.to_value()],
        LineShape::Session {
            session,
            model,
            cwd,
            surface,
        } => vec![
            session.to_value(),
            model.to_value(),
            cwd.to_value(),
            surface.to_value(),
        ],
        LineShape::Unheard { session, model } => vec![session.to_value(), model.to_value()],
        LineShape::Kept {
            session,
            model,
            cwd,
            surface,
            from,
        } => vec![
            session.to_value(),
            model.to_value(),
            cwd.to_value(),
            surface.to_value(),
            from.to_value(),
        ],
        LineShape::Scoped { scope } => vec![scope.to_value()],
        LineShape::Coding { proposal, session } => vec![proposal.to_value(), json(session)],
        LineShape::Erased { keys, erasure } => vec![keys.to_value(), json(erasure)],
        LineShape::Resealed { of, from } => vec![of.to_value(), from.to_value()],
    }
}

/// The shape a stored line spells.
fn shape_of(fields: Fields<'_>) -> Result<LineShape, ProdromeError> {
    Ok(match fields.name() {
        "Opened" => LineShape::Opened {
            model: fields.get("model")?,
            kind: fields.get("kind")?,
        },
        "Titled" => LineShape::Titled {
            by: parsed(fields, "by")?,
        },
        "Said" => LineShape::Said {
            turn: parsed(fields, "turn")?,
            by: fields.get("by")?,
        },
        "Proposed" => LineShape::Proposed {
            object: fields.get("object")?,
            revises: fields.get("revises")?,
        },
        "Reviewed" => LineShape::Reviewed {
            chain: fields.get("chain")?,
            batch: fields.get("batch")?,
            decision: crate::secretary::store::Decision::read(
                &fields.get::<String>("decision")?,
                &fields.get::<Vec<String>>("objects")?,
            )?,
        },
        "ReviewFailed" => LineShape::ReviewFailed {
            standing: fields.get("standing")?,
        },
        "Stopped" => LineShape::Stopped {
            by: fields.get("by")?,
        },
        "Rewound" => LineShape::Rewound {
            to: fields.get("to")?,
            by: fields.get("by")?,
        },
        "MemorySeen" => LineShape::MemorySeen {
            version: fields.get("version")?,
        },
        "Session" => LineShape::Session {
            session: fields.get("session")?,
            model: fields.get("model")?,
            cwd: fields.get("cwd")?,
            surface: fields.get("surface")?,
        },
        "Unheard" => LineShape::Unheard {
            session: fields.get("session")?,
            model: fields.get("model")?,
        },
        "Kept" => LineShape::Kept {
            session: fields.get("session")?,
            model: fields.get("model")?,
            cwd: fields.get("cwd")?,
            surface: fields.get("surface")?,
            from: fields.get("from")?,
        },
        "Scoped" => LineShape::Scoped {
            scope: fields.get("scope")?,
        },
        "Coding" => LineShape::Coding {
            proposal: fields.get("proposal")?,
            session: parsed(fields, "session")?,
        },
        "Erased" => LineShape::Erased {
            keys: fields.get("keys")?,
            erasure: parsed(fields, "erasure")?,
        },
        "Resealed" => LineShape::Resealed {
            of: fields.get("of")?,
            from: fields.get("from")?,
        },
        other => return Err(no_arm("a conversation line", other)),
    })
}

/// The signature a line of `shape` is stored under.
fn signature(shape: &LineShape) -> Constructor {
    match shape {
        LineShape::Opened { .. } => OPENED,
        LineShape::Titled { .. } => TITLED,
        LineShape::Said { .. } => SAID,
        LineShape::Proposed { .. } => PROPOSED,
        LineShape::Reviewed { .. } => REVIEWED,
        LineShape::ReviewFailed { .. } => REVIEW_FAILED,
        LineShape::Stopped { .. } => STOPPED,
        LineShape::Rewound { .. } => REWOUND,
        LineShape::MemorySeen { .. } => MEMORY_SEEN,
        LineShape::Session { .. } => SESSION,
        LineShape::Unheard { .. } => UNHEARD,
        LineShape::Kept { .. } => KEPT,
        LineShape::Scoped { .. } => SCOPED,
        LineShape::Coding { .. } => CODING,
        LineShape::Erased { .. } => ERASED,
        LineShape::Resealed { .. } => RESEALED,
    }
}

/// `Scrubbed(key, box, rules)`: a slot's words as stored, the key's name, the ciphertext in
/// base64, and the rules the words passed.
const SCRUBBED: Constructor = ("Scrubbed", &["key", "box", "rules"]);

/// `Locked(key, box)`: a slot locked before lines recorded their rules.
const LOCKED: Constructor = ("Locked", &["key", "box"]);

/// A slot's words as stored.
impl Literal for SealedSlot {
    fn to_value(&self) -> Value {
        let key = self.locked.key().to_value();
        let sealed = base64::engine::general_purpose::STANDARD
            .encode(self.locked.sealed())
            .to_value();
        match &self.under {
            ScrubbedUnder::Version(rules) => record(SCRUBBED, vec![key, sealed, rules.to_value()]),
            ScrubbedUnder::Unversioned => record(LOCKED, vec![key, sealed]),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, &[SCRUBBED, LOCKED])?;
        let sealed = base64::engine::general_purpose::STANDARD
            .decode(fields.get::<String>("box")?)
            .map_err(|failed| ProdromeError::invalid(format!("{}.box: {failed}", fields.name())))?;
        let under = if fields.name() == SCRUBBED.0 {
            ScrubbedUnder::Version(fields.get("rules")?)
        } else {
            ScrubbedUnder::Unversioned
        };
        Ok(Self {
            locked: Locked::read(fields.get("key")?, sealed),
            under,
        })
    }
}

/// Whether a line of `signature` carries `words`: every kind that may hold any.
fn has_words(signature: Constructor) -> bool {
    signature.1.contains(&"words")
}

/// `Opened(conversation, at, actor, held, carried, model, kind, words)`.
const OPENED: Constructor = (
    "Opened",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "model",
        "kind",
        "words",
    ],
);
/// `Titled(conversation, at, actor, held, carried, by, words)`.
const TITLED: Constructor = (
    "Titled",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "by",
        "words",
    ],
);
/// `Said(conversation, at, actor, held, carried, turn, by, words)`.
const SAID: Constructor = (
    "Said",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "turn",
        "by",
        "words",
    ],
);
/// `Proposed(conversation, at, actor, held, carried, object, revises)`.
const PROPOSED: Constructor = (
    "Proposed",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "object",
        "revises",
    ],
);
/// `Reviewed(conversation, at, actor, held, carried, chain, batch, decision, objects, words)`.
const REVIEWED: Constructor = (
    "Reviewed",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "chain",
        "batch",
        "decision",
        "objects",
        "words",
    ],
);
/// `ReviewFailed(conversation, at, actor, held, carried, standing, words)`.
const REVIEW_FAILED: Constructor = (
    "ReviewFailed",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "standing",
        "words",
    ],
);
/// `Stopped(conversation, at, actor, held, carried, by)`.
const STOPPED: Constructor = (
    "Stopped",
    &["conversation", "at", "actor", "held", "carried", "by"],
);
/// `Rewound(conversation, at, actor, held, carried, to, by)`.
const REWOUND: Constructor = (
    "Rewound",
    &["conversation", "at", "actor", "held", "carried", "to", "by"],
);
/// `MemorySeen(conversation, at, actor, held, carried, version)`.
const MEMORY_SEEN: Constructor = (
    "MemorySeen",
    &["conversation", "at", "actor", "held", "carried", "version"],
);
/// `Session(conversation, at, actor, held, carried, session, model, cwd, surface)`.
const SESSION: Constructor = (
    "Session",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "session",
        "model",
        "cwd",
        "surface",
    ],
);
/// `Unheard(conversation, at, actor, held, carried, session, model)`.
const UNHEARD: Constructor = (
    "Unheard",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "session",
        "model",
    ],
);
/// `Kept(conversation, at, actor, held, carried, session, model, cwd, surface, from)`.
const KEPT: Constructor = (
    "Kept",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "session",
        "model",
        "cwd",
        "surface",
        "from",
    ],
);
/// `Scoped(conversation, at, actor, held, carried, scope)`.
const SCOPED: Constructor = (
    "Scoped",
    &["conversation", "at", "actor", "held", "carried", "scope"],
);
/// `Coding(conversation, at, actor, held, carried, proposal, session, words)`.
const CODING: Constructor = (
    "Coding",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "proposal",
        "session",
        "words",
    ],
);
/// `Erased(conversation, at, actor, held, carried, keys, erasure)`.
const ERASED: Constructor = (
    "Erased",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "keys",
        "erasure",
    ],
);

/// `Resealed(conversation, at, actor, held, carried, of, from, words)`.
const RESEALED: Constructor = (
    "Resealed",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "of",
        "from",
        "words",
    ],
);

/// The lines' constructors, in print order.
const LINES: &[Constructor] = &[
    OPENED,
    TITLED,
    SAID,
    PROPOSED,
    REVIEWED,
    REVIEW_FAILED,
    STOPPED,
    REWOUND,
    MEMORY_SEEN,
    SESSION,
    UNHEARD,
    KEPT,
    SCOPED,
    CODING,
    ERASED,
    RESEALED,
];

/// The lines' whitelist: their constructors, `Scrubbed` and `Locked`, nothing else.
#[derive(Debug, Clone, Default)]
pub struct LineVocabulary;

impl Vocabulary for LineVocabulary {
    fn signature(&self, name: &str) -> Option<prodrome_core::literal::Signature<'_>> {
        Table(LINES)
            .find(name)
            .or_else(|| Table(&[SCRUBBED, LOCKED]).find(name))
    }
}

impl Schema for LineEvent {
    type Vocabulary = LineVocabulary;
    type Key = ConversationId;
    type Register = Only;

    fn registers(_: &LineVocabulary) -> Vec<Only> {
        vec![Only]
    }

    /// A conversation's one register, its transcript. A new line rests on every line no later
    /// line rests on, so the lines are causally ordered and the frontier is the heads. Its
    /// reading is the heads' distinct events ([`Whole`]); two heads that print alike are one
    /// line written twice.
    type Registers<'a> = One<'a, Whole<Self>>;

    fn to_value(&self) -> Value {
        let (signature, mut values) = self.bare();
        if has_words(signature) {
            values.push(self.words.to_value());
        }
        record(signature, values)
    }

    fn from_value(_: &LineVocabulary, value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, LINES)?;
        let shape = shape_of(fields)?;
        let words: Vec<SealedSlot> = if has_words(signature(&shape)) {
            fields.get("words")?
        } else {
            Vec::new()
        };
        if words.len() != shape.slots() {
            return Err(ProdromeError::invalid(format!(
                "{} holds {} slots of words, and its shape {}",
                fields.name(),
                words.len(),
                shape.slots()
            )));
        }
        Ok(Self {
            conversation: fields.get("conversation")?,
            at: fields.get("at")?,
            actor: fields.get("actor")?,
            held: fields.get("held")?,
            carried: fields.get("carried")?,
            shape,
            words,
        })
    }

    fn key(&self) -> &ConversationId {
        &self.conversation
    }

    fn at(&self) -> Datetime {
        self.at
    }

    fn actor(&self) -> &Actor {
        &self.actor
    }

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }
}

/// A part of a line still spelled as JSON inside its literal: a turn's shape, who named a
/// conversation, a coding session, an erasure. Changing it moves each slot's binding, so it
/// changes only with a carry that locks every slot again.
fn json(value: &impl Serialize) -> Value {
    Value::str(serde_json::to_string(value).expect("a line's parts are plain data"))
}

/// The JSON part `name` of a stored line, read back.
fn parsed<T: DeserializeOwned>(fields: Fields<'_>, name: &str) -> Result<T, ProdromeError> {
    serde_json::from_str(&fields.get::<String>(name)?)
        .map_err(|failed| ProdromeError::invalid(format!("{}.{name}: {failed}", fields.name())))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use prodrome_core::event::{canonical, parse_event};

    use proptest::prelude::Strategy as _;

    use super::*;
    use crate::capability::{ToolCall, Turn};
    use crate::secretary::salted_text::Words as _;
    use crate::secretary::words::{Gone, Words};

    /// Each key one more than the last.
    #[derive(Default)]
    struct Counting(AtomicU8);

    impl Entropy for Counting {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(self.0.fetch_add(1, Ordering::Relaxed));
            Ok(())
        }
    }

    fn at() -> Datetime {
        Datetime::new(2026, 10, 9, 9, 0, 0, 0).expect("an instant")
    }

    fn id() -> ConversationId {
        ConversationId::new("2026-10-09-1").expect("a slug")
    }

    fn model() -> Entry {
        Entry::Said {
            at: at(),
            turn: Turn::Model {
                text: "Noted: Mira is the accountant.".to_owned(),
                calls: vec![ToolCall {
                    verb: "remember".into(),
                    args: [(
                        "fact".to_owned(),
                        serde_json::json!("Mira is the accountant"),
                    )]
                    .into(),
                }],
            },
            by: None,
        }
    }

    fn written(entry: &Entry) -> (LineEvent, Vec<TextKey>) {
        LineEvent::write(
            id(),
            Actor::new("assistant").expect("an actor"),
            3,
            entry,
            &Counting::default(),
        )
        .expect("a line")
    }

    fn opened_by(keys: &[TextKey]) -> impl Fn(&KeyName) -> Words<TextKey> + '_ {
        |name| {
            keys.iter()
                .find(|key| &key.name() == name)
                .and_then(|key| TextKey::parse(name, key.bytes()))
                .map_or_else(|| Gone::Missing.into(), Words::kept)
        }
    }

    #[test]
    fn a_line_prints_no_words_reads_back_and_opens_to_its_entry() {
        let (line, keys) = written(&model());
        assert_eq!(
            keys.len(),
            2,
            "the prose and the one call, each its own key"
        );
        let print = canonical(&line);
        assert!(
            !print.contains("Mira") && !print.contains("accountant"),
            "{print}"
        );
        let read: LineEvent = parse_event(&LineVocabulary, &print).expect("parses");
        assert_eq!(read, line);
        assert_eq!(read.entry(&opened_by(&keys)), model());
    }

    /// A line's instant is the entry's, to the microsecond: nothing about an instant is
    /// refused on the way in, and it reads back as the same value.
    #[test]
    fn a_line_keeps_its_entrys_instant() {
        let at = Datetime::new(2026, 10, 9, 9, 0, 1, 250_000).expect("an instant");
        let entry = Entry::Stopped {
            at,
            by: "ada".to_owned(),
        };
        let (line, keys) = written(&entry);
        let read: LineEvent = parse_event(&LineVocabulary, &canonical(&line)).expect("parses");
        let back = read.entry(&opened_by(&keys));
        assert_eq!(back.at(), at);
        assert_eq!(back, entry);
    }

    /// One slot's key destroyed erases that slot alone: the prose stays, the call's words go.
    #[test]
    fn one_slot_gone_leaves_the_others() {
        let (line, keys) = written(&model());
        let prose = &keys[..1];
        let Entry::Said {
            turn: Turn::Erased { words, shape },
            ..
        } = line.entry(&opened_by(prose))
        else {
            panic!("a turn with a slot gone is erased");
        };
        assert_eq!(shape.slots(), 2);
        assert!(words[0].get().is_some());
        assert_eq!(words[1].gone(), Some(&Gone::Missing));
    }

    /// A slot's ciphertext opens only in its own slot of its own line.
    #[test]
    fn a_ciphertext_moved_does_not_open() {
        let (line, keys) = written(&model());
        let (other, _) = LineEvent::write(
            id(),
            Actor::new("assistant").expect("an actor"),
            4,
            &model(),
            &Counting::default(),
        )
        .expect("a line");
        let SealedSlot { locked, under } = &line.words[0];
        let opens = |binding: LineSlot| locked.open(&binding, &keys[0]).is_some();
        assert!(!opens(other.slot(0, under)), "another line");
        assert!(!opens(line.slot(1, under)), "another slot");
        assert!(
            !opens(line.slot(0, &ScrubbedUnder::Unversioned)),
            "other rules"
        );
        assert!(opens(line.slot(0, under)), "its own");
    }

    /// A slot moved into another conversation opens to nothing through the cache too, after
    /// its own line was read and kept: the cache is keyed by the binding opening reads.
    #[test]
    fn a_moved_slot_reads_missing_after_the_original_was_read() {
        let (line, keys) = written(&model());
        let mut moved = line.clone();
        moved.conversation = ConversationId::new("2026-10-09-elsewhere").expect("a slug");
        let opened = crate::secretary::opened::Opened::default();
        let held = crate::store::InMemoryKeys::default();
        for key in &keys {
            crate::store::Doc::set(&held, &key.name(), key).expect("kept");
        }
        let cached = crate::secretary::opened::Cached::new(&held, &opened);
        assert_eq!(line.entry(&cached), model(), "the original reads whole");
        let Entry::Said {
            turn: Turn::Erased { words, .. },
            ..
        } = moved.entry(&cached)
        else {
            panic!("a moved slot does not open");
        };
        assert!(words.iter().all(|slot| slot.gone() == Some(&Gone::Missing)));
    }

    /// Every count prints as itself and reads back, at the widest it holds.
    #[test]
    fn every_count_round_trips_at_its_widest() {
        for entry in [
            Entry::Rewound {
                to: usize::MAX,
                at: at(),
                by: "ada".to_owned(),
            },
            Entry::MemorySeen {
                at: at(),
                version: u64::MAX,
            },
            Entry::Proposed {
                at: at(),
                object: crate::secretary::inbox::ProposalId::new("p0").expect("an id"),
                revises: Some(usize::MAX),
            },
            Entry::ReviewFailed {
                at: at(),
                reason: Words::kept("down".to_owned()),
                standing: usize::MAX,
            },
        ] {
            let (line, _) = LineEvent::write(
                id(),
                Actor::new("assistant").expect("an actor"),
                u32::MAX,
                &entry,
                &Counting::default(),
            )
            .expect("a line");
            let print = canonical(&line);
            assert_eq!(parse_event(&LineVocabulary, &print), Ok(line), "{print}");
        }
    }

    #[test]
    fn an_erasure_prints_and_reads_back() {
        let line = LineEvent::erasure(
            id(),
            Datetime::new(2026, 10, 9, 9, 1, 0, 0).expect("an instant"),
            Actor::new("assistant").expect("an actor"),
            5,
            [KeyName::new("a".repeat(64)).expect("a name")].into(),
            Erasure::Forgot {
                fact: crate::secretary::memory::FactId(2),
            },
        );
        let read: LineEvent = parse_event(&LineVocabulary, &canonical(&line)).expect("parses");
        assert_eq!(read, line);
    }

    /// Text holding the credentials and numbers the door removes, among other words.
    fn said_text() -> impl proptest::strategy::Strategy<Value = String> {
        use proptest::prelude::*;
        let piece = prop_oneof![
            Just("reset at https://example.test/r?token=s3cret ".to_owned()),
            Just("SSN 123-45-6789 ".to_owned()),
            Just("Your verification code is 482913. ".to_owned()),
            Just("https://ada:hunter2@example.test/feed ".to_owned()),
            Just("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEifQ.c2lnbmF0dXJl ".to_owned()),
            Just("café — 確認 ".to_owned()),
            "[a-z ]{0,12}",
        ];
        proptest::collection::vec(piece, 0..6).prop_map(|pieces| pieces.concat())
    }

    proptest::proptest! {
        /// The law of the one locking path: every slot of a written line opens to the words
        /// written, scrubbed under the rules it records, and those are the current rules. So no
        /// stored line holds what the rules it records catch, and reading it scrubs nothing.
        #[test]
        fn a_written_line_holds_its_words_scrubbed(text in said_text(), fact in said_text()) {
            let entry = Entry::Said {
                at: at(),
                turn: Turn::Model {
                    text,
                    calls: vec![ToolCall {
                        verb: "remember".into(),
                        args: [("fact".to_owned(), serde_json::json!(fact))].into(),
                    }],
                },
                by: None,
            };
            let (line, keys) = written(&entry);
            let (_, words) = crate::secretary::line_shape::split(&entry);
            let words = words.expect("a whole turn has words");
            for (slot, said) in words.iter().enumerate() {
                let opened = line.open(slot, &keys[slot]).expect("opens");
                proptest::prop_assert_eq!(&opened, &said.scrubbed().expect("scrubs"));
                for secret in ["s3cret", "123-45-6789", "482913", "hunter2", "eyJzdWIi"] {
                    proptest::prop_assert!(!opened.print().contains(secret), "{secret}");
                }
            }
            proptest::prop_assert!(line.scrubbed_under().all(|under| under.is_current(())));
        }

        /// The law of reading: a slot locked under any rules but the current ones opens to its
        /// words scrubbed under the current rules, by `open` and through a reading alike, so no
        /// reader sees what the current rules catch before the migration locks it again.
        #[test]
        fn a_slot_under_older_rules_reads_scrubbed_under_the_current(
            text in said_text(),
            older in proptest::prop_oneof![
                proptest::strategy::Just(ScrubbedUnder::Unversioned),
                "[0-9a-f]{64}".prop_map(|name| ScrubbedUnder::Version(Rules::new(name).expect("hex"))),
            ],
        ) {
            proptest::prop_assume!(!older.is_current(()));
            let entry = Entry::Said {
                at: at(),
                turn: Turn::Model { text, calls: Vec::new() },
                by: None,
            };
            let (line, keys) = LineEvent::written_under(
                &older,
                id(),
                Actor::new("assistant").expect("an actor"),
                3,
                &entry,
                &Counting::default(),
            )
            .expect("a line");
            let (_, words) = crate::secretary::line_shape::split(&entry);
            let said = words.expect("a whole turn has words").remove(0).scrubbed().expect("scrubs");
            proptest::prop_assert_eq!(line.open(0, &keys[0]), Some(said.clone()));
            let Entry::Said { turn: Turn::Model { text, .. }, .. } = line.entry(&opened_by(&keys)) else {
                panic!("a model turn reads whole");
            };
            proptest::prop_assert_eq!(LineWords::Model { text }, said);
        }
    }

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        crate::own::assert_own(LINES);
        crate::own::assert_own(&[SCRUBBED, LOCKED]);
    }
}
