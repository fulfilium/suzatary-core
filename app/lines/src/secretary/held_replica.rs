//! The lines a tab's core holds, whole: the rows of their skeleton as its pull moves them
//! ([`LinePulling`]), and what the landed row means, read through [`Outlines`] (its nodes) and
//! [`HeldLines`] (the prints, opened with its keys). Every move of the rows answers a
//! [`LineFrame`] and the conversations it moved, and the readings are told the landed row's
//! readable nodes and keys after each, so they read that row and nothing else. A print is content
//! held by name, kept whatever the rows name; a line every key of whose words is let go is kept
//! as its husk ([`LineReplica`]), and its print is the device's to delete.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::secretary::held_lines::HeldLines;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_form::{Husked, LineDelta, LineFrame, LinePulling, LineReplica};
use crate::secretary::outlines::{NotANode, Outlines, Parsed, Scope};
use crate::secretary::own_line::OwnLine;
use crate::wire::lines::{Heads, SkeletonNode, SkeletonPayload};
use prodrome_core::event::{Actor, Envelope};
use suzatary_base::answer::Renderable;
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::ConversationId;
use suzatary_connect::connect::TemplateId;
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Title};
use suzatary_memory::secretary::key_ring::KeyRing;
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_memory::secretary::words::Words;
use suzatary_store::replica::held::Node as _;
use suzatary_store::replica::held::Said;
use suzatary_store::replica::row::Keys as _;
use suzatary_store::replica::writes::{Answer, Own, Writes};

/// The lines held: the rows, what this tab wrote, and the readings of the landed row with the
/// own lines it does not hold yet.
#[derive(Default)]
pub struct HeldReplica {
    /// The prints and the keys of their words, the landed row's keys and the own lines'.
    pub lines: HeldLines,
    outlines: Outlines,
    kept: LineReplica,
    own: Writes<OwnLine>,
    /// The own lines refused for good whose refusal the person acted on (dismissed it, or took its
    /// words back into the composer): a set that only grows, read from the device's row.
    acted: BTreeSet<Hash>,
}

/// The box's last ruling on a line this device wrote and the box does not hold: the line stays a
/// fact on the device whatever the ruling, and a page says its words from this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ruling {
    /// Not answered yet: it goes with the next hand-over.
    Unsent,
    /// Refused until its signer is trusted.
    Trust,
    /// Not taken yet, in the rule's words: posted again.
    Later { why: String },
    /// Refused for good, in the box's words, with what it gives the person back.
    Refused {
        why: String,
        words: crate::secretary::own_line::Words,
    },
}

/// A line a tab seals.
#[derive(Debug, Clone)]
pub enum Sealing {
    /// A conversation renamed.
    Rename {
        /// Which.
        conversation: ConversationId,
        /// Its new title.
        title: Title,
    },
    /// A topic opened under `title`; none leaves it for the title job.
    Open {
        /// Its title.
        title: Option<Title>,
    },
    /// A conversation set up from a template, titled by it.
    SetUp(TemplateId),
    /// A person's question in a conversation the tab reads, its own opening line among them.
    Say {
        conversation: ConversationId,
        question: Question,
    },
    /// A topic opened under `title` (none leaves it for the title job) and its first question,
    /// resting on the opening: one act of the person, so one move of the own writes.
    Begin {
        title: Option<Title>,
        question: Question,
    },
    /// The answer of a conversation the tab reads stopped.
    Stop { conversation: ConversationId },
}

/// A person's question as a tab seals it.
#[derive(Debug, Clone)]
pub struct Question {
    pub text: String,
    pub references: Vec<suzatary_base::reference::Reference>,
    /// Each file as the uploads hold it, named as the person named it.
    pub attachments: Vec<suzatary_held::attachment::Attachment>,
    /// Where the person asks from: a device, or the box's terminal.
    pub seat: suzatary_signatures::signatures::Seat,
}

impl Question {
    fn said(self, at: Datetime, actor: &Actor) -> Entry {
        Entry::Said {
            at,
            turn: suzatary_conversations::secretary::model::Turn::Person {
                text: self.text,
                references: self.references,
                attachments: self.attachments,
            },
            by: Some(suzatary_inbox::secretary::stamp::Stamp::Actor(
                suzatary_inbox::secretary::stamp::Speaker::at(actor.as_str(), self.seat),
            )),
        }
    }
}

/// A conversation as a tab reads it: whole, or lacking lines its skeleton names.
#[derive(Debug, Clone)]
pub enum Whole {
    /// Read from every line, or why its lines are no conversation.
    Read(
        Result<
            std::sync::Arc<crate::secretary::transcript::Transcript>,
            suzatary_conversations::secretary::store::NotAConversation,
        >,
    ),
    /// This many of its lines are not held yet: a reading asks for them first.
    Lacking(usize),
}

/// Why a tab cannot seal a line now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotSealed {
    /// The rows name no genesis of the lines yet: nothing was pulled.
    #[error("the lines are not on this device yet, so nothing is written to them")]
    Unpulled,
    /// The conversation renamed is not one the tab reads.
    #[error("no conversation {0} on this device")]
    Unknown(ConversationId),
    /// The line itself.
    #[error(transparent)]
    Line(#[from] crate::secretary::line_event::NotALine),
    /// The change over it.
    #[error("no change of the lines: {0}")]
    Change(String),
}

/// Which conversations a move of the landed row moved: every one, where the row was reset or
/// heard a later epoch; else those with a line come or gone, or a key of their words given or
/// taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    /// Every conversation.
    All,
    /// These.
    These(BTreeSet<ConversationId>),
}

/// What a record raised by [`HeldReplica::acted`] is refused as: the box refused it for good, and
/// another tab of the device told the person, who acted on it, or let it go.
pub const LET_GO: &str = "the box refused it for good, and another tab of this device let it go";

/// What the device may let go of its own records ([`HeldReplica::releasable`]): every record by
/// its print, and the names of those let go as refused, which the device keeps as acted on, so a
/// tab still holding one as unsent never posts it again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Released {
    /// Every record let go, by print.
    pub prints: BTreeSet<String>,
    /// Those let go as refused, by name.
    pub refused: BTreeSet<Hash>,
}

/// A move of the rows: the frame that makes it, and what it moved.
#[derive(Debug, Clone)]
pub struct Took {
    /// What another replica takes to make the same move.
    pub frame: LineFrame,
    /// What it moved.
    pub moved: Moved,
}

/// A skeleton's answer that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAnAnswer {
    /// A name that is no object's.
    #[error("the answer names {0:?}, which is no object's name")]
    Name(String),
    #[error("the answer names the key {0:?}, which is no key's name")]
    Key(String),
    /// A node that does not read.
    #[error(transparent)]
    Node(#[from] NotANode),
}

/// A husk the box answered in place of a line's print that this replica does not take.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotItsHusk {
    #[error("the box answered a husk for {0}, which names no line this replica reads")]
    Unknown(String),
    #[error("{0}: {1}")]
    Unread(String, suzatary_store::store::NotAHusk),
    #[error("the box's husk for {0} is not the line its node says")]
    Header(String),
    #[error("the box answered a husk for {0}, whose words a live key this replica holds opens")]
    Opens(String),
}

impl HeldReplica {
    /// The skeleton's nodes the landed row reads.
    #[must_use]
    pub fn outlines(&self) -> &Outlines {
        &self.outlines
    }

    /// The skeleton's nodes beside the prints, each to read the other by.
    pub fn reading(&mut self) -> (&Outlines, &mut HeldLines) {
        (&self.outlines, &mut self.lines)
    }

    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &LinePulling {
        &self.kept.rows
    }

    /// The replica as the device keeps it: the rows and the husks.
    #[must_use]
    pub fn kept(&self) -> &LineReplica {
        &self.kept
    }

    /// What this tab wrote, and how the box answered each.
    #[must_use]
    pub fn own(&self) -> &Writes<OwnLine> {
        &self.own
    }

    /// Whether the rows hold the line named `name`: the one test of a line the box took, whatever
    /// answer its own record carries (a post's answer may be lost after the box took it). Such a
    /// line has no ruling, and its record waits for nothing.
    #[must_use]
    pub fn holds(&self, name: &Hash) -> bool {
        self.kept.rows.holding().replica().read().holds(name)
    }

    /// Each own line of conversation `id` the box does not hold ([`HeldReplica::holds`]), by name,
    /// with the box's last ruling on it. A line refused for good, or resting on one, is ruled only
    /// while it gives the person words back and they have not acted on them; one with none is told
    /// and let go.
    #[must_use]
    pub fn rulings(&self, id: &ConversationId) -> Vec<(Hash, Ruling)> {
        use suzatary_store::replica::writes::Refusal;
        let held_back: BTreeMap<&Hash, String> = self
            .own
            .refusals()
            .into_iter()
            .map(|(own, _, reason)| (own.name(), reason))
            .collect();
        self.own
            .iter()
            .filter(|own| own.node.conversation() == Some(id.as_str()))
            .filter(|own| !self.holds(own.node.name()))
            .filter_map(|own| {
                let name = own.node.name();
                let ruling = match held_back.get(name) {
                    Some(_) if self.acted.contains(name) => return None,
                    Some(why) => Ruling::Refused {
                        why: why.clone(),
                        words: own.node.words()?,
                    },
                    None => match &own.answer {
                        Answer::Taken => return None,
                        Answer::Unsent => Ruling::Unsent,
                        Answer::Refused { kind, reason } => match kind {
                            Refusal::Trust => Ruling::Trust,
                            Refusal::Later => Ruling::Later {
                                why: reason.clone(),
                            },
                            // `Writes::refusals` holds every refusal for good: none reaches here.
                            Refusal::Final | Refusal::Outdated => return None,
                        },
                    },
                };
                Some((name.clone(), ruling))
            })
            .collect()
    }

    /// The topics this device began that the box refused: each conversation whose opening is one
    /// of this device's own lines refused for good, beside the box's sentence for it, while a line
    /// of it still gives the person words back they have not acted on; one they acted on whole is
    /// absent. Defined by what the device wrote, never by what the rows lack, which a reset or a
    /// pull in progress also lacks.
    #[must_use]
    pub fn refused_topics(&self) -> BTreeMap<ConversationId, String> {
        self.own
            .refusals()
            .into_iter()
            .filter(|(own, _, _)| own.opens() && !self.holds(own.name()))
            .filter_map(|(own, _, why)| {
                let id = ConversationId::new(own.conversation()?).ok()?;
                Some((id, why))
            })
            .filter(|(id, _)| {
                self.rulings(id)
                    .iter()
                    .any(|(_, ruling)| matches!(ruling, Ruling::Refused { .. }))
            })
            .collect()
    }

    /// Each own line refused for good, with words the person has not acted on, of a topic this
    /// device began that the box refused ([`HeldReplica::refused_topics`]): no conversation's
    /// page draws it, so the page that begins topics does.
    #[must_use]
    pub fn refused_elsewhere(&self) -> Vec<(Hash, Ruling)> {
        self.refused_topics()
            .keys()
            .flat_map(|id| self.rulings(id))
            .filter(|(_, ruling)| matches!(ruling, Ruling::Refused { .. }))
            .collect()
    }

    /// The person acted on the refusals of the own lines `names`, or the device let them go as
    /// refused (the device's row keeps them by name): what it moved. Only a refusal for good is
    /// acted on or let go as refused, so a record this tab still holds as unsent, or refused for
    /// now, because it missed the wake that told it otherwise is raised to a refusal for good
    /// ([`LET_GO`]): it is never posted again, and what rests on it is held back with it.
    pub fn acted(&mut self, names: &BTreeSet<Hash>) -> Moved {
        use suzatary_store::replica::writes::Refusal;
        let mut touched = BTreeSet::new();
        let mut raised = Vec::new();
        for own in self.own.iter() {
            let name = own.node.name();
            if !names.contains(name) {
                continue;
            }
            let behind = matches!(
                own.answer,
                Answer::Unsent
                    | Answer::Refused {
                        kind: Refusal::Trust | Refusal::Later,
                        ..
                    }
            );
            if self.acted.insert(name.clone()) || behind {
                touched.extend(
                    own.node
                        .conversation()
                        .and_then(|id| ConversationId::new(id).ok()),
                );
            }
            if behind {
                raised.push((
                    name.clone(),
                    Answer::Refused {
                        kind: Refusal::Final,
                        reason: LET_GO.to_owned(),
                    },
                ));
            }
        }
        if raised.is_empty() {
            return Moved::These(touched);
        }
        let _ = self.own.answered(raised);
        match self.moving(|_| ()).1 {
            Moved::All => Moved::All,
            Moved::These(more) => {
                touched.extend(more);
                Moved::These(touched)
            }
        }
    }

    /// `sealing` written by `actor` at `at`, its words sealed under keys drawn from `entropy`,
    /// resting on the heads of its conversation among the lines read: its lines, held and read at
    /// once in one move (a begun topic's opening and question together), the name of the first, and
    /// what it moved.
    ///
    /// # Errors
    /// [`NotSealed`]: nothing pulled yet, a rename of a conversation not read here, or words the
    /// door refuses; nothing is kept.
    pub fn seal(
        &mut self,
        sealing: Sealing,
        at: Datetime,
        actor: &Actor,
        entropy: &dyn Entropy,
    ) -> Result<(Vec<Own<OwnLine>>, Hash, Moved), NotSealed> {
        let genesis = self
            .kept
            .rows
            .landed()
            .replica()
            .read()
            .word()
            .and_then(|word| word.geneses.first().cloned())
            .ok_or(NotSealed::Unpulled)?;
        let own = |line: LineEvent,
                   keys: Vec<suzatary_memory::secretary::locked_text::TextKey>,
                   deps: Vec<Hash>|
         -> Result<OwnLine, NotSealed> {
            let change = prodrome_core::change::mk_change(genesis.clone(), deps, line)
                .map_err(|e| NotSealed::Change(e.to_string()))?;
            Ok(
                OwnLine::sealed(&Envelope::Change(change), &keys).unwrap_or_else(|_| {
                    unreachable!(
                        "a line this tab sealed reads back with the keys it sealed it under"
                    )
                }),
            )
        };
        let onto = |conversation: ConversationId, entry: Entry| {
            let (deps, held) = self.heads(&conversation)?;
            let (line, keys) =
                LineEvent::write(conversation, actor.clone(), held, &entry, entropy)?;
            own(line, keys, deps)
        };
        let opening = |title: Option<Title>| {
            let entry = Entry::Opened {
                at,
                title: Words::kept(title.map_or_else(String::new, String::from)),
                model: String::new(),
                kind: Kind::Topic,
            };
            let (line, keys) = LineEvent::begin(actor.clone(), &entry, entropy)?;
            Ok::<_, NotSealed>((line.begun_id(), own(line, keys, Vec::new())?))
        };
        let lines = match sealing {
            Sealing::Rename {
                conversation,
                title,
            } => vec![onto(
                conversation,
                Entry::Titled {
                    at,
                    title: Words::kept(title),
                    by: Namer::Person {
                        actor: actor.as_str().to_owned(),
                    },
                },
            )?],
            Sealing::Say {
                conversation,
                question,
            } => vec![onto(conversation, question.said(at, actor))?],
            Sealing::Begin { title, question } => {
                let (id, opened) = opening(title)?;
                // The question is the new conversation's second line, over its opening alone.
                let (line, keys) =
                    LineEvent::write(id, actor.clone(), 1, &question.said(at, actor), entropy)?;
                let said = own(line, keys, vec![opened.name().clone()])?;
                vec![opened, said]
            }
            Sealing::Stop { conversation } => vec![onto(
                conversation,
                Entry::Stopped {
                    at,
                    by: actor.as_str().to_owned(),
                },
            )?],
            Sealing::Open { title } => vec![opening(title)?.1],
            Sealing::SetUp(template) => {
                let entry = Entry::SetUp {
                    at,
                    title: Words::kept(template.template().called()),
                    template,
                };
                let (line, keys) = LineEvent::begin(actor.clone(), &entry, entropy)?;
                vec![own(line, keys, Vec::new())?]
            }
        };
        let name = lines[0].name().clone();
        self.lines.hold(
            lines
                .iter()
                .map(|own| (own.name().clone(), own.print().as_bytes().to_vec()))
                .collect(),
            &[],
        );
        let mut owned = self.own.write(lines);
        owned.extend(self.rested_on(&owned));
        let moved = self.moving(|_| ()).1;
        Ok((owned, name, moved))
    }

    /// Every own write the box has not taken that `owned` rests on, near or far, as it is held: kept
    /// again with the writes that rest on it, so a record another tab of the device let go before
    /// this seal read the row is back beside its dependent, and no kept line rests on a record the
    /// device no longer keeps. A record the box took, by its answer or by the rows, is not: the box
    /// holds it. One let go as refused is kept as refused for good, since the seal read the acts
    /// first ([`HeldReplica::acted`]), and so it is never posted again.
    fn rested_on(&self, owned: &[Own<OwnLine>]) -> Vec<Own<OwnLine>> {
        let by: BTreeMap<&Hash, &Own<OwnLine>> =
            self.own.iter().map(|own| (own.node.name(), own)).collect();
        let written: BTreeSet<&Hash> = owned.iter().map(|own| own.node.name()).collect();
        // Taken, by its answer or by the rows, which hold a line whose answer was lost.
        let taken =
            |own: &&Own<OwnLine>| own.answer == Answer::Taken || self.holds(own.node.name());
        let mut seen: BTreeSet<&Hash> = BTreeSet::new();
        let mut next: Vec<&Hash> = owned
            .iter()
            .flat_map(|own| own.node.rests_on().iter())
            .collect();
        while let Some(name) = next.pop() {
            if written.contains(name) || !seen.insert(name) {
                continue;
            }
            if let Some(own) = by.get(name).filter(|own| !taken(own)) {
                next.extend(own.node.rests_on().iter());
            }
        }
        seen.into_iter()
            .filter_map(|name| by.get(name))
            .filter(|own| !taken(own))
            .map(|own| (*own).clone())
            .collect()
    }

    /// The heads of `conversation` among the lines read, own lines included, and how many lines
    /// it holds.
    fn heads(&self, conversation: &ConversationId) -> Result<(Vec<Hash>, u32), NotSealed> {
        let lines: Vec<&crate::wire::lines::SkeletonNode> = self
            .outlines
            .given()
            .filter(|node| {
                matches!(&node.header, crate::wire::lines::NodeHeader::Line {
                    conversation: of, ..
                } if of == conversation.as_str())
            })
            .collect();
        if lines.is_empty() {
            return Err(NotSealed::Unknown(conversation.clone()));
        }
        let under: BTreeSet<&str> = lines
            .iter()
            .flat_map(|node| node.parents.iter().map(String::as_str))
            .collect();
        let deps = lines
            .iter()
            .filter(|node| !under.contains(node.hash.as_str()))
            .filter_map(|node| Hash::new(node.hash.clone()).ok())
            .collect();
        Ok((deps, u32::try_from(lines.len()).unwrap_or(u32::MAX)))
    }

    /// `owned` taken, whoever made it: another tab's writes, or the device's own read back; each
    /// line's print held. What moved, and how many of `owned` were new here or raised an answer.
    pub fn take_own(&mut self, owned: &[Own<OwnLine>]) -> (Moved, usize) {
        let fresh = self.own.moves(owned);
        let prints: Vec<(Hash, Vec<u8>)> = owned
            .iter()
            .filter(|own| matches!(own.node, OwnLine::Line { .. }))
            .map(|own| {
                (
                    own.node.name().clone(),
                    own.node.print().as_bytes().to_vec(),
                )
            })
            .collect();
        self.lines.hold(prints, &[]);
        self.own.apply(owned);
        (self.moving(|_| ()).1, fresh)
    }

    /// A move of the own writes made by `step`: what it joined in, and what it moved.
    pub fn answer_own(
        &mut self,
        step: impl FnOnce(&mut Writes<OwnLine>) -> Vec<Own<OwnLine>>,
    ) -> (Vec<Own<OwnLine>>, Moved) {
        let owned = step(&mut self.own);
        let moved = self.moving(|_| ()).1;
        (owned, moved)
    }

    /// Each own write the box took, by its answer or because the rows hold it ([`HeldReplica::holds`],
    /// the answer of a post that reached the box may be lost), beside whether the device may let
    /// its own record of it go:
    /// once the landed rows hold every key of a line, or the box retired it, since its record is
    /// the one other place they are kept; a signature's at once.
    #[must_use]
    pub fn own_taken(&self) -> Vec<(&OwnLine, bool)> {
        let landed = self.kept.rows.landed();
        let keys = landed.keys();
        // A key an erasure or a reseal retired is never served again: its record need not wait.
        let retired = self.outlines.retired(&self.pending(), &|_| true);
        self.own
            .iter()
            .filter(|own| own.answer == Answer::Taken || self.holds(own.node.name()))
            .map(|own| {
                let free = match &own.node {
                    OwnLine::Line { keys: opens, .. } => opens
                        .iter()
                        .all(|(name, _)| keys.holds(name) || retired.contains(name)),
                    OwnLine::Signature { .. } => true,
                };
                (&own.node, free)
            })
            .collect()
    }

    /// Every own record the device may let go, by its print, `told` naming the refusals the person
    /// was told of: a taken one whose keys the device holds elsewhere ([`HeldReplica::own_taken`]);
    /// a refused one the rows hold all the same ([`HeldReplica::holds`]); a refused one that gives
    /// the person words back once they acted on them, one that gives none once told, and any the
    /// device let go as refused before ([`HeldReplica::acted`]). None of
    /// these while an own record still held back rests on it, since what holds that one back is the
    /// record it rests on: letting the parent go would leave the child posted for ever, its words
    /// shown nowhere. The one owner of this answer; the device's row removes what it names.
    #[must_use]
    pub fn releasable(&self, told: &BTreeSet<String>) -> Released {
        let refused: BTreeMap<&Hash, &OwnLine> = self
            .own
            .refusals()
            .into_iter()
            .map(|(own, _, _)| (own.name(), own))
            .collect();
        let mut going: BTreeSet<&Hash> = refused
            .iter()
            .filter(|(name, own)| {
                self.holds(name)
                    || self.acted.contains(*name)
                    || (own.words().is_none() && told.contains(own.print()))
            })
            .map(|(name, _)| *name)
            .collect();
        loop {
            let blocked: Vec<&Hash> = going
                .iter()
                .copied()
                .filter(|parent| {
                    refused.iter().any(|(child, own)| {
                        !going.contains(child) && own.rests_on().contains(*parent)
                    })
                })
                .collect();
            if blocked.is_empty() {
                break;
            }
            for name in blocked {
                going.remove(name);
            }
        }
        Released {
            prints: self
                .own_taken()
                .into_iter()
                .filter(|(_, free)| *free)
                .map(|(own, _)| own.print().to_owned())
                .chain(going.iter().map(|name| refused[name].print().to_owned()))
                .collect(),
            refused: going
                .into_iter()
                .filter(|name| !self.holds(name))
                .cloned()
                .collect(),
        }
    }

    /// The own writes the box has not taken and may take, beside what is held, landed or not, in
    /// posts that each fit `fit` by the bytes `weight` gives a line, each line beside its signatures.
    #[must_use]
    pub fn unsent(
        &self,
        fit: suzatary_store::replica::writes::Fit,
        weight: impl Fn(&OwnLine) -> usize,
    ) -> Vec<Vec<&OwnLine>> {
        let holding = self.kept.rows.holding();
        let reading = holding.replica().read();
        self.own
            .pieces(|name| reading.holds(name), OwnLine::role, fit, weight)
    }

    /// The own lines no signature written here covers.
    #[must_use]
    pub fn unsigned(&self) -> Vec<&OwnLine> {
        let holding = self.kept.rows.holding();
        let reading = holding.replica().read();
        self.own.unsigned(|name| reading.holds(name), OwnLine::role)
    }

    /// The box's answers to the own writes.
    pub fn answered(&mut self, answers: Vec<(Hash, Answer)>) -> (Vec<Own<OwnLine>>, Moved) {
        self.answer_own(|own| own.answered(answers))
    }

    /// The piece the box answered, `answer` heard at `fetched_at` with the keys `keys`, kept
    /// beside the landed row; it moves no reading. The answer's own `keys` are not read: `keys`
    /// is what the piece holds. Heads the answer leaves unsaid are those of what is held now.
    ///
    /// # Errors
    /// [`NotAnAnswer`] for a name or a node that does not read; nothing is kept.
    pub fn piece(
        &mut self,
        answer: SkeletonPayload,
        keys: KeyRing,
        fetched_at: u64,
    ) -> Result<LineDelta, NotAnAnswer> {
        let SkeletonPayload {
            heads,
            geneses,
            epoch,
            nodes,
            since,
            frontier,
            ..
        } = answer;
        let heads = match heads {
            Heads::Held => self.kept.rows.holding().replica().read().heads(),
            Heads::These { heads } => names(heads)?,
        };
        let said = Said {
            heads,
            geneses: names(geneses)?,
            epoch,
            nodes: nodes
                .into_iter()
                .map(Parsed::of)
                .collect::<Result<_, _>>()?,
            since,
            digest: name(frontier.digest)?,
            count: u64::try_from(frontier.count).unwrap_or(u64::MAX),
        };
        Ok(self.kept.rows.piece(said, keys, fetched_at))
    }

    /// The pending pieces landed.
    pub fn land(&mut self) -> Took {
        self.took(LinePulling::land)
    }

    /// Every row reset, in the generation after both: a later epoch overtook what is held.
    pub fn start_over(&mut self) -> Took {
        self.took(LinePulling::start_over)
    }

    /// The keys `gone` let go, from every row, and every line the rows read each key of whose
    /// words is then let go kept as its husk ([`HeldReplica::bare`]).
    pub fn forget(&mut self, gone: BTreeSet<KeyName>) -> Took {
        let (delta, moved) = self.moving(|rows| rows.forget(gone));
        let names = self.bare();
        let husked: Vec<Husked> = self
            .lines
            .husk(&names)
            .into_iter()
            .map(|(name, parents, husk)| Husked {
                name,
                parents,
                husk,
            })
            .collect();
        for husked in &husked {
            self.kept.husks.insert(husked.name.clone(), husked.clone());
        }
        Took {
            frame: LineFrame { delta, husked },
            moved,
        }
    }

    /// The name of every line the rows read, landed or in a pull's pieces, held as a print with
    /// words each key of which no row holds: a line every slot of which is erased, which a purge
    /// keeps as its husk. A line a slot of which still opens is kept whole.
    #[must_use]
    pub fn bare(&self) -> BTreeSet<Hash> {
        let holding = self.kept.rows.holding();
        let ring = holding.keys().clone().union(&self.own_keys(holding.keys()));
        let pending: BTreeSet<Hash> = self.pending().iter().map(|n| n.name().clone()).collect();
        self.lines
            .bare(|key| ring.holds(key))
            .into_iter()
            .filter(|name| self.outlines.holds(name) || pending.contains(name))
            .collect()
    }

    /// A whole pull of the skeleton landed, each piece `ask` answers from the frontier named
    /// (`""` first).
    pub fn pulled<E: From<NotAnAnswer>>(
        mut ask: impl FnMut(&str) -> Result<SkeletonPayload, E>,
    ) -> Result<Self, E> {
        let mut replica = Self::default();
        let mut since = String::new();
        loop {
            let piece = ask(&since)?;
            let (rest, frontier) = (piece.rest, piece.frontier.digest.clone());
            let keys = piece
                .keys
                .iter()
                .map(|(name, key)| {
                    Ok((
                        KeyName::new(name.clone()).map_err(|_| NotAnAnswer::Key(name.clone()))?,
                        key.clone(),
                    ))
                })
                .collect::<Result<_, NotAnAnswer>>()?;
            replica.piece(piece, KeyRing::of(&keys), 0)?;
            if rest == 0 {
                let _ = replica.land();
                return Ok(replica);
            }
            since = frontier;
        }
    }

    /// A content answer taken: each literal held, each husk taken in its place.
    pub fn take(&mut self, content: Vec<crate::wire::lines::Content>) -> Result<Took, NotItsHusk> {
        let (mut prints, mut husks) = (Vec::new(), Vec::new());
        for object in content {
            match object {
                crate::wire::lines::Content::Literal { hash, literal } => {
                    prints.push((
                        Hash::new(hash.clone()).map_err(|_| NotItsHusk::Unknown(hash))?,
                        literal.into_bytes(),
                    ));
                }
                crate::wire::lines::Content::Husk { hash, husk } => {
                    husks.push((
                        Hash::new(hash.clone()).map_err(|_| NotItsHusk::Unknown(hash))?,
                        husk,
                    ));
                }
            }
        }
        self.lines.hold(prints, &[]);
        self.take_husks(husks)
    }

    /// The husks the box answered, each kept where it is its node's line and no live key held
    /// opens it, where a purge would have husked the print; else none is kept.
    pub fn take_husks(&mut self, given: Vec<(Hash, String)>) -> Result<Took, NotItsHusk> {
        let pending = self.pending();
        let ring = self.kept.rows.holding().keys().clone();
        // A key still held may be dead already, retired by a line held or a reseal the rows
        // read, and waiting on its purge, which fetches the lines naming it first.
        let mut dead = self.lines.dead(&|key| ring.holds(key));
        dead.extend(self.outlines.retired(&pending, &|key| ring.holds(key)));
        let husked = given
            .into_iter()
            .map(|(name, text)| {
                let shown = name.as_str().to_owned();
                let node = self
                    .outlines
                    .node(&name)
                    .or_else(|| {
                        pending
                            .iter()
                            .map(Parsed::given)
                            .find(|node| node.hash == shown)
                    })
                    .ok_or_else(|| NotItsHusk::Unknown(shown.clone()))?;
                let husk =
                    suzatary_store::store::Husk::<crate::secretary::line_event::LineEvent>::read(
                        &text,
                    )
                    .map_err(|e| NotItsHusk::Unread(shown.clone(), e))?;
                if crate::secretary::line_form::line_header(husk.event()) != node.header {
                    return Err(NotItsHusk::Header(shown));
                }
                if husk.opens(|key| ring.holds(key) && !dead.contains(key)) {
                    return Err(NotItsHusk::Opens(shown));
                }
                let parents = node
                    .parents
                    .iter()
                    .map(|parent| Hash::new(parent.clone()))
                    .collect::<Result<_, _>>()
                    .map_err(|_| NotItsHusk::Header(shown))?;
                Ok(Husked {
                    name,
                    parents,
                    husk,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.husked(husked.iter().cloned());
        let moved = Moved::These(
            husked
                .iter()
                .map(|h| h.husk.event().conversation().clone())
                .collect(),
        );
        // No row moves: the frame's delta forgets nothing, and its husks are the move.
        Ok(Took {
            frame: LineFrame {
                delta: LineDelta::Forgot(BTreeSet::new()),
                husked,
            },
            moved,
        })
    }

    /// `frame` taken: another tab's, or one read back from the log.
    pub fn apply(&mut self, frame: &LineFrame) -> Moved {
        let moved = self.moving(|rows| rows.apply(&frame.delta)).1;
        self.husked(frame.husked.iter().cloned());
        moved
    }

    /// The replica replaced by `kept`, as a warm load reads it; the prints are kept, and each of
    /// its husks stands in place of its line's print.
    pub fn load(&mut self, kept: LineReplica) -> Moved {
        let LineReplica { rows, husks } = kept;
        let moved = self.moving(|held| *held = rows).1;
        self.kept.husks = BTreeMap::new();
        self.husked(husks.into_values());
        moved
    }

    fn husked(&mut self, husks: impl IntoIterator<Item = Husked>) {
        let husks: Vec<Husked> = husks.into_iter().collect();
        self.lines.hold_husks(
            husks
                .iter()
                .map(|h| (h.name.clone(), h.parents.clone(), h.husk.clone()))
                .collect(),
        );
        self.kept
            .husks
            .extend(husks.into_iter().map(|h| (h.name.clone(), h)));
    }

    fn took(&mut self, step: impl FnOnce(&mut LinePulling) -> LineDelta) -> Took {
        let (delta, moved) = self.moving(step);
        Took {
            frame: LineFrame {
                delta,
                husked: Vec::new(),
            },
            moved,
        }
    }

    /// The keys of every own line that `held` lacks, less each a line held kills (an erasure, a
    /// reseal), as a landed key is let go. An own line's keys open its words after the box took
    /// it too: the tab holds its print, so it never fetches the line's content, nor a key the box
    /// would hand beside it, and the rows may never hold the key.
    fn own_keys(&self, held: &KeyRing) -> KeyRing {
        let mut keys = KeyRing::default();
        for written in self.own.iter() {
            if let OwnLine::Line { keys: opens, .. } = &written.node {
                keys = keys.union(opens);
            }
        }
        let already: BTreeSet<KeyName> = keys
            .iter()
            .map(|(name, _)| name.clone())
            .filter(|name| held.holds(name))
            .collect();
        let keys = keys.without(&already);
        if keys.is_empty() {
            return keys;
        }
        // What a line held kills, and what the box's rule retires over the skeleton the tab
        // holds, an erasure whose content it never fetched included.
        let mut dead = self.lines.dead(&|name| keys.holds(name));
        dead.extend(
            self.outlines
                .retired(&self.pending(), &|name| keys.holds(name)),
        );
        keys.without(&dead)
    }

    /// `step` taken, and the readings told the landed row it leaves.
    fn moving<A>(&mut self, step: impl FnOnce(&mut LinePulling) -> A) -> (A, Moved) {
        let landed = self.kept.rows.landed();
        let was = (landed.generation(), landed.replica().epoch());
        let taken = step(&mut self.kept.rows);
        let landed = self.kept.rows.landed();
        let reading = landed.replica().read();
        let mut own: BTreeMap<&Hash, &Parsed> = BTreeMap::new();
        for line in self
            .own
            .standing(|name| reading.holds(name), reading.gone())
        {
            if let OwnLine::Line { node, .. } = line {
                own.insert(node.name(), node);
            }
        }
        let came: Vec<Parsed> = reading
            .nodes()
            .chain(own.values().copied())
            .filter(|node| !self.outlines.holds(node.name()))
            .cloned()
            .collect();
        let went: Vec<Hash> = self
            .outlines
            .names()
            .filter(|name| !reading.holds(name) && !own.contains_key(name))
            .cloned()
            .collect();
        let mut touched = if came.is_empty() && went.is_empty() {
            BTreeSet::new()
        } else {
            self.outlines.hold(came, &went)
        };
        // After the outline holds what landed, so an erasure that came retires an own line's key.
        let keys = landed.keys().clone().union(&self.own_keys(landed.keys()));
        let rekeyed = self.lines.rekey(keys);
        touched.extend(self.outlines.naming(&rekeyed));
        let moved = if (landed.generation(), landed.replica().epoch()) == was {
            Moved::These(touched)
        } else {
            Moved::All
        };
        (taken, moved)
    }
}

fn name(text: String) -> Result<Hash, NotAnAnswer> {
    Hash::new(text.clone()).map_err(|_| NotAnAnswer::Name(text))
}

fn names(texts: Vec<String>) -> Result<BTreeSet<Hash>, NotAnAnswer> {
    texts.into_iter().map(name).collect()
}
/// What a device keeps of the lines' content: the plan within the budget and every print held,
/// which a reading loaded; and what of the plan the prints held lack, which a prefetch fetches.
pub struct Plan<'r> {
    /// Every name whose content the device keeps.
    pub keep: BTreeSet<&'r Hash>,
    /// The nodes of the plan whose print is not held, newest conversation first.
    pub ahead: Vec<&'r SkeletonNode>,
}

impl HeldReplica {
    /// Every node `scope` reads, of the landed row and of a pull's pieces not landed yet, whose
    /// print is not held, as it was given.
    #[must_use]
    pub fn lacking(&self, scope: &Scope) -> Vec<SkeletonNode> {
        let pending = self.pending();
        let given: BTreeMap<&Hash, &SkeletonNode> = pending
            .iter()
            .map(|node| (node.name(), node.given()))
            .collect();
        self.outlines
            .scope(scope, &pending)
            .into_iter()
            .filter(|name| !self.lines.holds(name))
            .filter_map(|name| {
                self.outlines
                    .node(name)
                    .or_else(|| given.get(name).copied())
            })
            .cloned()
            .collect()
    }

    /// Every key held, landed or in a pull's pieces, that is dead: one the prints held name as
    /// erased, one the box's rule retires over the landed row and the pieces, or one in `told`
    /// (what the box's pull says an erasure names); each with the blocks its words fed where the
    /// keys held open them. What a purge forgets, the blocks first and then the keys.
    pub fn owed(&mut self, told: &BTreeSet<KeyName>) -> BTreeMap<KeyName, Vec<Renderable>> {
        let holding = self.kept.rows.holding().into_owned();
        let ring = holding.keys();
        let pending = self.pending();
        let mut dead = self.lines.dead(&|key| ring.holds(key));
        dead.extend(self.outlines.retired(&pending, &|key| ring.holds(key)));
        dead.extend(told.iter().cloned());
        let owed: BTreeSet<KeyName> = ring
            .iter()
            .map(|(name, _)| name)
            .filter(|name| dead.contains(*name))
            .cloned()
            .collect();
        let mut erased = self.lines.erased(ring, &owed);
        erased.retain(|name, _| owed.contains(name));
        for name in owed {
            erased.entry(name).or_default();
        }
        erased
    }

    /// Conversation `id` read as its transcript, from every line its skeleton names: the order of
    /// its turns, what is queued and the claims are readings of the conversation whole, so a tab
    /// that holds a window of it reads nothing and is told how many lines it lacks. `None` for a
    /// conversation the tab does not hold.
    pub fn transcript(&mut self, id: &ConversationId) -> Option<Whole> {
        let lacking = self
            .lacking(&Scope::Conversations(BTreeSet::from([id.clone()])))
            .len();
        if lacking > 0 {
            return Some(Whole::Lacking(lacking));
        }
        self.lines.transcript(id).map(Whole::Read)
    }

    /// The nodes a pull's pieces read that the landed row's readings do not hold.
    fn pending(&self) -> Vec<Parsed> {
        self.kept
            .rows
            .pending()
            .replica()
            .read()
            .nodes()
            .filter(|node| !self.outlines.holds(node.name()))
            .cloned()
            .collect()
    }

    /// The plan of the skeleton held within `budget`, beside the prints held.
    #[must_use]
    pub fn plan(&self, budget: usize) -> Plan<'_> {
        let planned = self.outlines.plan(budget);
        let ahead = planned
            .iter()
            .filter(|name| !self.lines.holds(name))
            .filter_map(|name| self.outlines.node(name))
            .collect();
        let held = self.lines.names().filter(|name| self.outlines.holds(name));
        Plan {
            keep: planned.into_iter().chain(held).collect(),
            ahead,
        }
    }
}
