//! The secretary's routes as functions: the binary parses a request, calls one, serialises.
//!
//! [`tap`] alone ends in an irreversible act: it takes a [`Tap`] only the gate mints, and an index,
//! never a body.

use crate::server::Server;
use suzatary_base::capability::{Clock, Secret};
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::{ProposalRef, Reference, TodoRef};
use suzatary_base::refusal::{Answered, Refusal};
use suzatary_core::believed::Preview;
use suzatary_core::deployment::Deployment;
use suzatary_core::health::plugin::{Origin, PluginName};
use suzatary_core::held_value::{self, Filled, Reveal};
use suzatary_core::mail::oauth::Consent;
use suzatary_core::secretary::Bound;
use suzatary_core::secretary::act_lines::ActLine;
use suzatary_core::secretary::acts::Act;
use suzatary_core::secretary::attached::{self, Checked};
use suzatary_core::secretary::attachment_source::AttachmentSource;
use suzatary_core::secretary::changes::{self, Changeset};
use suzatary_core::secretary::conversations;
use suzatary_core::secretary::decide;
use suzatary_core::secretary::fix::Fix;
use suzatary_core::secretary::inbox::{self, ProposalId, Response};
use suzatary_core::secretary::model::Turn;
use suzatary_core::secretary::opening::Opening;
use suzatary_core::secretary::proposal::{Draft, Proposal};
use suzatary_core::secretary::read_holders::ReadHolders;
use suzatary_core::secretary::readings::{self, Draw, Dress, Window};
use suzatary_core::secretary::responded::{Done, Performed, Tried};
use suzatary_core::secretary::store::{Entry, Kind, Stored, Title};
use suzatary_core::secretary::transcript;
use suzatary_core::secrets::Secrets;
use suzatary_core::settings::{Edits, Settable, Settables, Writes};
use suzatary_core::signatures::Tap;
use suzatary_core::wire::topics as wire;

/// Sending a proposed message; in no capability bundle, so no verb can send.
pub trait Post {
    /// Send this message as `account`, with `body` filled in and `attachments`.
    fn send(&self, draft: &Draft, body: Filled, attachments: Checked) -> Performed;
}

/// Performs a proposed act; like [`Post`], in no bundle, behind a tap's [`Consent`].
pub trait Perform {
    /// Begin the act and answer the outcome to record now.
    fn begin(
        &self,
        id: &ConversationId,
        index: usize,
        object: &ProposalId,
        proposer: &suzatary_core::secretary::proposer::Proposer,
        act: &Act,
        consent: Consent,
    ) -> Performed;
}

/// What a conversation payload needs besides its transcript.
#[derive(Clone, Copy)]
pub struct Cards<'a> {
    /// The categories an act may name.
    pub deployment: &'a Deployment,
    /// Every chain this deployment serves.
    pub chains: &'a [Bound<'a>],
    /// The plugins the instance file declares now.
    pub plugins: Option<&'a [suzatary_core::plugin::Manifest]>,
    /// How the person reaches the app at a terminal.
    pub reach: &'a suzatary_core::connect::Reach,
    /// The names the deployment may set now.
    pub settables: &'a Settables,
}

/// The chains a standing changeset names, each folded once.
struct Drawn<'a> {
    nodes: std::collections::BTreeMap<String, (Preview, prodrome_core::policy::Untrusted)>,
    typeset: &'a dyn suzatary_base::capability::Typeset,
    now: suzatary_base::zone::Moment,
    categories: Vec<String>,
    routine: Option<suzatary_core::connect::RoutineId>,
    model: Option<Settable>,
    plugins: Vec<suzatary_core::plugin::Manifest>,
    reach: suzatary_core::connect::Reach,
    sources: std::collections::BTreeMap<String, String>,
}

impl<'a> Drawn<'a> {
    fn of(
        lines: &suzatary_core::secretary::store::Lines,
        cards: &Cards<'_>,
        at: suzatary_base::capability::Datetime,
        typeset: &'a dyn suzatary_base::capability::Typeset,
    ) -> Self {
        let wanted: std::collections::BTreeSet<&str> = lines
            .standings()
            .filter_map(|standing| match &standing.object.asked.proposal {
                Proposal::Changes(changeset) => Some(changeset.chain()),
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
            now: suzatary_base::zone::Moment {
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

/// The message of every held value a standing email carries.
fn sources(
    lines: &suzatary_core::secretary::store::Lines,
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
    fn routine(&self) -> Option<&suzatary_core::connect::RoutineId> {
        self.routine.as_ref()
    }

    fn model(&self) -> Option<&str> {
        self.model.as_ref().map(Settable::name)
    }

    fn source(&self, handle: &held_value::Handle) -> String {
        let file = match handle {
            held_value::Handle::Value(place) => match place.holder() {
                held_value::Holder::Message(message) => {
                    return self.sources.get(message).cloned().unwrap_or_else(|| {
                        suzatary_core::secretary::readings::UNREAD_SOURCE.to_owned()
                    });
                }
                held_value::Holder::File(file) => file,
            },
            held_value::Handle::File(file) => file,
        };
        format!("from {}", file.source().describe())
    }

    fn reach(&self) -> &suzatary_core::connect::Reach {
        &self.reach
    }

    fn origins(&self, plugin: &str) -> Option<Vec<String>> {
        self.plugins
            .iter()
            .find(|manifest| manifest.name().as_str() == plugin)
            .map(|manifest| manifest.origins().map(ToString::to_string).collect())
    }

    fn shown(
        &self,
        changeset: &Changeset,
    ) -> (Vec<suzatary_core::wire::inbox::ProposedObject>, String) {
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
                by: suzatary_core::secretary::WRITES_AS,
                categories: &known,
                untrusted,
                typeset: self.typeset,
            },
        )
    }
}

/// `POST /api/secretary/conversations`: open a topic.
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
    let id = begun(caps, cards, &title, None)?;
    newest(caps, cards, &id)
}

/// `POST /api/secretary/setup/<template>`: a conversation begun on `template`'s opening.
///
/// # Errors
/// The conversation store's.
pub fn setup(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    template: suzatary_core::connect::TemplateId,
) -> Answered<wire::ConversationPayload> {
    let template = template.template();
    let title: String = Title::new(template.called())
        .map_err(|refused| Refusal::internal(refused.to_string()))?
        .into();
    let at = caps.secretary.clock.now();
    let id = begun(
        caps,
        cards,
        &title,
        Some(&Entry::noted(
            at,
            template.opening(google_client_kept(caps.secretary.secrets, cards.settables)),
            Vec::new(),
        )),
    )?;
    newest(caps, cards, &id)
}

/// File the box's Secret card for a new value of `handle`'s secret, unless one already asks.
///
/// # Errors
/// [`Refusal::not_found`] for a handle no secret a Secret card asks for has; the stores' own.
pub fn replace(caps: &Server<'_>, cards: &Cards<'_>, handle: &str) -> Result<(), Refusal> {
    let name = cards
        .settables
        .by_handle(handle)
        .filter(|name| {
            cards.settables.declaration(name).is_some_and(|by| {
                by.iter()
                    .any(suzatary_core::settings::Declaration::by_a_secret_card)
            })
        })
        .ok_or_else(|| {
            Refusal::not_found(format!(
                "no secret this deployment stores is named {}",
                suzatary_base::refusal::shown(handle)
            ))
        })?;
    let at = caps.secretary.clock.now();
    suzatary_core::secretary::fixes::replace(&name, at)
        .holding()
        .file(caps.secretary.conversations, caps.secretary.inbox, at)?;
    Ok(())
}

fn begun(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    title: &str,
    first: Option<&Entry>,
) -> Result<ConversationId, Refusal> {
    let taken = transcript::keys(caps.secretary.conversations)
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let at = caps.secretary.clock.now();
    let today = cards
        .deployment
        .zone
        .day(prodrome_core::fpl::instant_of(at));
    let id = ConversationId::minted(today, &taken);
    conversations::start(
        caps.secretary.conversations,
        caps.secretary.model,
        &id,
        title,
        Kind::Topic,
        at,
    )?;
    if let Some(first) = first {
        conversations::append(caps.secretary.conversations, &id, first)?;
    }
    Ok(id)
}

/// `POST …/<id>/title`: rename one.
///
/// # Errors
/// [`suzatary_core::secretary::title::rename`]'s.
pub fn rename(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    by: &prodrome_core::event::Actor,
    id: &ConversationId,
    title: &str,
) -> Answered<wire::ConversationPayload> {
    let stored = suzatary_core::secretary::title::rename(
        caps.secretary.conversations,
        caps.secretary.clock,
        by,
        id,
        title,
    )?;
    payload(&stored, caps, cards)
}

/// `POST …/erase`'s body: `{"turns": [<index>, …]}` or `{"whole": true}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Erasing {
    /// These turns' words, by index.
    Turns(#[ts(as = "Vec<u32>")] suzatary_core::nonempty::NonEmpty<usize>),
    /// Every line's words, the name included.
    Whole(crate::wire::Yes),
}

/// `POST …/<id>/erase`: erase turns, or all of it, behind the same [`Tap`] as a send.
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
        Erasing::Whole(crate::wire::Yes) => (0..read.lines().len()).collect(),
        Erasing::Turns(turns) => turns
            .as_slice()
            .iter()
            .map(|&turn| {
                read.line_of(turn)
                    .ok_or_else(|| Refusal::not_found(format!("{id} has no turn {turn}")))
            })
            .collect::<Result<_, _>>()?,
    };
    let erased = suzatary_core::secretary::transcript::erasures(
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
            suzatary_core::secretary::erasure::Erasure::Asked {
                by: tap.who().clone(),
            },
            caps.secretary.clock.now(),
        )?;
    }
    // The summaries that read them are copies, deleted by the memory's sweep.
    caps.secretary.memories.sweep_after_write();
    newest(caps, cards, id)
}

/// One conversation's newest window, dressed: what a write answers.
///
/// # Errors
/// A name nothing is stored under, or a file this program cannot fold.
pub fn newest(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
) -> Answered<wire::ConversationPayload> {
    payload(
        &conversations::read(caps.secretary.conversations, id)?,
        caps,
        cards,
    )
}

/// `GET …/<id>/drawn?indices=<n>,…`: proposals drawn against the todo chains.
///
/// # Errors
/// [`Refusal::not_found`] for a conversation or a proposal there is none of, and the stores' own.
pub fn drawn(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
    indices: &[usize],
) -> Answered<crate::wire::DrawnPayload> {
    let stored = conversations::read(caps.secretary.conversations, id)?;
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
    Ok(crate::wire::DrawnPayload {
        title: stored.heading().into_owned(),
        proposals,
    })
}

pub(crate) const MAX_ATTACHMENTS_PER_TURN: usize = 4;

/// `POST …/<id>/say`: append the person's turn, stamped `by`; the model runs in the worker.
///
/// A question asked again once its line stands at or past `after` is that line.
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
    attachments: &[suzatary_core::attachment::Attachment],
    by: suzatary_core::secretary::stamp::Stamp,
    after: Option<usize>,
) -> Answered<crate::wire::SayPayload> {
    let turn = question(said, references, attachments)?;
    let before = conversations::read(caps.secretary.conversations, id)?;
    if after.is_some_and(|after| asked(&before, after, &turn, &by)) {
        return Ok(crate::wire::SayPayload {
            conversation: payload(&before, caps, cards)?,
        });
    }
    // Checked before appending: a turn the box will never get to must not join the transcript.
    let pending = suzatary_core::secretary::queue::pending(&before);
    if pending >= suzatary_core::secretary::queue::MAX_PENDING {
        return Err(Refusal::bad_request(format!(
            "{pending} lines are already waiting for the secretary; say it after they are answered"
        )));
    }
    transcript::push(
        caps.secretary.conversations,
        id,
        &Entry::Said {
            at: caps.secretary.clock.now(),
            turn,
            by: Some(by),
        },
    )?;
    let stored = conversations::read(caps.secretary.conversations, id)?;
    Ok(crate::wire::SayPayload {
        conversation: payload(&stored, caps, cards)?,
    })
}

fn asked(
    stored: &Stored,
    after: usize,
    turn: &Turn,
    by: &suzatary_core::secretary::stamp::Stamp,
) -> bool {
    stored.turns.iter().skip(after).any(|said| {
        &said.turn == turn
            && said
                .by
                .as_ref()
                .and_then(suzatary_core::secretary::stamp::Stamp::actor)
                == by.actor()
    })
}

fn question(
    said: &str,
    references: &[Reference],
    attachments: &[suzatary_core::attachment::Attachment],
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

/// `POST …/<id>/regenerate`: ask the last round again.
///
/// # Errors
/// [`suzatary_core::secretary::regenerate`]'s.
pub fn regenerate(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    id: &ConversationId,
    by: &prodrome_core::event::Actor,
) -> Answered<wire::ConversationPayload> {
    let stored = suzatary_core::secretary::regenerate(
        caps.secretary.conversations,
        caps.secretary.clock,
        by,
        id,
    )?;
    payload(&stored, caps, cards)
}

/// `POST …/<id>/edit`: replace the last round's question, under `say`'s rules.
///
/// # Errors
/// `question`'s and [`suzatary_core::secretary::edit`]'s.
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
    attachments: &[suzatary_core::attachment::Attachment],
    by: &prodrome_core::event::Actor,
    stamped: suzatary_core::secretary::stamp::Stamp,
) -> Answered<wire::ConversationPayload> {
    let turn = question(said, references, attachments)?;
    let stored = suzatary_core::secretary::edit(
        caps.secretary.conversations,
        caps.secretary.clock,
        by,
        stamped,
        id,
        turn,
    )?;
    payload(&stored, caps, cards)
}

/// What a tap can act through.
#[derive(Clone, Copy)]
pub struct Effectors<'a> {
    /// Sends a tapped email.
    pub post: &'a dyn Post,
    /// Reads a tapped email's held values and a forward's message.
    pub reveal: &'a dyn Reveal,
    /// Performs a tapped act (a sign-in, a web request).
    pub performer: &'a dyn Perform,
    /// Stores a tapped secret.
    pub secrets: &'a dyn Writes,
    /// Calls a model once with a credential before it is stored.
    pub verify: &'a dyn suzatary_core::settings::Verify,
    /// Syncs a plugin once its link is stored.
    pub checks: &'a dyn suzatary_core::settings::Checks,
    /// The sign-ins the box holds open.
    pub signing: &'a dyn suzatary_core::secretary::sign_in::Sessions,
    /// Edits the instance file.
    pub edits: &'a dyn Edits,
    /// Deletes what the box kept for a removed account.
    pub forget: &'a dyn suzatary_core::mail::cache::Forget,
    /// Registers a device's key on every chain; `None` without a root.
    pub enrolling: Option<suzatary_core::signatures::Enrolling<'a>>,
    /// Opens a sign-in with Google.
    pub consents: &'a dyn suzatary_core::secretary::google_sign_in::Consents,
}

/// What a tap's body carries beside its note: nothing, or exactly one of these.
#[derive(Debug)]
pub enum TapAnswer {
    /// Nothing but the tap: the card performed as it stands.
    Plain,
    /// A secret's value, or the code a sign-in's page shows.
    Value(Secret),
    /// The card's wider yes.
    Always,
    /// The tapping device's signature of a device card's registration.
    Endorsed(suzatary_core::signatures::Endorsement),
}

const VALUE_BELONGS: &str =
    "a value belongs only to a secret act, a sign-in's code or the Google client's file";
const ALWAYS_BELONGS: &str = "always belongs only to a web request that offers a rule, a \
                              document, or a mail account's removal";
const ENDORSEMENT_BELONGS: &str = "an endorsement belongs only to a device card";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Once,
    Always,
}

impl TapAnswer {
    fn plain(&self) -> Result<(), Refusal> {
        match self {
            TapAnswer::Plain => Ok(()),
            TapAnswer::Value(_) => Err(Refusal::bad_request(VALUE_BELONGS)),
            TapAnswer::Always => Err(Refusal::bad_request(ALWAYS_BELONGS)),
            TapAnswer::Endorsed(_) => Err(Refusal::bad_request(ENDORSEMENT_BELONGS)),
        }
    }

    fn choice(&self) -> Result<Choice, Refusal> {
        match self {
            TapAnswer::Plain => Ok(Choice::Once),
            TapAnswer::Always => Ok(Choice::Always),
            TapAnswer::Value(_) => Err(Refusal::bad_request(VALUE_BELONGS)),
            TapAnswer::Endorsed(_) => Err(Refusal::bad_request(ENDORSEMENT_BELONGS)),
        }
    }

    fn value(&self, missing: &str) -> Result<&Secret, Refusal> {
        match self {
            TapAnswer::Value(value) => Ok(value),
            TapAnswer::Plain => Err(Refusal::bad_request(format!("value is required{missing}"))),
            TapAnswer::Always => Err(Refusal::bad_request(ALWAYS_BELONGS)),
            TapAnswer::Endorsed(_) => Err(Refusal::bad_request(ENDORSEMENT_BELONGS)),
        }
    }

    fn endorsement(&self) -> Result<&suzatary_core::signatures::Endorsement, Refusal> {
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

/// How far a tap that performs an act allows.
#[derive(Debug, Clone, Copy)]
enum Allowing<'p> {
    Once,
    Rule(&'p suzatary_core::web::AllowRule),
    /// A document, whose folder is allowed.
    Document(&'p suzatary_base::drive_path::DrivePath),
}

/// What an account's removal does with the mail the box kept for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeptMail {
    Kept,
    Deleted,
}

/// A tapped card paired with its answer: a misplaced or missing answer is refused here alone.
enum Prepared<'p, 'c> {
    Email(&'p Draft),
    Forward(&'p suzatary_core::secretary::proposal::Forward),
    Changes(std::borrow::Cow<'p, Changeset>),
    Secret {
        name: Settable,
        value: &'p Secret,
    },
    Code {
        url: &'p suzatary_core::secretary::sign_in::SignInUrl,
        code: &'p Secret,
        name: Settable,
    },
    Connect(&'p suzatary_core::connect::Connection),
    Disconnect {
        connection: &'p suzatary_core::connect::Connection,
        mail: KeptMail,
    },
    Grant {
        plugin: &'p PluginName,
        origin: &'p Origin,
    },
    UnlockStore,
    Erase(&'p std::collections::BTreeSet<suzatary_core::secretary::locked_text::KeyName>),
    Afresh {
        conversation: &'p ConversationId,
        turn: usize,
    },
    Device {
        enrolling: suzatary_core::signatures::Enrolling<'c>,
        enrolment: &'p suzatary_core::signatures::Enrolment,
        endorsed: prodrome_core::sign::Signed,
    },
    Begin {
        act: &'p Act,
        allowing: Allowing<'p>,
    },
    /// Remember a card's words under the memory's hold, taken before the inbox's.
    Remember {
        held: suzatary_core::secretary::memories::Held<'c>,
        text: &'p suzatary_core::secretary::salted_text::TextName,
        kind: suzatary_core::secretary::memory::Kind,
        replaces: &'p suzatary_core::secretary::memory::Replaces,
    },
    /// Retract a summary under the memory's hold, whose sweep deletes its words.
    Retract {
        held: suzatary_core::secretary::memories::Held<'c>,
        node: &'p prodrome_core::event::Hash,
    },
    Decide(&'p suzatary_core::decisions::Draft),
    GoogleClient {
        redirect: &'p suzatary_core::google::RedirectUri,
        file: &'p Secret,
    },
    GoogleSignIn {
        account: &'p suzatary_core::mail::address::EmailAddress,
        scopes: &'p suzatary_core::google::Scopes,
    },
    CalendarEvent {
        act: &'p Act,
        account: &'p suzatary_core::mail::address::EmailAddress,
    },
}

struct Preparing<'r, 'c> {
    settables: &'r Settables,
    enrolling: Option<suzatary_core::signatures::Enrolling<'c>>,
    tap: &'r Tap,
    hold: &'r dyn Fn() -> Result<suzatary_core::secretary::memories::Held<'c>, Refusal>,
    /// The changes of the triage card a [`Act::Restore`] names.
    withdrawn: &'r dyn Fn(&ProposalRef) -> Result<Changeset, Refusal>,
}

impl<'p, 'c> Prepared<'p, 'c> {
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
                Prepared::Changes(std::borrow::Cow::Borrowed(changeset))
            }
            Proposal::Act(act) => Self::of_act(act, answer, reach)?,
            // His tap is who said it, since the round that asked had read something not his own.
            Proposal::Remember {
                text,
                kind,
                replaces,
            } => {
                answer.plain()?;
                Prepared::Remember {
                    held: (reach.hold)()?,
                    text,
                    kind: *kind,
                    replaces,
                }
            }
            Proposal::Decide(draft) => {
                answer.plain()?;
                Prepared::Decide(draft)
            }
        })
    }

    #[expect(clippy::too_many_lines, reason = "one arm per kind of act")]
    fn of_act(
        act: &'p Act,
        answer: &'p TapAnswer,
        reach: &Preparing<'_, 'c>,
    ) -> Result<Self, Refusal> {
        use suzatary_core::connect::Table;
        Ok(match act {
            Act::Restore { card } => {
                answer.plain()?;
                Prepared::Changes(std::borrow::Cow::Owned((reach.withdrawn)(card)?))
            }
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
            Act::Retract { node } => {
                answer.plain()?;
                Prepared::Retract {
                    held: (reach.hold)()?,
                    node,
                }
            }
            Act::Afresh { conversation, turn } => {
                answer.plain()?;
                Prepared::Afresh {
                    conversation,
                    turn: *turn,
                }
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
            | Act::SignIn { .. }
            | Act::Noise { .. } => {
                answer.plain()?;
                Prepared::Begin {
                    act,
                    allowing: Allowing::Once,
                }
            }
        })
    }
}

/// `name` minted against the names declared now; a name no longer declared stores nothing.
fn minted(settables: &Settables, name: &str) -> Result<Settable, Refusal> {
    Settable::mint(settables, name).map_err(|_| {
        Refusal::conflict(format!(
            "nothing was stored: {} is no longer a secret this deployment declares",
            suzatary_core::settings::Help::label(name)
        ))
    })
}

/// `POST …/proposals/<n>/tap`: submits one proposal, at most once under the inbox's hold.
///
/// A failure that may have happened completes the proposal; one that did not is a note.
///
/// # Errors
/// Not found, a wrong or missing answer, a secret no longer declared, a rejected rule, the stores'.
#[allow(clippy::too_many_arguments)]
pub fn tap(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    tap: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
    note: &str,
    answer: &TapAnswer,
) -> Answered<AfterTap> {
    let (id, index) = (&card.conversation, card.index);
    let stored = conversations::read(caps.secretary.conversations, id)?;
    let standing = stored.standing(index, &inbox::fold(caps.secretary.inbox)?)?;
    suzatary_core::secretary::sending::performs(tap, &standing.object.asked.proposal)?;
    let hold = || caps.secretary.memories.hold();
    let withdrawn =
        |card: &ProposalRef| suzatary_core::secretary::undo::withdrawn(caps.secretary.inbox, card);
    let prepared = Prepared::of(
        &standing.object.asked.proposal,
        answer,
        &Preparing {
            settables: cards.settables,
            enrolling: effectors.enrolling,
            tap,
            hold: &hold,
            withdrawn: &withdrawn,
        },
    )?;
    let mut then = None;
    let response = suzatary_core::secretary::tapped::Tapped {
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
    let opened = afterwards(caps, effectors, then, card)?;
    summaries_after(caps, &standing.object.asked.proposal);
    if matches!(response, Response::Ran(_)) {
        suzatary_core::secretary::act_lines::note_act(
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
            suzatary_core::secretary::stamp::Stamp::Actor(
                suzatary_core::secretary::stamp::Speaker::at(tap.who().as_str(), tap.seat()),
            ),
        )?;
    }
    Ok(AfterTap {
        conversation: payload(
            &conversations::read(caps.secretary.conversations, id)?,
            caps,
            cards,
        )?,
        opened,
    })
}

/// What a tap answers.
#[derive(Debug)]
pub struct AfterTap {
    /// The card's conversation, as it stands after the tap.
    pub conversation: wire::ConversationPayload,
    /// The conversation the tap opened.
    pub opened: Option<ConversationId>,
}

struct Performing<'r, 'a> {
    caps: &'r Server<'a>,
    effectors: &'r Effectors<'a>,
    cards: &'r Cards<'a>,
    tap: &'r Tap,
}

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
        Prepared::Changes(changeset) => apply(cards, &changeset),
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
        Prepared::Afresh { conversation, turn } => {
            // A device may write an inbox object too, so a card copies only a request said in
            // the conversation it is drawn in.
            if conversation != id {
                return Err(Refusal::conflict(format!(
                    "this card asks again a request of {conversation}, and it is drawn in {id}"
                )));
            }
            let (opened, performed) = afresh(caps, cards, conversation, turn)?;
            *then = Some(Then::Opened(opened));
            performed
        }
        Prepared::Device {
            enrolling,
            enrolment,
            endorsed,
        } => trusted(enrolling, enrolment, &endorsed),
        Prepared::Begin { act, allowing } => {
            // Written before the act: allowing a site is a decision about the site.
            allowed(effectors.edits, allowing)?;
            // The tap is the consent, minted by the gate, never from a request field.
            effectors.performer.begin(
                id,
                index,
                &object.id,
                &object.asked.proposer,
                act,
                Consent::given_by_tap(tap),
            )
        }
        Prepared::Remember {
            held,
            text,
            kind,
            replaces,
        } => remembered(
            &held,
            caps.secretary.clock,
            id,
            index,
            (text, kind, replaces),
        ),
        Prepared::Retract { held, node } => {
            match held.retract(node, caps.secretary.clock.now(), tap) {
                Ok(_) => Performed::Done(Done::Acted {
                    what: "retracted the summary; it is written again from your lines".to_owned(),
                }),
                Err(refusal) => Performed::Tried(Tried::NotSent {
                    reason: refusal.message().to_owned(),
                }),
            }
        }
        Prepared::Decide(draft) => decide::performed(caps.secretary.decides(), draft, tap.who()),
        Prepared::GoogleClient { redirect, file } => {
            google_client(effectors.secrets, cards.settables, redirect, file)?
        }
        Prepared::GoogleSignIn { account, scopes } => effectors.consents.begin(
            &object.id,
            account,
            scopes,
            tap.who(),
            caps.secretary.clock.now(),
            suzatary_core::secretary::google_sign_in::Then::Ended,
        ),
        Prepared::CalendarEvent { act, account } => calendar_event(
            (
                effectors.edits,
                (caps.secretary.secrets, cards.settables),
                effectors.performer,
            ),
            (id, index, &object.id, &object.asked.proposer),
            (act, account),
            Consent::given_by_tap(tap),
        ),
    })
}

/// A calendar event's try, or the lack that stops it.
fn calendar_event(
    (edits, secrets, performer): (&dyn Edits, (&dyn Secrets, &Settables), &dyn Perform),
    (id, index, object, proposer): (
        &ConversationId,
        usize,
        &ProposalId,
        &suzatary_core::secretary::proposer::Proposer,
    ),
    (act, account): (&Act, &suzatary_core::mail::address::EmailAddress),
    consent: Consent,
) -> Performed {
    match google_account(edits, secrets, account) {
        Ok(()) => performer.begin(id, index, object, proposer, act, consent),
        Err(tried) => Performed::Tried(tried),
    }
}

/// `POST …/proposals/<n>/tap` with `fix`: performs, as this tap, the card that fixes its lack.
///
/// # Errors
/// [`Refusal::conflict`] for nothing lacked or a fix that cannot begin; the stores'.
pub fn fix(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    tap: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
) -> Answered<AfterTap> {
    let (id, index) = (&card.conversation, card.index);
    let stored = conversations::read(caps.secretary.conversations, id)?;
    let lacked = stored
        .standing(index, &inbox::fold(caps.secretary.inbox)?)?
        .object;
    suzatary_core::secretary::sending::performs(tap, &lacked.asked.proposal)?;
    let lacking = lacked
        .reading
        .shown()
        .filter(|_| lacked.reading.performs())
        .and_then(|answer| answer.response.lacking())
        .ok_or_else(|| Refusal::conflict("this card lacks nothing a tap connects now"))?;
    let held = match lacking {
        suzatary_core::mail::lacking::Lacking::Grant { account, .. } => {
            caps.secretary.secrets.google_grant(account)
        }
        suzatary_core::mail::lacking::Lacking::GoogleClient => {
            suzatary_core::google::Held::NotSignedIn
        }
    };
    let fix = suzatary_core::secretary::fix::of(lacking, &held, effectors.consents.redirect())
        .map_err(|unfixable| Refusal::conflict(unfixable.to_string()))?;
    let now = caps.secretary.clock.now();
    let filed = suzatary_core::secretary::noticed::Owed::one(Proposal::Act(fix.act()), FIXING)
        .about(&[card.clone().into()])
        .since(now)
        .holding()
        .file(caps.secretary.conversations, caps.secretary.inbox, now)?;
    let fixing = filed
        .owed
        .first()
        .ok_or_else(|| Refusal::internal("the fix's card was not filed"))?;
    let fold = inbox::fold(caps.secretary.inbox)?;
    let at = fold.one(fixing)?;
    let begin = |account, scopes| {
        let then = retried(&lacked, tap);
        effectors
            .consents
            .begin(fixing, account, scopes, tap.who(), now, then)
    };
    let refused = match &fix {
        Fix::GoogleClient { .. } => None,
        // An open consent is opened again for this card.
        Fix::GoogleSignIn { account, scopes } if at.reading.running() => {
            match begin(account, scopes) {
                Performed::Tried(tried) => tried.said(),
                Performed::Done(_) => None,
            }
        }
        Fix::GoogleSignIn { .. } | Fix::MicrosoftSend { .. } => {
            let answered = suzatary_core::secretary::tapped::Tapped { now }.fire(
                caps.secretary.inbox,
                fixing,
                tap,
                |_| {
                    Ok(match &fix {
                        Fix::GoogleSignIn { account, scopes } => begin(account, scopes),
                        Fix::MicrosoftSend { .. } | Fix::GoogleClient { .. } => {
                            effectors.performer.begin(
                                &at.asked.asked.conversation,
                                at.asked.asked.index,
                                fixing,
                                &at.asked.proposer,
                                &fix.act(),
                                Consent::given_by_tap(tap),
                            )
                        }
                    })
                },
            )?;
            match answered {
                Response::Ran(suzatary_core::secretary::responded::Responded::Tried {
                    tried,
                    ..
                }) => tried.said(),
                Response::Ran(_) | Response::Recorded(_) => None,
            }
        }
    };
    if let Some(why) = refused {
        return Err(Refusal::conflict(why));
    }
    Ok(AfterTap {
        conversation: payload(
            &conversations::read(caps.secretary.conversations, id)?,
            caps,
            cards,
        )?,
        opened: None,
    })
}

const FIXING: &str = "You tapped Connect on a card that needs it.";

/// Only a calendar event is performed again: a send waits for its own Send tap.
fn retried(lacked: &inbox::Object, tap: &Tap) -> suzatary_core::secretary::google_sign_in::Then {
    use suzatary_core::secretary::google_sign_in::Then;
    match &lacked.asked.proposal {
        Proposal::Act(Act::CalendarEvent { .. }) => Then::Retry {
            card: lacked.id.clone(),
            consent: Consent::given_by_tap(tap),
        },
        Proposal::Act(_)
        | Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Changes(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => Then::Ended,
    }
}

/// After its fix's sign-in: performs `card` again when `signed` kept the grant, else says why not.
///
/// # Errors
/// The stores' own failures.
pub fn retry(
    caps: &suzatary_core::secretary::Secretary<'_>,
    (edits, settables, performer): (&dyn Edits, &Settables, &dyn Perform),
    (card, by, consent): (&ProposalId, &prodrome_core::event::Actor, Consent),
    signed: &Performed,
) -> Result<Option<ConversationId>, Refusal> {
    let fold = inbox::fold(caps.inbox)?;
    let Some(asked) = fold
        .objects()
        .find(|object| object.id == *card)
        .map(|object| object.asked.clone())
    else {
        return Ok(None);
    };
    let now = caps.clock.now();
    let at = &asked.asked;
    match signed {
        Performed::Done(_) => {
            inbox::submit(caps.inbox, card, by, now, |asked| {
                let Proposal::Act(act @ Act::CalendarEvent { account, .. }) = &asked.proposal
                else {
                    return Err(Refusal::conflict(
                        "only a calendar event is performed again",
                    ));
                };
                Ok(calendar_event(
                    (edits, (caps.secrets, settables), performer),
                    (&at.conversation, at.index, card, &asked.proposer),
                    (act, account),
                    consent,
                ))
            })?;
        }
        Performed::Tried(tried) => {
            let Some(why) = tried.said() else {
                return Ok(Some(at.conversation.clone()));
            };
            conversations::append(
                caps.conversations,
                &at.conversation,
                &Entry::noted(
                    now,
                    format!("Not connected, so nothing was done: {why}."),
                    vec![at.clone().into()],
                ),
            )?;
        }
    }
    Ok(Some(at.conversation.clone()))
}

/// After an erase card's tap, the memory's sweep, under its own hold taken after the inbox's.
fn summaries_after(caps: &Server<'_>, proposal: &Proposal) {
    if matches!(proposal, Proposal::Act(Act::Erase { .. })) {
        caps.secretary.memories.sweep_after_write();
    }
}

/// What a tap leaves for after its card's response is recorded.
enum Then {
    /// The sign-in card a code card finished.
    End(ProposalId, Performed),
    Grant(PluginName, Origin),
    Connect(suzatary_core::connect::Connection),
    Retire(suzatary_core::connect::Connection),
    Check(PluginName),
    /// What the box kept for a removed account, deleted outside the inbox's hold.
    Forget(
        suzatary_core::mail::account::MailAccount,
        suzatary_core::mail::removal::Removal,
    ),
    Opened(ConversationId),
}

fn afterwards(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    then: Option<Then>,
    card: &ProposalRef,
) -> Result<Option<ConversationId>, Refusal> {
    let done = match then {
        None => Ok(()),
        Some(Then::Opened(opened)) => return Ok(Some(opened)),
        Some(Then::End(began, ended)) => inbox::end(
            caps.secretary.inbox,
            &began,
            &transcript::writer(),
            caps.secretary.clock.now(),
            ended,
        )
        .map(|_| ()),
        Some(Then::Grant(plugin, origin)) => suzatary_core::secretary::connections::grant(
            caps.secretary.files_into(),
            &plugin,
            &origin,
        ),
        Some(Then::Connect(connection)) => suzatary_core::secretary::connections::connect(
            caps.secretary.files_into(),
            connection,
            card,
        ),
        Some(Then::Retire(link)) => {
            suzatary_core::secretary::connections::retire(caps.secretary.files_into(), link, card)
        }
        Some(Then::Check(plugin)) => {
            effectors
                .checks
                .check(&suzatary_core::health::SourceKey::Plugin(plugin));
            Ok(())
        }
        Some(Then::Forget(account, removal)) => {
            let said = effectors.forget.forget(&account, removal).sentence();
            conversations::append(
                caps.secretary.conversations,
                &card.conversation,
                &Entry::noted(
                    caps.secretary.clock.now(),
                    format!("Removing {}: {said}.", account.email),
                    vec![card.clone().into()],
                ),
            )
        }
    };
    done.map(|()| None)
}

/// What a secret's tap does with its value.
enum Link {
    Stored,
    /// A link its plugin may fetch, checked once it is stored.
    Fetched(PluginName),
    /// Not an http(s) URL, or one in the clear; the words never quote the value.
    Refused(String),
    Outside {
        plugin: PluginName,
        origin: Origin,
    },
    /// A Google account's own calendar: the account's connection is asked for instead.
    Connect(suzatary_core::connect::Connection),
    /// A calendar of an account signed in with Google: the link's removal is asked for instead.
    Retire(suzatary_core::connect::Connection),
}

/// Where `value` stands as `name`'s link.
fn linked((edits, settables): (&dyn Edits, &Settables), name: &Settable, value: &Secret) -> Link {
    let a_plugin_s = settables.declaration(name).is_some_and(|by| {
        by.iter()
            .any(|by| matches!(by, suzatary_core::settings::Declaration::Plugin(_)))
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
    match suzatary_core::plugin::PluginUrl::parse(value.expose().trim()) {
        None => Link::Refused(format!(
            "nothing was stored: that is not an http(s) link the {} plugin can fetch",
            manifest.name()
        )),
        Some(url) if let Some(calendar) = suzatary_core::google::ical::linked(&url) => {
            google_calendar(&declared, name.name(), calendar)
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

/// A Google calendar's link, never stored: the calendar is read through the sign-in with Google.
fn google_calendar(
    declared: &suzatary_core::connect::Declared,
    name: &str,
    calendar: suzatary_core::google::ical::Linked,
) -> Link {
    use suzatary_core::google::ical::Linked;
    const READ: &str = "nothing was stored: the box reads a Google calendar through its Google \
                        account, never by its link";
    let Linked::Account(account) = calendar else {
        return Link::Refused(format!("{READ}; connect the Google account that holds it"));
    };
    match suzatary_core::connect::Connection::parse("google", &[account.as_str().to_owned()]) {
        Ok(connection) => match declared.connections().get(&connection.key()) {
            Some(held) if !connection.moves(held) => declared
                .connections()
                .into_values()
                .find(|link| {
                    matches!(
                        link.template().table,
                        suzatary_core::connect::Table::Link(_)
                    ) && link.secrets().first().map(String::as_str) == Some(name)
                })
                .map_or_else(
                    || Link::Refused(format!("{READ}, and {account} is connected already")),
                    Link::Retire,
                ),
            Some(_) | None => Link::Connect(connection),
        },
        Err(failed) => Link::Refused(format!("{READ}; {account} cannot be connected: {failed}")),
    }
}

fn secret(
    writer: &dyn Writes,
    verify: &dyn suzatary_core::settings::Verify,
    reading: (&dyn Edits, &Settables),
    name: &Settable,
    value: &Secret,
    then: &mut Option<Then>,
) -> Performed {
    if reading.1.declaration(name).is_some_and(|by| {
        by.iter()
            .any(suzatary_core::settings::Declaration::own_card)
    }) {
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
        Link::Retire(link) => {
            let reason = format!(
                "nothing was stored: that calendar is a Google account's the box reads through \
                 its sign-in with Google, so the card beside this one removes {}",
                link.called()
            );
            *then = Some(Then::Retire(link));
            Performed::Tried(Tried::NotSent { reason })
        }
    }
}

/// Write a connection's table; a refusal wrote nothing, so tapping again is safe.
fn connected(edits: &dyn Edits, connection: &suzatary_core::connect::Connection) -> Performed {
    // A lieer account moves onto its sign-in with Google in the Google callback, never this tap.
    let moves = edits.declared().is_ok_and(|declared| {
        declared
            .connections()
            .get(&connection.key())
            .is_some_and(|held| connection.moves(held))
    });
    if moves {
        return Performed::Done(Done::Acted {
            what: format!(
                "nothing changed yet: lieer reads {}'s mail as before until its sign-in with \
                 Google is kept and has read its mail, which the box asks for next",
                connection.key().name
            ),
        });
    }
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

/// Take a connection's table out and clear its secrets no connection reads.
fn disconnected(
    removing: &Effectors<'_>,
    cards: &Cards<'_>,
    connection: &suzatary_core::connect::Connection,
    mail: KeptMail,
    then: &mut Option<Then>,
) -> Performed {
    let key = connection.key();
    let Ok(before) = removing.edits.declared() else {
        return Performed::Tried(Tried::NotSent {
            reason: "nothing was removed: the instance file does not read".to_owned(),
        });
    };
    let account = before
        .accounts
        .iter()
        .find(|account| account.email.as_str() == key.name)
        .cloned();
    let declared = cards
        .settables
        .model()
        .map(|model| {
            Settables::derive(
                model.name(),
                before.channels.iter().copied(),
                cards.deployment.coding.as_ref(),
                &before.plugins,
            )
        })
        .unwrap_or_default()
        .join(Settables::google(&before.accounts));
    let owned: Vec<Settable> = connection
        .kept()
        .iter()
        .filter_map(|name| Settable::mint(&declared, name).ok())
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
                .flat_map(suzatary_core::connect::Connection::kept)
                .collect();
            owned
                .iter()
                .filter(|name| !read.iter().any(|held| held == name.name()))
                .map(|name| {
                    let label = suzatary_core::settings::Help::label(name.name());
                    match removing.secrets.clear_secret(name) {
                        Ok(()) => format!("; forgot {label}"),
                        Err(failed) => format!("; {label} is still stored: {failed}"),
                    }
                })
                .collect::<String>()
        }
        // A file that does not read cannot say which another connection reads, so all are kept.
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
            match suzatary_core::mail::removal::Removal::of(account.microsoft().is_some(), deleted)
            {
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

fn granted(edits: &dyn Edits, plugin: &PluginName, origin: &Origin) -> Performed {
    match edits.add_origin(plugin, origin) {
        Ok(()) => Performed::Done(Done::Acted {
            what: format!("the {plugin} plugin may reach {origin}"),
        }),
        Err(failed) => Performed::Tried(Tried::NotSent {
            reason: format!("nothing was written: {failed}"),
        }),
    }
}

fn allowed(edits: &dyn Edits, allowing: Allowing<'_>) -> Result<(), Refusal> {
    match allowing {
        Allowing::Once => Ok(()),
        Allowing::Rule(rule) => edits
            .allow_web(rule)
            .map_err(|failed| Refusal::internal(format!("could not allow {rule}: {failed}"))),
        Allowing::Document(path) => {
            let folder = suzatary_core::secretary::acts::folder_of(path);
            edits
                .allow_drive(&folder)
                .map_err(|failed| Refusal::internal(format!("could not allow {folder}: {failed}")))
        }
    }
}

/// How many device cards may wait at once.
pub const WAITING_DEVICES: usize = 3;

/// `POST /api/act/key`: a device asks to be trusted as the gate's `actor`; files one card.
///
/// # Errors
/// [`WAITING_DEVICES`] cards already waiting for other keys, and the stores' own failures.
pub fn ask_trust(
    caps: &Server<'_>,
    actor: &prodrome_core::event::Actor,
    key: &prodrome_core::sign::PublicKey,
) -> Answered<usize> {
    let secretary = caps.secretary;
    let stored = conversations::read_or_open_default(
        caps.secretary.conversations,
        caps.secretary.model,
        &suzatary_base::conversation_id::DEFAULT,
        caps.secretary.clock.now(),
    )?;
    // Bounded, since a key nobody trusts can ask: a stray key cannot flood the Inbox.
    // An unplaced line holds no act this box can read, so it is no device this box could trust.
    let lines = stored.lines(&inbox::fold(caps.secretary.inbox)?);
    let waiting: Vec<&suzatary_core::signatures::Enrolment> = lines
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
                | Act::Retract { .. }
                | Act::Afresh { .. }
                | Act::GoogleClient { .. }
                | Act::GoogleSignIn { .. }
                | Act::CalendarEvent { .. }
                | Act::Noise { .. }
                | Act::Restore { .. },
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
    let reasons = suzatary_core::secretary::proposal::Reasons::new(
        format!("a device asked to be trusted to write as {actor}"),
        vec![asked],
    )
    .map_err(|failed| Refusal::internal(failed.to_string()))?;
    suzatary_core::secretary::verbs::propose(
        &secretary,
        &[],
        &stored.id,
        Proposal::Act(Act::Device(suzatary_core::signatures::Enrolment {
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

/// Close, as done another way, every waiting device card whose key `history` already trusts.
///
/// # Errors
/// The stores' own.
pub fn withdraw_trusted(
    inbox: &dyn suzatary_core::store::Prodrome<suzatary_core::secretary::inbox_event::InboxEvent>,
    history: &dyn suzatary_core::store::Prodrome<suzatary_record::record::TodoEvent>,
    root: &prodrome_core::sign::PublicKey,
    at: suzatary_base::capability::Datetime,
) -> Result<Vec<ProposalId>, Refusal> {
    let swept = suzatary_core::secretary::done_elsewhere::sweep(inbox, at, |proposal| {
        use suzatary_core::secretary::done_elsewhere::Seen;
        let Proposal::Act(Act::Device(enrolment)) = proposal else {
            return Ok(Seen::Waits);
        };
        let trusted = suzatary_core::signatures::trusted_for(history, root, &enrolment.key)?;
        Ok(if trusted.contains(&enrolment.actor) {
            Seen::Closed(
                suzatary_core::secretary::responded::Withdrawal::DoneElsewhere {
                    what: ALREADY_TRUSTED.to_owned(),
                },
            )
        } else {
            Seen::Waits
        })
    })?;
    Ok(swept.closed)
}

/// A device card's endorsement checked first: only a device the chain already trusts for the
/// same person vouches (SPEC law 42), so a new phone cannot tap its own card.
fn vouched(
    enrolling: suzatary_core::signatures::Enrolling<'_>,
    tap: &Tap,
    enrolment: &suzatary_core::signatures::Enrolment,
    endorsement: &suzatary_core::signatures::Endorsement,
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

/// Register a vouched device's key on every chain.
fn trusted(
    enrolling: suzatary_core::signatures::Enrolling<'_>,
    enrolment: &suzatary_core::signatures::Enrolment,
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

/// Send a draft with its held values and attachments read again and checked.
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
    let read = match conversations::read(caps.secretary.conversations, id) {
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

/// One attachment's bytes as its source holds them now, by its digest.
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
    suzatary_core::secretary::opening::open(&caps.secretary, source, read)
        .map(|opened| opened.bytes)
        .map_err(|refused| refused.message().to_owned())
}

/// Forward a stored message as its card showed it, only while its digest holds, or nothing.
fn forwarded(
    caps: &Server<'_>,
    effectors: &Effectors<'_>,
    id: &ConversationId,
    forward: &suzatary_core::secretary::proposal::Forward,
) -> Performed {
    use suzatary_core::held_value::Reopened;
    use suzatary_core::secretary::proposal::Style;
    let not_sent = |reason: String| Performed::Tried(Tried::NotSent { reason });
    let file = &forward.message.file;
    let read = match conversations::read(caps.secretary.conversations, id) {
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
    let read = suzatary_core::attachment::AttachmentId::of(&original.raw);
    match attached::read_from(&file.sha256, &read, forward.parts(original)) {
        Ok(parts) => effectors.post.send(&forward.draft(), body, parts),
        Err(changed) => not_sent(changed.to_string()),
    }
}

/// Whether `account`'s Google calendar may be written (`google::reads_calendars`); else the first
/// step the box owes, or why no step gives it: an address the instance file does not declare is
/// no account of this box, so no grant would make the event its own.
fn google_account(
    edits: &dyn Edits,
    (secrets, settables): (&dyn Secrets, &Settables),
    account: &suzatary_core::mail::address::EmailAddress,
) -> Result<(), Tried> {
    let declared = edits.declared().map_err(|failed| Tried::NotSent {
        reason: format!("nothing was written: the instance file does not read ({failed})"),
    })?;
    let Some(declared) = declared
        .accounts
        .iter()
        .find(|held| held.email.is(account.as_str()))
    else {
        return Err(Tried::Refused {
            reason: format!(
                "nothing was written: {account} is no account the instance file declares"
            ),
        });
    };
    if suzatary_core::google::reads_calendars(&declared.sync, &secrets.google_grant(account)) {
        Ok(())
    } else if google_client_kept(secrets, settables).is_set() {
        Err(Tried::Lacks(suzatary_core::mail::lacking::Lacking::Grant {
            account: account.clone(),
            grant: suzatary_core::mail::lacking::Grant::GoogleCalendar,
        }))
    } else {
        Err(Tried::Lacks(
            suzatary_core::mail::lacking::Lacking::GoogleClient,
        ))
    }
}

/// Whether the box's Google client is stored, its id and its secret both.
fn google_client_kept(
    secrets: &dyn Secrets,
    settables: &Settables,
) -> suzatary_core::secrets::SecretState {
    suzatary_core::secrets::SecretState::all(
        [
            suzatary_core::google::CLIENT_ID,
            suzatary_core::google::CLIENT_SECRET,
        ]
        .map(|name| {
            Settable::mint(settables, name)
                .map_or(suzatary_core::secrets::SecretState::Missing, |name| {
                    secrets.secret_state(&name)
                })
        }),
    )
}

/// Keep the pasted Google client only when it is a web client listing `redirect`; a refusal
/// never quotes the secret.
fn google_client(
    writer: &dyn Writes,
    settables: &Settables,
    redirect: &suzatary_core::google::RedirectUri,
    pasted: &Secret,
) -> Result<Performed, Refusal> {
    let client = match suzatary_core::google::Client::of_download(pasted, redirect) {
        Ok(client) => client,
        Err(refused) => {
            return Ok(Performed::Tried(Tried::NotSent {
                reason: format!("nothing was kept: {refused}"),
            }));
        }
    };
    let mint = |name: &str| {
        Settable::mint(settables, name).map_err(|_| {
            Refusal::conflict(
                "no Google account is declared any more, so the box keeps no Google client",
            )
        })
    };
    let (id, secret) = (
        mint(suzatary_core::google::CLIENT_ID)?,
        mint(suzatary_core::google::CLIENT_SECRET)?,
    );
    let written = writer
        .write_secret(&id, &Secret::new(client.id.as_str()))
        .and_then(|()| writer.write_secret(&secret, &Secret::new(client.secret.expose())));
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
    keys: &std::collections::BTreeSet<suzatary_core::secretary::locked_text::KeyName>,
    tap: &Tap,
) -> Result<Performed, Refusal> {
    suzatary_core::secretary::transcript::erase(
        caps.secretary.conversations,
        id,
        keys.clone(),
        suzatary_core::secretary::erasure::Erasure::Asked {
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

/// A topic opened for turn `turn`, read again.
fn afresh(
    caps: &Server<'_>,
    cards: &Cards<'_>,
    conversation: &ConversationId,
    turn: usize,
) -> Result<(ConversationId, Performed), Refusal> {
    let request = suzatary_core::secretary::afresh::Request::at_tap(
        &conversations::read(caps.secretary.conversations, conversation)?,
        turn,
    )?;
    let opened = begun(
        caps,
        cards,
        "",
        Some(&request.entry(caps.secretary.clock.now())),
    )?;
    let what = format!("asked your request again in a fresh conversation, {opened}");
    Ok((opened, Done::Acted { what }.into()))
}

/// Ask the store again for every declared name.
fn store_answers(secrets: &dyn Secrets, settables: &Settables) -> Performed {
    match suzatary_core::secrets::Snapshot::read(secrets, settables.iter()).answers() {
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

/// Keep `value` under `name` once a model answers a test call with it.
fn verified(
    writer: &dyn Writes,
    verify: &dyn suzatary_core::settings::Verify,
    name: &Settable,
    value: &Secret,
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
            what: format!(
                "stored {}",
                suzatary_core::settings::Help::label(name.name())
            ),
        }),
        // A put, so a retry is safe whether or not this one landed.
        Err(reason) => {
            // The store's reason never names a value.
            tracing::warn!(secret = name.name(), %reason, "a tapped secret was not stored");
            Performed::Tried(Tried::NotSent {
                reason: format!("could not store {}: {reason}", name.name()),
            })
        }
    }
}

struct SignedIn<'a> {
    signing: &'a dyn suzatary_core::secretary::sign_in::Sessions,
    verify: &'a dyn suzatary_core::settings::Verify,
    writer: &'a dyn Writes,
}

fn coded(
    effectors: &Effectors<'_>,
    url: &suzatary_core::secretary::sign_in::SignInUrl,
    code: &Secret,
    name: &Settable,
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

/// Type the code into the sign-in at `url` and keep its token under `name`; the token goes to the
/// test call and the store alone.
fn signed_in(
    reach: &SignedIn<'_>,
    url: &suzatary_core::secretary::sign_in::SignInUrl,
    code: &Secret,
    name: &Settable,
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

/// Remember a tapped card's words on its authority.
fn remembered(
    held: &suzatary_core::secretary::memories::Held<'_>,
    clock: &dyn Clock,
    id: &ConversationId,
    index: usize,
    (text, kind, replaces): (
        &suzatary_core::secretary::salted_text::TextName,
        suzatary_core::secretary::memory::Kind,
        &suzatary_core::secretary::memory::Replaces,
    ),
) -> Performed {
    let origin = suzatary_core::secretary::memory::Origin::Tapped {
        conversation: id.clone(),
        proposal: index,
    };
    match held.write_kept(text, kind, replaces, &origin, clock.now()) {
        Ok(remembered) => Performed::Done(Done::Acted {
            what: match replaces {
                suzatary_core::secretary::memory::Replaces::Nothing => {
                    format!("remembered as fact {}", remembered.id)
                }
                suzatary_core::secretary::memory::Replaces::Facts(sources) => {
                    format!("merged {sources} into fact {}", remembered.id)
                }
            },
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
    match suzatary_core::secretary::gate::apply(
        &bound.caps,
        bound.policy,
        &known,
        cards.deployment.zone,
        changeset,
        suzatary_core::secretary::WRITES_AS,
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

/// `POST …/proposals/<n>/dismiss`: answers a proposal no, with a reason.
///
/// # Errors
/// As [`suzatary_core::secretary::dismiss`].
pub fn dismiss(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
    reason: &str,
) -> Answered<wire::ConversationPayload> {
    let after = suzatary_core::secretary::dismiss(
        caps.secretary.conversations,
        caps.secretary.memories,
        caps.secretary.clock,
        by.who(),
        card,
        reason,
    )?;
    payload(&after, caps, cards)
}

/// `POST …/proposals/<n>/acknowledge`: the owner checked a send that may have gone.
///
/// # Errors
/// As [`suzatary_core::secretary::acknowledge`].
pub fn acknowledge(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
) -> Answered<wire::ConversationPayload> {
    let after = suzatary_core::secretary::acknowledge(
        caps.secretary.conversations,
        caps.secretary.inbox,
        caps.secretary.clock,
        by.who(),
        card,
    )?;
    payload(&after, caps, cards)
}

/// `POST …/todo/<id>/build`: file the card whose tap fires a coding session for `todo`.
///
/// # Errors
/// The builder's, the build module's and the stores'.
pub fn build(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    todo: &TodoRef,
) -> Answered<wire::ConversationPayload> {
    cards.deployment.builder(&todo.chain)?;
    let bound = suzatary_core::secretary::bound_of(cards.chains, Some(todo.chain.as_str()))?;
    let at = caps.secretary.clock.now();
    let believed = suzatary_core::believed::Believed::of(
        &bound.caps.chain.nodes()?,
        at,
        &bound.policy.untrusted,
    )
    .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let record = suzatary_core::secretary::build::record(&believed, todo)?;
    let (conversations, inbox) = (caps.secretary.conversations, caps.secretary.inbox);
    // One hold, so two builds asked at once are not the same attempt.
    let _held = inbox.locked()?;
    let task = suzatary_core::secretary::build::task(
        todo,
        record,
        suzatary_core::secretary::build::attempt(&inbox::fold(inbox)?, todo),
    )?;
    let conversation = &*suzatary_core::secretary::build::CONVERSATION;
    let proposer = suzatary_core::secretary::proposer::Proposer::Box;
    suzatary_core::secretary::filed::opened(conversations, conversation, "Builds", &proposer, at)?;
    suzatary_core::secretary::filed::file_held(
        conversations,
        inbox,
        cards.chains,
        at,
        suzatary_core::secretary::filed::Filing {
            conversation,
            proposer,
            proposal: Proposal::Act(Act::CodingSession { task }),
            reasons: suzatary_core::secretary::proposal::Reasons::new(
                format!("{} asked for {todo} to be built.", by.who().as_str()),
                vec![Reference::Todo(todo.clone())],
            )
            .map_err(|failed| Refusal::internal(failed.to_string()))?,
            serves: Some(todo.clone()),
            revises: None,
        },
    )?;
    payload(
        &conversations::read(conversations, conversation)?,
        caps,
        cards,
    )
}

/// `POST …/proposals/<n>/drop`: let a session go, with a reason.
///
/// # Errors
/// As [`suzatary_core::secretary::coding::dropped`].
pub fn drop_session(
    caps: &Server<'_>,
    by: &Tap,
    cards: &Cards<'_>,
    card: &ProposalRef,
    reason: &str,
) -> Answered<wire::ConversationPayload> {
    let (id, index) = (&card.conversation, card.index);
    suzatary_core::secretary::coding::dropped(
        caps.secretary.conversations,
        id,
        index,
        by.who(),
        reason,
        caps.secretary.clock.now(),
    )?;
    payload(
        &conversations::read(caps.secretary.conversations, id)?,
        caps,
        cards,
    )
}

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

/// `GET /api/secretary/memory`: every standing fact and the remember cards' kept words.
///
/// # Errors
/// The stores' own.
pub fn memory(
    memories: suzatary_core::secretary::memories::Memories<'_>,
) -> Answered<wire::MemoryPayload> {
    Ok(wire::MemoryPayload {
        cards: card_words(memories)?,
        ..memory_payload(&memories.reads().read()?)
    })
}

fn card_words(
    memories: suzatary_core::secretary::memories::Memories<'_>,
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

/// `POST /api/secretary/memory/<id>/forget`: the fact forgotten and its words deleted.
///
/// # Errors
/// [`Refusal::not_found`] for a number no standing fact carries, and the store's own.
pub fn forget(
    memories: suzatary_core::secretary::memories::Memories<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
    id: suzatary_core::secretary::memory::FactId,
) -> Answered<wire::MemoryPayload> {
    memories.forget(id, clock.now(), by)?;
    memory(memories)
}

/// `GET /api/secretary/record`: the summaries a session opening is told.
///
/// # Errors
/// The stores' own.
pub fn record(
    digests: &suzatary_core::secretary::digests::Digests<'_>,
) -> Answered<crate::wire::RecordPayload> {
    let record = suzatary_core::secretary::shown_record::OwnersRecord::of(
        suzatary_core::secretary::compaction::Reading::of(&digests.reads())?,
    );
    Ok(crate::wire::RecordPayload {
        summaries: record
            .summaries(suzatary_core::secretary::digest_view::VIEW_BYTES)
            .into_iter()
            .map(|(part, line)| crate::wire::RecordSummary {
                node: part.node.as_str().to_owned(),
                handle: part.handle(),
                lines: u32::try_from(part.span.len()).unwrap_or(u32::MAX),
                text: line.as_str().to_owned(),
            })
            .collect(),
    })
}

/// `POST /api/secretary/record/<node>/retract`: the summary and every one above it are asked again.
///
/// # Errors
/// [`Refusal::not_found`] for a node that shows no summary, and the stores' own.
pub fn retract(
    memories: suzatary_core::secretary::memories::Memories<'_>,
    clock: &dyn Clock,
    tap: &suzatary_core::signatures::Tap,
    node: &prodrome_core::event::Hash,
) -> Answered<crate::wire::RecordPayload> {
    memories.hold()?.retract(node, clock.now(), tap)?;
    record(&memories.digests)
}

fn memory_payload(memory: &suzatary_core::secretary::memory::Memory) -> wire::MemoryPayload {
    use suzatary_core::secretary::provenance::Wrote;
    let number = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    wire::MemoryPayload {
        facts: memory
            .facts
            .iter()
            .map(|remembered| wire::FactRow {
                id: remembered.id,
                at: suzatary_core::secretary::stamp::stamp(remembered.at),
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
        cap: number(suzatary_core::secretary::memory::MAX_LIVE),
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

fn said_on(origin: &suzatary_core::secretary::memory::Origin) -> wire::SaidOn {
    let number = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    match origin {
        suzatary_core::secretary::memory::Origin::Turn { conversation, turn } => {
            wire::SaidOn::Turn {
                conversation: conversation.as_str().to_owned(),
                turn: number(*turn),
            }
        }
        suzatary_core::secretary::memory::Origin::Tapped {
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
    use suzatary_core::secretary::answers::Answers;
    use suzatary_core::secretary::proposal::Reasons;
    use suzatary_core::secretary::readings::{Place, proposal, turn};
    use suzatary_core::secretary::responded::Responded;
    use suzatary_core::secretary::store::Asking;
    use suzatary_core::secretary::store::Said;
    use suzatary_core::secretary::store::Standing;

    /// `proposal`, the conversation's first, answered `responses` as the inbox reads them.
    fn standing_of(proposal: Proposal, responses: &[Responded]) -> Standing {
        use suzatary_core::secretary::inbox_event::{InboxEvent, Said};
        let asked = inbox::Asked {
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
            proposal,
            reasons: reasons(),
            asked: "proposal:default/0".parse().expect("a reference"),
            price: inbox::Price::unpriced(),
        };
        let at =
            suzatary_base::capability::Datetime::new(2026, 9, 11, 9, 0, 0, 0).expect("an instant");
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
    use suzatary_base::capability::Parses;

    fn no_chain() -> Drawn<'static> {
        Drawn {
            nodes: std::collections::BTreeMap::new(),
            typeset: &Parses,
            now: suzatary_base::zone::Moment {
                at: suzatary_base::capability::Datetime::new(2026, 9, 9, 9, 2, 0, 0)
                    .expect("a real instant"),
                zone: suzatary_base::zone::Zone::UTC,
            },
            categories: Vec::new(),
            model: None,
            routine: None,
            plugins: Vec::new(),
            reach: suzatary_core::connect::PLAIN.clone(),
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
        let suzatary_core::wire::inbox::ProposedAct::Email {
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
            _block: &suzatary_base::answer::Renderable,
        ) -> Result<suzatary_core::render::Html, suzatary_core::secretary::answers::AnswerError>
        {
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
                at: suzatary_base::capability::Datetime::new(2026, 9, 9, 9, 1, 0, 0)
                    .expect("an instant"),
                turn: suzatary_core::secretary::model::Turn::Person {
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
    fn receipts_memory() -> suzatary_core::secretary::memory::Memory {
        use suzatary_core::secretary::memory::{
            Fact, FactId, Forgotten, Kind, Memory, Merge, Origin, Remembered,
        };
        Memory {
            facts: vec![Remembered {
                id: FactId(1),
                at: suzatary_base::capability::Datetime::new(2026, 10, 2, 9, 0, 0, 0)
                    .expect("an instant"),
                fact: Fact::new("He takes the 7:40 train").expect("a fact"),
                kind: Kind::Owner,
                said: suzatary_core::secretary::provenance::Provenance::said(Origin::Turn {
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
                at: suzatary_base::capability::Datetime::new(2026, 10, 2, 9, 2, 0, 0)
                    .expect("an instant"),
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
        use suzatary_core::secretary::memory::{FactId, Memory};
        use suzatary_core::secretary::outcome::Outcome;
        let memory = receipts_memory();
        let row =
            |turn_said: suzatary_core::secretary::model::Turn, memory: Option<&Memory>| match turn(
                &NeverAsked,
                &Said {
                    at: suzatary_base::capability::Datetime::new(2026, 10, 2, 9, 0, 0, 0)
                        .expect("an instant"),
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
                suzatary_core::secretary::model::Turn::Tool {
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
        let erased = suzatary_core::secretary::turn_shape::join(
            suzatary_core::secretary::turn_shape::TurnShape::Tool {
                verb: "remember".into(),
                outcome: remembered(1),
                files: Vec::new(),
            },
            vec![
                suzatary_core::secretary::words::Gone::Erased {
                    erasure: suzatary_core::secretary::erasure::Erasure::Asked {
                        by: prodrome_core::event::Actor::new("ada").expect("an actor"),
                    },
                }
                .into(),
            ],
        );
        assert!(matches!(
            erased,
            suzatary_core::secretary::model::Turn::Erased { .. }
        ));
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
            block: &suzatary_base::answer::Renderable,
        ) -> Result<suzatary_core::render::Html, suzatary_core::secretary::answers::AnswerError>
        {
            Ok(suzatary_core::render::Html::from_render(format!(
                "<p>{}</p>",
                block.as_str().trim()
            )))
        }
    }

    struct NeverRenders;
    impl Answers for NeverRenders {
        fn block(
            &self,
            _block: &suzatary_base::answer::Renderable,
        ) -> Result<suzatary_core::render::Html, suzatary_core::secretary::answers::AnswerError>
        {
            Err(suzatary_core::secretary::answers::AnswerError(
                "typst refused".to_owned(),
            ))
        }
    }

    /// A model turn is drawn in blocks, each asked of the renderer; a refused one is text. That
    /// `html` is refused is the closed world's law (`suzatary_io::render`), not a scan here.
    #[test]
    fn a_model_turns_blocks_each_ask_the_renderer() {
        let model_turn = Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 9, 9, 2, 0, 0)
                .expect("an instant"),
            turn: suzatary_core::secretary::model::Turn::Model {
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
        assert_eq!(rendered[0].kind, suzatary_base::answer::Kind::Heading);
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
            at: suzatary_base::capability::Datetime::new(2026, 9, 9, 9, 1, 0, 0)
                .expect("an instant"),
            turn: suzatary_core::secretary::model::Turn::Person {
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
            at: suzatary_base::capability::Datetime::new(2026, 9, 9, 9, 3, 0, 0)
                .expect("an instant"),
            turn: suzatary_core::secretary::model::Turn::Act {
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
            &suzatary_core::signatures::scene("ada"),
            &suzatary_core::signatures::ActHeaders {
                act: Some("1"),
                client: Some(suzatary_core::signatures::CLIENT),
                content_type: Some("application/json"),
                site: Some("same-origin"),
            },
            suzatary_core::signatures::Seat::Device,
        )
        .expect("a tap")
    }

    fn value() -> TapAnswer {
        TapAnswer::Value(suzatary_base::capability::Secret::new("a value"))
    }

    fn endorsed() -> TapAnswer {
        TapAnswer::Endorsed(suzatary_core::signatures::Endorsement {
            key: suzatary_core::cast::keys::root(),
            signature: prodrome_core::sign::Signature::new("00".repeat(64)).expect("a signature"),
        })
    }

    /// The names a deployment with an ntfy channel declares.
    fn with_ntfy() -> suzatary_core::settings::Settables {
        suzatary_core::settings::Settables::derive(
            suzatary_core::settings::MODEL_TOKEN,
            [suzatary_core::connect::Channel::Ntfy],
            None,
            &[],
        )
    }

    /// `proposal` paired with `answer` against `settables`, on a deployment with no root, by
    /// a tap that takes no memory hold.
    fn prepared<'p>(
        proposal: &'p Proposal,
        answer: &'p TapAnswer,
        settables: &suzatary_core::settings::Settables,
    ) -> Result<Prepared<'p, 'static>, Refusal> {
        let tap = tapped();
        let hold = || Err(Refusal::internal("no memory in this test"));
        let withdrawn = |_: &ProposalRef| Err(Refusal::internal("no inbox in this test"));
        Prepared::of(
            proposal,
            answer,
            &Preparing {
                settables,
                enrolling: None,
                tap: &tap,
                hold: &hold,
                withdrawn: &withdrawn,
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
            assert_eq!(refusal.status(), suzatary_base::refusal::Status::BadRequest);
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
        let undeclared = suzatary_core::settings::Settables::derive(
            suzatary_core::settings::MODEL_TOKEN,
            std::iter::empty(),
            None,
            &[],
        );
        let refusal = refused(prepared(&card, &answer, &undeclared));
        assert_eq!(refusal.status(), suzatary_base::refusal::Status::Conflict);
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
            url: suzatary_core::secretary::sign_in::SignInUrl::parse(
                "https://sign-in.example.test/oauth/authorize?code=true&state=s",
            )
            .expect("a link"),
        });
        let answer = value();
        match prepared(&card, &answer, &with_ntfy()) {
            Ok(Prepared::Code { name, .. }) => {
                assert_eq!(name.name(), suzatary_core::settings::MODEL_TOKEN);
            }
            Ok(_) => panic!("not a code"),
            Err(refusal) => panic!("{refusal}"),
        }
        let refusal = refused(prepared(
            &card,
            &answer,
            &suzatary_core::settings::Settables::default(),
        ));
        assert_eq!(refusal.status(), suzatary_base::refusal::Status::Conflict);
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
        let request = suzatary_core::web::WebRequest::Fetch {
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
            connection: suzatary_core::connect::Connection::parse(
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
            connection: suzatary_core::connect::Connection::parse("ics", &["Work".to_owned()])
                .expect("a calendar"),
        });
        let refusal = refused(prepared(&calendar, &TapAnswer::Always, &with_ntfy()));
        assert_eq!(refusal.message(), ALWAYS_BELONGS);
    }

    /// A device card is tapped from a trusted device, which signs it, on a deployment with a
    /// root; without either nothing runs.
    #[test]
    fn a_device_card_needs_an_endorsement_and_a_root() {
        let card = Proposal::Act(Act::Device(suzatary_core::signatures::Enrolment {
            actor: prodrome_core::event::Actor::new("ada").expect("an actor"),
            key: suzatary_core::cast::keys::root(),
        }));
        let refusal = refused(prepared(&card, &TapAnswer::Plain, &with_ntfy()));
        assert_eq!(refusal.status(), suzatary_base::refusal::Status::Forbidden);
        let answer = endorsed();
        let refusal = refused(prepared(&card, &answer, &with_ntfy()));
        assert_eq!(refusal.status(), suzatary_base::refusal::Status::BadRequest);
        assert!(refusal.message().contains("declares no root"), "{refusal}");
        let refusal = refused(prepared(&card, &value(), &with_ntfy()));
        assert_eq!(refusal.message(), VALUE_BELONGS);
    }
}
