//! The secretary's routes as functions: the binary pulls strings out of a request, calls one, and
//! serialises the answer.
//!
//! [`tap`] is the one route that ends in an irreversible act; it takes a [`Tap`] only the gate can
//! mint and carries an index, never a body, so what is sent is what was stored and shown.

use crate::believed::Preview;
use crate::capability::{Clock, Server, Turn};
use crate::deployment::Deployment;
use crate::held_value::{self, Filled, Reveal};
use crate::mail::oauth::Consent;
use crate::reference::{ProposalRef, Reference, TodoRef};
use crate::refusal::{Answered, Refusal};
use crate::secretary::Bound;
use crate::secretary::acts::{Act, ActLine};
use crate::secretary::attached::{self, Checked};
use crate::secretary::attachment_source::AttachmentSource;
use crate::secretary::changes::{self, Changeset};
use crate::secretary::decide;
use crate::secretary::inbox::{self, ProposalId, Response};
use crate::secretary::opening::Opening;
use crate::secretary::read_holders::ReadHolders;
use crate::secretary::readings::{self, Draw, Dress, Window};
use crate::secretary::responded::{Done, Performed, Tried, Withdrawal};
use crate::secretary::store::{ConversationId, Draft, Entry, Kind, Proposal, Stored, Title};
use crate::secretary::tap::Tap;
use crate::secretary::transcript;
use crate::wire;

/// Sending a proposed message, the one effect behind a tap; deliberately in no capability bundle,
/// so no verb can send.
pub trait Post {
    /// Put this message on a wire, as `account`, with `body` in the draft's body's place and
    /// `attachments`, and answer what happened: sent, or the transport's own words and whether
    /// the far end may hold the message.
    fn send(&self, draft: &Draft, body: Filled, attachments: Checked) -> Performed;
}

/// Performs a proposed act; like [`Post`], in no bundle, and it takes the [`Consent`] only a tap
/// can make.
pub trait Perform {
    /// Begin the act, and answer the outcome to record now; a flow that runs on appends its own
    /// turns and ends in the inbox.
    fn begin(
        &self,
        id: &ConversationId,
        index: usize,
        object: &ProposalId,
        proposer: &crate::secretary::proposer::Proposer,
        act: &Act,
        consent: Consent,
    ) -> Performed;
}

/// What a conversation payload needs besides its transcript: the chains a changeset is drawn
/// against and the category vocabulary.
#[derive(Clone, Copy)]
pub struct Cards<'a> {
    /// The categories an act may name.
    pub deployment: &'a Deployment,
    /// Every chain this deployment serves.
    pub chains: &'a [Bound<'a>],
    /// The plugins the instance file declares now, `None` when it does not read: what a
    /// connection's card says its plugin may reach.
    pub plugins: Option<&'a [crate::plugin::Manifest]>,
    /// How the person reaches the app at a terminal, which a step's command is spelled in.
    pub reach: &'a crate::connect::Reach,
    /// The names the deployment may set as it reads now, each with what declares it.
    pub settables: &'a crate::settings::Settables,
}

/// Each named chain's events, folded once, reading only the chains a changeset names because the
/// page polls this route.
struct Drawn<'a> {
    nodes: std::collections::BTreeMap<String, (Preview, prodrome_core::policy::Untrusted)>,
    /// What an act's markup is admitted through, as it would be on the tap.
    typeset: &'a dyn crate::capability::Typeset,
    now: crate::zone::Moment,
    categories: Vec<String>,
    /// The routine `[coding]` declares.
    routine: Option<crate::coding::RoutineId>,
    /// The name the deployment stores its model's credential under.
    model: Option<crate::settings::Settable>,
    /// The plugins the instance file declares now.
    plugins: Vec<crate::plugin::Manifest>,
    /// How the person reaches the app at a terminal.
    reach: crate::connect::Reach,
    /// Each held value a standing email carries, by its message's notmuch id: that message in
    /// the box's words.
    sources: std::collections::BTreeMap<String, String>,
}

impl<'a> Drawn<'a> {
    fn of(
        lines: &crate::secretary::store::Lines,
        cards: &Cards<'_>,
        at: crate::capability::Datetime,
        typeset: &'a dyn crate::capability::Typeset,
    ) -> Self {
        // An unplaced line names no changeset, so it reads no chain.
        let wanted: std::collections::BTreeSet<&str> = lines
            .standings()
            .filter_map(|standing| match &standing.object.asked.proposal {
                Proposal::Changes(changeset) => Some(changeset.chain()),
                // Neither reads a chain: an email leaves the box and an act is performed by the
                // box itself.
                Proposal::Email(_)
                | Proposal::Forward(_)
                | Proposal::Act(_)
                | Proposal::Remember { .. }
                | Proposal::Decide(_) => None,
            })
            .collect();
        let mut read = std::collections::BTreeMap::new();
        for bound in cards.chains {
            if !wanted.contains(bound.name) {
                continue;
            }
            // An unreadable chain draws no rows and a problem line, never an empty diff.
            if let Some(preview) = bound
                .caps
                .chain
                .objects()
                .ok()
                .and_then(|objects| Preview::of(objects).ok())
            {
                read.insert(
                    bound.name.to_owned(),
                    (preview, bound.policy.untrusted.clone()),
                );
            }
        }
        Self {
            nodes: read,
            typeset,
            now: crate::zone::Moment {
                at,
                zone: cards.deployment.zone,
            },
            categories: cards.deployment.categories.keys().cloned().collect(),
            model: cards.settables.model(),
            routine: cards
                .deployment
                .coding
                .as_ref()
                .map(|coding| coding.routine.clone()),
            plugins: cards.plugins.map(<[_]>::to_vec).unwrap_or_default(),
            reach: cards.reach.clone(),
            sources: sources(lines, cards),
        }
    }
}

/// The message of every held value a standing email carries, read once per reading.
fn sources(
    lines: &crate::secretary::store::Lines,
    cards: &Cards<'_>,
) -> std::collections::BTreeMap<String, String> {
    let Some(mail) = cards.chains.first().map(|bound| bound.caps.mail) else {
        return std::collections::BTreeMap::new();
    };
    let messages: std::collections::BTreeSet<String> = lines
        .standings()
        .filter_map(|standing| match &standing.object.asked.proposal {
            Proposal::Email(draft) => held_value::Written::parse(&draft.body).ok(),
            Proposal::Forward(_)
            | Proposal::Changes(_)
            | Proposal::Act(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => None,
        })
        .flat_map(|written| {
            written
                .held()
                .filter_map(|held| match held.reach() {
                    held_value::Reach::Revealable(held_value::Handle::Value(place)) => {
                        match place.holder() {
                            held_value::Holder::Message(message) => Some(message.clone()),
                            held_value::Holder::File(_) => None,
                        }
                    }
                    held_value::Reach::Revealable(held_value::Handle::File(_))
                    | held_value::Reach::Withheld => None,
                })
                .collect::<Vec<_>>()
        })
        .collect();
    messages
        .into_iter()
        .map(|message| {
            let said = match mail.message(&message) {
                Ok(read) => {
                    let date = read.row.date.map_or(String::new(), |date| {
                        format!(", {:04}-{:02}-{:02}", date.year(), date.month(), date.day())
                    });
                    format!(
                        "from the message from {}, \u{201c}{}\u{201d}{date}",
                        read.row.sender, read.row.subject
                    )
                }
                Err(_) => "from a message no longer in the mail index".to_owned(),
            };
            (message, said)
        })
        .collect()
}

impl Draw for Drawn<'_> {
    fn routine(&self) -> Option<&crate::coding::RoutineId> {
        self.routine.as_ref()
    }

    fn model(&self) -> Option<&str> {
        self.model.as_ref().map(crate::settings::Settable::name)
    }

    fn source(&self, handle: &held_value::Handle) -> String {
        let file =
            match handle {
                held_value::Handle::Value(place) => match place.holder() {
                    held_value::Holder::Message(message) => {
                        return self.sources.get(message).cloned().unwrap_or_else(|| {
                            crate::secretary::readings::UNREAD_SOURCE.to_owned()
                        });
                    }
                    held_value::Holder::File(file) => file,
                },
                held_value::Handle::File(file) => file,
            };
        format!("from {}", file.source().describe())
    }

    fn reach(&self) -> &crate::connect::Reach {
        &self.reach
    }

    fn origins(&self, plugin: &str) -> Option<Vec<String>> {
        self.plugins
            .iter()
            .find(|manifest| manifest.name().as_str() == plugin)
            .map(|manifest| manifest.origins().map(ToString::to_string).collect())
    }

    fn shown(&self, changeset: &Changeset) -> (Vec<wire::ProposedObject>, String) {
        let Some((nodes, untrusted)) = self.nodes.get(changeset.chain()) else {
            return (
                Vec::new(),
                format!(
                    "this box cannot read the chain {} that these acts are against",
                    changeset.chain()
                ),
            );
        };
        let known: Vec<&str> = self.categories.iter().map(String::as_str).collect();
        changes::shown(
            changeset,
            nodes,
            &changes::Building {
                now: self.now,
                by: crate::secretary::WRITES_AS,
                categories: &known,
                untrusted,
                typeset: self.typeset,
            },
        )
    }
}

/// `POST /api/secretary/conversations`: open a topic, named for today; a blank `title` is left to
/// the title job.
///
/// # Errors
/// [`Refusal::bad_request`] for a title that is not one, and the store's own failures.
pub fn open(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    title: &str,
) -> Answered<wire::ConversationPayload> {
    let title = if title.trim().is_empty() {
        String::new()
    } else {
        Title::new(title)
            .map_err(|refused| Refusal::bad_request(refused.to_string()))?
            .into()
    };
    let taken = transcript::keys(caps.secretary.conversations)
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let today = cards
        .deployment
        .zone
        .day(prodrome_core::fpl::instant_of(caps.secretary.clock.now()));
    let id = ConversationId::minted(today, &taken);
    let stored = super::start(
        caps.secretary.conversations,
        caps.secretary.model,
        &id,
        &title,
        Kind::Topic,
        caps.secretary.clock.now(),
    )?;
    payload(&stored, caps, cards)
}

/// `POST …/<id>/title`: rename one, as the actor the gate resolved.
///
/// # Errors
/// [`super::title::rename`]'s.
pub fn rename(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    by: &prodrome_core::event::Actor,
    id: &ConversationId,
    title: &str,
) -> Answered<wire::ConversationPayload> {
    let stored = super::title::rename(
        caps.secretary.conversations,
        caps.secretary.clock,
        by,
        id,
        title,
    )?;
    payload(&stored, caps, cards)
}

/// What the owner erases of one conversation: `POST …/erase`'s body, `{"turns": [<index>, …]}`
/// or `{"whole": true}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Erasing {
    /// These turns' words, by index.
    Turns(#[ts(as = "Vec<u32>")] crate::nonempty::NonEmpty<usize>),
    /// Every line's words, the conversation's name included.
    Whole(wire::Yes),
}

/// `POST …/<id>/erase`: the owner erases turns of a conversation, or all of it, behind the same
/// [`Tap`] as a send: an erasure line naming their keys as his, then the sweep.
///
/// # Errors
/// [`Refusal::not_found`] for a turn there is none of, and [`transcript::erase`]'s.
pub fn erase(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    tap: &Tap,
    id: &ConversationId,
    erasing: &Erasing,
) -> Answered<wire::ConversationPayload> {
    let read = transcript::read(caps.secretary.conversations, id)?;
    let lines: Vec<usize> = match erasing {
        Erasing::Whole(wire::Yes) => (0..read.lines().len()).collect(),
        Erasing::Turns(turns) => turns
            .as_slice()
            .iter()
            .map(|&turn| {
                read.line_of(turn)
                    .ok_or_else(|| Refusal::not_found(format!("{id} has no turn {turn}")))
            })
            .collect::<Result<_, _>>()?,
    };
    let erased = crate::secretary::transcript::erasures(
        read.lines()
            .iter()
            .map(|written| (&written.name, &written.line)),
    );
    let keys: std::collections::BTreeSet<_> = lines
        .iter()
        .flat_map(|&line| read.lines()[line].line.keys())
        .filter(|key| !erased.contains_key(*key))
        .cloned()
        .collect();
    if !keys.is_empty() {
        transcript::erase(
            caps.secretary.conversations,
            id,
            keys,
            crate::secretary::erasure::Erasure::Asked {
                by: tap.who().clone(),
            },
            caps.secretary.clock.now(),
        )?;
    }
    // The summaries that read them are copies, deleted by the memory's sweep.
    caps.secretary.memories.sweep_after_write();
    newest(caps, cards, id)
}

/// One conversation's newest window, dressed with the box's renderer, its memory and its chains:
/// what a write answers once it is written.
///
/// # Errors
/// A name nothing is stored under, or a file this program cannot fold.
pub fn newest(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
) -> Answered<wire::ConversationPayload> {
    payload(&super::read(caps.secretary.conversations, id)?, caps, cards)
}

/// `GET …/<id>/drawn?indices=<n>,…`: proposals `indices` drawn against the todo chains, which a
/// tab's replicas of the inbox and the conversations cannot: a standing changeset's rows. Each
/// the whole row, so its card is the one function's; the conversation, the inbox and the memory are read once for all of them.
///
/// # Errors
/// [`Refusal::not_found`] for a conversation or a proposal there is none of, and the stores' own.
pub fn drawn(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
    indices: &[usize],
) -> Answered<wire::DrawnPayload> {
    let stored = super::read(caps.secretary.conversations, id)?;
    let fold = inbox::fold(caps.secretary.inbox)?;
    let drawn = Drawn::of(
        &stored.lines(&fold),
        cards,
        caps.secretary.clock.now(),
        caps.secretary.render,
    );
    let memory = memory(caps.secretary.memories).ok();
    let proposals = indices
        .iter()
        .map(|&index| {
            let standing = stored.standing(index, &fold)?;
            Ok(readings::drawn(
                &stored,
                index,
                &standing,
                &drawn,
                memory.as_ref(),
            ))
        })
        .collect::<Result<_, Refusal>>()?;
    Ok(wire::DrawnPayload { proposals })
}

/// How many files one turn may carry.
pub const MAX_ATTACHMENTS_PER_TURN: usize = 4;

/// `POST …/<id>/say`: append the person's turn, stamped `by` (the gate's actor, as
/// [`crate::secretary::stamp::pointing`] read what its references point at); the model runs in the
/// binary's worker, not here.
///
/// A question names how many turns its asker saw (`after`), so the same question asked again
/// once its line stands at or past that point is that line: asking twice is asking once.
///
/// # Errors
/// The store's own, an empty turn, too many files, or [`Refusal::bad_request`] when the queue is
/// full.
#[expect(
    clippy::too_many_arguments,
    reason = "a turn's parts, its asker and what it saw, each its own field of the request"
)]
pub fn say(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
    said: &str,
    references: &[Reference],
    attachments: &[crate::attachment::Attachment],
    by: crate::secretary::stamp::Stamp,
    after: Option<usize>,
) -> Answered<wire::SayPayload> {
    let turn = question(said, references, attachments)?;
    // Read first, so an unknown name is a 404 before anything is written.
    let before = super::read(caps.secretary.conversations, id)?;
    if after.is_some_and(|after| asked(&before, after, &turn, &by)) {
        return Ok(wire::SayPayload {
            conversation: payload(&before, caps, cards)?,
        });
    }
    // Checked before appending: a turn the box will never get to must not join the transcript.
    let pending = crate::secretary::queue::pending(&before);
    if pending >= crate::secretary::queue::MAX_PENDING {
        return Err(Refusal::bad_request(format!(
            "{pending} lines are already waiting for the secretary; say it after they are answered"
        )));
    }
    // Disambiguated, because `C` carries both `Log::push` and `Prodrome::append`.
    transcript::push(
        caps.secretary.conversations,
        id,
        &Entry::Said {
            at: caps.secretary.clock.now(),
            turn,
            by: Some(by),
        },
    )?;
    let stored = super::read(caps.secretary.conversations, id)?;
    Ok(wire::SayPayload {
        conversation: payload(&stored, caps, cards)?,
    })
}

/// Whether `turn`, said by `by`, already stands at or past turn `after`.
fn asked(stored: &Stored, after: usize, turn: &Turn, by: &crate::secretary::stamp::Stamp) -> bool {
    stored.turns.iter().skip(after).any(|said| {
        &said.turn == turn
            && said
                .by
                .as_ref()
                .and_then(crate::secretary::stamp::Stamp::actor)
                == by.actor()
    })
}

/// A person's turn, or why the words and files are not one.
fn question(
    said: &str,
    references: &[Reference],
    attachments: &[crate::attachment::Attachment],
) -> Result<Turn, Refusal> {
    if said.trim().is_empty() && attachments.is_empty() {
        return Err(Refusal::bad_request("say something"));
    }
    if attachments.len() > MAX_ATTACHMENTS_PER_TURN {
        return Err(Refusal::bad_request(format!(
            "at most {MAX_ATTACHMENTS_PER_TURN} files in one turn"
        )));
    }
    Ok(Turn::Person {
        text: said.to_owned(),
        references: references.to_vec(),
        attachments: attachments.to_vec(),
    })
}

/// `POST …/<id>/regenerate`: ask the last round again, as the actor the gate resolved.
///
/// # Errors
/// [`super::regenerate`]'s.
pub fn regenerate(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
    by: &prodrome_core::event::Actor,
) -> Answered<wire::ConversationPayload> {
    let stored = super::regenerate(caps.secretary.conversations, caps.secretary.clock, by, id)?;
    payload(&stored, caps, cards)
}

/// `POST …/<id>/edit`: replace the last round's question, under `say`'s rules for a question and
/// its stamp.
///
/// # Errors
/// `question`'s and [`super::edit`]'s.
#[expect(
    clippy::too_many_arguments,
    reason = "a turn's parts, who set the last round aside and the stamp it is written with"
)]
pub fn edit(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
    said: &str,
    references: &[Reference],
    attachments: &[crate::attachment::Attachment],
    by: &prodrome_core::event::Actor,
    stamped: crate::secretary::stamp::Stamp,
) -> Answered<wire::ConversationPayload> {
    let turn = question(said, references, attachments)?;
    let stored = super::edit(
        caps.secretary.conversations,
        caps.secretary.clock,
        by,
        stamped,
        id,
        turn,
    )?;
    payload(&stored, caps, cards)
}

/// What a tap can act through, one door for each kind of act.
#[derive(Clone, Copy)]
pub struct Effectors<'a> {
    /// Sends a tapped email.
    pub post: &'a dyn Post,
    /// Reads a tapped email's held values, and a tapped forward's message, to put them on the
    /// wire.
    pub reveal: &'a dyn Reveal,
    /// Performs a tapped act (a sign-in, a web request).
    pub performer: &'a dyn Perform,
    /// Stores a tapped secret.
    pub secrets: &'a dyn crate::settings::Writes,
    /// Calls a model once with a credential before it is stored.
    pub verify: &'a dyn crate::settings::Verify,
    /// Syncs a plugin once its link is stored, so the link is checked at the tap.
    pub checks: &'a dyn crate::settings::Checks,
    /// The sign-ins the box holds open, which a code card's tap finishes.
    pub signing: &'a dyn crate::secretary::sign_in::Sessions,
    /// Edits the instance file (an "Always allow" rule, a connection, a grant).
    pub edits: &'a dyn crate::settings::Edits,
    /// Deletes what the box kept for a removed account: its sign-ins, and its mail on the
    /// card's wider yes.
    pub forget: &'a dyn crate::mail::cache::Forget,
    /// Registers a device's key on every chain, signed by the root; `None` for a deployment
    /// that declares no root, which trusts no device.
    pub enrolling: Option<crate::signatures::Enrolling<'a>>,
    /// Opens a sign-in with Google, bound to the owner who tapped it.
    pub consents: &'a dyn crate::secretary::google_sign_in::Consents,
}

/// What a tap's body carries beside its note: nothing, or exactly one of these, read once where
/// the request is parsed. Which one a card takes is the card's to say (`Prepared::of`).
#[derive(Debug)]
pub enum TapAnswer {
    /// Nothing but the tap: the card performed as it stands.
    Plain,
    /// A secret's value, or the code a sign-in's page shows.
    Value(crate::capability::Secret),
    /// The card's wider yes: "Always allow" for a web request with a rule or a document, and
    /// deleting its mail too for a mail account's removal.
    Always,
    /// The tapping device's signature of a device card's registration.
    Endorsed(crate::signatures::Endorsement),
}

/// What a refusal says of an answer the card does not take.
const VALUE_BELONGS: &str =
    "a value belongs only to a secret act, a sign-in's code or the Google client's file";
const ALWAYS_BELONGS: &str = "always belongs only to a web request that offers a rule, a \
                              document, or a mail account's removal";
const ENDORSEMENT_BELONGS: &str = "an endorsement belongs only to a device card";

/// How a card with a wider yes was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Once,
    Always,
}

impl TapAnswer {
    /// The answer of a card that takes nothing beside its note.
    fn plain(&self) -> Result<(), Refusal> {
        match self {
            TapAnswer::Plain => Ok(()),
            TapAnswer::Value(_) => Err(Refusal::bad_request(VALUE_BELONGS)),
            TapAnswer::Always => Err(Refusal::bad_request(ALWAYS_BELONGS)),
            TapAnswer::Endorsed(_) => Err(Refusal::bad_request(ENDORSEMENT_BELONGS)),
        }
    }

    /// The answer of a card that has a wider yes.
    fn choice(&self) -> Result<Choice, Refusal> {
        match self {
            TapAnswer::Plain => Ok(Choice::Once),
            TapAnswer::Always => Ok(Choice::Always),
            TapAnswer::Value(_) => Err(Refusal::bad_request(VALUE_BELONGS)),
            TapAnswer::Endorsed(_) => Err(Refusal::bad_request(ENDORSEMENT_BELONGS)),
        }
    }

    /// The value of a card that stores one; `missing` finishes the sentence refusing a tap
    /// without it.
    fn value(&self, missing: &str) -> Result<&crate::capability::Secret, Refusal> {
        match self {
            TapAnswer::Value(value) => Ok(value),
            TapAnswer::Plain => Err(Refusal::bad_request(format!("value is required{missing}"))),
            TapAnswer::Always => Err(Refusal::bad_request(ALWAYS_BELONGS)),
            TapAnswer::Endorsed(_) => Err(Refusal::bad_request(ENDORSEMENT_BELONGS)),
        }
    }

    /// The endorsement a device card is tapped with.
    fn endorsement(&self) -> Result<&crate::signatures::Endorsement, Refusal> {
        match self {
            TapAnswer::Endorsed(endorsement) => Ok(endorsement),
            TapAnswer::Plain => Err(Refusal::forbidden(
                "a device card is tapped from a device already trusted, which signs it",
            )),
            TapAnswer::Value(_) => Err(Refusal::bad_request(VALUE_BELONGS)),
            TapAnswer::Always => Err(Refusal::bad_request(ALWAYS_BELONGS)),
        }
    }
}

/// How far a tap that performs an act allows: this once, or also what "Always allow" writes
/// into the instance before the act runs.
#[derive(Debug, Clone, Copy)]
enum Allowing<'p> {
    /// This act alone.
    Once,
    /// A web request's drafted rule.
    Rule(&'p crate::web::AllowRule),
    /// A document, whose folder is allowed.
    Document(&'p crate::drive::DrivePath),
}

/// What an account's removal does with the mail the box kept for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeptMail {
    /// Left where it is.
    Kept,
    /// Deleted with the account, on the card's wider yes.
    Deleted,
}

/// A tapped card paired with what its tap brought, before anything is performed: each case holds
/// exactly what performing that card needs, so a misplaced or missing answer is refused here
/// and nowhere else.
enum Prepared<'p, 'c> {
    /// Send this message.
    Email(&'p Draft),
    /// Forward this stored message.
    Forward(&'p crate::secretary::store::Forward),
    /// Append this changeset.
    Changes(&'p Changeset),
    /// Store `value` under a name minted against the names declared at the tap.
    Secret {
        name: crate::settings::Settable,
        value: &'p crate::capability::Secret,
    },
    /// Finish the sign-in at `url` with `code`, keeping its token under `name`.
    Code {
        url: &'p crate::secretary::sign_in::SignInUrl,
        code: &'p crate::capability::Secret,
        name: crate::settings::Settable,
    },
    /// Write a connection's table.
    Connect(&'p crate::connect::Connection),
    /// Take a connection's table out, and its mail with it or not.
    Disconnect {
        connection: &'p crate::connect::Connection,
        mail: KeptMail,
    },
    /// Let a plugin reach one more origin.
    Grant {
        plugin: &'p crate::plugin::PluginName,
        origin: &'p crate::plugin::Origin,
    },
    /// Ask the secret store again.
    UnlockStore,
    /// Erase these passages.
    Erase(&'p std::collections::BTreeSet<crate::secretary::locked_text::KeyName>),
    /// Register a vouched device's key on every chain.
    Device {
        enrolling: crate::signatures::Enrolling<'c>,
        enrolment: &'p crate::signatures::Enrolment,
        endorsed: prodrome_core::sign::Signed,
    },
    /// Begin an act the performer runs, once what it allows is written.
    Begin {
        act: &'p Act,
        allowing: Allowing<'p>,
    },
    /// Remember a card's words under the memory's hold, taken before the inbox's.
    Remember {
        held: crate::secretary::memories::Held<'c>,
        text: &'p crate::secretary::salted_text::TextName,
        kind: crate::secretary::memory::Kind,
    },
    /// Record a decision.
    Decide(&'p crate::decisions::Draft),
    /// Keep the Google client in the file pasted into its card, when it lists `redirect`.
    GoogleClient {
        redirect: &'p crate::google::RedirectUri,
        file: &'p crate::capability::Secret,
    },
    /// Open a sign-in with Google for `account`, asking `scopes`.
    GoogleSignIn {
        account: &'p crate::mail::address::EmailAddress,
        scopes: &'p crate::google::Scopes,
    },
    /// Write one event into `account`'s calendar, once the instance file still declares it a
    /// Google account.
    CalendarEvent {
        act: &'p Act,
        account: &'p crate::mail::address::EmailAddress,
    },
}

/// What preparing a tap reads besides the card and its answer.
struct Preparing<'r, 'c> {
    /// The names declared now, which a secret's name is minted against.
    settables: &'r crate::settings::Settables,
    /// The device registry, `None` for a deployment that declares no root.
    enrolling: Option<crate::signatures::Enrolling<'c>>,
    /// Who tapped.
    tap: &'r Tap,
    /// Takes the memory's hold.
    hold: &'r dyn Fn() -> Result<crate::secretary::memories::Held<'c>, Refusal>,
}

impl<'p, 'c> Prepared<'p, 'c> {
    /// `proposal` paired with `answer`, refused when the answer is not one the card takes.
    fn of(
        proposal: &'p Proposal,
        answer: &'p TapAnswer,
        reach: &Preparing<'_, 'c>,
    ) -> Result<Self, Refusal> {
        Ok(match proposal {
            Proposal::Email(draft) => {
                answer.plain()?;
                Prepared::Email(draft)
            }
            Proposal::Forward(forward) => {
                answer.plain()?;
                Prepared::Forward(forward)
            }
            Proposal::Changes(changeset) => {
                answer.plain()?;
                Prepared::Changes(changeset)
            }
            Proposal::Act(act) => Self::of_act(act, answer, reach)?,
            // His tap is who said it, since the round that asked had read something not his own.
            Proposal::Remember { text, kind } => {
                answer.plain()?;
                Prepared::Remember {
                    held: (reach.hold)()?,
                    text,
                    kind: *kind,
                }
            }
            Proposal::Decide(draft) => {
                answer.plain()?;
                Prepared::Decide(draft)
            }
        })
    }

    /// An act's card paired with its answer.
    fn of_act(
        act: &'p Act,
        answer: &'p TapAnswer,
        reach: &Preparing<'_, 'c>,
    ) -> Result<Self, Refusal> {
        use crate::connect::Table;
        Ok(match act {
            Act::Secret { name, .. } => Prepared::Secret {
                value: answer.value(&format!(" to store {name}"))?,
                name: minted(reach.settables, name)?,
            },
            Act::SignInCode { url } => Prepared::Code {
                url,
                code: answer.value(": the code the sign-in page shows")?,
                name: reach.settables.model().ok_or_else(|| {
                    Refusal::conflict(
                        "nothing was stored: this deployment declares no model credential",
                    )
                })?,
            },
            Act::Web { rule, .. } => Prepared::Begin {
                act,
                allowing: match (answer.choice()?, rule) {
                    (Choice::Once, Some(_) | None) => Allowing::Once,
                    (Choice::Always, Some(rule)) => Allowing::Rule(rule),
                    (Choice::Always, None) => return Err(Refusal::bad_request(ALWAYS_BELONGS)),
                },
            },
            Act::DriveRead { path, .. } => Prepared::Begin {
                act,
                allowing: match answer.choice()? {
                    Choice::Once => Allowing::Once,
                    Choice::Always => Allowing::Document(path),
                },
            },
            Act::Disconnect { connection } => Prepared::Disconnect {
                connection,
                mail: match (answer.choice()?, &connection.template().table) {
                    (Choice::Once, _) => KeptMail::Kept,
                    (Choice::Always, Table::Account(_)) => KeptMail::Deleted,
                    (
                        Choice::Always,
                        Table::Always
                        | Table::Model
                        | Table::Link(_)
                        | Table::Folder
                        | Table::Channel(_),
                    ) => return Err(Refusal::bad_request(ALWAYS_BELONGS)),
                },
            },
            Act::Device(enrolment) => {
                let endorsement = answer.endorsement()?;
                let enrolling = reach.enrolling.ok_or_else(|| {
                    Refusal::bad_request(
                        "this deployment declares no root, so it trusts no device key",
                    )
                })?;
                Prepared::Device {
                    enrolling,
                    enrolment,
                    endorsed: vouched(enrolling, reach.tap, enrolment, endorsement)?,
                }
            }
            Act::Connect { connection } => {
                answer.plain()?;
                Prepared::Connect(connection)
            }
            Act::Grant { plugin, origin } => {
                answer.plain()?;
                Prepared::Grant { plugin, origin }
            }
            Act::UnlockStore { .. } => {
                answer.plain()?;
                Prepared::UnlockStore
            }
            Act::Erase { keys, .. } => {
                answer.plain()?;
                Prepared::Erase(keys)
            }
            Act::GoogleClient { redirect } => Prepared::GoogleClient {
                redirect,
                file: answer.value(": the client's downloaded file, whole")?,
            },
            Act::GoogleSignIn { account, scopes } => {
                answer.plain()?;
                Prepared::GoogleSignIn { account, scopes }
            }
            Act::CalendarEvent { account, .. } => {
                answer.plain()?;
                Prepared::CalendarEvent { act, account }
            }
            Act::Auth { .. }
            | Act::AuthSend { .. }
            | Act::Patch { .. }
            | Act::CodingSession { .. }
            | Act::SignIn { .. } => {
                answer.plain()?;
                Prepared::Begin {
                    act,
                    allowing: Allowing::Once,
                }
            }
        })
    }
}

/// `name` minted against the names declared now: a card asking for a name its deployment has
/// since stopped declaring stores nothing.
fn minted(
    settables: &crate::settings::Settables,
    name: &str,
) -> Result<crate::settings::Settable, Refusal> {
    crate::settings::Settable::mint(settables, name).map_err(|_| {
        Refusal::conflict(format!(
            "nothing was stored: {} is no longer a secret this deployment declares",
            crate::settings::Help::label(name)
        ))
    })
}

/// `POST …/proposals/<n>/tap`: submits one proposal, at most once under the inbox's hold.
///
/// A failure that may have happened completes the proposal; one that did not is a note, so tapping
/// again runs the same stored act.
///
/// # Errors
/// [`Refusal::not_found`] for an unknown index; [`Refusal::bad_request`] for an answer the card
/// does not take, or none where it needs one; [`Refusal::conflict`] for a secret no longer
/// declared; a rejected allow rule, and the stores' own failures.
#[allow(clippy::too_many_arguments)]
pub fn tap(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    tap: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
    note: &str,
    answer: &TapAnswer,
) -> Answered<wire::ConversationPayload> {
    let (id, index) = (&card.conversation, card.index);
    let stored = super::read(caps.secretary.conversations, id)?;
    let standing = stored.standing(index, &inbox::fold(caps.secretary.inbox)?)?;
    let hold = || caps.secretary.memories.hold();
    let prepared = Prepared::of(
        &standing.object.asked.proposal,
        answer,
        &Preparing {
            settables: cards.settables,
            enrolling: effectors.enrolling,
            tap,
            hold: &hold,
        },
    )?;
    let mut then = None;
    let response = crate::secretary::tapped::Tapped {
        now: caps.secretary.clock.now(),
    }
    // The proposal read under the hold is the one prepared: its id names its content.
    .fire(caps.secretary.inbox, &standing.object.id, tap, |_| {
        performed(
            &Performing {
                caps,
                effectors,
                cards,
                tap,
            },
            prepared,
            (id, index, &standing.object),
            &mut then,
        )
    })?;
    afterwards(caps, effectors, then, card)?;
    summaries_after(caps, &standing.object.asked.proposal);
    if matches!(response, Response::Ran(_)) {
        crate::secretary::acts::note_act(
            caps.secretary.conversations,
            id,
            &ActLine {
                at: caps.secretary.clock.now(),
                actor: tap.who().as_str().to_owned(),
                verb: "tapped",
                reference: card.clone().into(),
                note: note.to_owned(),
            },
            // A proposal's index is the box's; nothing it points at was chosen by mail.
            crate::secretary::stamp::Stamp::Actor(tap.who().as_str().to_owned()),
        )?;
    }
    payload(&super::read(caps.secretary.conversations, id)?, caps, cards)
}

/// What performing a prepared card reaches.
struct Performing<'r, 'a> {
    caps: &'r Server<'a>,
    effectors: &'r Effectors<'a>,
    cards: &'r Cards<'a>,
    tap: &'r Tap,
}

/// Perform a prepared card under the inbox's hold.
fn performed(
    reach: &Performing<'_, '_>,
    prepared: Prepared<'_, '_>,
    (id, index, object): (&ConversationId, usize, &inbox::Object),
    then: &mut Option<Then>,
) -> Result<Performed, Refusal> {
    let Performing {
        caps,
        effectors,
        cards,
        tap,
    } = *reach;
    Ok(match prepared {
        Prepared::Email(draft) => mailed(caps, effectors, id, draft),
        Prepared::Forward(forward) => forwarded(caps, effectors, id, forward),
        Prepared::Changes(changeset) => apply(cards, changeset),
        Prepared::Secret { name, value } => secret(
            effectors.secrets,
            effectors.verify,
            (effectors.edits, cards.settables),
            &name,
            value,
            then,
        ),
        Prepared::Code { url, code, name } => coded(effectors, url, code, &name, then),
        Prepared::Connect(connection) => connected(effectors.edits, connection),
        Prepared::Disconnect { connection, mail } => {
            disconnected(effectors, cards, connection, mail, then)
        }
        Prepared::Grant { plugin, origin } => granted(effectors.edits, plugin, origin),
        Prepared::UnlockStore => store_answers(caps.secretary.secrets, cards.settables),
        Prepared::Erase(keys) => erased(caps, id, keys, tap)?,
        Prepared::Device {
            enrolling,
            enrolment,
            endorsed,
        } => trusted(enrolling, enrolment, &endorsed),
        Prepared::Begin { act, allowing } => {
            // Written before the act: allowing a site is a decision about the site, not this
            // page.
            allowed(effectors.edits, allowing)?;
            // The tap is the consent, made from what the gate minted, never from a request
            // field.
            effectors.performer.begin(
                id,
                index,
                &object.id,
                &object.asked.proposer,
                act,
                Consent::given_by_tap(tap),
            )
        }
        Prepared::Remember { held, text, kind } => {
            remembered(&held, caps.secretary.clock, id, index, text, kind)
        }
        Prepared::Decide(draft) => decide::performed(caps, draft, tap.who()),
        Prepared::GoogleClient { redirect, file } => {
            google_client(effectors.secrets, cards.settables, redirect, file)?
        }
        Prepared::GoogleSignIn { account, scopes } => effectors.consents.begin(
            &object.id,
            account,
            scopes,
            tap.who(),
            caps.secretary.clock.now(),
        ),
        Prepared::CalendarEvent { act, account } => {
            match google_account(effectors.edits, account) {
                Ok(()) => effectors.performer.begin(
                    id,
                    index,
                    &object.id,
                    &object.asked.proposer,
                    act,
                    Consent::given_by_tap(tap),
                ),
                Err(reason) => Performed::Tried(Tried::NotSent { reason }),
            }
        }
    })
}

/// After an erase card's tap, the memory's sweep: a summary that read the erased words is a copy
/// of them. Under the memory's own hold, taken only once the inbox's is let go, as every other
/// writer orders the two.
fn summaries_after(caps: &Server<'_>, proposal: &Proposal) {
    if matches!(proposal, Proposal::Act(Act::Erase { .. })) {
        caps.secretary.memories.sweep_after_write();
    }
}

/// What a tap leaves for once its card's response is recorded, outside that card's hold, since
/// each is its own proposal.
enum Then {
    /// The sign-in card a code card finished, ended as its outcome says.
    End(ProposalId, Performed),
    /// The origin a link was served from, which its plugin may not reach: its card is asked for.
    Grant(crate::plugin::PluginName, crate::plugin::Origin),
    /// The Google account whose calendar's link was pasted: its connection's card is asked for.
    Connect(crate::connect::Connection),
    /// A plugin whose link was stored, synced once so the link is checked now.
    Check(crate::plugin::PluginName),
    /// What the box kept for a removed account, deleted outside the inbox's hold, since it
    /// waits on the sync's: its sign-in, its mail on the card's wider yes, or both.
    Forget(
        crate::mail::account::MailAccount,
        crate::mail::removal::Removal,
    ),
}

/// Do what the tap of `card` left.
fn afterwards(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    then: Option<Then>,
    card: &ProposalRef,
) -> Result<(), Refusal> {
    match then {
        None => Ok(()),
        Some(Then::End(began, ended)) => inbox::end(
            caps.secretary.inbox,
            &began,
            &super::writes_as(),
            caps.secretary.clock.now(),
            ended,
        )
        .map(|_| ()),
        Some(Then::Grant(plugin, origin)) => {
            crate::secretary::connections::grant(caps, &plugin, &origin)
        }
        Some(Then::Connect(connection)) => crate::secretary::connections::connect(caps, connection),
        Some(Then::Check(plugin)) => {
            effectors.checks.check(&plugin);
            Ok(())
        }
        Some(Then::Forget(account, removal)) => {
            let said = effectors.forget.forget(&account, removal).sentence();
            super::append(
                caps.secretary.conversations,
                &card.conversation,
                &Entry::noted(
                    caps.secretary.clock.now(),
                    format!("Removing {}: {said}.", account.email),
                    vec![card.clone().into()],
                ),
            )
        }
    }
}

/// What a secret's tap does with its value: store it, or, for a plugin's link, refuse a value
/// that is no link it can fetch and ask first for an origin its manifest does not allow.
enum Link {
    /// Not a plugin's link.
    Stored,
    /// A link its plugin may fetch, which is checked once it is stored.
    Fetched(crate::plugin::PluginName),
    /// A plugin's link that is no http(s) URL, or one in the clear; the words never quote the
    /// value.
    Refused(String),
    /// A link served from an origin its plugin may not reach.
    Outside {
        plugin: crate::plugin::PluginName,
        origin: crate::plugin::Origin,
    },
    /// A Google account's own calendar, which the deployment does not hold: the account's
    /// connection is asked for in its place.
    Connect(crate::connect::Connection),
}

/// Where `value` stands as `name`'s link, read against the manifests the file declares now; a
/// plugin's credential is refused while the file does not read, since whether it is a link,
/// and which origins it may reach, are the file's to say.
fn linked(
    (edits, settables): (&dyn crate::settings::Edits, &crate::settings::Settables),
    name: &crate::settings::Settable,
    value: &crate::capability::Secret,
) -> Link {
    let a_plugin_s = settables.declaration(name).is_some_and(|by| {
        by.iter()
            .any(|by| matches!(by, crate::settings::Declaration::Plugin(_)))
    });
    let declared = match edits.declared() {
        Ok(declared) => declared,
        Err(failed) if a_plugin_s => {
            return Link::Refused(format!(
                "nothing was stored: the instance file does not read, so this link cannot be \
                 checked against its plugin ({failed})"
            ));
        }
        Err(_) => return Link::Stored,
    };
    let Some(manifest) = declared
        .plugins
        .iter()
        .find(|manifest| manifest.declared(name.name()).is_some())
    else {
        return Link::Stored;
    };
    match crate::plugin::PluginUrl::parse(value.expose().trim()) {
        None => Link::Refused(format!(
            "nothing was stored: that is not an http(s) link the {} plugin can fetch",
            manifest.name()
        )),
        Some(url) if let Some(calendar) = crate::google::ical::linked(&url) => {
            google_calendar(&declared, calendar)
        }
        // The plugin's fetch sends a secret link over TLS or loopback alone, so a grant for any
        // other origin would allow what is never fetched.
        Some(url) if !url.origin().is_confidential() => Link::Refused(format!(
            "nothing was stored: the {} plugin fetches a secret link only over https",
            manifest.name()
        )),
        Some(url) if manifest.allows(url.origin()) => Link::Fetched(manifest.name().clone()),
        Some(url) => Link::Outside {
            plugin: manifest.name().clone(),
            origin: url.origin().clone(),
        },
    }
}

/// A link to a Google calendar: never stored, since a Google account's calendars are read through
/// its sign-in with Google. An account's own calendar asks for that account's connection, unless
/// the deployment already holds the account.
fn google_calendar(
    declared: &crate::connect::Declared,
    calendar: crate::google::ical::Linked,
) -> Link {
    use crate::google::ical::Linked;
    const READ: &str = "nothing was stored: the box reads a Google calendar through its Google \
                        account, never by its link";
    let Linked::Account(account) = calendar else {
        return Link::Refused(format!("{READ}; connect the Google account that holds it"));
    };
    match crate::connect::Connection::parse("google", &[account.as_str().to_owned()]) {
        Ok(connection) if declared.connections().contains_key(&connection.key()) => {
            Link::Refused(format!("{READ}, and {account} is connected already"))
        }
        Ok(connection) => Link::Connect(connection),
        Err(failed) => Link::Refused(format!("{READ}; {account} cannot be connected: {failed}")),
    }
}

/// A secret's tap: stored, or for a plugin's link, refused, or held for the origin it is
/// served from, whose card `then` asks for once this one's response is recorded; a link stored
/// leaves its plugin's sync for `then`, so it is checked at once.
fn secret(
    writer: &dyn crate::settings::Writes,
    verify: &dyn crate::settings::Verify,
    reading: (&dyn crate::settings::Edits, &crate::settings::Settables),
    name: &crate::settings::Settable,
    value: &crate::capability::Secret,
    then: &mut Option<Then>,
) -> Performed {
    if reading
        .1
        .declaration(name)
        .is_some_and(|by| by.iter().any(crate::settings::Declaration::own_card))
    {
        return Performed::Tried(Tried::NotSent {
            reason: "nothing was stored: this name is asked for by a card of its own".to_owned(),
        });
    }
    match linked(reading, name, value) {
        Link::Stored => verified(writer, verify, name, value),
        Link::Fetched(plugin) => {
            let stored = verified(writer, verify, name, value);
            if matches!(stored, Performed::Done(_)) {
                *then = Some(Then::Check(plugin));
            }
            stored
        }
        Link::Refused(reason) => Performed::Tried(Tried::NotSent { reason }),
        Link::Outside { plugin, origin } => {
            // A step, not a failure: the card that allows the origin stands beside this one.
            let reason = format!(
                "first allow {origin}: the link is served from there, which the {plugin} plugin \
                 may not reach yet, so nothing was stored"
            );
            *then = Some(Then::Grant(plugin, origin));
            Performed::Tried(Tried::NotSent { reason })
        }
        Link::Connect(connection) => {
            let reason = format!(
                "nothing was stored: the box reads a Google calendar through its Google account, \
                 never by its link, so the card beside this one connects {}",
                connection.called()
            );
            *then = Some(Then::Connect(connection));
            Performed::Tried(Tried::NotSent { reason })
        }
    }
}

/// Write a connection's table; a refusal wrote nothing, so tapping again is safe.
fn connected(
    edits: &dyn crate::settings::Edits,
    connection: &crate::connect::Connection,
) -> Performed {
    match edits.add_connection(connection) {
        Ok(()) => Performed::Done(Done::Acted {
            what: format!(
                "connected {}: wrote {}",
                connection.key(),
                connection.writes()
            ),
        }),
        Err(failed) => Performed::Tried(Tried::NotSent {
            reason: format!("nothing was written: {failed}"),
        }),
    }
}

/// Take a connection's table out, clear every secret of it no connection left reads, and, on
/// a mail account's wider yes, delete the mail kept for it.
fn disconnected(
    removing: &Effectors<'_>,
    cards: &Cards<'_>,
    connection: &crate::connect::Connection,
    mail: KeptMail,
    then: &mut Option<Then>,
) -> Performed {
    let key = connection.key();
    let Ok(before) = removing.edits.declared() else {
        return Performed::Tried(Tried::NotSent {
            reason: "nothing was removed: the instance file does not read".to_owned(),
        });
    };
    // Read before the table goes: the account names where its mail is kept, and a secret is
    // minted against the names the instance declares while it still declares them.
    let account = before
        .accounts
        .iter()
        .find(|account| account.email.as_str() == key.name)
        .cloned();
    let declared = cards
        .settables
        .model()
        .map(|model| {
            crate::settings::Settables::derive(
                model.name(),
                before.channels.iter().copied(),
                cards.deployment.coding.as_ref(),
                &before.plugins,
            )
        })
        .unwrap_or_default()
        .join(crate::settings::Settables::google(&before.accounts));
    let owned: Vec<crate::settings::Settable> = connection
        .kept()
        .iter()
        .filter_map(|name| crate::settings::Settable::mint(&declared, name).ok())
        .collect();
    if let Err(failed) = removing.edits.remove_connection(connection) {
        return Performed::Tried(Tried::NotSent {
            reason: format!("nothing was removed: {failed}"),
        });
    }
    let cleared = match removing.edits.declared() {
        Ok(after) => {
            let read: Vec<String> = after
                .connections()
                .values()
                .flat_map(crate::connect::Connection::kept)
                .collect();
            owned
                .iter()
                .filter(|name| !read.iter().any(|held| held == name.name()))
                .map(|name| {
                    let label = crate::settings::Help::label(name.name());
                    match removing.secrets.clear_secret(name) {
                        Ok(()) => format!("; forgot {label}"),
                        Err(failed) => format!("; {label} is still stored: {failed}"),
                    }
                })
                .collect::<String>()
        }
        // A file that does not read cannot say which of these another connection still reads,
        // and a secret cleared from under one is lost, so all of them are kept.
        Err(failed) => format!(
            "; its secrets are kept, since the instance file does not read after the removal: \
             {failed}"
        ),
    };
    let removed = format!("removed {key}: took out {}{cleared}", connection.writes());
    // A sign-in is the account's, so it goes with the account whatever the card's wider yes;
    // both wait on the sync's hold, so they run once the inbox's is let go.
    let what = match (account, mail) {
        (Some(account), mail) => {
            let deleted = mail == KeptMail::Deleted;
            match crate::mail::removal::Removal::of(account.microsoft().is_some(), deleted) {
                Some(removal) => {
                    *then = Some(Then::Forget(account, removal));
                    format!("{removed}; {}", removal.next())
                }
                None => removed,
            }
        }
        (None, KeptMail::Deleted) => format!("{removed}; no mail was kept for it"),
        (None, KeptMail::Kept) => removed,
    };
    Performed::Done(Done::Acted { what })
}

/// Let a plugin reach one more origin.
fn granted(
    edits: &dyn crate::settings::Edits,
    plugin: &crate::plugin::PluginName,
    origin: &crate::plugin::Origin,
) -> Performed {
    match edits.add_origin(plugin, origin) {
        Ok(()) => Performed::Done(Done::Acted {
            what: format!("the {plugin} plugin may reach {origin}"),
        }),
        Err(failed) => Performed::Tried(Tried::NotSent {
            reason: format!("nothing was written: {failed}"),
        }),
    }
}

/// Write what `allowing` allows into the instance: nothing for this act alone, else the web
/// request's rule or the document's folder.
fn allowed(edits: &dyn crate::settings::Edits, allowing: Allowing<'_>) -> Result<(), Refusal> {
    match allowing {
        Allowing::Once => Ok(()),
        Allowing::Rule(rule) => edits
            .allow_web(rule)
            .map_err(|failed| Refusal::internal(format!("could not allow {rule}: {failed}"))),
        Allowing::Document(path) => {
            let folder = crate::secretary::acts::folder_of(path);
            edits
                .allow_drive(&folder)
                .map_err(|failed| Refusal::internal(format!("could not allow {folder}: {failed}")))
        }
    }
}

/// How many device cards may wait at once.
pub const WAITING_DEVICES: usize = 3;

/// `POST /api/act/key`: a device asks to be trusted to write as `actor`, the gate's actor for
/// its login (never a field of the request). Files one card in the default conversation for a
/// trusted device to tap; asking again answers the card already there.
///
/// # Errors
/// [`WAITING_DEVICES`] cards already waiting for other keys, and the stores' own failures.
pub fn ask_trust(
    caps: &Server<'_>,
    actor: &prodrome_core::event::Actor,
    key: &prodrome_core::sign::PublicKey,
) -> Answered<usize> {
    let secretary = caps.secretary;
    let stored = super::read_or_open_default(
        caps.secretary.conversations,
        caps.secretary.model,
        &super::store::DEFAULT,
        caps.secretary.clock.now(),
    )?;
    // A key nobody trusts can ask, so asking is bounded: past a few waiting device cards the
    // owner answers those first, and a stray key cannot flood the Inbox.
    // An unplaced line holds no act this box can read, so it is no device this box could trust.
    let lines = stored.lines(&inbox::fold(caps.secretary.inbox)?);
    let waiting: Vec<&crate::signatures::Enrolment> = lines
        .standings()
        .filter(|standing| standing.object.reading.open())
        .filter_map(|standing| match &standing.object.asked.proposal {
            Proposal::Act(Act::Device(enrolment)) => Some(enrolment),
            Proposal::Act(
                Act::Auth { .. }
                | Act::AuthSend { .. }
                | Act::Secret { .. }
                | Act::Web { .. }
                | Act::Patch { .. }
                | Act::DriveRead { .. }
                | Act::CodingSession { .. }
                | Act::SignIn { .. }
                | Act::SignInCode { .. }
                | Act::UnlockStore { .. }
                | Act::Connect { .. }
                | Act::Disconnect { .. }
                | Act::Grant { .. }
                | Act::Erase { .. }
                | Act::GoogleClient { .. }
                | Act::GoogleSignIn { .. }
                | Act::CalendarEvent { .. },
            )
            | Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Changes(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => None,
        })
        .collect();
    let asked_before = waiting.iter().any(|enrolment| enrolment.key == *key);
    if !asked_before && waiting.len() >= WAITING_DEVICES {
        let waiting = waiting.len();
        return Err(Refusal::too_many(format!(
            "{waiting} devices already wait to be trusted: answer their cards in the Inbox first"
        )));
    }
    // The request is its own evidence: the card names the key whole, and a person checks it.
    let asked = Reference::Proposal(ProposalRef {
        conversation: stored.id.clone(),
        index: stored.proposals.len(),
    });
    let reasons = super::store::Reasons::new(
        format!("a device asked to be trusted to write as {actor}"),
        vec![asked],
    )
    .map_err(|failed| Refusal::internal(failed.to_string()))?;
    super::verbs::propose(
        &secretary,
        &[],
        &stored.id,
        Proposal::Act(Act::Device(crate::signatures::Enrolment {
            actor: actor.clone(),
            key: key.clone(),
        })),
        &reasons,
        None,
        None,
    )
}

/// Why a device card leaves the Inbox when its key was trusted another way.
pub const ALREADY_TRUSTED: &str =
    "this device's key is trusted now: it was trusted at the box or with a pairing code";

/// Withdraw every waiting device card whose key `history` already trusts for its actor under
/// `root`: a key trusted at the terminal or with a pairing code leaves no request behind it.
/// Answers the cards withdrawn; run again, it withdraws nothing.
///
/// # Errors
/// The stores' own.
pub fn withdraw_trusted(
    inbox: &dyn crate::store::Prodrome<crate::secretary::inbox_event::InboxEvent>,
    history: &dyn crate::store::Prodrome<crate::record::TodoEvent>,
    root: &prodrome_core::sign::PublicKey,
    at: crate::capability::Datetime,
) -> Result<Vec<ProposalId>, Refusal> {
    let waiting: Vec<(ProposalId, crate::signatures::Enrolment)> = inbox::fold(inbox)?
        .objects()
        .filter(|object| object.reading.performs())
        .filter_map(|object| match &object.asked.proposal {
            Proposal::Act(Act::Device(enrolment)) => Some((object.id.clone(), enrolment.clone())),
            Proposal::Act(
                Act::Auth { .. }
                | Act::AuthSend { .. }
                | Act::Secret { .. }
                | Act::Web { .. }
                | Act::Patch { .. }
                | Act::DriveRead { .. }
                | Act::CodingSession { .. }
                | Act::SignIn { .. }
                | Act::SignInCode { .. }
                | Act::UnlockStore { .. }
                | Act::Connect { .. }
                | Act::Disconnect { .. }
                | Act::Grant { .. }
                | Act::GoogleClient { .. }
                | Act::GoogleSignIn { .. }
                | Act::CalendarEvent { .. }
                | Act::Erase { .. },
            )
            | Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Changes(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => None,
        })
        .collect();
    let mut withdrawn = Vec::new();
    for (id, enrolment) in waiting {
        if crate::signatures::trusted_for(history, root, &enrolment.key)?.contains(&enrolment.actor)
        {
            inbox::withdraw(
                inbox,
                &id,
                &super::writes_as(),
                at,
                Withdrawal::NoLongerNeeded {
                    why: ALREADY_TRUSTED.to_owned(),
                },
            )?;
            withdrawn.push(id);
        }
    }
    Ok(withdrawn)
}

/// A device card's endorsement checked before anything runs, and refused on any other card: a
/// device is trusted only by the person it writes as, from a device the chain already trusts
/// for them (SPEC law 42), so a new phone on the same login cannot tap its own card.
fn vouched(
    enrolling: crate::signatures::Enrolling<'_>,
    tap: &Tap,
    enrolment: &crate::signatures::Enrolment,
    endorsement: &crate::signatures::Endorsement,
) -> Result<prodrome_core::sign::Signed, Refusal> {
    if enrolment.actor != *tap.who() {
        return Err(Refusal::forbidden(format!(
            "a device is trusted by the person it writes as, {}",
            enrolment.actor
        )));
    }
    enrolling
        .endorsed(tap.who(), enrolment, endorsement)
        .map_err(|failed| Refusal::internal(failed.to_string()))?
        .map_err(|why| Refusal::forbidden(format!("this device cannot vouch: {}", why.said())))
}

/// Register a vouched device's key on every chain: a write that fails is an outcome, and
/// tapping again writes the same objects.
fn trusted(
    enrolling: crate::signatures::Enrolling<'_>,
    enrolment: &crate::signatures::Enrolment,
    endorsed: &prodrome_core::sign::Signed,
) -> Performed {
    match enrolling.enrol(enrolment, endorsed) {
        Ok(()) => Performed::Done(Done::Acted {
            what: format!("the device key {} trusted", enrolment.key.as_str()),
        }),
        Err(failed) => Performed::Tried(Tried::NotSent {
            reason: format!("could not register the key: {failed}"),
        }),
    }
}

/// Send a draft with its held values read again and checked against the card, each from a
/// message or file the proposal's conversation read, and its attachments fetched again and
/// checked, or nothing.
fn mailed(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    id: &ConversationId,
    draft: &Draft,
) -> Performed {
    let not_sent = |reason: String| Performed::Tried(Tried::NotSent { reason });
    let written = match held_value::Written::parse(&draft.body) {
        Ok(written) => written,
        Err(malformed) => return not_sent(malformed.to_string()),
    };
    let read = match super::read(caps.secretary.conversations, id) {
        Ok(stored) => ReadHolders::of(&stored),
        Err(refused) => return not_sent(refused.message().to_owned()),
    };
    if let Some(unread) = written.held().find(|held| match held.reach() {
        held_value::Reach::Revealable(handle) => !read.holds(&handle),
        held_value::Reach::Withheld => false,
    }) {
        return not_sent(format!(
            "the {} comes from a message or file this conversation did not read",
            unread.described()
        ));
    }
    let opening = Opening {
        caps: &caps.secretary,
        read: &read,
    };
    let holders = held_value::Holders {
        mail: effectors.reveal,
        files: &opening,
    };
    let body = match held_value::fill(&written, &holders, &draft.attachments) {
        Ok(body) => body,
        Err(unfilled) => return not_sent(unfilled.to_string()),
    };
    match attached::checked(&draft.attachments, |one| fetch(caps, &read, one)) {
        Ok(parts) => effectors.post.send(draft, body, parts),
        Err(refused) => not_sent(refused.to_string()),
    }
}

/// One attachment's bytes as its source holds them now, a repository's file as the box kept it
/// when proposed, by its digest.
fn fetch(
    caps: &Server<'_>,
    read: &ReadHolders,
    one: &attached::Attached,
) -> Result<Vec<u8>, String> {
    let source = match &one.source {
        AttachmentSource::Repo { .. } => &AttachmentSource::Kept {
            id: one.sha256.clone(),
        },
        source @ (AttachmentSource::Mail { .. }
        | AttachmentSource::Drive { .. }
        | AttachmentSource::Kept { .. }
        | AttachmentSource::Upload { .. }) => source,
    };
    crate::secretary::opening::open(&caps.secretary, source, read)
        .map(|opened| opened.bytes)
        .map_err(|refused| refused.message().to_owned())
}

/// Forward a stored message as its card showed it, or nothing: only one the proposal's
/// conversation read, its file read once and sent, or its text and attachments read from it,
/// only while its digest holds.
fn forwarded(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    id: &ConversationId,
    forward: &crate::secretary::store::Forward,
) -> Performed {
    use crate::held_value::Reopened;
    use crate::secretary::store::Style;
    let not_sent = |reason: String| Performed::Tried(Tried::NotSent { reason });
    let file = &forward.message.file;
    let read = match super::read(caps.secretary.conversations, id) {
        Ok(stored) => ReadHolders::of(&stored),
        Err(refused) => return not_sent(refused.message().to_owned()),
    };
    if !read.reads(&file.id) {
        return not_sent(
            "the message comes from no mail this conversation read; nothing was sent".to_owned(),
        );
    }
    let original = match effectors.reveal.original(&file.id, file.size) {
        Err(failed) => return not_sent(format!("the message could not be read: {failed}")),
        Ok(Reopened::Gone) => {
            return not_sent("the corpus no longer holds the message; nothing was sent".to_owned());
        }
        Ok(Reopened::Resized) => {
            return not_sent(attached::Unattachable::OriginalChanged.to_string());
        }
        Ok(Reopened::Read(original)) => original,
    };
    let note = match held_value::Written::parse(&forward.note)
        .map_err(|malformed| malformed.to_string())
        .and_then(|written| {
            let opening = Opening {
                caps: &caps.secretary,
                read: &read,
            };
            let holders = held_value::Holders {
                mail: effectors.reveal,
                files: &opening,
            };
            held_value::fill(&written, &holders, &[]).map_err(|unfilled| unfilled.to_string())
        }) {
        Ok(note) => note,
        Err(why) => return not_sent(why),
    };
    let body = match forward.message.style {
        Style::Inline => note.then(&forward.quoted(&original)),
        Style::Attached => note,
    };
    let read = crate::attachment::AttachmentId::of(&original.raw);
    match attached::read_from(&file.sha256, &read, forward.parts(original)) {
        Ok(parts) => effectors.post.send(&forward.draft(), body, parts),
        Err(changed) => not_sent(changed.to_string()),
    }
}

/// Whether the instance file declares `account` as a Google account now, or why not.
fn google_account(
    edits: &dyn crate::settings::Edits,
    account: &crate::mail::address::EmailAddress,
) -> Result<(), String> {
    let declared = edits.declared().map_err(|failed| {
        format!("nothing was written: the instance file does not read ({failed})")
    })?;
    if declared.accounts.iter().any(|held| {
        held.email.is(account.as_str()) && matches!(held.sync, crate::mail::account::Sync::Google)
    }) {
        Ok(())
    } else {
        Err(format!(
            "nothing was written: {account} is not a Google account the instance file declares"
        ))
    }
}

/// Keep the Google client pasted into its card: the downloaded file whole, kept only when it is
/// a web client listing `redirect`, its id and secret stored and the file's other words dropped.
/// A refusal stores nothing and says what to do, never quoting the secret.
fn google_client(
    writer: &dyn crate::settings::Writes,
    settables: &crate::settings::Settables,
    redirect: &crate::google::RedirectUri,
    pasted: &crate::capability::Secret,
) -> Result<Performed, Refusal> {
    let client = match crate::google::Client::of_download(pasted, redirect) {
        Ok(client) => client,
        Err(refused) => {
            return Ok(Performed::Tried(Tried::NotSent {
                reason: format!("nothing was kept: {refused}"),
            }));
        }
    };
    let mint = |name: &str| {
        crate::settings::Settable::mint(settables, name).map_err(|_| {
            Refusal::conflict(
                "no Google account is declared any more, so the box keeps no Google client",
            )
        })
    };
    let (id, secret) = (
        mint(crate::google::CLIENT_ID)?,
        mint(crate::google::CLIENT_SECRET)?,
    );
    let written = writer
        .write_secret(&id, &crate::capability::Secret::new(client.id.as_str()))
        .and_then(|()| {
            writer.write_secret(
                &secret,
                &crate::capability::Secret::new(client.secret.expose()),
            )
        });
    Ok(match written {
        Ok(()) => Performed::Done(Done::Acted {
            what: format!(
                "kept the Google client {}; each Google account's sign-in card follows",
                client.id.as_str()
            ),
        }),
        Err(failed) => Performed::Tried(Tried::NotSent {
            reason: format!("the secret store did not keep it: {failed}"),
        }),
    })
}

/// An erase card's words, erased as the owner's: the card's own conversation's, and no other's.
fn erased(
    caps: &Server<'_>,
    id: &ConversationId,
    keys: &std::collections::BTreeSet<crate::secretary::locked_text::KeyName>,
    tap: &Tap,
) -> Result<Performed, Refusal> {
    crate::secretary::transcript::erase(
        caps.secretary.conversations,
        id,
        keys.clone(),
        crate::secretary::erasure::Erasure::Asked {
            by: tap.who().clone(),
        },
        caps.secretary.clock.now(),
    )?;
    Ok(Done::Acted {
        what: format!(
            "erased {} passage{}",
            keys.len(),
            if keys.len() == 1 { "" } else { "s" }
        ),
    }
    .into())
}

/// The store asked again for every declared name: the unlock card is done once it answers for
/// them all, and may be tried again while it does not.
fn store_answers(
    secrets: &dyn crate::capability::Secrets,
    settables: &crate::settings::Settables,
) -> Performed {
    match crate::capability::Snapshot::read(secrets, settables.iter()).answers() {
        Ok(_) => Done::Acted {
            what: "the secret store answers".to_owned(),
        }
        .into(),
        Err(_) => Tried::NotSent {
            reason: "the secret store still could not answer; unlock it at the box and tap again"
                .to_owned(),
        }
        .into(),
    }
}

/// Keep `value` under `name` once a model that reads it answers a test call; a value that does
/// not answer is not kept, and the card may be tried again.
fn verified(
    writer: &dyn crate::settings::Writes,
    verify: &dyn crate::settings::Verify,
    name: &crate::settings::Settable,
    value: &crate::capability::Secret,
) -> Performed {
    if let Err(why) = verify.verify(name, value) {
        return Performed::Tried(Tried::NotSent {
            reason: format!(
                "kept nothing: a model reading {} did not answer: {why}",
                name.name()
            ),
        });
    }
    match writer.write_secret(name, value) {
        Ok(()) => Performed::Done(Done::Acted {
            what: format!("stored {}", crate::settings::Help::label(name.name())),
        }),
        // A put, so a retry is safe whether or not this one landed.
        Err(reason) => Performed::Tried(Tried::NotSent {
            reason: format!("could not store {}: {reason}", name.name()),
        }),
    }
}

/// What finishing a sign-in reaches: the open sessions, the test call and the store.
struct SignedIn<'a> {
    signing: &'a dyn crate::secretary::sign_in::Sessions,
    verify: &'a dyn crate::settings::Verify,
    writer: &'a dyn crate::settings::Writes,
}

/// A code card's tap: the sign-in it waits for, finished, whose own card `then` ends once this
/// one's response is recorded.
fn coded(
    effectors: &Effectors<'_>,
    url: &crate::secretary::sign_in::SignInUrl,
    code: &crate::capability::Secret,
    name: &crate::settings::Settable,
    then: &mut Option<Then>,
) -> Performed {
    let (outcome, began) = signed_in(
        &SignedIn {
            signing: effectors.signing,
            verify: effectors.verify,
            writer: effectors.secrets,
        },
        url,
        code,
        name,
    );
    *then = began.map(|(began, ended)| Then::End(began, ended));
    outcome
}

/// Type the tapped code into the sign-in at `url` and keep the token it gives under `name`, the
/// subscription's: the code card's outcome, and the sign-in card's to end it with. The token goes
/// from the session to the test call and the store and nowhere else.
fn signed_in(
    reach: &SignedIn<'_>,
    url: &crate::secretary::sign_in::SignInUrl,
    code: &crate::capability::Secret,
    name: &crate::settings::Settable,
) -> (Performed, Option<(ProposalId, Performed)>) {
    let again = |why: String| {
        Performed::Tried(Tried::NotSent {
            reason: format!("{why}; tap Connect to start another sign-in"),
        })
    };
    match reach.signing.finish(url, code) {
        None => (again("this sign-in is no longer open".to_owned()), None),
        Some((began, Err(why))) => {
            let ended = again(format!("no token came of the sign-in: {why}"));
            (ended.clone(), Some((began, ended)))
        }
        Some((began, Ok(token))) => match verified(reach.writer, reach.verify, name, &token) {
            stored @ Performed::Done(Done::Acted { .. }) => (
                stored,
                Some((
                    began,
                    Performed::Done(Done::Acted {
                        what: "signed in; the model's sign-in is stored".to_owned(),
                    }),
                )),
            ),
            refused => (
                refused,
                Some((began, again("the token it gave was not kept".to_owned()))),
            ),
        },
    }
}

/// Remember a tapped card's words on its authority, the fact naming the card's own text; a
/// refusal is an outcome, like a failed send.
fn remembered(
    held: &crate::secretary::memories::Held<'_>,
    clock: &dyn Clock,
    id: &ConversationId,
    index: usize,
    text: &crate::secretary::salted_text::TextName,
    kind: crate::secretary::memory::Kind,
) -> Performed {
    let origin = crate::secretary::memory::Origin::Tapped {
        conversation: id.clone(),
        proposal: index,
    };
    match held.remember_kept(text, kind, &origin, clock.now()) {
        Ok(remembered) => Performed::Done(Done::Acted {
            what: format!("remembered as fact {}", remembered.id),
        }),
        Err(refusal) => Performed::Tried(Tried::NotSent {
            reason: refusal.message().to_owned(),
        }),
    }
}

/// Append a changeset's acts against the chain the changeset names, never one the request names.
fn apply(cards: &Cards<'_>, changeset: &Changeset) -> Performed {
    let Some(bound) = cards
        .chains
        .iter()
        .find(|bound| bound.name == changeset.chain())
    else {
        return Performed::Tried(Tried::NotSent {
            reason: format!(
                "this deployment does not serve a chain named {}",
                changeset.chain()
            ),
        });
    };
    let known: Vec<&str> = cards
        .deployment
        .categories
        .keys()
        .map(String::as_str)
        .collect();
    match changes::apply(
        &bound.caps,
        bound.policy,
        &known,
        cards.deployment.zone,
        changeset,
        crate::secretary::WRITES_AS,
    ) {
        Ok(names) => Performed::Done(Done::Applied {
            objects: names.iter().map(|name| name.as_str().to_owned()).collect(),
            chain: changeset.chain().to_owned(),
        }),
        Err(changes::Unapplied::Refused(refusal)) => Performed::Tried(Tried::NotSent {
            reason: refusal.message().to_owned(),
        }),
        Err(changes::Unapplied::Appending(refusal)) => Performed::Done(Done::MayHaveGone {
            reason: refusal.message().to_owned(),
        }),
    }
}

/// `POST …/proposals/<n>/dismiss`: answers a proposal no, with a reason, behind the same [`Tap`].
///
/// # Errors
/// As [`super::dismiss`].
pub fn dismiss(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
    reason: &str,
) -> Answered<wire::ConversationPayload> {
    let after = super::dismiss(
        caps.secretary.conversations,
        caps.secretary.memories,
        caps.secretary.clock,
        by.who(),
        card,
        reason,
    )?;
    payload(&after, caps, cards)
}

/// `POST …/proposals/<n>/acknowledge`: a send that may have gone leaves the Inbox once checked.
///
/// # Errors
/// As [`super::acknowledge`].
pub fn acknowledge(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
) -> Answered<wire::ConversationPayload> {
    let after = super::acknowledge(
        caps.secretary.conversations,
        caps.secretary.inbox,
        caps.secretary.clock,
        by.who(),
        card,
    )?;
    payload(&after, caps, cards)
}

/// `POST …/todo/<id>/build`: file the coding-session card that builds `todo`, serving it, in
/// the box's `build` conversation; the owner's tap on that card is what fires a session.
///
/// # Errors
/// As [`Deployment::builder`], [`super::build::record`], [`super::build::admitted`] and
/// [`super::build::task`]; the stores' own.
pub fn build(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    todo: &TodoRef,
) -> Answered<wire::ConversationPayload> {
    cards.deployment.builder(&todo.chain)?;
    let bound = super::bound_of(cards.chains, Some(todo.chain.as_str()))?;
    let at = caps.secretary.clock.now();
    let believed =
        crate::believed::Believed::of(&bound.caps.chain.nodes()?, at, &bound.policy.untrusted)
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let record = super::build::record(&believed, todo)?;
    let (conversations, inbox) = (caps.secretary.conversations, caps.secretary.inbox);
    // The attempt read and the card filed under one hold, so two builds asked at once are not
    // the same attempt.
    let _held = inbox.locked()?;
    let task = super::build::task(
        todo,
        record,
        super::build::attempt(&inbox::fold(inbox)?, todo),
    )?;
    let conversation = &*super::build::CONVERSATION;
    let proposer = crate::secretary::proposer::Proposer::Box;
    super::filed::opened(conversations, conversation, "Builds", &proposer, at)?;
    super::filed::file_held(
        conversations,
        inbox,
        cards.chains,
        at,
        super::filed::Filing {
            conversation,
            proposer,
            proposal: Proposal::Act(Act::CodingSession { task }),
            reasons: crate::secretary::store::Reasons::new(
                format!("{} asked for {todo} to be built.", by.who().as_str()),
                vec![Reference::Todo(todo.clone())],
            )
            .map_err(|failed| Refusal::internal(failed.to_string()))?,
            serves: Some(todo.clone()),
            revises: None,
        },
    )?;
    payload(&super::read(conversations, conversation)?, caps, cards)
}

/// `POST …/proposals/<n>/drop`: let a session go, with a reason.
///
/// # Errors
/// As [`crate::secretary::coding::dropped`].
pub fn drop_session(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
    reason: &str,
) -> Answered<wire::ConversationPayload> {
    let (id, index) = (&card.conversation, card.index);
    crate::secretary::coding::dropped(
        caps.secretary.conversations,
        id,
        index,
        by.who(),
        reason,
        caps.secretary.clock.now(),
    )?;
    payload(&super::read(caps.secretary.conversations, id)?, caps, cards)
}

/// The newest window, full-size, which always holds the round that just happened, dressed with
/// the box's renderer, its memory and its chains.
fn payload(
    stored: &Stored,
    caps: &Server<'_>,
    cards: &Cards<'_>,
) -> Answered<wire::ConversationPayload> {
    let fold = inbox::fold(caps.secretary.inbox)?;
    let drawn = Drawn::of(
        &stored.lines(&fold),
        cards,
        caps.secretary.clock.now(),
        caps.secretary.render,
    );
    // A memory that will not read draws every receipt and every card's words as unread.
    let memory = memory(caps.secretary.memories).ok();
    Ok(readings::conversation(
        stored,
        &fold,
        &Window::newest(),
        &Dress {
            answers: caps.answers,
            memory: memory.as_ref(),
            draws: &drawn,
        },
    ))
}

/// `GET /api/secretary/memory`: every standing fact, oldest first, and the words the text store
/// keeps for the inbox's remember cards, by the name each card gives them.
///
/// # Errors
/// The stores' own.
pub fn memory(memories: crate::secretary::memories::Memories<'_>) -> Answered<wire::MemoryPayload> {
    Ok(wire::MemoryPayload {
        cards: card_words(memories)?,
        ..memory_payload(&memories.reads().read()?)
    })
}

/// Every remember card's words the text store still keeps, once per name.
fn card_words(
    memories: crate::secretary::memories::Memories<'_>,
) -> Result<Vec<wire::CardWords>, Refusal> {
    let mut named = std::collections::BTreeSet::new();
    let mut words = Vec::new();
    for object in inbox::fold(memories.cards)?.objects() {
        if let Proposal::Remember { text, .. } = &object.asked.proposal
            && named.insert(text.clone())
            && let Some(fact) = memories.reads().words(text)?
        {
            words.push(wire::CardWords {
                text: text.as_str().to_owned(),
                fact: fact.as_str().to_owned(),
            });
        }
    }
    Ok(words)
}

/// `POST /api/secretary/memory/<id>/forget`: the fact forgotten and its words deleted;
/// transcripts that quoted it keep it.
///
/// # Errors
/// [`Refusal::not_found`] for a number no standing fact carries, and the store's own.
pub fn forget(
    memories: crate::secretary::memories::Memories<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
    id: crate::secretary::memory::FactId,
) -> Answered<wire::MemoryPayload> {
    memories.forget(id, clock.now(), by)?;
    memory(memories)
}

/// `GET /api/secretary/record`: the summaries of the owner's words a session opening is told.
///
/// # Errors
/// The stores' own.
pub fn record(digests: &crate::secretary::digests::Digests<'_>) -> Answered<wire::RecordPayload> {
    let record = crate::secretary::shown_record::OwnersRecord::of(
        crate::secretary::compaction::Reading::of(&digests.reads())?,
    );
    Ok(wire::RecordPayload {
        summaries: record
            .summaries(crate::secretary::digest_view::VIEW_BYTES)
            .into_iter()
            .map(|(part, line)| wire::RecordSummary {
                node: part.node.as_str().to_owned(),
                handle: part.handle(),
                lines: u32::try_from(part.span.len()).unwrap_or(u32::MAX),
                text: line.as_str().to_owned(),
            })
            .collect(),
    })
}

/// `POST /api/secretary/record/<node>/retract`: the owner's tap says the summary `node` shows is
/// wrong. Its node is asked again, and so is every summary above it.
///
/// # Errors
/// [`Refusal::not_found`] for a node that shows no summary, and the stores' own.
pub fn retract(
    memories: crate::secretary::memories::Memories<'_>,
    clock: &dyn Clock,
    tap: &crate::secretary::tap::Tap,
    node: &prodrome_core::event::Hash,
) -> Answered<wire::RecordPayload> {
    memories.hold()?.retract(node, clock.now(), tap)?;
    record(&memories.digests)
}

fn memory_payload(memory: &crate::secretary::memory::Memory) -> wire::MemoryPayload {
    use crate::secretary::provenance::Wrote;
    let number = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    wire::MemoryPayload {
        facts: memory
            .facts
            .iter()
            .map(|remembered| wire::FactRow {
                id: remembered.id,
                at: crate::secretary::store::stamp(remembered.at),
                fact: remembered.fact.as_str().to_owned(),
                kind: remembered.kind,
                said: match &remembered.said.wrote {
                    Wrote::Said(origin) => wire::FactSaid::Said {
                        on: said_on(origin),
                    },
                    Wrote::Merged { from, round } => wire::FactSaid::Merged {
                        from: from.iter().copied().collect(),
                        origins: remembered.said.origins.iter().map(said_on).collect(),
                        round: said_on(round),
                    },
                },
            })
            .collect(),
        cap: number(crate::secretary::memory::MAX_LIVE),
        forgotten: memory.forgotten.iter().map(|gone| gone.id).collect(),
        merged: memory
            .merged
            .iter()
            .map(|merge| wire::MergedFact {
                fact: merge.fact,
                into: merge.into,
            })
            .collect(),
        cards: Vec::new(),
    }
}

/// Where `origin` was said, as the page links it.
fn said_on(origin: &crate::secretary::memory::Origin) -> wire::SaidOn {
    let number = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    match origin {
        crate::secretary::memory::Origin::Turn { conversation, turn } => wire::SaidOn::Turn {
            conversation: conversation.as_str().to_owned(),
            turn: number(*turn),
        },
        crate::secretary::memory::Origin::Tapped {
            conversation,
            proposal,
        } => wire::SaidOn::Tapped {
            conversation: conversation.as_str().to_owned(),
            proposal: number(*proposal),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Answers;
    use crate::secretary::readings::{Place, proposal, turn};
    use crate::secretary::responded::Responded;
    use crate::secretary::store::Asking;
    use crate::secretary::store::Reasons;
    use crate::secretary::store::Said;
    use crate::secretary::store::Standing;

    /// `proposal`, the conversation's first, answered `responses` as the inbox reads them.
    fn standing_of(proposal: Proposal, responses: &[Responded]) -> Standing {
        use crate::secretary::inbox_event::{InboxEvent, Said};
        let asked = inbox::Asked {
            proposer: crate::secretary::proposer::Proposer::Secretary,
            proposal,
            reasons: reasons(),
            asked: "proposal:default/0".parse().expect("a reference"),
            price: inbox::Price::unpriced(),
        };
        let at = crate::capability::Datetime::new(2026, 9, 11, 9, 0, 0, 0).expect("an instant");
        let id = ProposalId::of(&asked, at);
        let said = |said| InboxEvent {
            proposal: id.clone(),
            at,
            actor: prodrome_core::event::Actor::new("ada").expect("an actor"),
            said,
        };
        let events: Vec<InboxEvent> = std::iter::once(said(Said::Asked(Box::new(asked))))
            .chain(
                responses
                    .iter()
                    .map(|response| said(Said::Responded(response.clone()))),
            )
            .collect();
        let object = inbox::Fold::of_chain(&events)
            .get(&id)
            .expect("held")
            .clone();
        Standing {
            asking: Asking {
                index: 0,
                at,
                object: id,
                after_turn: 0,
                revises: None,
            },
            object,
            revises: None,
        }
    }

    fn reasons() -> Reasons {
        Reasons::new(
            "He asked for the documents and the last scan is done.",
            vec!["mail:a1b2c3d4e5f6".parse().expect("a reference")],
        )
        .expect("reasons")
    }

    /// An empty [`Drawn`], for a test that never proposes a changeset.
    use crate::capability::Parses;

    fn no_chain() -> Drawn<'static> {
        Drawn {
            nodes: std::collections::BTreeMap::new(),
            typeset: &Parses,
            now: crate::zone::Moment {
                at: crate::capability::Datetime::new(2026, 9, 9, 9, 2, 0, 0)
                    .expect("a real instant"),
                zone: crate::zone::Zone::UTC,
            },
            categories: Vec::new(),
            model: None,
            routine: None,
            plugins: Vec::new(),
            reach: crate::connect::PLAIN.clone(),
            sources: std::collections::BTreeMap::new(),
        }
    }

    /// Every field of a proposal reaches the wire.
    #[test]
    fn a_proposal_row_carries_the_whole_message() {
        let draft = Draft {
            account: "ada@example.com".to_owned(),
            to: vec!["counsel@example.test".to_owned()],
            cc: vec!["ada@example.org".to_owned()],
            subject: "The permit".to_owned(),
            body: "Sending the documents today, the last one is scanned.".to_owned(),
            attachments: Vec::new(),
        };
        let mut standing = standing_of(Proposal::Email(draft.clone()), &[]);
        standing.asking.after_turn = 3;
        let row = proposal(2, &standing, &no_chain(), None);
        assert_eq!(row.index, 2);
        assert_eq!(row.turn, 3, "the position travels to the wire too");
        let wire::ProposedAct::Email {
            account,
            to,
            cc,
            subject,
            body,
            attachments,
            ..
        } = &row.act
        else {
            panic!("an email proposal drew a changes act: {:?}", row.act);
        };
        assert_eq!(*account, draft.account);
        assert_eq!(*to, draft.to);
        assert_eq!(*cc, draft.cc);
        assert_eq!(*subject, draft.subject);
        assert_eq!(*body, draft.body);
        assert!(attachments.is_empty());
        assert_eq!(row.state, wire::ProposalState::Standing);
        // The reasons are part of the confirmation.
        assert_eq!(row.why, reasons().why());
        assert_eq!(
            row.sources,
            vec![
                "mail:a1b2c3d4e5f6"
                    .parse::<Reference>()
                    .expect("a reference")
            ]
        );
    }

    /// A renderer that panics if asked, since a verb turn has no rendering.
    struct NeverAsked;
    impl Answers for NeverAsked {
        fn block(
            &self,
            _block: &crate::secretary::answer::Renderable,
        ) -> Result<crate::capability::Html, crate::capability::AnswerError> {
            panic!("a turn that is not the model's was rendered");
        }
    }

    /// A row prints its speaker as `who` beside only that speaker's fields: a person's row has
    /// no verb, refusal, receipt or blocks to misread.
    #[test]
    fn a_turn_row_holds_only_its_speakers_fields() {
        let person = turn(
            &NeverAsked,
            &Said {
                at: crate::capability::Datetime::new(2026, 9, 9, 9, 1, 0, 0).expect("an instant"),
                turn: crate::capability::Turn::Person {
                    text: "what changed".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: None,
            },
            Place::at(0),
            None,
        );
        let printed = serde_json::to_value(&person).expect("a row prints");
        let keys: std::collections::BTreeSet<&str> = printed
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "at",
                "attachments",
                "attempt",
                "gone",
                "index",
                "queued",
                "references",
                "text",
                "who"
            ]
            .into_iter()
            .collect(),
            "{printed}"
        );
        assert_eq!(printed["who"], "person");
    }

    /// A memory of one standing fact, one forgotten and one replaced by the standing one.
    fn receipts_memory() -> crate::secretary::memory::Memory {
        use crate::secretary::memory::{
            Fact, FactId, Forgotten, Kind, Memory, Merge, Origin, Remembered,
        };
        Memory {
            facts: vec![Remembered {
                id: FactId(1),
                at: crate::capability::Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant"),
                fact: Fact::new("He takes the 7:40 train").expect("a fact"),
                kind: Kind::Owner,
                said: crate::secretary::provenance::Provenance::said(Origin::Turn {
                    conversation: ConversationId::new("default").expect("a slug"),
                    turn: 0,
                }),
            }],
            merged: vec![Merge {
                fact: FactId(4),
                into: FactId(1),
            }],
            forgotten: vec![Forgotten {
                id: FactId(2),
                at: crate::capability::Datetime::new(2026, 10, 2, 9, 2, 0, 0).expect("an instant"),
                by: "ada".to_owned(),
                version: 3,
            }],
            version: 3,
        }
    }

    /// A receipt's words are the memory's, whatever the answer quoted: an answer stored before the
    /// verb stopped echoing its fact still names it, and a forgotten fact still draws none.
    #[test]
    fn a_receipt_is_the_shapes_fact_and_its_words_the_memorys() {
        use crate::secretary::memory::{FactId, Memory};
        use crate::secretary::outcome::Outcome;
        let memory = receipts_memory();
        let row = |turn_said: crate::capability::Turn, memory: Option<&Memory>| match turn(
            &NeverAsked,
            &Said {
                at: crate::capability::Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant"),
                turn: turn_said,
                by: None,
            },
            Place::at(2),
            memory.map(memory_payload).as_ref(),
        )
        .who
        {
            wire::Speaker::Verb { outcome, .. } => outcome,
            other => panic!("a verb's turn reads as {other:?}"),
        };
        // The answer names another fact than the shape, so a reading of the answer would show.
        let drawn = |outcome: Outcome, memory: Option<&Memory>| {
            row(
                crate::capability::Turn::Tool {
                    verb: "remember".into(),
                    answer: r#"{"remembered": 5, "kind": "owner"}"#.to_owned(),
                    outcome,
                    files: Vec::new(),
                },
                memory,
            )
        };
        let remembered = |fact: u64| Outcome::Remembered { fact: FactId(fact) };
        let receipt = |receipt| wire::VerbOutcome::Remembered { receipt };
        let standing = receipt(wire::Receipt::Standing {
            fact: FactId(1),
            text: "He takes the 7:40 train".to_owned(),
        });
        assert_eq!(drawn(remembered(1), Some(&memory)), standing);
        // A forgotten or replaced fact names no word, nor does one whose words are lost.
        for (fact, drawn_as) in [
            (2, wire::Receipt::Forgotten { fact: FactId(2) }),
            (
                4,
                wire::Receipt::Merged {
                    fact: FactId(4),
                    into: FactId(1),
                },
            ),
            (9, wire::Receipt::Unread { fact: FactId(9) }),
        ] {
            assert_eq!(drawn(remembered(fact), Some(&memory)), receipt(drawn_as));
        }
        assert_eq!(
            drawn(remembered(2), None),
            receipt(wire::Receipt::Unread { fact: FactId(2) }),
            "a memory that will not read"
        );
        assert_eq!(
            drawn(Outcome::Answered, Some(&memory)),
            wire::VerbOutcome::Answered,
            "an answer that wrote no fact, such as a card filed, has no receipt"
        );
        assert_eq!(
            drawn(Outcome::Refused, Some(&memory)),
            wire::VerbOutcome::Refused
        );
        // Erased by the owner, not by a forget: the shape still names the fact.
        let erased = crate::secretary::turn_shape::join(
            crate::secretary::turn_shape::TurnShape::Tool {
                verb: "remember".into(),
                outcome: remembered(1),
                files: Vec::new(),
            },
            vec![
                crate::secretary::words::Gone::Erased {
                    erasure: crate::secretary::erasure::Erasure::Asked {
                        by: prodrome_core::event::Actor::new("ada").expect("an actor"),
                    },
                }
                .into(),
            ],
        );
        assert!(matches!(erased, crate::capability::Turn::Erased { .. }));
        assert_eq!(
            row(erased, Some(&memory)),
            standing,
            "a receipt outlives its turn's words"
        );
    }

    struct AlwaysRenders;
    impl Answers for AlwaysRenders {
        fn block(
            &self,
            block: &crate::secretary::answer::Renderable,
        ) -> Result<crate::capability::Html, crate::capability::AnswerError> {
            Ok(crate::capability::Html::from_render(format!(
                "<p>{}</p>",
                block.as_str().trim()
            )))
        }
    }

    struct NeverRenders;
    impl Answers for NeverRenders {
        fn block(
            &self,
            _block: &crate::secretary::answer::Renderable,
        ) -> Result<crate::capability::Html, crate::capability::AnswerError> {
            Err(crate::capability::AnswerError("typst refused".to_owned()))
        }
    }

    /// A model turn is drawn in blocks, each asked of the renderer; a refused one is text. That
    /// `html` is refused is the closed world's law (`suzatary_io::render`), not a scan here.
    #[test]
    fn a_model_turns_blocks_each_ask_the_renderer() {
        let model_turn = Said {
            at: crate::capability::Datetime::new(2026, 9, 9, 9, 2, 0, 0).expect("an instant"),
            turn: crate::capability::Turn::Model {
                text: "= Hello\n\n#html.elem(\"b\")[x]".to_owned(),
                calls: Vec::new(),
            },
            by: None,
        };
        let blocks =
            |answers: &dyn Answers| match turn(answers, &model_turn, Place::at(0), None).who {
                wire::Speaker::Model { blocks } => blocks,
                other => panic!("a model's turn reads as {other:?}"),
            };
        let rendered = blocks(&AlwaysRenders);
        assert_eq!(rendered.len(), 2);
        assert_eq!(rendered[0].html.as_deref(), Some("<p>= Hello</p>"));
        assert_eq!(rendered[0].plain, "Hello");
        assert_eq!(rendered[0].kind, crate::secretary::answer::Kind::Heading);
        assert_eq!(
            rendered[1].html.as_deref(),
            Some("<p>#html.elem(\"b\")[x]</p>"),
            "the renderer, not a scan, answers for this block"
        );
        let refused = blocks(&NeverRenders);
        assert!(refused.iter().all(|block| block.html.is_none()));
    }

    /// Neither a person's turn nor an act asks the renderer.
    #[test]
    fn only_a_model_turn_is_ever_rendered() {
        let person = Said {
            at: crate::capability::Datetime::new(2026, 9, 9, 9, 1, 0, 0).expect("an instant"),
            turn: crate::capability::Turn::Person {
                text: "what changed".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        };
        assert!(matches!(
            turn(&NeverAsked, &person, Place::at(0), None).who,
            wire::Speaker::Person { .. }
        ));
        let act = Said {
            at: crate::capability::Datetime::new(2026, 9, 9, 9, 3, 0, 0).expect("an instant"),
            turn: crate::capability::Turn::Act {
                text: "dismissed proposal 0".to_owned(),
                references: Vec::new(),
            },
            by: None,
        };
        assert!(matches!(
            turn(&NeverAsked, &act, Place::at(0), None).who,
            wire::Speaker::Box { .. }
        ));
    }

    /// The tap's own [`Tap`], minted as the gate mints it.
    fn tapped() -> Tap {
        Tap::of(
            &crate::signatures::scene("ada"),
            &crate::gate::ActHeaders {
                act: Some("1"),
                client: Some(crate::gate::CLIENT),
                content_type: Some("application/json"),
                site: Some("same-origin"),
            },
        )
        .expect("a tap")
    }

    fn value() -> TapAnswer {
        TapAnswer::Value(crate::capability::Secret::new("a value"))
    }

    fn endorsed() -> TapAnswer {
        TapAnswer::Endorsed(crate::signatures::Endorsement {
            key: crate::cast::keys::root(),
            signature: prodrome_core::sign::Signature::new("00".repeat(64)).expect("a signature"),
        })
    }

    /// The names a deployment with an ntfy channel declares.
    fn with_ntfy() -> crate::settings::Settables {
        crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            [crate::connect::Channel::Ntfy],
            None,
            &[],
        )
    }

    /// `proposal` paired with `answer` against `settables`, on a deployment with no root, by
    /// a tap that takes no memory hold.
    fn prepared<'p>(
        proposal: &'p Proposal,
        answer: &'p TapAnswer,
        settables: &crate::settings::Settables,
    ) -> Result<Prepared<'p, 'static>, Refusal> {
        let tap = tapped();
        let hold = || Err(Refusal::internal("no memory in this test"));
        Prepared::of(
            proposal,
            answer,
            &Preparing {
                settables,
                enrolling: None,
                tap: &tap,
                hold: &hold,
            },
        )
    }

    fn refused(prepared: Result<Prepared<'_, '_>, Refusal>) -> Refusal {
        match prepared {
            Ok(_) => panic!("prepared"),
            Err(refusal) => refusal,
        }
    }

    /// A card that takes nothing beside its note refuses each answer it has no use for, by name,
    /// before anything runs.
    #[test]
    fn an_answer_a_card_does_not_take_is_refused_before_anything_runs() {
        let email = Proposal::Email(Draft::default());
        assert!(matches!(
            prepared(&email, &TapAnswer::Plain, &with_ntfy()),
            Ok(Prepared::Email(_))
        ));
        for (answer, words) in [
            (value(), VALUE_BELONGS),
            (TapAnswer::Always, ALWAYS_BELONGS),
            (endorsed(), ENDORSEMENT_BELONGS),
        ] {
            let refusal = refused(prepared(&email, &answer, &with_ntfy()));
            assert_eq!(refusal.status(), crate::refusal::Status::BadRequest);
            assert_eq!(refusal.message(), words);
        }
    }

    /// A secret card stores its text, and the tap mints it against the names declared at the
    /// tap: one its deployment stopped declaring stores nothing, and one with no value is refused.
    #[test]
    fn a_secret_s_name_is_minted_at_the_tap_against_the_names_declared_then() {
        let card = Proposal::Act(Act::Secret {
            name: "NTFY_TOPIC".to_owned(),
            why: "the ntfy notification channel reads it".to_owned(),
        });
        let answer = value();
        match prepared(&card, &answer, &with_ntfy()) {
            Ok(Prepared::Secret { name, .. }) => assert_eq!(name.name(), "NTFY_TOPIC"),
            Ok(_) => panic!("not a secret"),
            Err(refusal) => panic!("{refusal}"),
        }
        let undeclared = crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            std::iter::empty(),
            None,
            &[],
        );
        let refusal = refused(prepared(&card, &answer, &undeclared));
        assert_eq!(refusal.status(), crate::refusal::Status::Conflict);
        assert!(
            refusal
                .message()
                .contains("no longer a secret this deployment declares"),
            "{refusal}"
        );
        let refusal = refused(prepared(&card, &TapAnswer::Plain, &with_ntfy()));
        assert_eq!(refusal.message(), "value is required to store NTFY_TOPIC");
    }

    /// A code card keeps its token under the subscription's name, minted at the tap.
    #[test]
    fn a_code_card_mints_the_subscription_s_name_at_the_tap() {
        let card = Proposal::Act(Act::SignInCode {
            url: crate::secretary::sign_in::SignInUrl::parse(
                "https://sign-in.example.test/oauth/authorize?code=true&state=s",
            )
            .expect("a link"),
        });
        let answer = value();
        match prepared(&card, &answer, &with_ntfy()) {
            Ok(Prepared::Code { name, .. }) => {
                assert_eq!(name.name(), crate::settings::MODEL_TOKEN);
            }
            Ok(_) => panic!("not a code"),
            Err(refusal) => panic!("{refusal}"),
        }
        let refusal = refused(prepared(
            &card,
            &answer,
            &crate::settings::Settables::default(),
        ));
        assert_eq!(refusal.status(), crate::refusal::Status::Conflict);
        let refusal = refused(prepared(&card, &TapAnswer::Plain, &with_ntfy()));
        assert_eq!(
            refusal.message(),
            "value is required: the code the sign-in page shows"
        );
    }

    /// "Always" is taken by a web request with a rule, a document and a mail account's removal,
    /// and refused by the same kinds of card that offer no wider yes.
    #[test]
    fn always_is_taken_only_by_a_card_with_a_wider_yes() {
        let request = crate::web::WebRequest::Fetch {
            url: "https://example.com/a".parse().expect("a public URL"),
        };
        let ruled = Proposal::Act(Act::Web {
            request: request.clone(),
            rule: Some("*.example.com".parse().expect("a rule")),
        });
        assert!(matches!(
            prepared(&ruled, &TapAnswer::Always, &with_ntfy()),
            Ok(Prepared::Begin {
                allowing: Allowing::Rule(_),
                ..
            })
        ));
        assert!(matches!(
            prepared(&ruled, &TapAnswer::Plain, &with_ntfy()),
            Ok(Prepared::Begin {
                allowing: Allowing::Once,
                ..
            })
        ));
        let unruled = Proposal::Act(Act::Web {
            request,
            rule: None,
        });
        let refusal = refused(prepared(&unruled, &TapAnswer::Always, &with_ntfy()));
        assert_eq!(refusal.message(), ALWAYS_BELONGS);

        let account = Proposal::Act(Act::Disconnect {
            connection: crate::connect::Connection::parse(
                "lieer",
                &["ada@example.test".to_owned()],
            )
            .expect("an account"),
        });
        assert!(matches!(
            prepared(&account, &TapAnswer::Always, &with_ntfy()),
            Ok(Prepared::Disconnect {
                mail: KeptMail::Deleted,
                ..
            })
        ));
        assert!(matches!(
            prepared(&account, &TapAnswer::Plain, &with_ntfy()),
            Ok(Prepared::Disconnect {
                mail: KeptMail::Kept,
                ..
            })
        ));
        let calendar = Proposal::Act(Act::Disconnect {
            connection: crate::connect::Connection::parse("ics", &["Work".to_owned()])
                .expect("a calendar"),
        });
        let refusal = refused(prepared(&calendar, &TapAnswer::Always, &with_ntfy()));
        assert_eq!(refusal.message(), ALWAYS_BELONGS);
    }

    /// A device card is tapped from a trusted device, which signs it, on a deployment with a
    /// root; without either nothing runs.
    #[test]
    fn a_device_card_needs_an_endorsement_and_a_root() {
        let card = Proposal::Act(Act::Device(crate::signatures::Enrolment {
            actor: prodrome_core::event::Actor::new("ada").expect("an actor"),
            key: crate::cast::keys::root(),
        }));
        let refusal = refused(prepared(&card, &TapAnswer::Plain, &with_ntfy()));
        assert_eq!(refusal.status(), crate::refusal::Status::Forbidden);
        let answer = endorsed();
        let refusal = refused(prepared(&card, &answer, &with_ntfy()));
        assert_eq!(refusal.status(), crate::refusal::Status::BadRequest);
        assert!(refusal.message().contains("declares no root"), "{refusal}");
        let refusal = refused(prepared(&card, &value(), &with_ntfy()));
        assert_eq!(refusal.message(), VALUE_BELONGS);
    }
}
