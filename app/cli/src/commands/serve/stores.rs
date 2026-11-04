//! The secretary's stores as replicas a tab holds. The inbox and the lines are written only by
//! the box, so a tab only pulls them; the lines' pull carries the keys of the words it sends.
//! Literals are asked for by hash. The two topic stores are written by a tab too.

use axum::Json;
use suzatary_core::capability::Server;
use suzatary_core::refusal::Refusal;
use suzatary_core::store::StoreError;
use suzatary_core::wire::StoreName;
use suzatary_core::{query, routes, wire};

use super::Chained;
use super::call::Call;
use super::chain::Literals;
use super::guard::Reader;
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
            suzatary_core::refusal::Status::UpgradeRequired,
            "the conversations are read from the lines now: a new version of this page is ready, and Reload loads it",
        ));
    }
    Err(Refusal::not_found(format!(
        "a tab holds the inbox, the lines, the topics and the topic aliases, not {}",
        suzatary_core::refusal::shown(segment)
    )))
}

/// A store's heads, sorted.
///
/// # Errors
/// The store's own.
pub fn heads(store: StoreName, caps: &Server<'_>) -> Result<Vec<String>, StoreError> {
    let tips = match store {
        StoreName::Inbox => caps.secretary.inbox.tips()?,
        StoreName::Lines => caps.secretary.conversations.lines.tips()?,
        StoreName::Topics => caps.secretary.topics.memberships.tips()?,
        StoreName::TopicAliases => caps.secretary.topics.aliases.tips()?,
    };
    let mut heads: Vec<String> = tips.iter().map(|h| h.as_str().to_owned()).collect();
    heads.sort();
    Ok(heads)
}

/// A store, and the frontier a replica of it holds.
pub struct Since {
    store: StoreName,
    since: prodrome_core::store::frontier::Since,
}

impl Params for Since {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            store: named(asked.segment("store")?)?,
            since: query::since(asked.query("since"))?,
        })
    }
}

/// `GET /api/stores/<store>/objects?since=<frontier>`: a piece of what a replica holding that
/// frontier lacks, and how much is left after it.
pub fn objects(call: &Call<Reader, Since, ()>) -> Result<Json<wire::StorePayload>, Refusal> {
    let caps = call.state.first.caps;
    let Since { store, since } = &call.params;
    let (books, now) = (call.state.books(), caps.secretary.clock.now());
    let cut = routes::Cut::PIECE;
    match store {
        StoreName::Inbox => {
            routes::store_objects_since(caps.secretary.inbox, since, books, now, cut)
        }
        StoreName::Lines => {
            routes::lines_since(caps.secretary.conversations, since, books, now, cut)
        }
        StoreName::Topics => {
            routes::store_objects_since(caps.secretary.topics.memberships, since, books, now, cut)
        }
        StoreName::TopicAliases => {
            routes::store_objects_since(caps.secretary.topics.aliases, since, books, now, cut)
        }
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
            StoreName::Inbox | StoreName::Lines => Err(Refusal::method_not_allowed(
                "the box alone writes the inbox and the lines",
            )),
        }
    }
}

/// `POST /api/stores/<store>/objects`: a topic store replica's changes, each written as the
/// actor the request was proven for.
pub fn take(
    call: &Call<prodrome_core::event::Actor, Written, Literals>,
) -> Result<Json<wire::ReceiptPayload>, Refusal> {
    let topics = call.state.first.caps.secretary.topics;
    let (posted, actor, frontiers) = (&call.body.posted(), &call.guard, &call.state.frontiers);
    match call.params {
        Written::Topics => routes::receive_changes(topics.memberships, posted, frontiers, actor),
        Written::TopicAliases => routes::receive_changes(topics.aliases, posted, frontiers, actor),
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
) -> Result<Json<wire::SkeletonPayload>, Refusal> {
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

/// `GET /api/stores/<store>/content?hashes=<hashes>`: those objects' literals, kept for good.
pub fn content(
    call: &Call<Reader, Wanted, ()>,
) -> Result<Immutable<Json<wire::ContentPayload>>, Refusal> {
    let caps = call.state.first.caps;
    let Wanted { store, hashes } = &call.params;
    match store {
        StoreName::Inbox => routes::content(caps.secretary.inbox, hashes),
        StoreName::Lines => routes::content(caps.secretary.conversations.lines, hashes),
        StoreName::Topics => routes::content(caps.secretary.topics.memberships, hashes),
        StoreName::TopicAliases => routes::content(caps.secretary.topics.aliases, hashes),
    }
    .map(|answer| Immutable(Json(answer)))
}

/// `GET /api/waiting`: the Inbox badge's count, so a tab shows it without pulling the
/// conversations.
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
    routes::waiting(&inbox, caps.secretary.conversations.lines, &set, &chains).map(Json)
}
