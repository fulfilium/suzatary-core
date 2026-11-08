//! The box's own routes, no chain's.

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use prodrome_core::event::Actor;
use suzatary_core::clienterror::{MAX_BODY_BYTES, Report, Sent};
use suzatary_core::refusal::Refusal;
use suzatary_core::release::Plan;
use suzatary_core::{devices, routes, wire};

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::{Asker, Mapped, Reader, Reporter};
use super::params::{Asked, Params};
use super::reply::refused;
use super::{Chained, State};

/// The day a path names, `YYYY-MM-DD`.
pub struct Day(chrono::NaiveDate);

impl Params for Day {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let raw = asked.segment("date")?;
        chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .map(Self)
            .map_err(|_| {
                Refusal::bad_request(format!(
                    "a date is ISO (YYYY-MM-DD), got {}",
                    suzatary_core::refusal::shown(raw)
                ))
            })
    }
}

impl Body for wire::Registering {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = Self;

    fn parse(registering: Self, _: &State) -> Result<Self, Refusal> {
        Ok(registering)
    }
}

impl Body for Report {
    const CAP: usize = MAX_BODY_BYTES;

    type Wire = Sent;

    fn parse(sent: Sent, _: &State) -> Result<Self, Refusal> {
        Report::of(&sent)
    }
}

/// `GET /api/briefings`: every day that has one.
pub fn briefings(call: &Call<Reader, (), ()>) -> Result<Json<wire::BriefingsPayload>, Refusal> {
    let caps = call.chain.caps;
    routes::briefings(caps.secretary.briefings, caps.secretary.conversations).map(Json)
}

/// `GET /api/briefings/<date>`: one day's, block by block.
pub fn briefing(call: &Call<Reader, Day, ()>) -> Result<Json<wire::BriefingPayload>, Refusal> {
    let caps = call.chain.caps;
    routes::briefing(
        caps.secretary.briefings,
        caps.secretary.render,
        caps.secretary.conversations,
        caps.answers,
        call.params.0,
    )
    .map(Json)
}

/// `GET /api/calendar/<date>`: a week of the owner's calendars from that day.
pub fn calendar(call: &Call<Reader, Day, ()>) -> Result<Json<wire::CalendarPayload>, Refusal> {
    let caps = call.chain.caps;
    routes::calendar(
        caps.secretary.calendar,
        caps.secretary.render,
        call.params.0,
    )
    .map(Json)
}

/// `GET /api/presence`: a writer's open page quiets the phone.
pub fn presence(call: &Call<Mapped, (), ()>) -> StatusCode {
    call.state.presence.seen();
    StatusCode::NO_CONTENT
}

/// `GET /api/about`: the version and build this process is.
pub fn about(call: &Call<Reader, (), ()>) -> Result<Json<wire::AboutPayload>, Refusal> {
    let now = call.chain.caps.secretary.clock.now();
    call.state
        .stamp
        .about(&call.state.chains_read().set(now))
        .map(Json)
}

/// `GET /api/portal`: how this box wakes the Android portal.
pub fn portal(call: &Call<Reader, (), ()>) -> Json<wire::PortalPayload> {
    Json(wire::PortalPayload::from(call.state.portal.as_ref()))
}

/// `GET /api/releases`: the releases this build's plan declares.
pub fn releases(call: &Call<Reader, (), ()>) -> Result<Json<wire::ReleasesPayload>, Refusal> {
    let plan = Plan::declared().map_err(|e| Refusal::internal(e.to_string()))?;
    let now = call.chain.caps.secretary.clock.now();
    routes::releases(&plan, &call.state.chains_read().set(now)).map(Json)
}

/// `POST /api/client-errors`: a tab's uncaught failure, logged as one `warn` line.
pub fn client_errors(call: &Call<Reporter, (), Report>) -> StatusCode {
    tracing::warn!("{}", call.body.line(&call.guard.0));
    StatusCode::NO_CONTENT
}

/// `POST /api/act/device`: the phone saying "wake me here"; nothing on the chain.
pub fn device(
    call: &Call<Actor, (), wire::Registering>,
) -> Result<Json<wire::DevicePayload>, Refusal> {
    let caps = call.chain.caps;
    devices::register_device(caps.devices, caps.secretary.clock, &call.body, &call.guard).map(Json)
}

/// A device's public key, as `POST /api/act/key` carries it.
pub struct DeviceKey(prodrome_core::sign::PublicKey);

impl Body for DeviceKey {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = wire::KeyAsked;

    fn parse(asked: wire::KeyAsked, _: &State) -> Result<Self, Refusal> {
        prodrome_core::sign::PublicKey::new(&asked.key)
            .map(Self)
            .map_err(|e| Refusal::bad_request(format!("key: {e}")))
    }
}

/// `POST /api/act/key`: a device asking to be trusted to write as the gate's actor. Files one
/// Inbox card; trusting is the tap on it from a device already trusted.
pub fn key(call: &Call<Asker, (), DeviceKey>) -> Result<Json<serde_json::Value>, Refusal> {
    // The request is signed by the key it asks for, so nobody files a card for a key they do
    // not hold.
    if call.guard.1 != call.body.0 {
        return Err(Refusal::forbidden(
            "a device asks for its own key: this request is signed by another",
        ));
    }
    let caps = call.state.first.caps;
    let index = suzatary_core::secretary::routes::ask_trust(caps, &call.guard.0, &call.body.0)?;
    call.state.heard(&suzatary_core::conversation_id::DEFAULT);
    Ok(Json(serde_json::json!({ "proposed": index })))
}

/// A pairing code, as `POST /api/act/pair` carries it.
pub struct PairingCode(suzatary_core::signatures::Code);

impl Body for PairingCode {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = wire::Pairing;

    fn parse(pairing: wire::Pairing, _: &State) -> Result<Self, Refusal> {
        suzatary_core::signatures::Code::typed(&pairing.code)
            .map_err(|refused| Refusal::bad_request(refused.to_string()))
            .map(Self)
    }
}

/// `POST /api/act/pair`: a device typing the code `suzatary device pair` printed at the box.
/// The key it trusts is the one that signed this request, never a field of it, so a code
/// trusts only the device that typed it; neither the code nor its hash is logged.
pub fn pair(call: &Call<Asker, (), PairingCode>) -> Result<Json<serde_json::Value>, Refusal> {
    use suzatary_core::signatures::Presented;
    let state = &call.state;
    let Asker(actor, device) = &call.guard;
    let pairings = &*state.pairings;
    let Some(keys) = state.keys.as_deref() else {
        return Err(Refusal::forbidden(
            "this box trusts no device by a code: it holds no root key to register one with",
        ));
    };
    // Asked before the code is read, so a revoked key spends no try and no code.
    for chain in state.chain_stores() {
        if suzatary_core::signatures::revoked_for(chain, &keys.root().public(), actor, device)? {
            return Err(Refusal::forbidden(
                "This device's key was revoked, so no code can trust it again. Use a new key on \
                 this device (Settings, This device), then type the code.",
            ));
        }
    }
    let presented =
        suzatary_core::signatures::present(pairings, &call.body.0, actor, (state.wall)())?;
    match presented {
        Presented::Refused(refused) => Err(Refusal::forbidden(refused.said())),
        Presented::Matched => {
            for chain in state.chain_stores() {
                suzatary_core::signatures::trust(chain, keys.root(), actor, device)?;
            }
            tracing::info!(
                "paired: trusted the device key {} for {actor}",
                device.as_str()
            );
            let caps = state.first.caps;
            match suzatary_core::secretary::routes::withdraw_trusted(
                caps.secretary.inbox,
                caps.secretary.chain,
                &keys.root().public(),
                caps.secretary.clock.now(),
            ) {
                Ok(withdrawn) if !withdrawn.is_empty() => {
                    state.heard(&suzatary_core::conversation_id::DEFAULT);
                }
                Ok(_) => {}
                Err(failed) => tracing::warn!("paired: its card: {}", failed.message()),
            }
            Ok(Json(serde_json::json!({ "trusted": device.as_str() })))
        }
    }
}

/// A GET under `/api/act/` is 405, or the frontend's index would answer it.
pub async fn act_read(request: Request) -> Response {
    refused(&Refusal::method_not_allowed(format!(
        "{} is POST-only; a read is one of /api/chain, /api/fold, /api/todo/<id>",
        request.uri().path()
    )))
}
