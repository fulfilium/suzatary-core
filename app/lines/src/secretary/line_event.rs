//! The conversations' schema: one line of one conversation, its shape in the clear and each slot
//! of its words locked under a key of its own.
//!
//! Each slot's associated data is the line's own print without its words, with the slot's number
//! and its rules (`LineSlot`), so a ciphertext moved to another slot, line or store does not open,
//! and a slot cannot claim other rules. Nothing in an object is words: the redaction door is
//! applied in `seal`, before the words are locked.

use std::borrow::Borrow;
use std::collections::BTreeSet;

use base64::Engine as _;
use prodrome_data::event::{Actor, Hash};
use prodrome_data::literal::{Datetime, ProdromeError, Table, Value, Vocabulary, print_literal};
use prodrome_data::schema::Schema;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::secretary::answer::Answer;
use crate::secretary::line_shape::{self, LineShape};
use crate::secretary::saying::Saying;
use crate::secretary::spoken::{Plain, widened};
use prodrome_data::fold::sole::{One, Only, Whole};
use prodrome_data::literal::codec::{Constructor, Fields, Literal, no_arm, record};
use suzatary_base::capability::Entropy;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Entry;
use suzatary_conversations::secretary::store::Said;
use suzatary_conversations::secretary::taint::{self, Taint};
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_held::redact::{Rules, Scrubbed};
use suzatary_memory::secretary::carried::Carried;
use suzatary_memory::secretary::erasure::Erasure;
use suzatary_memory::secretary::line_words::LineWords;
use suzatary_memory::secretary::locked_text::{FreshKey, KeyName, Locked, Shape, TextKey};
use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
use suzatary_memory::secretary::slots::Slots;
use suzatary_memory::secretary::words::{Gone, Words};

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
    seal: Seal,
    under: ScrubbedUnder,
}

/// A slot's words: locked, or, in a husk, dropped, the name of the key they were locked under
/// all that is left of them.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seal {
    Locked(Locked<LineSlot>),
    Dropped(KeyName),
}

impl Seal {
    fn key(&self) -> &KeyName {
        match self {
            Self::Locked(locked) => locked.key(),
            Self::Dropped(key) => key,
        }
    }
}

impl SealedSlot {
    /// The words, opened by `key` for `binding`: as locked when they passed the current rules, and
    /// scrubbed under the current rules now when they passed any other, so no reader sees what the
    /// current rules catch; withheld when they no longer pass the door at all. `None` for a key
    /// that does not open them. No reader opens a seal a reseal supersedes.
    fn open(&self, binding: impl FnOnce() -> LineSlot, key: &TextKey) -> Option<Words<LineWords>> {
        let Seal::Locked(locked) = &self.seal else {
            return None;
        };
        let words = locked.open(&binding(), key)?;
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
    #[error("a line is written with its words, and some of these are gone")]
    Gone,
    #[error("its words do not pass the redaction door: {0}")]
    Unscrubbable(String),
    #[error("no key for its words: {0}")]
    NoKey(String),
    #[error("an edit's question is a turn said, and this entry is none")]
    Unsaid,
    #[error("a retired kind of line is never written")]
    Retired,
}

/// The domain of a conversation's id.
const CONVERSATION: &[u8] = b"suzatary conversation v1\n";

impl LineEvent {
    /// What the line weighs in a call's window: the bytes its words were sealed in, fixed when it
    /// was written, so no erasure, lost key or new redaction rule moves it. A husk's dropped slot,
    /// which only a tab holds, weighs nothing; the box holds every line whole.
    #[must_use]
    pub fn weight(&self) -> usize {
        self.words
            .iter()
            .map(|slot| match &slot.seal {
                Seal::Locked(locked) => locked.sealed_len(),
                Seal::Dropped(_) => 0,
            })
            .sum()
    }

    /// Line `entry` of `conversation`, written here by `actor` holding `held` lines, each slot of
    /// its words through the redaction door and locked under a key drawn from `entropy`; the keys
    /// that open them are the only thing to keep. A verb's answer stores its class. No model's
    /// turn is a [`Plain`] entry; [`LineEvent::answered`] writes one.
    ///
    /// ```compile_fail,E0308
    /// use suzatary_conversations::secretary::model::Turn;
    /// use suzatary_conversations::secretary::store::Entry;
    /// use suzatary_lines::secretary::line_event::LineEvent;
    ///
    /// fn writes(
    ///     conversation: suzatary_base::conversation_id::ConversationId,
    ///     actor: prodrome_data::event::Actor,
    ///     at: suzatary_base::capability::Datetime,
    ///     entropy: &dyn suzatary_base::capability::Entropy,
    /// ) {
    ///     let model = Entry::Said {
    ///         at,
    ///         turn: Turn::Model { text: String::new(), calls: Vec::new() },
    ///         by: None,
    ///     };
    ///     let _ = LineEvent::write(conversation, actor, 0, &model, entropy);
    /// }
    /// ```
    pub fn write(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Plain,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let (mut line, words) = Self::unsealed(conversation, actor, held, entry)?;
        if let LineShape::Said {
            turn: TurnShape::Tool { carries, .. },
            ..
        } = &mut line.shape
        {
            *carries = Said::of(&widened(entry.clone())).map(|said| taint::written(&said));
        }
        line.sealed(&words, entropy)
    }

    /// The box's model turn `answer`, storing its carry.
    pub fn answered(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        answer: &Answer,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let (mut line, words) = Self::unsealed(conversation, actor, held, &answer.entry())?;
        if let LineShape::Said {
            turn: TurnShape::Model { carries, .. },
            ..
        } = &mut line.shape
        {
            *carries = Some(answer.carries());
        }
        line.sealed(&words, entropy)
    }

    /// `entry` as an older box wrote it, storing no carry, for tests and fixtures.
    #[cfg(any(test, feature = "nothing", feature = "fixture"))]
    pub fn older(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Entry,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let (line, words) = Self::unsealed(conversation, actor, held, entry)?;
        line.sealed(&words, entropy)
    }

    /// What the box's own line stores that its turn carries; another writer's says nothing.
    #[must_use]
    pub fn carries(&self) -> Option<Taint> {
        match &self.shape {
            LineShape::Said {
                turn: TurnShape::Model { carries, .. } | TurnShape::Tool { carries, .. },
                ..
            } if crate::secretary::transcript::by_the_box(&self.actor) => *carries,
            _ => None,
        }
    }

    /// [`LineEvent::write`] of a `Said` entry as an edit's own question ([`Saying::Edit`]).
    pub fn edited(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Plain,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let (mut line, words) = Self::unsealed(conversation, actor, held, entry)?;
        let LineShape::Said { saying, .. } = &mut line.shape else {
            return Err(NotALine::Unsaid);
        };
        *saying = Saying::Edit;
        line.sealed(&words, entropy)
    }

    /// The first line of a new conversation, `entry`, written here by `actor`: the
    /// conversation is [`LineEvent::begun_id`] of the line, set before its words are sealed,
    /// since each slot is bound to it.
    pub fn begin(
        actor: Actor,
        entry: &Plain,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let placeholder = suzatary_base::conversation_id::DEFAULT.clone();
        let (mut line, words) = Self::unsealed(placeholder, actor, 0, entry)?;
        line.conversation = line.begun_id();
        line.sealed(&words, entropy)
    }

    /// The conversation a line that begins one names: the first 32 hex digits of a SHA-256 over a
    /// domain string and the line's print without its words and with its conversation printed as
    /// `''`. It is a pure function of the line's clear fields, never of its words, which are
    /// erasable while an id is kept forever; and it is never an id the box mints by day or by name.
    #[must_use]
    pub fn begun_id(&self) -> ConversationId {
        use sha2::{Digest as _, Sha256};
        let ((name, fields), mut values) = self.bare();
        if let Some(conversation) = values.first_mut() {
            *conversation = String::new().to_value();
        }
        let print = print_literal(&Value::call(
            name,
            fields
                .iter()
                .copied()
                .filter(|field| *field != "words")
                .map(str::to_owned)
                .zip(values)
                .collect(),
        ));
        let digest = Sha256::new()
            .chain_update(CONVERSATION)
            .chain_update(print.as_bytes())
            .finalize();
        let hex = digest
            .iter()
            .take(16)
            .fold(String::with_capacity(32), |mut hex, b| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{b:02x}");
                hex
            });
        ConversationId::new(hex)
            .unwrap_or_else(|_| unreachable!("32 lowercase hex digits are a slug"))
    }

    /// The line's head and shape, its words not yet sealed, and the words to seal.
    fn unsealed(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Entry<impl Borrow<Turn>>,
    ) -> Result<(Self, Vec<LineWords>), NotALine> {
        let (shape, words) = line_shape::split(entry).ok_or(NotALine::Retired)?;
        let words = words.ok_or(NotALine::Gone)?;
        let line = Self {
            conversation,
            at: entry.at(),
            actor,
            held,
            carried: Carried::Written,
            shape,
            words: Vec::new(),
        };
        Ok((line, words))
    }

    /// The line with each of `words` sealed in its slot, and the keys that open them.
    fn sealed(
        mut self,
        words: &[LineWords],
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let mut keys = Vec::with_capacity(words.len());
        for (slot, words) in words.iter().enumerate() {
            let (sealed, opens) = seal(&self, slot, words, entropy)?;
            self.words.push(sealed);
            keys.push(opens);
        }
        Ok((self, keys))
    }

    /// The words of slot `of` of a line of `conversation`, sealed again from the seal locked under
    /// `from`: `words`, already through the redaction door, locked under a key drawn from
    /// `entropy`; answers the key that opens it.
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
        words: &Scrubbed<LineWords>,
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
        let (sealed, opens) = locked(&line, 0, words, entropy)?;
        line.words.push(sealed);
        Ok((line, opens))
    }

    /// A line of a retired kind with no words, as the session bookkeeping stored it, for the
    /// tests and fixtures that read a store holding one; `None` for a shape still written.
    #[cfg(any(test, feature = "nothing", feature = "fixture"))]
    #[must_use]
    pub fn retired(
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        at: Datetime,
        shape: LineShape,
    ) -> Option<Self> {
        matches!(
            shape,
            LineShape::MemorySeen { .. }
                | LineShape::Session { .. }
                | LineShape::Kept { .. }
                | LineShape::Unheard { .. }
        )
        .then_some(Self {
            conversation,
            at,
            actor,
            held,
            carried: Carried::Written,
            shape,
            words: Vec::new(),
        })
    }

    /// Line `entry` locked as scrubbed `under` rules other than the current ones, its words not
    /// scrubbed, for the tests and the fixture's stale store.
    #[cfg(any(test, feature = "nothing", feature = "fixture"))]
    pub fn written_under(
        under: &ScrubbedUnder,
        conversation: ConversationId,
        actor: Actor,
        held: u32,
        entry: &Entry,
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let (shape, words) = line_shape::split(entry).ok_or(NotALine::Retired)?;
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
                seal: Seal::Locked(locked),
                under: under.clone(),
            });
            keys.push(opens);
        }
        Ok((line, keys))
    }

    /// A line of `shape` holding `words`, through the door and sealed, with no entry to say the
    /// words are what the shape reads: a line a writer that is not the box's could post.
    #[cfg(any(test, feature = "nothing", feature = "fixture"))]
    pub fn sealed_as(
        conversation: ConversationId,
        at: Datetime,
        actor: Actor,
        held: u32,
        shape: LineShape,
        words: &[LineWords],
        entropy: &dyn Entropy,
    ) -> Result<(Self, Vec<TextKey>), NotALine> {
        let line = Self {
            conversation,
            at,
            actor,
            held,
            carried: Carried::Written,
            shape,
            words: Vec::new(),
        };
        line.sealed(words, entropy)
    }

    /// The line as if carried from `object` of an earlier store, its words as they were: what no
    /// tab may post.
    #[cfg(any(test, feature = "nothing", feature = "fixture"))]
    #[must_use]
    pub fn carried_from(mut self, object: Hash) -> Self {
        self.carried = Carried::From(object);
        self
    }

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

    #[must_use]
    pub fn conversation(&self) -> &ConversationId {
        &self.conversation
    }

    /// When it was written, by its writer's clock.
    #[must_use]
    pub fn at(&self) -> Datetime {
        self.at
    }

    #[must_use]
    pub fn held(&self) -> u32 {
        self.held
    }

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

    #[must_use]
    pub fn shape(&self) -> &LineShape {
        &self.shape
    }

    /// The name of each slot's key, in slot order.
    pub fn keys(&self) -> impl Iterator<Item = &KeyName> {
        self.words.iter().map(|sealed| sealed.seal.key())
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

    /// Slot `slot`'s words, opened by `key` and read under the current rules: kept, or withheld
    /// when they no longer pass it; `None` for a slot the line does not hold or a key that does not
    /// open it.
    #[must_use]
    pub fn read(&self, slot: usize, key: &TextKey) -> Option<Words<LineWords>> {
        let sealed = self.words.get(slot)?;
        sealed.open(|| self.slot(slot, &sealed.under), key)
    }

    /// Slot `slot`'s words as they were locked, under whatever rules: for sealing them again and
    /// naming their blocks, never for a reader.
    pub(crate) fn unscrubbed(&self, slot: usize, key: &TextKey) -> Option<LineWords> {
        let sealed = self.words.get(slot)?;
        let Seal::Locked(locked) = &sealed.seal else {
            return None;
        };
        locked.open(&self.slot(slot, &sealed.under), key)
    }

    /// Slot `slot`'s words as `slots` reads them by its key's name, opened with this line's binding
    /// for that slot; missing for a slot the line does not hold.
    #[must_use]
    pub(crate) fn slot_words(&self, slot: usize, slots: &impl Slots) -> Words<LineWords> {
        let Some(sealed) = self.words.get(slot) else {
            return Gone::Missing.into();
        };
        slots.words(sealed.seal.key(), &|key| self.read(slot, key))
    }

    /// Whether slot `slot` opens as `slots` reads it by its key's name: its words are not read
    /// where `slots` can say so by its key alone, so a slot under older rules is not scrubbed to
    /// answer.
    pub(crate) fn slot_opens(&self, slot: usize, slots: &impl Slots) -> bool {
        let Some(sealed) = self.words.get(slot) else {
            return false;
        };
        slots.opens(
            sealed.seal.key(),
            &|| self.slot_words(slot, slots),
            &|key| self.unscrubbed(slot, key).is_some(),
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
    let words = Scrubbed::of(words).map_err(|failed| NotALine::Unscrubbable(failed.to_string()))?;
    locked(line, slot, &words, entropy)
}

/// Slot `slot` of `line` holding `words`, locked under a key drawn from `entropy` as words
/// through the current rules' door.
fn locked(
    line: &LineEvent,
    slot: usize,
    words: &Scrubbed<LineWords>,
    entropy: &dyn Entropy,
) -> Result<(SealedSlot, TextKey), NotALine> {
    let under = ScrubbedUnder::Version(Rules::current(()).clone());
    let key = FreshKey::draw(entropy).map_err(NotALine::NoKey)?;
    let (locked, opens) = Locked::lock(&line.slot(slot, &under), words.get(), key);
    Ok((
        SealedSlot {
            seal: Seal::Locked(locked),
            under,
        },
        opens,
    ))
}

/// The fields a shape prints after the head, in signature order.
fn shape_fields(shape: &LineShape) -> Vec<Value> {
    match shape {
        LineShape::Opened { model, kind } => vec![model.to_value(), kind.to_value()],
        LineShape::SetUp { template } => vec![template.to_value()],
        LineShape::Titled { by } => vec![json(by)],
        LineShape::Said { turn, by, .. } => vec![json(turn), by.to_value()],
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
        LineShape::Began { covers, run } => vec![covers.to_value(), run.to_value()],
        LineShape::Again { question, how, by } => {
            vec![question.to_value(), how.to_value(), by.to_value()]
        }
    }
}

/// The shape a stored line spells.
fn shape_of(fields: Fields<'_>) -> Result<LineShape, ProdromeError> {
    Ok(match fields.name() {
        "Opened" => LineShape::Opened {
            model: fields.get("model")?,
            kind: fields.get("kind")?,
        },
        "SetUp" => LineShape::SetUp {
            template: fields.get("template")?,
        },
        "Titled" => LineShape::Titled {
            by: parsed(fields, "by")?,
        },
        "Said" => LineShape::Said {
            turn: parsed(fields, "turn")?,
            by: fields.get("by")?,
            saying: Saying::Plain,
        },
        "Edited" => LineShape::Said {
            turn: parsed(fields, "turn")?,
            by: fields.get("by")?,
            saying: Saying::Edit,
        },
        "Proposed" => LineShape::Proposed {
            object: fields.get("object")?,
            revises: fields.get("revises")?,
        },
        "Reviewed" => LineShape::Reviewed {
            chain: fields.get("chain")?,
            batch: fields.get("batch")?,
            decision: suzatary_conversations::secretary::store::Decision::read(
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
        "Began" => LineShape::Began {
            covers: fields.get("covers")?,
            run: fields.get("run")?,
        },
        "Again" => LineShape::Again {
            question: fields.get("question")?,
            how: fields.get("how")?,
            by: fields.get("by")?,
        },
        other => return Err(no_arm("LineShape", other)),
    })
}

/// The signature a line of `shape` is stored under.
fn signature(shape: &LineShape) -> Constructor {
    match shape {
        LineShape::Opened { .. } => OPENED,
        LineShape::SetUp { .. } => SET_UP,
        LineShape::Titled { .. } => TITLED,
        LineShape::Said {
            saying: Saying::Plain,
            ..
        } => SAID,
        LineShape::Said {
            saying: Saying::Edit,
            ..
        } => EDITED,
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
        LineShape::Began { .. } => BEGAN,
        LineShape::Again { .. } => AGAIN,
    }
}

/// `Scrubbed(key, box, rules)`: a slot's words as stored, the key's name, the ciphertext in
/// base64, and the rules the words passed.
const SCRUBBED: Constructor = ("Scrubbed", &["key", "box", "rules"]);

/// `Locked(key, box)`: a slot that records no rules.
const LOCKED: Constructor = ("Locked", &["key", "box"]);

/// `Dropped(key, rules)`: a husk's slot, its words dropped, the key's name and the rules they had
/// passed. No store's vocabulary reads it: a husk is the device's, and never an object.
const DROPPED: Constructor = ("Dropped", &["key", "rules"]);

/// `DroppedLocked(key)`: a husk's slot that records no rules.
const DROPPED_LOCKED: Constructor = ("DroppedLocked", &["key"]);

impl Literal for SealedSlot {
    fn to_value(&self) -> Value {
        let key = self.seal.key().to_value();
        match (&self.seal, &self.under) {
            (Seal::Locked(locked), under) => {
                let sealed = base64::engine::general_purpose::STANDARD
                    .encode(locked.sealed())
                    .to_value();
                match under {
                    ScrubbedUnder::Version(rules) => {
                        record(SCRUBBED, vec![key, sealed, rules.to_value()])
                    }
                    ScrubbedUnder::Unversioned => record(LOCKED, vec![key, sealed]),
                }
            }
            (Seal::Dropped(_), ScrubbedUnder::Version(rules)) => {
                record(DROPPED, vec![key, rules.to_value()])
            }
            (Seal::Dropped(_), ScrubbedUnder::Unversioned) => record(DROPPED_LOCKED, vec![key]),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, "SealedSlot", &[SCRUBBED, LOCKED])?;
        let sealed = base64::engine::general_purpose::STANDARD
            .decode(fields.get::<String>("box")?)
            .map_err(|failed| ProdromeError::invalid(format!("{}.box: {failed}", fields.name())))?;
        let under = if fields.name() == SCRUBBED.0 {
            ScrubbedUnder::Version(fields.get("rules")?)
        } else {
            ScrubbedUnder::Unversioned
        };
        Ok(Self {
            seal: Seal::Locked(Locked::read(fields.get("key")?, sealed)),
            under,
        })
    }
}

/// Whether a line of `signature` carries `words`: every kind that may hold any.
fn has_words(signature: Constructor) -> bool {
    signature.1.contains(&"words")
}

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
const SET_UP: Constructor = (
    "SetUp",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "template",
        "words",
    ],
);
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
/// An edit's own question: a `Said` by another name ([`Saying::Edit`]).
const EDITED: Constructor = ("Edited", SAID.1);
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
const STOPPED: Constructor = (
    "Stopped",
    &["conversation", "at", "actor", "held", "carried", "by"],
);
const REWOUND: Constructor = (
    "Rewound",
    &["conversation", "at", "actor", "held", "carried", "to", "by"],
);
const MEMORY_SEEN: Constructor = (
    "MemorySeen",
    &["conversation", "at", "actor", "held", "carried", "version"],
);
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
const SCOPED: Constructor = (
    "Scoped",
    &["conversation", "at", "actor", "held", "carried", "scope"],
);
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

const BEGAN: Constructor = (
    "Began",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "covers",
        "run",
    ],
);

const AGAIN: Constructor = (
    "Again",
    &[
        "conversation",
        "at",
        "actor",
        "held",
        "carried",
        "question",
        "how",
        "by",
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
    SET_UP,
    BEGAN,
    AGAIN,
    EDITED,
];

/// The lines' whitelist: their constructors, `Scrubbed`, `Locked` and a claim's `Covered`,
/// nothing else.
#[derive(Debug, Clone, Default)]
pub struct LineVocabulary;

impl Vocabulary for LineVocabulary {
    fn signature(&self, name: &str) -> Option<prodrome_data::literal::Signature<'_>> {
        Table(LINES).find(name).or_else(|| {
            Table(&[
                SCRUBBED,
                LOCKED,
                suzatary_conversations::secretary::store::COVERED,
            ])
            .find(name)
        })
    }
}

impl Schema for LineEvent {
    type Vocabulary = LineVocabulary;
    type Key = ConversationId;
    type Register = Only;

    fn registers(_: &LineVocabulary) -> Vec<Only> {
        vec![Only]
    }

    /// A conversation's one register, its transcript. A new line rests on every line no later line
    /// rests on, so the frontier is the heads; two heads that print alike are one line written
    /// twice.
    type Registers<'a> = One<'a, Whole<Self>>;

    fn to_value(&self) -> Value {
        let (signature, mut values) = self.bare();
        if has_words(signature) {
            values.push(self.words.to_value());
        }
        record(signature, values)
    }

    fn from_value(_: &LineVocabulary, value: &Value) -> Result<Self, ProdromeError> {
        line_of(value, |fields| fields.get("words"))
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

    fn husk(
        print: &prodrome_data::husk::Other<'_>,
    ) -> Option<prodrome_data::husk::HuskObject<Self>> {
        prodrome_data::husk::HuskObject::of_other(print).ok()
    }
}

/// The line `value` prints, its words read by `words` from its fields.
fn line_of(
    value: &Value,
    words: impl FnOnce(Fields<'_>) -> Result<Vec<SealedSlot>, ProdromeError>,
) -> Result<LineEvent, ProdromeError> {
    let fields = Fields::among(value, "LineEvent", LINES)?;
    let shape = shape_of(fields)?;
    let words = if has_words(signature(&shape)) {
        words(fields)?
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
    Ok(LineEvent {
        conversation: fields.get("conversation")?,
        at: fields.get("at")?,
        actor: fields.get("actor")?,
        held: fields.get("held")?,
        carried: fields.get("carried")?,
        shape,
        words,
    })
}

/// A line every key of whose words is let go is kept as its husk: each slot's words dropped, the
/// name of the key they were locked under all that is left of them.
impl prodrome_data::husk::Husking for LineEvent {
    type Seal = KeyName;
    type Husks = HuskVocabulary;

    fn drop_sealed(&self) -> Option<Self> {
        if self.words.is_empty() {
            return None;
        }
        let mut husk = self.clone();
        for slot in &mut husk.words {
            slot.seal = Seal::Dropped(slot.seal.key().clone());
        }
        Some(husk)
    }

    fn read_dropped(value: &Value) -> Result<Self, ProdromeError> {
        let line = line_of(value, |fields| {
            Ok(fields
                .get::<Vec<DroppedSlot>>("words")?
                .into_iter()
                .map(|DroppedSlot(slot)| slot)
                .collect())
        })?;
        if line.words.is_empty() {
            return Err(ProdromeError::invalid(
                "a line with no words has no husk".to_owned(),
            ));
        }
        Ok(line)
    }

    fn sealed_under(&self) -> impl Iterator<Item = &KeyName> {
        self.keys()
    }
}

/// A husk's whitelist: the lines' constructors and the two dropped slots.
#[derive(Debug, Default)]
pub struct HuskVocabulary;

impl Vocabulary for HuskVocabulary {
    fn signature(&self, name: &str) -> Option<prodrome_data::literal::Signature<'_>> {
        Table(LINES)
            .find(name)
            .or_else(|| Table(&[DROPPED, DROPPED_LOCKED]).find(name))
    }
}

/// A husk's slot, read: dropped, and nothing else.
struct DroppedSlot(SealedSlot);

impl Literal for DroppedSlot {
    fn to_value(&self) -> Value {
        self.0.to_value()
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, "DroppedSlot", &[DROPPED, DROPPED_LOCKED])?;
        let under = if fields.name() == DROPPED.0 {
            ScrubbedUnder::Version(fields.get("rules")?)
        } else {
            ScrubbedUnder::Unversioned
        };
        Ok(Self(SealedSlot {
            seal: Seal::Dropped(fields.get("key")?),
            under,
        }))
    }
}

/// A part of a line spelled as JSON inside its literal: changing it moves each slot's binding, so
/// it changes only with a carry that locks every slot again.
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

    use prodrome_data::event::{canonical, parse_event};

    use proptest::prelude::Strategy as _;

    use super::*;
    use suzatary_conversations::secretary::model::{ToolCall, Turn};
    use suzatary_memory::secretary::salted_text::Words as _;
    use suzatary_memory::secretary::words::{Gone, Words};

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

    /// `entry` as an older box wrote it, with no carry stored: what these laws of the print and
    /// the slots read.
    fn written(entry: &Entry) -> (LineEvent, Vec<TextKey>) {
        LineEvent::older(
            id(),
            Actor::new("assistant").expect("an actor"),
            3,
            entry,
            &Counting::default(),
        )
        .expect("a line")
    }

    /// Whether a slot opens is the same question asked either way: `slot_opens` answers as
    /// `slot_words` is not missing.
    #[test]
    fn a_slot_opens_exactly_where_its_words_are_not_missing() {
        let entry = Entry::Said {
            at: at(),
            turn: Turn::Model {
                text: "Your verification code is 482913. fee".to_owned(),
                calls: Vec::new(),
            },
            by: None,
        };
        let older = ScrubbedUnder::Version(
            Rules::new("67d7f8df2fd4d019fd90b6d59f17f44b046e102ea2d8ad4da122d6e7517c4f4a")
                .expect("a rules name"),
        );
        let actor = Actor::new("assistant").expect("an actor");
        let stale = LineEvent::written_under(&older, id(), actor, 3, &entry, &Counting::default())
            .expect("a line");
        let (other, others) = written(&entry);
        for (line, keys) in [written(&entry), stale] {
            let wrong: Vec<TextKey> = others
                .iter()
                .map(|key| TextKey::parse(&keys[0].name(), key.bytes()).expect("a key"))
                .collect();
            let own: Vec<TextKey> = keys
                .iter()
                .map(|key| TextKey::parse(&key.name(), key.bytes()).expect("a key"))
                .collect();
            for held in [own, wrong, Vec::new()] {
                let slots = opened_by(&held);
                let words = line.slot_words(0, &slots);
                let missing = matches!(
                    words,
                    Words::Gone {
                        gone: Gone::Missing
                    }
                );
                assert_eq!(line.slot_opens(0, &slots), !missing, "{} keys", held.len());
            }
        }
        assert!(
            !other.slot_opens(9, &opened_by(&others)),
            "a slot the line does not hold"
        );
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

    /// D1: a model's turn written now stores its carry in its unsealed shape, which reads back,
    /// opens its words (the slot is bound to it) and is believed of the box alone; a line written
    /// before stores none, and its print is the one an older box wrote, byte for byte, so every
    /// store in the tree reads, and is named, as it was.
    #[test]
    fn a_model_turn_stores_what_it_carried_and_an_older_line_reads_as_it_was() {
        let box_actor = Actor::new("assistant").expect("an actor");
        let Entry::Said {
            at,
            turn: Turn::Model { text, calls },
            ..
        } = model()
        else {
            panic!("a model's turn");
        };
        let answer = crate::secretary::answer::Answer::replied(at, text, calls, Taint::Tainted);
        let (line, keys) =
            LineEvent::answered(id(), box_actor.clone(), 3, &answer, &Counting::default())
                .expect("a line");
        let print = canonical(&line);
        assert!(print.contains(r#""carries":"tainted""#), "{print}");
        let read: LineEvent = parse_event(&LineVocabulary, &print).expect("parses");
        assert_eq!(read, line);
        assert_eq!(read.carries(), Some(Taint::Tainted));
        assert_eq!(read.entry(&opened_by(&keys)), model(), "its words open");
        let mut another = line.clone();
        another.actor = Actor::new("ada").expect("an actor");
        assert_eq!(
            another.carries(),
            None,
            "another writer's carry is not believed"
        );

        let (older, _) = written(&model());
        let older_print = canonical(&older);
        assert!(!older_print.contains("carries"), "{older_print}");
        let reread: LineEvent = parse_event(&LineVocabulary, &older_print).expect("parses");
        assert_eq!(
            canonical(&reread),
            older_print,
            "an older line prints as it was written"
        );
        assert_eq!(reread.carries(), None);
    }

    /// F8: a model's turn as `origin/master`'s binary wrote it, its print copied from the web
    /// vectors that binary generated (`api/tests/web/replicas.json`, the conversation
    /// `about-the-deed`) and not made by today's writer: it parses, prints back byte for byte,
    /// keeps its name, and stores no carry.
    #[test]
    fn a_line_an_older_box_wrote_reads_and_prints_as_written() {
        const PRINT: &str = r#"Change(genesis='7b818ef0fdd17acb5a6c38b43fa0281f189cc0854dab58dcff68c681ce13ad38', deps=('cbc23509b1201a83ddd03c815d37c00ff50a551bbeedfc4767b8174249deaf13',), event=Said(conversation='about-the-deed', at=datetime(2026, 9, 2, 12, 0, 20), actor='assistant', held=2, carried=None, turn='{"who":"model","calls":[]}', by=None, words=(Scrubbed(key='6cf6abc29c87ca5fa1dc5399205a10bed451b2ff0cbef2a2739c5ac845963890', box='P4y04x6VZ3a7qybDXD1GrBezyIBohrCbaZlM3Zwm/n/fCrt2k7cKanF41EgiJWdKC/RxUwq9NQ35MB8ytPyKyx343FAeWZxsn8a2ybxOMVPmubfYq+8m8YnGaUpGKvGXGIt/tUhDqdpZ9pjCpFrD9JfsQQvaW/sUatvs2Im+j1lunxasnA1dZ4+nkcAlrnsnabWhuw3rY71kjtT7kA/k3jPEf87nZN3Bn1hCq5ExmomDaGcqYa8UWwtMGqpzMyiz0MoRxq8oljXXMLGbWZtYj5euS9JSbjqu41M7TL6XAglIl8RB470XX1gHlHrL+UwgAa01xeKr/9wXEGTCFCEw3kahLmj395oQ3GeZ94mMPuM=', rules='f7eb6c9fe2ada434c6e1e51ec8bb69a3e10d03e227d8b731458c318793162d8a'),)))"#;
        const NAME: &str = "2929a192f5c4488e0bc8fbdc829e771fa1f67e69cd0ce4b391ffd5b3b8eb33b5";
        let envelope = prodrome_data::event::parse_envelope::<LineEvent>(&LineVocabulary, PRINT)
            .expect("an older box's line parses");
        let print = prodrome_data::event::canonical_envelope(&envelope);
        assert_eq!(print, PRINT, "it prints back byte for byte");
        assert_eq!(
            prodrome_data::event::seal_hash(&envelope).as_str(),
            NAME,
            "it keeps its name"
        );
        let line = envelope.event().expect("a line");
        assert!(matches!(
            line.shape(),
            LineShape::Said {
                turn: TurnShape::Model { carries: None, .. },
                ..
            }
        ));
        assert_eq!(line.carries(), None);
    }

    /// A line's instant is the entry's, to the microsecond, and reads back as the same value.
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
        let (other, _) = LineEvent::older(
            id(),
            Actor::new("assistant").expect("an actor"),
            4,
            &model(),
            &Counting::default(),
        )
        .expect("a line");
        let SealedSlot {
            seal: Seal::Locked(locked),
            under,
        } = &line.words[0]
        else {
            panic!("a written line's slot is locked");
        };
        let opens = |binding: LineSlot| locked.open(&binding, &keys[0]).is_some();
        assert!(!opens(other.slot(0, under)), "another line");
        assert!(!opens(line.slot(1, under)), "another slot");
        assert!(
            !opens(line.slot(0, &ScrubbedUnder::Unversioned)),
            "other rules"
        );
        assert!(opens(line.slot(0, under)), "its own");
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
            Entry::Proposed {
                at: at(),
                object: suzatary_inbox::secretary::inbox::ProposalId::new("p0").expect("an id"),
                revises: Some(usize::MAX),
            },
            Entry::ReviewFailed {
                at: at(),
                reason: Words::kept("down".to_owned()),
                standing: usize::MAX,
            },
            Entry::Began {
                at: at(),
                covers: vec![
                    suzatary_conversations::secretary::store::Covered {
                        line: Hash::new("a".repeat(64)).expect("a name"),
                        taint: suzatary_conversations::secretary::taint::Taint::Tainted,
                    },
                    suzatary_conversations::secretary::store::Covered {
                        line: Hash::new("b".repeat(64)).expect("a name"),
                        taint: suzatary_conversations::secretary::taint::Taint::Clean,
                    },
                ],
                run: suzatary_conversations::secretary::store::RunId::of([u8::MAX; 16]),
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

    /// THE RETIRED LINES: every shape the session bookkeeping stored still prints and reads
    /// back as itself, its widest counts included, and reads as [`Entry::Retired`]; no entry of
    /// that kind is written.
    #[test]
    fn a_retired_line_reads_back_and_is_never_written() {
        use suzatary_conversations::secretary::model::SessionId;
        use suzatary_conversations::secretary::store::Retired;
        let session = SessionId::new("96f709a3-43d7-4453-bf59-6fb5291b5a3c").expect("a uuid");
        for (shape, retired) in [
            (
                LineShape::MemorySeen { version: u64::MAX },
                Retired::MemorySeen,
            ),
            (
                LineShape::Session {
                    session: session.clone(),
                    model: "a-cli/small".to_owned(),
                    cwd: "/srv".to_owned(),
                    surface: Some("ab".repeat(32)),
                },
                Retired::Session,
            ),
            (
                LineShape::Kept {
                    session: session.clone(),
                    model: "a-cli/small".to_owned(),
                    cwd: "/srv".to_owned(),
                    surface: None,
                    from: usize::MAX,
                },
                Retired::Session,
            ),
            (
                LineShape::Unheard {
                    session: session.clone(),
                    model: "a-cli/small".to_owned(),
                },
                Retired::Unheard {
                    model: "a-cli/small".to_owned(),
                },
            ),
        ] {
            let line = LineEvent {
                conversation: id(),
                at: at(),
                actor: Actor::new("assistant").expect("an actor"),
                held: u32::MAX,
                carried: Carried::Written,
                shape,
                words: Vec::new(),
            };
            let print = canonical(&line);
            let read: LineEvent =
                parse_event(&LineVocabulary, &print).expect("a stored line reads");
            assert_eq!(read, line, "{print}");
            let entry = read.entry(&opened_by(&[]));
            assert_eq!(
                entry,
                Entry::Retired {
                    at: at(),
                    retired: retired.clone()
                }
            );
            let plain: Plain = Entry::Retired {
                at: at(),
                retired: retired.clone(),
            };
            assert!(matches!(
                LineEvent::write(
                    id(),
                    Actor::new("assistant").expect("an actor"),
                    0,
                    &plain,
                    &Counting::default(),
                ),
                Err(NotALine::Retired)
            ));
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
                fact: suzatary_memory::secretary::memory::FactId(2),
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
        /// written, scrubbed under the rules it records, and those are the current rules.
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
            let (_, words) = crate::secretary::line_shape::split(&entry).expect("a written kind");
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
        /// words scrubbed under the current rules, by `open` and through a reading alike.
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
            let (_, words) = crate::secretary::line_shape::split(&entry).expect("a written kind");
            let said = words.expect("a whole turn has words").remove(0).scrubbed().expect("scrubs");
            proptest::prop_assert_eq!(line.open(0, &keys[0]), Some(said.clone()));
            let Entry::Said { turn: Turn::Model { text, .. }, .. } = line.entry(&opened_by(&keys)) else {
                panic!("a model turn reads whole");
            };
            proptest::prop_assert_eq!(LineWords::Model { text }, said);
        }
    }

    /// A conversation set up from a template is a line of its own, `SetUp`, that prints and parses
    /// back to itself; a template this build does not hold is no line at all.
    #[test]
    fn a_set_up_line_reads_back_and_an_unknown_template_is_no_line() {
        use suzatary_connect::connect::TemplateId;
        use suzatary_conversations::secretary::store::{Begun, Kind};
        let entry = Entry::SetUp {
            at: at(),
            title: Words::kept("Connect Gmail".to_owned()),
            template: TemplateId::Gmail,
        };
        let (line, keys) = LineEvent::begin(
            Actor::new("ada").expect("an actor"),
            &entry,
            &Counting::default(),
        )
        .expect("a line");
        let print = canonical(&line);
        assert!(print.starts_with("SetUp("), "{print}");
        let back: LineEvent = parse_event(&LineVocabulary, &print).expect("parses");
        assert_eq!(back, line);
        let read = back.entry(&opened_by(&keys));
        assert_eq!(read, widened(entry.clone()));
        let start = read.start().expect("a set-up line begins a conversation");
        assert_eq!(start.kind, Kind::Topic);
        assert_eq!(start.begun, Begun::SetUp(TemplateId::Gmail));
        assert_eq!(start.title.get().map(String::as_str), Some("Connect Gmail"));
        let unknown = print.replace("template='lieer'", "template='fax'");
        assert_ne!(unknown, print, "the print names its template");
        assert!(parse_event::<LineEvent>(&LineVocabulary, &unknown).is_err());
    }

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        suzatary_base::own::assert_own(LINES);
        suzatary_base::own::assert_own(&[SCRUBBED, LOCKED]);
    }
}
