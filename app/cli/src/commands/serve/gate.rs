//! The gate, one layer in front of everything including the static files.
//!
//! It resolves the peer once and stashes it in the request's extensions, so write
//! routes never resolve their own.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::extract::{ConnectInfo, Request, State as AxumState};
use axum::http::uri::{Authority, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use suzatary_api::gate::{self, Door, Knock, Named};
use suzatary_base::capability::{Peer, Whois, WhoisError};
use suzatary_base::refusal::{Refusal, shown};
use suzatary_signatures::signatures::Admitted;

use super::State;
use super::reply::refused;

/// How long a whois answer is believed: one visit's worth of assets.
const TTL: Duration = Duration::from_mins(1);

/// What the daemon said about an address, and until when: its owner and whether it is this
/// box's own node.
///
/// One per server in [`State`], never a static, so two servers in one process
/// cannot answer each other's whois.
pub type Remembering = HashMap<IpAddr, (Peer, Instant)>;

fn peer_of(
    known: &Mutex<Remembering>,
    whois: &dyn Whois,
    address: IpAddr,
) -> Result<Peer, WhoisError> {
    let now = Instant::now();
    {
        let held = super::locks::held(known);
        if let Some((peer, until)) = held.get(&address)
            && *until > now
        {
            return Ok(peer.clone());
        }
    }
    // Looked up outside the lock so one slow or wedged daemon answer does not
    // serialise every request.
    let found = whois.peer(address)?;
    super::locks::held(known).insert(address, (found.clone(), now + TTL));
    Ok(found)
}

/// A [`Whois`] that answers from this server's own memo.
struct Remembered<'a>(&'a dyn Whois, &'a Mutex<Remembering>);

impl Whois for Remembered<'_> {
    fn peer(&self, address: IpAddr) -> Result<Peer, WhoisError> {
        peer_of(self.1, self.0, address)
    }
}

/// Where `app_url` says the app answers, read by http's own parser; `None` for a URL that names
/// no host or a scheme other than http and https.
#[must_use]
pub fn door(app_url: &str) -> Option<Door> {
    let uri: Uri = app_url.parse().ok()?;
    let scheme_port = match uri.scheme_str()? {
        "https" => 443,
        "http" => 80,
        _ => return None,
    };
    uri.authority()
        .map(|authority| Door::new(named(authority), scheme_port))
}

/// The door `app_url` names, refused when the gate could not read it or when it names a port
/// other than the one bound: either would answer every request 403.
///
/// # Errors
/// Either refusal, naming `app_url`.
pub fn door_at(app_url: &str, at: SocketAddr) -> anyhow::Result<suzatary_api::gate::Door> {
    let door = door(app_url).with_context(|| {
        format!(
            "[web] app_url {} names no http or https host for the gate to answer at",
            shown(app_url)
        )
    })?;
    anyhow::ensure!(
        door.port() == at.port(),
        "[web] app_url {} answers at port {}, and this server binds {at}: the gate would refuse every request",
        shown(app_url),
        door.port()
    );
    Ok(door)
}

fn named(authority: &Authority) -> Named<'_> {
    Named {
        host: authority.host(),
        port: authority.port_u16(),
    }
}

/// The layer. Admit, stash, or refuse.
pub async fn admit(
    AxumState(state): AxumState<State>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let address = peer.ip();
    // `Host` for HTTP/1.1, the request's own authority for HTTP/2.
    let addressed: Option<Authority> = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .or_else(|| request.uri().authority().cloned());
    let knock = Knock {
        peer: address,
        addressed: addressed.as_ref().map(named),
        forwarded: gate::PROXY_HEADERS
            .into_iter()
            .find(|header| request.headers().contains_key(*header)),
        operator: claim(request.headers()),
    };
    let admitted = gate::admit(
        &Remembered(state.first.caps.whois, &state.known),
        &state.deployment,
        &state.door,
        &knock,
    );
    match admitted {
        Err(refusal) => {
            // Named in the log, not the body, so a stranger learns no logins.
            tracing::warn!("refused {address}: {}", refusal.message());
            refused(&refusal).into_response()
        }
        Ok(who) => {
            request.extensions_mut().insert(who);
            next.run(request).await
        }
    }
}

/// The key and actor a request names as its operator's, where it names both.
fn claim(headers: &axum::http::HeaderMap) -> Option<gate::Claim<'_>> {
    let read = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    Some(gate::Claim {
        key: read(super::asked::KEY_HEADER)?,
        actor: read(gate::OPERATOR_HEADER)?,
    })
}

/// What the gate decided about this request. A request the gate never saw is refused, never
/// read as the box: a route mounted outside the layer must fail closed.
///
/// # Errors
/// [`Refusal::forbidden`] when nothing was stashed.
pub fn who(request: &axum::http::Extensions) -> Result<Admitted, Refusal> {
    request
        .get::<Admitted>()
        .cloned()
        .ok_or_else(|| Refusal::forbidden("this request did not pass the gate"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request the gate stashed nothing on is refused, not admitted as the box.
    #[test]
    fn a_request_the_gate_never_saw_is_refused() {
        let empty = axum::http::Extensions::new();
        assert_eq!(
            who(&empty).map_err(|refused| refused.status().code()),
            Err(403)
        );
        let mut seen = axum::http::Extensions::new();
        seen.insert(Admitted::Local);
        assert_eq!(who(&seen), Ok(Admitted::Local));
    }

    fn at(text: &str) -> SocketAddr {
        text.parse().expect("an address")
    }

    /// A door at a port other than the one bound, or one the gate cannot read, stops `serve`
    /// rather than answering every request 403.
    #[test]
    fn serve_starts_only_where_the_door_is() {
        let door = door_at("https://box.example.test", at("100.64.0.9:443")).expect("443");
        assert_eq!(door.to_string(), "box.example.test:443");
        assert!(door_at("http://127.0.0.1:8787/", at("127.0.0.1:8787")).is_ok());
        for (url, bound) in [
            ("https://box.example.test", "100.64.0.9:8787"),
            ("http://127.0.0.1:8787", "127.0.0.1:8788"),
            ("ftp://box.example.test", "100.64.0.9:21"),
            ("not a url", "100.64.0.9:443"),
        ] {
            assert!(door_at(url, at(bound)).is_err(), "{url} at {bound}");
        }
    }
}
