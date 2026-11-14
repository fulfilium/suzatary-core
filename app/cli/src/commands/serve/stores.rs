//! The secretary's stores as replicas a tab holds; the box alone writes the inbox, the lines and
//! the asks, and a tab writes the topic stores, each object signed by its device.

use axum::Json;
use axum::response::{IntoResponse as _, Response};
use suzatary_api::server::Server;
use suzatary_api::wire::StoreName;
use suzatary_api::{query, routes, wire};
use suzatary_base::refusal::Refusal;
use suzatary_core::asks::AskEvent;
use suzatary_core::wire::lines::ReceiptPayload;
use suzatary_store::store::{Prodrome, StoreError};

use super::Chained;
use super::call::Call;
use super::chain::Literals;
use super::guard::{Poster, Reader};
use super::params::{Asked, Params};
use super::reply::Immutable;

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

/// A store a tab writes: one of the topic stores.
pub enum Written {
    Topics,
    TopicAliases,
}

impl Params for Written {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        match named(asked.segment("store")?)? {
            StoreName::Topics => Ok(Self::Topics),
            StoreName::TopicAliases => Ok(Self::TopicAliases),
            StoreName::Inbox | StoreName::Lines | StoreName::Asks => {
                Err(Refusal::method_not_allowed(
                    "the box alone writes the inbox, the lines and the asks",
                ))
            }
        }
    }
}

/// `POST /api/stores/<store>/objects`: a topic store's changes, each taken only beside a trusted signature.
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
    match call.params {
        Written::Topics => {
            routes::receive_changes(topics.memberships, posted, frontiers, writer, recording)
        }
        Written::TopicAliases => {
            routes::receive_changes(topics.aliases, posted, frontiers, writer, recording)
        }
    }
    .map(Json)
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
) -> Result<Json<suzatary_core::wire::lines::SkeletonPayload>, Refusal> {
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
    let inbox = suzatary_core::secretary::inbox::fold(caps.secretary.inbox)?;
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
