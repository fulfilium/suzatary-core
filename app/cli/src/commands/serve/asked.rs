//! Signed requests (SPEC law 42): a request is named by its method, target, device instant and
//! body digest, signed by a device key, and proven against every chain the route can write, so a
//! revoked key writes through no route.
//!
//! A request is performed once: its record and answer are kept under its name, and a resend is
//! answered from there with no new effect. Its instant must be within
//! `signatures::WINDOW_SECONDS` of the box's UTC clock, so an answer need not be kept past twice
//! that.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use prodrome_core::event::Hash;
use prodrome_core::sign::PublicKey;
use suzatary_core::gate::{actor_of, writer_of};
use suzatary_core::record::TodoEvent;
use suzatary_core::refusal::Refusal;
use suzatary_core::signatures::Admitted;
use suzatary_core::signatures::{Answer, Asking, Proven, Request, Requested, refused};
use suzatary_core::store::Prodrome;

use super::State;
use super::guard::{Asks, Proof};
use super::route::blocking;

/// The header naming the device key that signed the request.
pub const KEY_HEADER: &str = "x-suzatary-key";
/// The header carrying its signature of the request's name.
pub const SIGNATURE_HEADER: &str = "x-suzatary-signature";
/// The header carrying the instant its device stamped on it, RFC 3339.
pub const AT_HEADER: &str = "x-suzatary-at";
/// The header carrying the nonce its device drew for it, which the box never records.
pub const NONCE_HEADER: &str = "x-suzatary-nonce";
/// The header a replayed answer carries, so a caller can tell it was answered from the record.
pub const REPLAYED_HEADER: &str = "x-suzatary-replayed";
/// The largest answer kept: an act's answer is a payload, not a document.
const ANSWER_CAP: usize = 16 << 20;

/// The names being performed now, so a request sent twice at once is performed once.
pub type InFlight = Arc<Mutex<BTreeSet<Hash>>>;

/// The body, read whole where a signature covers it, and the door's proof `P` of the request.
/// `trusting` is every chain the route can write.
///
/// # Errors
/// A body over its cap, a request dated outside the window, or one its signature does not prove,
/// in the door's words.
pub async fn asked<P: Proof>(
    state: &State,
    admitted: &Admitted,
    trusting: Vec<&'static dyn Prodrome<TodoEvent>>,
    parts: &Parts,
    body: Body,
    cap: usize,
) -> Result<(Body, P), Refusal> {
    let root = state.deployment.chains.first().root.clone();
    if P::ASKS == Asks::Nothing {
        read_by_a_kept_key(state, admitted, &trusting, &root, parts).await?;
        let proof = P::proved(None, &trusting, &root).map_err(|why| refused(&why))?;
        return Ok((body, proof));
    }
    let bytes = axum::body::to_bytes(body, cap)
        .await
        .map_err(|_| Refusal::bad_request(format!("the body is capped at {cap} bytes")))?;
    let header = |name: &str| {
        parts
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
    };
    let target = parts
        .uri
        .path_and_query()
        .map_or_else(|| parts.uri.path(), |asked| asked.as_str());
    let asking = Asking::of(
        &Request {
            method: parts.method.as_str(),
            target,
            at: header(AT_HEADER),
            nonce: header(NONCE_HEADER),
            body: &bytes,
            key: header(KEY_HEADER),
            signature: header(SIGNATURE_HEADER),
        },
        writer_of(&state.deployment, admitted)?,
        (state.wall)(),
    )?;
    let proof =
        blocking(move || P::proved(Some(asking), &trusting, &root).map_err(|why| refused(&why)))
            .await?;
    Ok((Body::from(bytes), proof))
}

/// A signed read whose signature proves a key revoked for the gate's actor is answered
/// [`Refusal::wipe`], on which the tab deletes what it holds. An unsigned or unverified read is a
/// read by the login alone, so only a device that proves it holds the revoked key is wiped.
async fn read_by_a_kept_key(
    state: &State,
    admitted: &Admitted,
    trusting: &[&'static dyn Prodrome<TodoEvent>],
    root: &PublicKey,
    parts: &Parts,
) -> Result<(), Refusal> {
    let header = |name: &str| {
        parts
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
    };
    if header(KEY_HEADER).is_none() {
        return Ok(());
    }
    let target = parts
        .uri
        .path_and_query()
        .map_or_else(|| parts.uri.path(), |asked| asked.as_str());
    let Ok(actor) = actor_of(&state.deployment, admitted) else {
        return Ok(());
    };
    let Ok(asking) = Asking::of(
        &Request {
            method: parts.method.as_str(),
            target,
            at: header(AT_HEADER),
            nonce: header(NONCE_HEADER),
            body: b"",
            key: header(KEY_HEADER),
            signature: header(SIGNATURE_HEADER),
        },
        actor,
        (state.wall)(),
    ) else {
        return Ok(());
    };
    let chains = trusting.to_vec();
    let root = root.clone();
    blocking(move || match asking.revoked(&chains, &root)? {
        Some(why) => Err(refused(&why)),
        None => Ok(()),
    })
    .await
}

/// The answer already given to this request, as it was given, if the box performed it; a
/// conflict where it was performed and its answer is no longer kept here.
///
/// # Errors
/// The record store unreadable.
pub async fn answered(state: &State, asking: &Performing) -> Result<Option<Response>, Refusal> {
    let requests = Arc::clone(&state.requests);
    let name = asking.name.clone();
    let (answer, performed) = blocking(move || {
        Ok(match requests.answer(&name)? {
            Some(answer) => (Some(answer), true),
            None => (None, requests.get(&name)?.is_some()),
        })
    })
    .await?;
    if performed && answer.is_none() {
        return Err(Refusal::conflict(
            "this request was performed already, and its answer is no longer kept",
        ));
    }
    let Some(answer) = answer else {
        return Ok(None);
    };
    let status = StatusCode::from_u16(answer.status).map_err(|_| {
        Refusal::internal(format!("a kept answer holds no status: {}", answer.status))
    })?;
    let mut response = (status, answer.body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(REPLAYED_HEADER, HeaderValue::from_static("1"));
    Ok(Some(response))
}

/// A request being performed: held while its route runs, so the same request arriving meanwhile
/// is refused.
pub struct Performing {
    record: Requested,
    name: Hash,
    device: PublicKey,
    state: State,
}

impl Performing {
    #[must_use]
    pub fn device(&self) -> &PublicKey {
        &self.device
    }

    /// Claim the request's name.
    ///
    /// # Errors
    /// A conflict, when the same request is being performed now.
    pub fn claim(state: &State, proven: &Proven) -> Result<Self, Refusal> {
        let record = proven.record().clone();
        let name = Hash::new(record.name.clone()).map_err(|e| Refusal::internal(e.to_string()))?;
        if !super::locks::held(&state.performing).insert(name.clone()) {
            return Err(Refusal::conflict(
                "this request is being performed now; its answer comes to the first asking",
            ));
        }
        Ok(Self {
            record,
            name,
            device: proven.device().clone(),
            state: state.clone(),
        })
    }

    /// Keep the [`Answer`] and the request's record under its name, and hand the answer on
    /// unchanged.
    ///
    /// # Errors
    /// An answer too large to keep, or the record store refusing it.
    pub async fn kept(mut self, response: Response, wrote: Vec<Hash>) -> Result<Response, Refusal> {
        let requests = Arc::clone(&self.state.requests);
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, ANSWER_CAP)
            .await
            .map_err(|_| Refusal::internal("an answer too large to keep"))?;
        let answer = Answer::of(parts.status.as_u16(), &bytes);
        self.record.status = answer.status;
        self.record.wrote = wrote.iter().map(|name| name.as_str().to_owned()).collect();
        let record = self.record.clone();
        blocking(move || Ok(requests.keep(&record, &answer)?)).await?;
        Ok(Response::from_parts(parts, Body::from(bytes)))
    }
}

impl Drop for Performing {
    fn drop(&mut self) {
        super::locks::held(&self.state.performing).remove(&self.name);
    }
}
