//! The secretary's stores as replicas a tab holds. Who writes each is its row's (`Kind::writer`):
//! the box alone, or a tab too, each object signed by its device and taken by the store's rule.

use axum::Json;
use axum::response::{IntoResponse as _, Response};
use suzatary_api::server::Server;
use suzatary_api::wire::{StoreName, WrittenName};
use suzatary_api::{query, routes, wire};
use suzatary_base::refusal::Refusal;
use suzatary_core::asks::AskEvent;
use suzatary_lines::secretary::lines_rule::SlotKeys;
use suzatary_lines::wire::lines::ReceiptPayload;
use suzatary_store::admission::Beside;
use suzatary_store::store::{Prodrome, StoreError};

use super::Chained;
use super::call::Call;
use super::chain::Literals;
use super::guard::{Poster, Reader};
use super::params::{Asked, Params};
use super::reply::Immutable;
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
            suzatary_base::refusal::Status::UpgradeRequired,
            "the conversations are read from the lines now: a new version of this page is ready, and Reload loads it",
        ));
    }
    Err(Refusal::not_found(format!(
        "a tab holds the inbox, the lines, the topics, the topic aliases and the asks, not {}",
        suzatary_base::refusal::shown(segment)
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
        let store = named(asked.segment("store")?)?;
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

/// What a store a tab writes takes beside its objects, read at the door.
fn beside<B: Beside>(literals: &Literals) -> Result<B, Refusal> {
    B::parse(&literals.keys).map_err(Refusal::bad_request)
}

/// `POST /api/stores/<store>/objects`: a store a tab writes, each change taken only beside a
/// trusted signature and where the rule its row names takes it. The lines keep each slot's key a
/// taken line came with before they take the line, and no key of a line they refuse.
pub fn take(call: &Call<Poster, Written, Literals>) -> Result<Json<ReceiptPayload>, Refusal> {
    let caps = call.state.first.caps;
    let topics = caps.secretary.topics;
    let chains = call.state.chain_stores();
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
    let nothing = |_: &[prodrome_core::event::Hash]| Ok(());
    match call.params.0 {
        WrittenName::Lines => {
            let conversations = caps.secretary.conversations;
            let keys = beside::<SlotKeys>(&call.body)?;
            let keep = |taken: &[prodrome_core::event::Hash]| -> Result<(), Refusal> {
                for name in taken {
                    for key in keys.of(name) {
                        conversations.keys.set(&key.name(), key)?;
                    }
                }
                Ok(())
            };
            let beside = routes::Beside {
                posted: &keys,
                keep: &keep,
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
            };
            let memberships = topics.memberships;
            routes::receive_changes(memberships, posted, &shared, frontiers, writer, recording)
        }
        WrittenName::TopicAliases => {
            let shared = routes::Beside {
                posted: &keyless()?,
                keep: &nothing,
            };
            let aliases = topics.aliases;
            routes::receive_changes(aliases, posted, &shared, frontiers, writer, recording)
        }
    }
    .map(Json)
}

/// Each conversation a line the box took is in told to its worker, so what the line starts (a
/// setup's opening note) is written.
fn woken(state: &super::State, posted: &[String], receipt: &ReceiptPayload) {
    let vocabulary = line_event::LineVocabulary;
    let conversations: std::collections::BTreeSet<_> = posted
        .iter()
        .filter_map(|literal| {
            let envelope =
                prodrome_core::event::parse_envelope::<line_event::LineEvent>(&vocabulary, literal)
                    .ok()?;
            let name = prodrome_core::event::seal_hash(&envelope);
            let taken = receipt.adopted.iter().any(|n| n == name.as_str());
            taken.then(|| envelope.event().map(|line| line.conversation().clone()))?
        })
        .collect();
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
