//! The mail's routes, read-only: the inbox, a search, one message and one thread.

use axum::Json;
use suzatary_api::{query, routes, wire};
use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;

use suzatary_core::mail::watch::{self, WatchId};
use suzatary_signatures::signatures::Tap;

use super::Chained;
use super::call::Call;
use super::guard::Reader;
use super::params::{Asked, Params};

/// One page of a list: which account, how many, and older than when.
pub struct Page {
    account: Option<String>,
    limit: usize,
    before: Option<Datetime>,
}

/// A notmuch query and the page of its answer.
pub struct Search {
    q: String,
    page: Page,
}

pub struct Message(String);

/// The thread a path names, and how many of its messages.
pub struct Thread {
    id: String,
    limit: usize,
}

impl Params for Page {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            account: asked.query("account").map(str::to_owned),
            limit: query::limit(asked.query("limit"))?,
            before: query::before(asked.query("before"))?,
        })
    }
}

impl Params for Search {
    fn parse(asked: &Asked, chain: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            q: query::search_query(asked.query("q"))?,
            page: Page::parse(asked, chain)?,
        })
    }
}

impl Params for Message {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self(asked.segment("id")?.to_owned()))
    }
}

pub struct Watched(WatchId);

impl Params for Watched {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        WatchId::new(asked.segment("id")?)
            .map(Self)
            .map_err(|refused| Refusal::not_found(refused.to_string()))
    }
}

impl Params for Thread {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        Ok(Self {
            id: asked.segment("id")?.to_owned(),
            limit: query::limit(asked.query("limit"))?,
        })
    }
}

/// `GET /api/mail/inbox`: the newest mail, by date.
pub fn inbox(call: &Call<Reader, Page, ()>) -> Result<Json<wire::InboxPayload>, Refusal> {
    let Page {
        account,
        limit,
        before,
    } = &call.params;
    routes::inbox(
        call.chain.caps.secretary.mail,
        account.as_deref(),
        *limit,
        *before,
    )
    .map(Json)
}

/// `GET /api/mail/search?q=`: a notmuch query, verbatim.
pub fn search(call: &Call<Reader, Search, ()>) -> Result<Json<wire::MailSearchPayload>, Refusal> {
    let Search { q, page } = &call.params;
    routes::search(
        call.chain.caps.secretary.mail,
        q,
        page.account.as_deref(),
        page.limit,
        page.before,
    )
    .map(Json)
}

/// `GET /api/mail/message/<id>`: one message, with the todos it sourced.
pub fn message(call: &Call<Reader, Message, ()>) -> Result<Json<wire::MailPage>, Refusal> {
    let caps = call.chain.caps;
    routes::message(
        caps.secretary.mail,
        caps.secretary.render,
        &call.chain.policy,
        &call.state.chains_read().set(caps.secretary.clock.now()),
        &call.params.0,
    )
    .map(Json)
}

/// `GET /api/mail/thread/<id>`: one thread, newest `?limit=` messages.
pub fn thread(call: &Call<Reader, Thread, ()>) -> Result<Json<wire::ThreadPayload>, Refusal> {
    let caps = call.chain.caps;
    routes::thread(
        caps.secretary.mail,
        caps.secretary.render,
        &call.chain.policy,
        &call.state.chains_read().set(caps.secretary.clock.now()),
        &call.params.id,
        call.params.limit,
    )
    .map(Json)
}

/// `GET /api/mail/watches`: every watch on expected mail that waits.
pub fn watches(
    call: &Call<Reader, (), ()>,
) -> Result<Json<suzatary_core::wire::triage::WatchesPayload>, Refusal> {
    let caps = call.chain.caps;
    let now = prodrome_core::fpl::instant_of(caps.secretary.clock.now());
    Ok(Json(watch::listed(caps.secretary.watches, now)?))
}

/// `POST /api/mail/watches/<id>/cancel`: the owner's tap closes a watch; answers the rest.
pub fn cancel_watch(
    call: &Call<Tap, Watched, ()>,
) -> Result<Json<suzatary_core::wire::triage::WatchesPayload>, Refusal> {
    let caps = call.chain.caps;
    let now = prodrome_core::fpl::instant_of(caps.secretary.clock.now());
    watch::cancel(caps.secretary.watches, &call.params.0, now)?;
    Ok(Json(watch::listed(caps.secretary.watches, now)?))
}
