//! The lines' own rule for what a tab posts to them: a tab writes a rename, an opening, a setup,
//! a person's turn, a stop and an `Again`, each sealed as the box seals it, under the current
//! redaction rules, by the actor its signer proves, and each opening under its own conversation's
//! id.
//!
//! What it checks is what the post proves, never what a line claims: a line is written here and
//! never carried; each slot is opened with the key posted beside it and scrubbed again; a stamp
//! names the poster at its signer's seat; an attachment is one the uploads hold; an opening's id
//! is recomputed; only the conversations a post names are read.
//!
//! Each line taken is held before the next is judged, so what the rule keeps for one line holds
//! for any post, however a writer splits or orders its lines.
//!
//! A line resting on an unheld line waits, and one resting on a refused line is refused with it.
//! Of two conflicting concurrent lines the first by arrival is taken, within one post the first by
//! name.
//!
//! By design, and not a gap:
//! - a tab may say, stop or ask again in any conversation the store holds: the store is the
//!   owner's;
//! - a stop settles everything beneath it, other devices' questions included;
//! - an attachment is matched by its id against the whole upload store, not against uploads of
//!   the posting device.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use suzatary_conversations::secretary::queue;

use prodrome_dag::dag::Dag;
use prodrome_data::change::Change;
use prodrome_data::event::{Envelope, Hash};
use prodrome_data::literal::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_held::attachment::{Attachment, AttachmentId};
use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_memory::secretary::words::{Gone, Words};
use suzatary_signatures::signatures::Seat;
use suzatary_store::admission::Ruling;
use suzatary_store::admission::{Admission, Beside, Inadmissible, Post};

use crate::secretary::claim::Standing;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_shape::LineShape;
use crate::secretary::saying::Saying;
use crate::secretary::skew::Skew;
use crate::secretary::transcript::{Transcript, Write};
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Rewind, Title};
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_memory::secretary::carried::Carried;
use suzatary_memory::secretary::line_words::LineWords;

pub const MAX_ATTACHMENTS_PER_TURN: usize = 4;

/// The most bytes of words one person's turn holds.
pub(crate) const MAX_SAID_BYTES: usize = 32_768;

#[derive(Debug, Clone, Copy)]
pub enum LinesRule {}

/// Each slot's key, in slot order, by the name of the line it opens.
#[derive(Debug, Default)]
pub struct SlotKeys(BTreeMap<Hash, Vec<TextKey>>);

impl SlotKeys {
    /// The keys of the line named `name`, in slot order; none for a line posted without any.
    #[must_use]
    pub fn of(&self, name: &Hash) -> &[TextKey] {
        self.0.get(name).map_or(&[], Vec::as_slice)
    }

    pub fn names(&self) -> impl Iterator<Item = &Hash> {
        self.0.keys()
    }
}

impl Beside for SlotKeys {
    fn parse(posted: &BTreeMap<String, Vec<String>>) -> Result<Self, String> {
        posted
            .iter()
            .map(|(name, keys)| {
                let name = Hash::new(name.clone()).map_err(|_| "a key's line is no object name")?;
                let keys = keys
                    .iter()
                    .map(|key| TextKey::from_hex(key).ok_or("a slot's key is not 64 hex digits"))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((name, keys))
            })
            .collect::<Result<_, &str>>()
            .map(Self)
            .map_err(str::to_owned)
    }
}

/// What the lines' rule reads of the lines: every conversation they hold, the name of every key a
/// line of theirs was sealed under (the keys an erasure destroyed among them), and each
/// conversation a post names read as a transcript.
pub struct Held<'e> {
    conversations: BTreeSet<ConversationId>,
    keys: BTreeSet<KeyName>,
    lines: BTreeMap<&'e ConversationId, Vec<(&'e Hash, &'e Change<LineEvent>)>>,
    copied: RefCell<BTreeMap<ConversationId, Rc<Lines>>>,
    taken: Lines,
    read: RefCell<BTreeMap<ConversationId, Option<Rc<Transcript>>>>,
    /// The line being judged, by name, and its conversation as read with it taken.
    after: RefCell<Option<(Hash, Option<Rc<Transcript>>)>>,
}

type Lines = Vec<(Hash, Change<LineEvent>)>;

impl Held<'_> {
    /// Conversation `id` as the lines held read it, without its words; none for one that does
    /// not read.
    fn transcript(&self, id: &ConversationId) -> Option<Rc<Transcript>> {
        self.read
            .borrow_mut()
            .entry(id.clone())
            .or_insert_with(|| self.read_with(id, None).map(Rc::new))
            .clone()
    }

    /// Conversation `id` as the lines held read it with `line` taken too, without its words; read
    /// afresh and never kept.
    fn read_with(
        &self,
        id: &ConversationId,
        line: Option<(&Hash, &Change<LineEvent>)>,
    ) -> Option<Transcript> {
        // Copied out once a post: a transcript reads them many times, faster from one block.
        let copied = Rc::clone(
            self.copied
                .borrow_mut()
                .entry(id.clone())
                .or_insert_with(|| {
                    Rc::new(
                        self.lines
                            .get(id)
                            .into_iter()
                            .flatten()
                            .map(|(name, change)| ((*name).clone(), (*change).clone()))
                            .collect(),
                    )
                }),
        );
        let held = copied.iter().map(|(name, change)| (name, change));
        let taken = self.taken.iter().map(|(name, change)| (name, change));
        let writes: Vec<Write<'_>> = held
            .chain(
                taken
                    .chain(line)
                    .filter(|(_, change)| change.event.conversation() == id),
            )
            .map(|(name, change)| Write {
                name,
                line: &change.event,
                parents: &change.deps,
            })
            .collect();
        let keyless = |_: &KeyName| -> Words<TextKey> { Gone::Missing.into() };
        Transcript::of(id, &writes, &keyless).ok()
    }

    /// The conversation of the line `name` judges, as read with that line taken; read once a line.
    fn after(&self, name: &Hash, change: &Change<LineEvent>) -> Option<Rc<Transcript>> {
        let mut after = self.after.borrow_mut();
        match after.as_ref() {
            Some((judged, read)) if judged == name => read.clone(),
            Some(_) | None => {
                let read = self
                    .read_with(change.event.conversation(), Some((name, change)))
                    .map(Rc::new);
                *after = Some((name.clone(), read.clone()));
                read
            }
        }
    }

    fn owed(&self, id: &ConversationId) -> usize {
        self.transcript(id).map_or(0, |read| read.owed().len())
    }
}

/// What the lines' rule reads beyond the lines.
pub trait World {
    fn now(&self) -> Datetime;

    /// The seat a line signed by `signer` speaks from: the operator's for the deployment's root
    /// key, a device's for any other.
    fn seat(&self, signer: &prodrome_data::sign::PublicKey) -> Seat;

    /// The upload `id` as a person's turn holds it, or none where the box holds no such upload.
    fn upload(&self, id: &AttachmentId) -> Result<Option<Upload>, String>;
}

/// An upload as a person's turn holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upload {
    /// On the box: a turn names it under a name of the tab's.
    Here(Attachment),
    /// In the owner's Drive: a turn holds it exactly as it was kept.
    Private(Attachment),
}

impl Admission for LinesRule {
    type Schema = LineEvent;
    type Beside = SlotKeys;
    type Held<'e> = Held<'e>;
    type World = dyn World;

    fn held(history: &Dag<LineEvent>) -> Held<'_> {
        let mut conversations = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut lines: BTreeMap<&ConversationId, Vec<_>> = BTreeMap::new();
        for (name, object) in history.objects() {
            if let Some(line) = object.event() {
                conversations.insert(line.conversation().clone());
                keys.extend(line.keys().cloned());
            }
            if let Envelope::Change(change) = object {
                lines
                    .entry(change.event.conversation())
                    .or_default()
                    .push((name, change));
            }
        }
        Held {
            conversations,
            keys,
            lines,
            copied: RefCell::new(BTreeMap::new()),
            taken: Vec::new(),
            read: RefCell::new(BTreeMap::new()),
            after: RefCell::new(None),
        }
    }

    fn take(held: &mut Held<'_>, name: &Hash, change: &Change<LineEvent>) {
        let conversation = change.event.conversation();
        held.conversations.insert(conversation.clone());
        held.keys.extend(change.event.keys().cloned());
        held.read.borrow_mut().remove(conversation);
        *held.after.borrow_mut() = None;
        held.taken.push((name.clone(), change.clone()));
    }

    fn refuses(change: &Change<LineEvent>, post: &Post<'_, '_, Self>) -> Option<Inadmissible> {
        let line = &change.event;
        let keys = post.beside.of(post.name);
        let held = post.held;
        written(line)
            .err()
            .or_else(|| early(line, post.world).err())
            .or_else(|| fresh(line, held).err())
            .or_else(|| begins(line, held).err())
            .or_else(|| opened(change, held).err())
            .or_else(|| stamped(line, change, post).err())
            .or_else(|| sealed(line, keys).err())
            .or_else(|| said(change, keys, post).err())
            .or_else(|| stops(change, held).err())
            .or_else(|| again(change, post).err())
            .or_else(|| room(change, post).err())
    }
}

/// Whether `line` was written here and not carried: the digest reads the object a carried line
/// names as the line's leaf.
fn written(line: &LineEvent) -> Result<(), Inadmissible> {
    match line.carried() {
        Carried::Written => Ok(()),
        Carried::From(_) => Err(Inadmissible::finally(
            "a tab's line is written here, and this one says it was carried from another store",
        )),
    }
}

/// Whether `line` is dated no further ahead of the box's clock than [`Skew`] allows; a line dated
/// in the past is taken as it is.
fn early(line: &LineEvent, world: &dyn World) -> Result<(), Inadmissible> {
    let ahead = micros(line.at())
        .zip(micros(world.now()))
        .map(|(at, now)| (at - now) / 1_000_000);
    match ahead.and_then(Skew::of) {
        Some(skew @ Skew::Ahead { .. }) => Err(Inadmissible::later(skew.said())),
        Some(Skew::Behind { .. }) | None => Ok(()),
    }
}

/// An instant's microseconds since the epoch, in UTC, where it names a day.
fn micros(at: Datetime) -> Option<i64> {
    chrono::NaiveDate::from_ymd_opt(at.year(), at.month(), at.day())?
        .and_hms_micro_opt(at.hour(), at.minute(), at.second(), at.microsecond())
        .map(|t| t.and_utc().timestamp_micros())
}

/// Whether every key `line` was sealed under is fresh: one no line the store holds names, nor
/// another slot of this line. A held key taken again would open its words to whoever posted it,
/// and an erasure names its words by the key alone.
fn fresh(line: &LineEvent, held: &Held<'_>) -> Result<(), Inadmissible> {
    if line.keys().any(|key| held.keys.contains(key)) {
        return Err(Inadmissible::finally(
            "its words are sealed under a key a line the box holds already names; a tab seals under a key of its own",
        ));
    }
    let mut own = BTreeSet::new();
    if line.keys().all(|key| own.insert(key)) {
        Ok(())
    } else {
        Err(Inadmissible::finally(
            "two of its slots are sealed under one key; every slot has a key of its own",
        ))
    }
}

/// Whether `line` is a shape a tab writes, and begins or names its conversation rightly: an
/// opening under its own id, of a conversation the store does not hold; a line of one it holds.
fn begins(line: &LineEvent, held: &Held<'_>) -> Result<(), Inadmissible> {
    let conversation = line.conversation();
    match line.shape() {
        LineShape::Opened { kind, .. } if *kind != Some(Kind::Topic) => Err(Inadmissible::finally(
            "a tab opens a topic, and the box opens every other kind of conversation",
        )),
        LineShape::Opened { model, .. } if !model.is_empty() => Err(Inadmissible::finally(
            "a tab's opening names no model: the box says which model answers",
        )),
        LineShape::Opened { .. } | LineShape::SetUp { .. } => {
            if *conversation != line.begun_id() {
                Err(Inadmissible::finally(
                    "a new conversation is named by its opening line, and this one names another",
                ))
            } else if held.conversations.contains(conversation) {
                Err(Inadmissible::finally(format!(
                    "the conversation {conversation} already exists"
                )))
            } else {
                Ok(())
            }
        }
        LineShape::Titled { .. }
        | LineShape::Said {
            turn: TurnShape::Person { .. },
            ..
        } => {
            if held.conversations.contains(conversation) {
                Ok(())
            } else {
                Err(Inadmissible::finally(format!(
                    "no conversation {conversation} to {}",
                    if matches!(line.shape(), LineShape::Titled { .. }) {
                        "rename"
                    } else {
                        "say it in"
                    }
                )))
            }
        }
        LineShape::Stopped { .. } | LineShape::Again { .. } => {
            if held.conversations.contains(conversation) {
                Ok(())
            } else {
                Err(Inadmissible::finally(format!(
                    "no conversation {conversation} to write to"
                )))
            }
        }
        LineShape::Said { .. }
        | LineShape::Proposed { .. }
        | LineShape::Reviewed { .. }
        | LineShape::ReviewFailed { .. }
        | LineShape::Rewound { .. }
        | LineShape::MemorySeen { .. }
        | LineShape::Session { .. }
        | LineShape::Unheard { .. }
        | LineShape::Kept { .. }
        | LineShape::Scoped { .. }
        | LineShape::Coding { .. }
        | LineShape::Erased { .. }
        | LineShape::Resealed { .. }
        | LineShape::Began { .. } => Err(Inadmissible::finally(format!(
            "the box alone writes a {} line",
            line.shape().name()
        ))),
    }
}

/// Whether a line other than an opening rests on its conversation's opening.
fn opened(change: &Change<LineEvent>, held: &Held<'_>) -> Result<(), Inadmissible> {
    let line = &change.event;
    let conversation = line.conversation();
    if matches!(
        line.shape(),
        LineShape::Opened { .. } | LineShape::SetUp { .. }
    ) {
        return Ok(());
    }
    let rests = held.transcript(conversation).is_some_and(|read| {
        read.beneath_names(&change.deps).into_iter().any(|at| {
            matches!(
                read.lines()[at].entry,
                Entry::Opened { .. } | Entry::SetUp { .. }
            )
        })
    });
    if rests {
        Ok(())
    } else {
        Err(Inadmissible::finally(format!(
            "a line of {conversation} rests on its opening, and this one does not"
        )))
    }
}

/// Whether `line`'s stamp is what its signer proves: a rename, a stop and an `Again` name their
/// poster, the actor the shared rule took the change as; a person's turn is stamped as the poster
/// at the seat its signer speaks from, never as the box's or as pointing at an unconfirmed
/// todo, which the box judges at the claim.
fn stamped(
    line: &LineEvent,
    change: &Change<LineEvent>,
    post: &Post<'_, '_, LinesRule>,
) -> Result<(), Inadmissible> {
    use prodrome_data::schema::Schema as _;
    let poster = change.event.actor().as_str();
    let someone_else =
        || Inadmissible::finally("a line is the poster's own, and this one names someone else");
    match line.shape() {
        LineShape::Titled { by } => match by {
            Namer::Person { actor } if actor.as_str() == poster => Ok(()),
            Namer::Person { .. } | Namer::Job { .. } => Err(someone_else()),
        },
        LineShape::Stopped { by } | LineShape::Again { by, .. } if by.as_str() == poster => Ok(()),
        LineShape::Stopped { .. } | LineShape::Again { .. } => Err(someone_else()),
        LineShape::Said { by, .. } => {
            let signed = |seat| *by == Some(Stamp::Actor(Speaker::at(poster, seat)));
            if post
                .signers
                .iter()
                .any(|signer| signed(post.world.seat(signer)))
            {
                Ok(())
            } else {
                Err(Inadmissible::finally(
                    "a person's turn is stamped as its poster where the post came from",
                ))
            }
        }
        LineShape::Opened { .. }
        | LineShape::SetUp { .. }
        | LineShape::Proposed { .. }
        | LineShape::Reviewed { .. }
        | LineShape::ReviewFailed { .. }
        | LineShape::Rewound { .. }
        | LineShape::MemorySeen { .. }
        | LineShape::Session { .. }
        | LineShape::Unheard { .. }
        | LineShape::Kept { .. }
        | LineShape::Scoped { .. }
        | LineShape::Coding { .. }
        | LineShape::Erased { .. }
        | LineShape::Resealed { .. }
        | LineShape::Began { .. } => Ok(()),
    }
}

/// Whether each slot of `line` opens with its posted key and holds words as the box would seal
/// them: under the current rules (else outdated), and unchanged by a scrub under them.
fn sealed(line: &LineEvent, keys: &[TextKey]) -> Result<(), Inadmissible> {
    let slots = line.shape().slots();
    if keys.len() != slots {
        return Err(Inadmissible::finally(format!(
            "a {} line has {slots} slots of words, and {} keys came with it",
            line.shape().name(),
            keys.len()
        )));
    }
    for (slot, (key, under)) in keys.iter().zip(line.scrubbed_under()).enumerate() {
        let Some(words) = line.unscrubbed(slot, key) else {
            return Err(Inadmissible::finally(format!(
                "slot {slot} does not open with the key posted for it"
            )));
        };
        if !under.is_current(()) {
            return Err(Inadmissible {
                grounds: Ruling::Outdated,
                reason: "this page is older than the box: reload to send it".to_owned(),
            });
        }
        if !matches!(words.scrubbed(), Ok(scrubbed) if scrubbed == words) {
            return Err(Inadmissible::finally(format!(
                "slot {slot}'s words are not as the box would seal them"
            )));
        }
        fits(line.shape(), &words)?;
    }
    Ok(())
}

/// Whether `words` are what a line of `shape` says, read as the box reads it: a rename's are a
/// [`Title`]; an opening's are empty or a [`Title`]; a setup's are its template's own name; a
/// person's are a turn's ([`said`] reads the rest).
fn fits(shape: &LineShape, words: &LineWords) -> Result<(), Inadmissible> {
    if let (LineShape::Said { .. }, LineWords::Person { .. }) = (shape, words) {
        return Ok(());
    }
    let LineWords::Title { title } = words else {
        return Err(Inadmissible::finally(
            "its words are not what its line says",
        ));
    };
    let titled = || match Title::new(title.as_str()) {
        Ok(read) if read.as_str() == title => Ok(()),
        Ok(_) => Err(Inadmissible::finally(
            "a title is held as it reads, with no space around it",
        )),
        Err(refused) => Err(Inadmissible::finally(format!(
            "its title is refused: {refused}"
        ))),
    };
    match shape {
        LineShape::Opened { .. } if title.is_empty() => Ok(()),
        LineShape::Titled { .. } | LineShape::Opened { .. } => titled(),
        LineShape::SetUp { template } if *title == template.template().called() => Ok(()),
        LineShape::SetUp { .. } => Err(Inadmissible::finally(
            "a setup is titled by its template, and this one names another title",
        )),
        LineShape::Said { .. }
        | LineShape::Proposed { .. }
        | LineShape::Reviewed { .. }
        | LineShape::ReviewFailed { .. }
        | LineShape::Stopped { .. }
        | LineShape::Rewound { .. }
        | LineShape::MemorySeen { .. }
        | LineShape::Session { .. }
        | LineShape::Unheard { .. }
        | LineShape::Kept { .. }
        | LineShape::Scoped { .. }
        | LineShape::Coding { .. }
        | LineShape::Erased { .. }
        | LineShape::Resealed { .. }
        | LineShape::Began { .. }
        | LineShape::Again { .. } => Err(Inadmissible::finally(format!(
            "the box alone writes a {} line",
            shape.name()
        ))),
    }
}

/// Whether a person's turn is one the box would write: words within their bounds, and each file
/// the one the uploads hold.
fn said(
    change: &Change<LineEvent>,
    keys: &[TextKey],
    post: &Post<'_, '_, LinesRule>,
) -> Result<(), Inadmissible> {
    let line = &change.event;
    if !matches!(line.shape(), LineShape::Said { .. }) {
        return Ok(());
    }
    let posted = Posted(keys);
    let Entry::Said {
        turn: Turn::Person {
            text, attachments, ..
        },
        ..
    } = line.entry(&posted)
    else {
        return Err(Inadmissible::finally(
            "the box alone writes a turn that is not a person's",
        ));
    };
    if text.trim().is_empty() && attachments.is_empty() {
        return Err(Inadmissible::finally("say something"));
    }
    if attachments.len() > MAX_ATTACHMENTS_PER_TURN {
        return Err(Inadmissible::finally(format!(
            "at most {MAX_ATTACHMENTS_PER_TURN} files in one turn"
        )));
    }
    if text.len() > MAX_SAID_BYTES {
        return Err(Inadmissible::finally(format!(
            "a turn holds at most {MAX_SAID_BYTES} bytes of words"
        )));
    }
    for attachment in &attachments {
        held_upload(attachment, post.world)?;
    }
    Ok(())
}

/// Whether the conversation has room for what `change` makes owed: a line that leaves more turns
/// owed than before is taken only while the owed stay within [`queue::MAX_PENDING`].
fn room(change: &Change<LineEvent>, post: &Post<'_, '_, LinesRule>) -> Result<(), Inadmissible> {
    if !matches!(
        change.event.shape(),
        LineShape::Said {
            turn: TurnShape::Person { .. },
            ..
        } | LineShape::Again { .. }
    ) {
        return Ok(());
    }
    let id = change.event.conversation();
    let owed = post.held.owed(id);
    let after = post
        .held
        .after(post.name, change)
        .map_or(owed, |read| read.owed().len());
    if after > owed && after > queue::MAX_PENDING {
        return Err(Inadmissible::finally(format!(
            "{owed} lines already wait for the secretary; say it after they are answered"
        )));
    }
    Ok(())
}

/// Whether `posted` is the upload the box holds under its id: a file on the box under the name
/// the tab gave it, a private file exactly as it was kept.
fn held_upload(posted: &Attachment, world: &dyn World) -> Result<(), Inadmissible> {
    let held = world
        .upload(&posted.id)
        .map_err(Inadmissible::finally)?
        .ok_or_else(|| Inadmissible::finally(format!("no upload {} on the box", posted.id)))?;
    let read = match held {
        Upload::Here(attachment) => attachment.named(posted.name.as_deref().unwrap_or("")).ok(),
        Upload::Private(attachment) => Some(attachment),
    };
    if read.as_ref() == Some(posted) {
        Ok(())
    } else {
        Err(Inadmissible::finally(format!(
            "the upload {} is not as the box holds it",
            posted.id
        )))
    }
}

/// Whether a stop stops something: a turn beneath it that is owed or a running round covers.
fn stops(change: &Change<LineEvent>, held: &Held<'_>) -> Result<(), Inadmissible> {
    if !matches!(change.event.shape(), LineShape::Stopped { .. }) {
        return Ok(());
    }
    let nothing = || Inadmissible::finally("nothing here is being answered, so nothing stops");
    let read = held
        .transcript(change.event.conversation())
        .ok_or_else(nothing)?;
    let beneath = read.beneath_names(&change.deps);
    let waiting = read.lines().iter().enumerate().any(|(at, written)| {
        beneath.contains(&at)
            && matches!(
                read.claims().standing(at),
                Standing::Queued | Standing::Claimed { .. }
            )
            && is_turn(&written.entry)
    });
    if waiting { Ok(()) } else { Err(nothing()) }
}

/// Whether a line that asks again does so of a conversation as the tab read it: a regenerate's
/// `Again`, an edit's, and the edit's own question, which takes up an edit's `Again` (only that
/// line sets anything aside, so it is judged by its `Again`'s checks as it arrives).
fn again(change: &Change<LineEvent>, post: &Post<'_, '_, LinesRule>) -> Result<(), Inadmissible> {
    use prodrome_data::schema::Schema as _;
    let refused = |why: &str| Err(Inadmissible::finally(why));
    let id = change.event.conversation();
    let Some(read) = post.held.transcript(id) else {
        return match change.event.shape() {
            LineShape::Again { .. } => refused("no conversation to ask again in"),
            _ => Ok(()),
        };
    };
    let lines = read.lines();
    let (question, how, deps): (
        &prodrome_data::event::Hash,
        Rewind,
        Vec<prodrome_data::event::Hash>,
    ) = if let LineShape::Again { question, how, .. } = change.event.shape() {
        (question, *how, change.deps.clone())
    } else {
        let edits = matches!(
            change.event.shape(),
            LineShape::Said {
                saying: Saying::Edit,
                ..
            }
        );
        let Some((edit, question)) = taken_up(&read, change, post) else {
            return if edits {
                refused(
                    "an edit's question takes up the edit it rests on alone, and this one takes up none",
                )
            } else {
                Ok(())
            };
        };
        // An edit is one person's act: another actor's question would set the first's aside on
        // its own word.
        if lines[edit].line.actor() != change.event.actor() {
            return refused(
                "an edit's question is its edit's writer's, and this edit is someone else's",
            );
        }
        let deps = lines[edit]
            .rests_on
            .iter()
            .map(|&at| lines[at].name.clone())
            .collect();
        (question, Rewind::Edit, deps)
    };
    let Some(asked) = lines.iter().position(|written| written.name == *question) else {
        return refused("its question is no line of this conversation");
    };
    let stored = read.stored();
    let live_turn = |at: usize| read.turn_of(at).filter(|&turn| stored.is_live(turn));
    let person = |at: usize| {
        matches!(
            lines[at].line.shape(),
            LineShape::Said {
                turn: TurnShape::Person { .. },
                ..
            }
        )
    };
    if !person(asked) || live_turn(asked).is_none() {
        return refused("only a live question of a person can be asked again");
    }
    if matches!(read.claims().standing(asked), Standing::Claimed { .. }) {
        return refused("the answer is still coming: stop it first");
    }
    if how == Rewind::Regenerate && read.claims().standing(asked) != Standing::Settled {
        return refused("this question has no answer yet to ask again");
    }
    let beneath = read.beneath_names(&deps);
    let descends = read.descending(asked);
    let moved_on = descends
        .iter()
        .any(|&at| live_turn(at).is_some() && !beneath.contains(&at));
    if moved_on {
        return refused("the conversation moved on since this page read it");
    }
    let wrote = descends.iter().any(|&at| {
        live_turn(at).is_some() && writes(lines[at].line.shape(), stored, live_turn(at))
            || matches!(lines[at].line.shape(), LineShape::Proposed { .. })
    });
    if wrote {
        return refused(
            "this answer wrote something (a todo, a memory, a proposal) or holds a second \
             message, and asking again never hides one",
        );
    }
    if reopens(&read, change, post, question) {
        return refused(
            "the answer it sets aside also settled another question, and asking again \
             reopens no other",
        );
    }
    Ok(())
}

/// The position, in `read`, of an edit's `Again` that read as nothing before `change` and sets its
/// question aside with `change` taken, beside that question.
fn taken_up<'r>(
    read: &'r Transcript,
    change: &Change<LineEvent>,
    post: &Post<'_, '_, LinesRule>,
) -> Option<(usize, &'r prodrome_data::event::Hash)> {
    let after = post.held.after(post.name, change)?;
    let lines = read.lines();
    (0..lines.len()).find_map(|at| {
        if let LineShape::Again {
            question,
            how: Rewind::Edit,
            ..
        } = lines[at].line.shape()
            && crate::secretary::transcript::inert(lines, at)
            && after
                .lines()
                .iter()
                .position(|written| written.name == lines[at].name)
                .is_some_and(|then| !crate::secretary::transcript::inert(after.lines(), then))
        {
            Some((at, question))
        } else {
            None
        }
    })
}

/// Whether taking `change` leaves owed a live question other than `question` that the conversation
/// held settled before, which a third run could send or the person had stopped. Only the question
/// asked again may be owed again: a close or stop set aside withdraws what it settled and spends
/// no attempt.
fn reopens(
    read: &Transcript,
    change: &Change<LineEvent>,
    post: &Post<'_, '_, LinesRule>,
    question: &prodrome_data::event::Hash,
) -> bool {
    let Some(after) = post.held.after(post.name, change) else {
        return false;
    };
    let stored = read.stored();
    let lines = read.lines();
    let settled: BTreeSet<&prodrome_data::event::Hash> = read
        .turn_lines()
        .into_iter()
        .enumerate()
        .filter(|&(turn, name)| {
            *name != *question
                && stored.is_live(turn)
                && lines
                    .iter()
                    .position(|written| written.name == *name)
                    .is_some_and(|at| read.claims().standing(at) == Standing::Settled)
        })
        .map(|(_, name)| name)
        .collect();
    after.owed().iter().any(|(turn, _)| {
        after
            .turn_lines()
            .get(*turn)
            .is_some_and(|name| settled.contains(name))
    })
}

/// Whether a turn set aside would hide a write: a verb that writes, an act that is no stop, or a
/// second person's turn.
fn writes(
    shape: &LineShape,
    stored: &suzatary_conversations::secretary::store::Stored,
    turn: Option<usize>,
) -> bool {
    let LineShape::Said { turn: said, .. } = shape else {
        return false;
    };
    match said {
        TurnShape::Model { .. } => false,
        TurnShape::Tool { verb, outcome, .. } => {
            !outcome.refused()
                && verb.verb().is_some_and(|verb| {
                    verb.powers().effect
                        != suzatary_conversations::secretary::verb_name::Effect::Reads
                })
        }
        TurnShape::Act { .. } => !turn.is_some_and(|turn| stored.stops.contains(&turn)),
        TurnShape::Person { .. } => true,
    }
}

fn is_turn(entry: &Entry) -> bool {
    matches!(entry, Entry::Said { .. } | Entry::Stopped { .. })
}

/// The keys posted beside a line, read as the slots they open.
struct Posted<'k>(&'k [TextKey]);

impl suzatary_memory::secretary::slots::Slots for Posted<'_> {
    fn words(
        &self,
        name: &KeyName,
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords> {
        self.0
            .iter()
            .find(|key| key.name() == *name)
            .and_then(open)
            .unwrap_or_else(|| Gone::Missing.into())
    }
}
