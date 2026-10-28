//! A chain's routes: its objects, its folds, one todo, and who is reading it.

use axum::Json;
use prodrome_core::event::{Actor, TodoId};
use suzatary_core::capability::Datetime;
use suzatary_core::reference::TodoRef;
use suzatary_core::refusal::{Refusal, Status};
use suzatary_core::routes;
use suzatary_core::secretary::acts::{self, ActLine};
use suzatary_core::secretary::store::DEFAULT;
use suzatary_core::secretary::tap::Tap;
use suzatary_core::{query, wire};

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::{Poster, Reader};
use super::params::{Asked, Params};
use super::reply::Immutable;
use super::{Chained, State};

/// `?t=` and `?limit=` of a fold.
pub struct Folding {
    at: Datetime,
    limit: Option<usize>,
}

/// One todo, at `?t=`.
pub struct TodoAt {
    id: String,
    at: Datetime,
}

/// `?t=` and `?by=` of the tree.
pub struct Treeing {
    at: Datetime,
    by: wire::TreeBy,
}

/// `?since=<frontier>`: what a replica already holds.
pub struct Since(prodrome_core::store::frontier::Since);

/// `?hashes=`: the objects asked for by name.
pub struct Wanted(query::Hashes);

/// A replica's objects, each a canonical literal, refused one by one later, and the frontier it
/// held before them.
pub struct Literals {
    pub(super) objects: Vec<String>,
    pub(super) since: prodrome_core::store::frontier::Since,
}

impl Literals {
    /// The post as the routes read it.
    pub(super) fn posted(&self) -> routes::Posted<'_> {
        routes::Posted {
            objects: &self.objects,
            since: &self.since,
        }
    }
}

/// A replica's post is a whole chain segment, not one action line.
const MAX_OBJECTS_BYTES: usize = 4_194_304;

fn moment(asked: &Asked, chain: &Chained) -> Result<Datetime, Refusal> {
    query::moment("t", asked.query("t"), chain.caps.secretary.clock.now())
}

impl Params for Folding {
    fn parse(asked: &Asked, chain: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            at: moment(asked, chain)?,
            limit: query::fold_limit(asked.query("limit"))?,
        })
    }
}

impl Params for TodoAt {
    fn parse(asked: &Asked, chain: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            id: asked.segment("id")?.to_owned(),
            at: moment(asked, chain)?,
        })
    }
}

/// The todo the path names, on the chain it names.
impl Params for TodoRef {
    fn parse(asked: &Asked, chain: &Chained) -> Result<Self, Refusal> {
        let text = asked.segment("id")?;
        Ok(Self {
            chain: chain.policy.name.clone(),
            id: TodoId::new(text)
                .map_err(|_| Refusal::not_found(format!("{text:?} is not a todo id")))?,
        })
    }
}

impl Params for Treeing {
    fn parse(asked: &Asked, chain: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            at: moment(asked, chain)?,
            by: query::tree_by(asked.query("by"))?,
        })
    }
}

impl Params for Since {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        query::since(asked.query("since")).map(Self)
    }
}

impl Params for Wanted {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        query::Hashes::parse(asked.query("hashes")).map(Self)
    }
}

impl Body for Literals {
    const CAP: usize = MAX_OBJECTS_BYTES;

    type Wire = wire::PostedObjects;

    fn parse(posted: wire::PostedObjects, _: &State) -> Result<Self, Refusal> {
        Ok(Self {
            since: query::since(Some(&posted.since))?,
            objects: posted.objects,
        })
    }
}

impl Body for wire::Authoring {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(asked: Self, _: &State) -> Result<Self, Refusal> {
        Ok(asked)
    }
}

/// `GET …/chain`: every object, with the policy it is read under.
pub fn chain(call: &Call<Reader, (), ()>) -> Result<Json<wire::ChainPayload>, Refusal> {
    routes::chain(call.chain.caps.secretary.chain, &call.chain.policy).map(Json)
}

/// `GET …/content?hashes=<hashes>`: the literals of those objects of the chain, kept for good
/// by whoever asked.
pub fn content(
    call: &Call<Reader, Wanted, ()>,
) -> Result<Immutable<Json<wire::ContentPayload>>, Refusal> {
    routes::content(call.chain.caps.secretary.chain, &call.params.0)
        .map(|answer| Immutable(Json(answer)))
}

/// `GET …/tip`: the heads a replica compares against.
pub fn tip(call: &Call<Reader, (), ()>) -> Result<Json<wire::TipPayload>, Refusal> {
    routes::tip(call.chain.caps.secretary.chain).map(Json)
}

/// `GET …/fold`: every todo as the folds see it at `?t=`.
pub fn fold(call: &Call<Reader, Folding, ()>) -> Result<Json<wire::FoldPayload>, Refusal> {
    let caps = call.chain.caps;
    routes::fold(
        caps.secretary.render,
        &call.chain.policy.name,
        &call.chain.policy.untrusted,
        &call.state.chains_read().set(call.params.at),
        call.params.limit,
    )
    .map(Json)
}

/// `GET …/todo/<id>`: one todo, its reading page included.
pub fn todo(call: &Call<Reader, TodoAt, ()>) -> Result<Json<wire::TodoPayload>, Refusal> {
    let caps = call.chain.caps;
    // A chain no session builds offers no button; a plan that does not read is the box's fault.
    let builder = match call.state.deployment.builder(&call.chain.policy.name) {
        Ok(routine) => Some(routine),
        Err(refused) if refused.status() == Status::Conflict => None,
        Err(refused) => return Err(refused),
    };
    routes::todo(
        caps.secretary.render,
        &call.chain.policy,
        call.state.deployment.zone,
        &call.state.chains_read().set(call.params.at),
        &call.params.id,
        builder,
    )
    .map(Json)
}

/// `POST …/todo/<id>/build`: the owner asks for the todo to be built, and the box files the
/// coding-session card that serves it; only his tap on that card fires a session.
pub fn build(call: &Call<Tap, TodoRef, ()>) -> Result<Json<wire::ConversationPayload>, Refusal> {
    call.state
        .with_cards(|caps, cards| {
            suzatary_core::secretary::routes::build(caps, &call.guard, cards, &call.params)
        })
        .map(Json)
}

/// `GET …/tree`: the open list as a tree.
pub fn tree(call: &Call<Reader, Treeing, ()>) -> Result<Json<wire::TreePayload>, Refusal> {
    routes::tree(
        &call.chain.policy,
        &call.state.chains_read().set(call.params.at),
        call.params.by,
    )
    .map(Json)
}

/// `GET …/me`: the login the gate named, and the actor it writes as, if any.
pub fn me(call: &Call<Reader, (), ()>) -> Json<wire::MePayload> {
    Json(routes::me(
        &call.state.deployment,
        &call.chain.policy,
        &call.admitted,
    ))
}

/// `GET …/objects?since=<frontier>`: what this replica does not hold.
pub fn objects(call: &Call<Reader, Since, ()>) -> Result<Json<wire::ObjectsPayload>, Refusal> {
    routes::objects_since(
        call.chain.caps.secretary.chain,
        &call.chain.policy,
        &call.params.0,
        call.state.books(),
        call.state.first.caps.secretary.clock.now(),
    )
    .map(Json)
}

/// `POST …/objects`: a replica handing its objects over, sealed as the gate's actor.
pub fn take_objects(
    call: &Call<Poster, (), Literals>,
) -> Result<Json<wire::ReceiptPayload>, Refusal> {
    let caps = call.chain.caps;
    let (receipt, heard) = routes::receive_objects(
        caps.secretary.chain,
        caps.secretary.render,
        &call.chain.policy,
        &call.body.posted(),
        &call.state.frontiers,
        &call.guard.0,
    )?;
    // Adoption is the one moment a sealed lifecycle event's note is known here.
    if !heard.is_empty() {
        noted(
            &call.state,
            &heard,
            call.state.first.caps.secretary.clock.now(),
        );
    }
    Ok(Json(receipt))
}

/// `POST …/author`: the authoring, without the append; the write's decision, so the
/// write's guard.
pub fn author(
    call: &Call<Actor, (), wire::Authoring>,
) -> Result<Json<wire::AuthoredPayload>, Refusal> {
    let asked = &call.body;
    let built = routes::author(
        call.chain.caps,
        &call.state.deployment,
        &call.chain.policy,
        asked,
        &call.guard,
    )?;
    let noted_as = match asked {
        wire::Authoring::Update(update) => Some(("updated", &update.note)),
        wire::Authoring::Confirm(confirm) => Some(("confirmed", &confirm.note)),
        wire::Authoring::Add(_) | wire::Authoring::Undo(_) | wire::Authoring::Redo(_) => None,
    };
    // The one moment a person's line for `update` or `confirm` is known server-side.
    if let Some((verb, note)) = noted_as
        && !note.trim().is_empty()
        && let Ok(id) = TodoId::new(built.todo.clone())
    {
        let reference =
            suzatary_core::reference::Reference::Todo(suzatary_core::reference::TodoRef {
                chain: call.chain.policy.name.clone(),
                id,
            });
        let now = call.state.first.caps.secretary.clock.now();
        let line = ActLine {
            at: now,
            actor: call.guard.as_str().to_owned(),
            verb,
            reference,
            note: note.clone(),
        };
        noted(&call.state, &[line], now);
    }
    Ok(Json(built))
}

/// Append acts to the default conversation, opening it if nobody has, and hear it.
///
/// A failure to note is not the write's failure, so it is a line in the log, not the answer's.
fn noted(state: &State, lines: &[ActLine], now: Datetime) {
    let first = state.first.caps;
    let noting = suzatary_core::secretary::read_or_open_default(
        first.secretary.conversations,
        first.secretary.model,
        &DEFAULT,
        now,
    )
    .and_then(|_| {
        let read = state.chains_read();
        let set = read.set(now);
        lines.iter().try_for_each(|line| {
            let by = suzatary_core::secretary::stamp::pointing(
                &line.actor,
                std::slice::from_ref(&line.reference),
                &set,
            );
            acts::note_act(first.secretary.conversations, &DEFAULT, line, by)
        })
    });
    if let Err(failed) = noting {
        tracing::warn!("act: written, and not noted in the conversation: {failed}");
    }
    state.heard(&DEFAULT);
}
