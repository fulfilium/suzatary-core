//! The secretary's stores as replicas a tab holds. Who writes each is its row's (`Kind::writer`):
//! the box alone, or a tab too, each object signed by its device and taken by the store's rule.

use axum::Json;
use axum::response::{IntoResponse as _, Response};
use suzatary_api::wire::{StoreName, WrittenName};
use suzatary_api::{query, routes, wire};
use suzatary_base::refusal::Refusal;
use suzatary_lines::secretary::lines_rule::SlotKeys;
use suzatary_lines::wire::lines::ReceiptPayload;
use suzatary_server::server::Server;
use suzatary_store::admission::Beside;
use suzatary_store::store::{Prodrome, StoreError};
use suzatary_stores::asks::AskEvent;

use super::Chained;
use super::call::Call;
use super::chain::Literals;
use super::guard::{Poster, Reader};
use super::params::{Asked, Params};
use super::reply::Immutable;
use prodrome_core::event;
use prodrome_core::event::Hash;
use suzatary_base::refusal;
use suzatary_lines::secretary::line_event;

/// # Errors
/// [`Refusal::not_found`] for a name that is no store's.
pub fn named(segment: &str) -> Result<StoreName, Refusal> {
    if let Some(store) = StoreName::ALL
        .into_iter()
        .find(|store| store.kind().name() == segment)
    {
        return Ok(store);
    }
    // An older page asks for the store the lines replaced.
    if segment == "conversations" {
        return Err(Refusal::new(
            refusal::Status::UpgradeRequired,
            "the conversations are read from the lines now: a new version of this page is ready, and Reload loads it",
        ));
    }
    Err(Refusal::not_found(format!(
        "a tab holds the inbox, the lines, the topics, the topic aliases and the asks, not {}",
        refusal::shown(segment)
    )))
}

/// A store's heads, sorted.
///
/// # Errors
/// The store's own.
pub fn heads(
    store: StoreName,
    caps: &Server<'_>,
    asks: &dyn Prodrome<AskEvent>,
) -> Result<Vec<String>, StoreError> {
    let tips = match store {
        StoreName::Inbox => caps.secretary.inbox.tips()?,
        StoreName::Lines => caps.secretary.conversations.lines.tips()?,
        StoreName::Topics => caps.secretary.topics.memberships.tips()?,
        StoreName::TopicAliases => caps.secretary.topics.aliases.tips()?,
        StoreName::Asks => asks.tips()?,
    };
    let mut heads: Vec<String> = tips.iter().map(|h| h.as_str().to_owned()).collect();
    heads.sort();
    Ok(heads)
}

/// A store a replica pulls whole, by its objects: every one but the lines, pulled by skeleton.
enum Pulled {
    Inbox,
    Topics,
    TopicAliases,
    Asks,
}

/// A store pulled by its objects, and the frontier a replica of it holds.
pub struct Since {
    store: Pulled,
    since: prodrome_core::store::frontier::Since,
}

impl Params for Since {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let store = match named(asked.segment("store")?)? {
            StoreName::Inbox => Pulled::Inbox,
            StoreName::Topics => Pulled::Topics,
            StoreName::TopicAliases => Pulled::TopicAliases,
            StoreName::Asks => Pulled::Asks,
            StoreName::Lines => {
                return Err(Refusal::not_found(
                    "the lines are pulled by their skeleton: GET /api/stores/lines/skeleton",
                ));
            }
        };
        Ok(Self {
            store,
            since: query::since(asked.query("since"))?,
        })
    }
}

/// `GET /api/stores/<store>/objects?since=<frontier>`: a piece of what that frontier lacks.
pub fn objects(call: &Call<Reader, Since, ()>) -> Result<Json<wire::StorePayload>, Refusal> {
    let caps = call.state.first.caps;
    let Since { store, since } = &call.params;
    let (books, now) = (call.state.books(), caps.secretary.clock.now());
    let cut = routes::Cut::PIECE;
    match store {
        Pulled::Inbox => routes::store_objects_since(caps.secretary.inbox, since, books, now, cut),
        Pulled::Topics => {
            routes::store_objects_since(caps.secretary.topics.memberships, since, books, now, cut)
        }
        Pulled::TopicAliases => {
            routes::store_objects_since(caps.secretary.topics.aliases, since, books, now, cut)
        }
        Pulled::Asks => routes::store_objects_since(&*call.state.asks, since, books, now, cut),
    }
    .map(Json)
}

/// A store a tab writes, as its row says (`Kind::writer`).
pub struct Written(WrittenName);

impl Params for Written {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        // A page posts only to a store its build writes: one this box does not keep is another
        // build's, and the page posts it again once they agree.
        let store = named(asked.segment("store")?).map_err(|refused| {
            if refused.status() == refusal::Status::NotFound {
                Refusal::other_build(format!(
                    "this box runs another build: {}; it goes again once the box and this page agree",
                    refused.message()
                ))
            } else {
                refused
            }
        })?;
        WrittenName::named(store.kind().name())
            .map(Self)
            .ok_or_else(|| {
                Refusal::method_not_allowed(format!(
                    "the box alone writes the {}",
                    store.kind().name()
                ))
            })
    }
}

/// What a store a tab writes takes beside its objects, read at the door. A post's own fault is
/// answered 422, never 400: a 400 is what a box of another build answers a body it does not read,
/// so the page holds those lines for later and refuses for good only on this build's word.
fn beside<B: Beside>(literals: &Literals) -> Result<B, Refusal> {
    B::parse(&literals.keys).map_err(Refusal::unprocessable)
}

/// `POST /api/stores/<store>/objects`: a store a tab writes, each change taken only beside a
/// trusted signature and where the rule its row names takes it. The lines keep each slot's key a
/// taken line came with before they take the line, and no key of a line they refuse.
pub fn take(call: &Call<Poster, Written, Literals>) -> Result<Json<ReceiptPayload>, Refusal> {
    let caps = call.state.first.caps;
    let topics = caps.secretary.topics;
    let chains = call.state.keeping();
    let (posted, frontiers) = (&call.body.posted(), &call.state.frontiers);
    let writer = routes::Writer {
        actor: &call.guard.0,
        chains: &chains,
        root: &call.chain.policy.root,
    };
    let recording = routes::Recording {
        asks: &*call.state.asks,
        at: caps.secretary.clock.now(),
    };
    let keyless = || beside::<()>(&call.body);
    let nothing = |_: &[Hash]| Ok(());
    match call.params.0 {
        WrittenName::Lines => {
            let conversations = caps.secretary.conversations;
            let keys = beside::<SlotKeys>(&call.body)?;
            // Keys travel beside the post's lines and nothing else: the rule opens a line's slots
            // with them, and only a taken line's are kept.
            let lines = posted_lines(&call.body.objects);
            if let Some(stray) = keys.names().find(|name| !lines.contains(*name)) {
                return Err(Refusal::unprocessable(format!(
                    "keys are posted beside {stray}, which is no line of this post"
                )));
            }
            let keep = |taken: &[Hash]| -> Result<(), Refusal> {
                for name in taken.iter().filter(|name| lines.contains(*name)) {
                    for key in keys.of(name) {
                        conversations.keys.set(&key.name(), key)?;
                    }
                }
                Ok(())
            };
            let world: &dyn suzatary_lines::secretary::lines_rule::World = &Beyond {
                root: call.chain.policy.root.clone(),
                uploads: call.state.uploads.clone(),
                now: caps.secretary.clock.now(),
            };
            let beside = routes::Beside {
                posted: &keys,
                keep: &keep,
                world,
            };
            let lines = conversations.lines;
            let receipt =
                routes::receive_changes(lines, posted, &beside, frontiers, writer, recording)?;
            woken(&call.state, &call.body.objects, &receipt);
            Ok(receipt)
        }
        WrittenName::Topics => {
            let shared = routes::Beside {
                posted: &keyless()?,
                keep: &nothing,
                world: &(),
            };
            let memberships = topics.memberships;
            routes::receive_changes(memberships, posted, &shared, frontiers, writer, recording)
        }
        WrittenName::TopicAliases => {
            let shared = routes::Beside {
                posted: &keyless()?,
                keep: &nothing,
                world: &(),
            };
            let aliases = topics.aliases;
            routes::receive_changes(aliases, posted, &shared, frontiers, writer, recording)
        }
    }
    .map(Json)
}

/// What the lines' rule reads beyond the lines: the deployment's root key, whose lines are the
/// operator's, and the uploads a person's turn names.
struct Beyond {
    root: prodrome_core::sign::PublicKey,
    uploads: Option<std::sync::Arc<super::Uploads>>,
    /// The box's clock as the post came.
    now: prodrome_core::literal::Datetime,
}

impl suzatary_lines::secretary::lines_rule::World for Beyond {
    fn now(&self) -> prodrome_core::literal::Datetime {
        self.now
    }

    fn seat(
        &self,
        signer: &prodrome_core::sign::PublicKey,
    ) -> suzatary_signatures::signatures::Seat {
        if *signer == self.root {
            suzatary_signatures::signatures::Seat::Operator
        } else {
            suzatary_signatures::signatures::Seat::Device
        }
    }

    fn upload(
        &self,
        id: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<suzatary_lines::secretary::lines_rule::Upload>, String> {
        use suzatary_io::attachments::Stored;
        use suzatary_lines::secretary::lines_rule::Upload;
        let Some(uploads) = &self.uploads else {
            return Ok(None);
        };
        Ok(uploads
            .store
            .attachment(id)
            .map_err(|failed| failed.to_string())?
            .map(|stored| match stored {
                Stored::Here { attachment, .. } => Upload::Here(attachment),
                Stored::Private { attachment, .. } => Upload::Private(attachment),
            }))
    }
}

/// The name of every line `posted` holds, a `Change` of the lines; never a signature.
fn posted_lines(posted: &[String]) -> std::collections::BTreeSet<Hash> {
    posted
        .iter()
        .filter_map(|literal| {
            let envelope = event::parse_envelope::<line_event::LineEvent>(
                &line_event::LineVocabulary,
                literal,
            )
            .ok()?;
            matches!(envelope, event::Envelope::Change(_)).then(|| event::seal_hash(&envelope))
        })
        .collect()
}

/// Each conversation a line the box took is in told to its worker, so what the line starts (a
/// round for a question, a setup's opening note) is written; and for a stop the box took, each
/// round the stop's line ended told to stop.
fn woken(state: &super::State, posted: &[String], receipt: &ReceiptPayload) {
    use prodrome_core::schema::Schema as _;
    let vocabulary = line_event::LineVocabulary;
    let mut conversations = std::collections::BTreeSet::new();
    for literal in posted {
        let Ok(envelope) = event::parse_envelope::<line_event::LineEvent>(&vocabulary, literal)
        else {
            continue;
        };
        let name = event::seal_hash(&envelope);
        let Some(line) = envelope.event() else {
            continue;
        };
        if !receipt.adopted.iter().any(|n| n == name.as_str()) {
            continue;
        }
        if let suzatary_lines::secretary::line_shape::LineShape::Stopped { .. } = line.shape() {
            super::conversation_worker::Workers::stopped(
                std::sync::Arc::new(state.clone()),
                line.conversation(),
                name.clone(),
                line.actor().clone(),
            );
        }
        conversations.insert(line.conversation().clone());
    }
    for id in &conversations {
        state.heard(id);
    }
}

/// The frontier a replica of the lines holds.
pub struct LinesSince(prodrome_core::store::frontier::Since);

impl Params for LinesSince {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        query::since(asked.query("since")).map(Self)
    }
}

/// `GET /api/stores/lines/skeleton?since=<frontier>`: what the replica lacks, without literals.
pub fn skeleton(
    call: &Call<Reader, LinesSince, ()>,
) -> Result<Json<suzatary_lines::wire::lines::SkeletonPayload>, Refusal> {
    let caps = call.state.first.caps;
    routes::lines_skeleton(
        caps.secretary.conversations,
        &call.params.0,
        call.state.books(),
        caps.secretary.clock.now(),
        routes::Cut::PIECE,
    )
    .map(Json)
}

/// A store, and the objects asked of it by hash.
pub struct Wanted {
    store: StoreName,
    hashes: query::Hashes,
}

impl Params for Wanted {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            store: named(asked.segment("store")?)?,
            hashes: query::Hashes::parse(asked.query("hashes"))?,
        })
    }
}

/// `GET /api/stores/<store>/content?hashes=<hashes>`: those objects' literals, kept for good but
/// the lines', since a kept line would hold its dead words on the device after its key retires.
pub fn content(call: &Call<Reader, Wanted, ()>) -> Result<Response, Refusal> {
    let caps = call.state.first.caps;
    let Wanted { store, hashes } = &call.params;
    let kept = |answer: wire::ContentPayload| Immutable(Json(answer)).into_response();
    match store {
        StoreName::Inbox => routes::content(caps.secretary.inbox, hashes).map(kept),
        StoreName::Lines => routes::lines_content(caps.secretary.conversations, hashes)
            .map(|answer| Json(answer).into_response()),
        StoreName::Topics => routes::content(caps.secretary.topics.memberships, hashes).map(kept),
        StoreName::TopicAliases => routes::content(caps.secretary.topics.aliases, hashes).map(kept),
        StoreName::Asks => routes::content(&*call.state.asks, hashes).map(kept),
    }
}

/// `GET /api/waiting`: the Inbox badge's count, without pulling the conversations.
pub fn waiting(call: &Call<Reader, (), ()>) -> Result<Json<wire::WaitingPayload>, Refusal> {
    let caps = call.state.first.caps;
    let inbox = suzatary_inbox::secretary::inbox::fold(caps.secretary.inbox)?;
    let read = call.state.chains_read();
    let set = read.set(caps.secretary.clock.now());
    let chains: Vec<_> = call
        .state
        .chains
        .values()
        .map(|chained| chained.policy.name.clone())
        .collect();
    routes::waiting(&inbox, caps.secretary.conversations.reads(), &set, &chains).map(Json)
}
