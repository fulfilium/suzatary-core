//! The one table of every route this server mounts.

use axum::routing::get;

use super::route::{Route, Scope};
use super::{chain, google, held, mail, proposals, secretary, settings, stores, stream, the_box};

/// Every route.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one entry per route, so the table reads against the path strings"
)]
pub fn table() -> Vec<Route> {
    vec![
        Route::get("/chain", Scope::Chain, chain::chain),
        Route::get("/tip", Scope::Chain, chain::tip),
        Route::get("/fold", Scope::Chain, chain::fold),
        Route::get("/todo/{id}", Scope::Chain, chain::todo),
        // Files a card and fires nothing: the owner's tap on that card starts the session.
        Route::post("/todo/{id}/build", Scope::Chain, chain::build),
        Route::get("/tree", Scope::Chain, chain::tree),
        Route::get("/me", Scope::Chain, chain::me),
        // Replica sync: the GET is what the caller lacks, the POST its objects, rehashed.
        Route::get("/objects", Scope::Chain, chain::objects),
        // An object's literal by its name, kept for good by the browser: it never changes.
        Route::get("/content", Scope::Chain, chain::content),
        Route::post("/objects", Scope::Chain, chain::take_objects),
        // Appends nothing: the answer is an event's print, which the phone seals.
        Route::post("/author", Scope::Chain, chain::author),
        Route::special("/events", Scope::Box, get(stream::events)),
        Route::special("/events", Scope::Named, get(stream::events)),
        Route::get("/api/briefings", Scope::Box, the_box::briefings),
        Route::get("/api/briefings/{date}", Scope::Box, the_box::briefing),
        Route::get("/api/calendar/{date}", Scope::Box, the_box::calendar),
        Route::get("/api/presence", Scope::Box, the_box::presence),
        Route::get("/api/about", Scope::Box, the_box::about),
        Route::get("/api/releases", Scope::Box, the_box::releases),
        Route::get("/api/portal", Scope::Box, the_box::portal),
        // A browser's failure: a write's gate, a per-login budget, one log line, nothing stored.
        Route::post("/api/client-errors", Scope::Box, the_box::client_errors),
        Route::get("/api/usage", Scope::Box, the_box::usage),
        Route::post("/api/act/device", Scope::Box, the_box::device),
        // A device asks to be trusted; the tap on its card, from a trusted device, trusts it.
        Route::post("/api/act/key", Scope::Box, the_box::key),
        Route::post("/api/act/pair", Scope::Box, the_box::pair),
        Route::post("/api/held/reveal", Scope::Box, held::reveal),
        // A GET under /api/act/ is 405 here, or the catch-all would answer the index.
        Route::special("/api/act/{*verb}", Scope::Box, get(the_box::act_read)),
        // What is connected; a change to it is a card, tapped through the route below.
        Route::get("/api/overview", Scope::Box, settings::overview),
        // Google sends the owner's browser back here from its consent page, so it is a GET with
        // no CSRF triple: the state one tap drew, taken once and only for that tap's actor, is
        // what admits it.
        Route::get(
            suzatary_core::google::CALLBACK,
            Scope::Box,
            google::callback,
        ),
        // A sign-in card's button: the browser is sent on to the consent its tap opened.
        Route::get(
            "/api/oauth/google/consent/{card}",
            Scope::Box,
            google::consent,
        ),
        Route::get("/api/mail/inbox", Scope::Box, mail::inbox),
        Route::get("/api/mail/search", Scope::Box, mail::search),
        Route::get("/api/mail/message/{id}", Scope::Box, mail::message),
        Route::get("/api/mail/thread/{id}", Scope::Box, mail::thread),
        Route::get("/api/mail/watches", Scope::Box, mail::watches),
        Route::post(
            "/api/mail/watches/{id}/cancel",
            Scope::Box,
            mail::cancel_watch,
        ),
        // The secretary's stores, as replicas a tab pulls; a tab posts only to the topic stores.
        Route::get("/api/stores/{store}/objects", Scope::Box, stores::objects),
        Route::post("/api/stores/{store}/objects", Scope::Box, stores::take),
        Route::get("/api/stores/{store}/content", Scope::Box, stores::content),
        Route::get("/api/stores/lines/skeleton", Scope::Box, stores::skeleton),
        Route::special("/api/stores/events", Scope::Box, get(stream::store_events)),
        // The Inbox badge, counted on the box so a tab need not hold the conversations.
        Route::get("/api/waiting", Scope::Box, stores::waiting),
        Route::post(
            "/api/secretary/conversations/{conversation}/proposals/{index}/tap",
            Scope::Box,
            proposals::tap,
        ),
        Route::post(
            "/api/secretary/conversations/{conversation}/proposals/{index}/dismiss",
            Scope::Box,
            proposals::dismiss,
        ),
        // Drawn against the todo chains, which a tab does not hold whole: every proposal it
        // names in one read.
        Route::get(
            "/api/secretary/conversations/{conversation}/drawn",
            Scope::Box,
            proposals::drawn,
        ),
        // A send that may have gone waits until the owner has checked Sent.
        Route::post(
            "/api/secretary/conversations/{conversation}/proposals/{index}/acknowledge",
            Scope::Box,
            proposals::acknowledge,
        ),
        Route::post(
            "/api/secretary/conversations/{conversation}/proposals/{index}/drop",
            Scope::Box,
            proposals::drop_session,
        ),
        Route::post("/api/secretary/conversations", Scope::Box, secretary::open),
        Route::post(
            "/api/secretary/conversations/{conversation}/title",
            Scope::Box,
            secretary::rename,
        ),
        // An erasure is answered by the same gate as a send: a witness that the owner asked.
        Route::post(
            "/api/secretary/conversations/{conversation}/erase",
            Scope::Box,
            secretary::erase,
        ),
        Route::special(
            "/api/secretary/conversations/{conversation}/stream",
            Scope::Box,
            get(stream::conversation),
        ),
        Route::post(
            "/api/secretary/conversations/{conversation}/say",
            Scope::Box,
            secretary::say,
        ),
        Route::post(
            "/api/secretary/conversations/{conversation}/stop",
            Scope::Box,
            secretary::stop,
        ),
        Route::post(
            "/api/secretary/conversations/{conversation}/regenerate",
            Scope::Box,
            secretary::regenerate,
        ),
        Route::post(
            "/api/secretary/conversations/{conversation}/edit",
            Scope::Box,
            secretary::edit,
        ),
        // Named by its sha, never by text: rendering is asked by content.
        Route::get(
            "/api/secretary/conversations/{conversation}/blocks/{sha}",
            Scope::Box,
            secretary::block,
        ),
        Route::post("/api/secretary/attachments", Scope::Box, secretary::upload),
        Route::get(
            "/api/secretary/attachments/{id}",
            Scope::Box,
            secretary::photo,
        ),
        Route::get("/api/secretary/memory", Scope::Box, secretary::memory),
        Route::post(
            "/api/secretary/memory/{id}/forget",
            Scope::Box,
            secretary::forget,
        ),
    ]
}

/// Every mounted route as `METHOD path guard`, sorted by path, then method.
#[must_use]
pub fn surface() -> Vec<String> {
    let mut lines: Vec<String> = table().iter().flat_map(Route::listed).collect();
    lines.sort_by_cached_key(|line| {
        let mut words = line.split(' ');
        let method = words.next().unwrap_or_default().to_owned();
        (words.next().unwrap_or_default().to_owned(), method)
    });
    lines
}
