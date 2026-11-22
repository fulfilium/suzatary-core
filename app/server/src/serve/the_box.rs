//! The box's own routes, no chain's.

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::Response;
use prodrome_core::event::Actor;
use prodrome_core::sign::PublicKey;
use suzatary_api::clienterror::{MAX_BODY_BYTES, Report, Sent};
use suzatary_api::{routes, wire};
use suzatary_base::conversation_id::DEFAULT;
use suzatary_base::refusal::Refusal;
use suzatary_signatures::devices;
use suzatary_signatures::signatures::Code;
use suzatary_signatures::wire::signatures::Registering;

use super::body::{Body, MAX_ACT_BYTES};
use super::call::Call;
use super::guard::{Asker, Mapped, Reader, Reporter};
use super::params::{Asked, Params};
use super::reply::refused;
use super::{Chained, State};
use suzatary_signatures::signatures;
use suzatary_signatures::signatures::Presented;

/// The day a path names, `YYYY-MM-DD`.
pub(crate) struct Day(chrono::NaiveDate);

impl Params for Day {
    fn parse(asked: &Asked, _: &Chained) -> Result<Self, Refusal> {
        let raw = asked.segment("date")?;
        chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .map(Self)
            .map_err(|_| {
                Refusal::bad_request(format!(
                    "a date is ISO (YYYY-MM-DD), got {}",
                    suzatary_base::refusal::shown(raw)
                ))
            })
    }
}

impl Body for Registering {
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
pub(crate) fn briefings(
    call: &Call<Reader, (), ()>,
) -> Result<Json<wire::BriefingsPayload>, Refusal> {
    let caps = call.chain.caps;
    crate::routes::briefings(caps.secretary.briefings, caps.secretary.conversations).map(Json)
}

/// `GET /api/briefings/<date>`: one day's, block by block.
pub(crate) fn briefing(
    call: &Call<Reader, Day, ()>,
) -> Result<Json<wire::BriefingPayload>, Refusal> {
    let caps = call.chain.caps;
    crate::routes::briefing(
        caps.secretary.briefings,
        caps.secretary.render,
        caps.secretary.conversations,
        caps.answers,
        call.params.0,
    )
    .map(Json)
}

/// `GET /api/calendar/<date>`: a week of the owner's calendars from that day.
pub(crate) fn calendar(
    call: &Call<Reader, Day, ()>,
) -> Result<Json<wire::CalendarPayload>, Refusal> {
    let caps = call.chain.caps;
    crate::routes::calendar(
        caps.secretary.calendar,
        caps.secretary.render,
        call.params.0,
    )
    .map(Json)
}

/// `GET /api/presence`: a writer's open page quiets the phone.
pub(crate) fn presence(call: &Call<Mapped, (), ()>) -> StatusCode {
    call.state.presence.seen();
    StatusCode::NO_CONTENT
}

/// `GET /api/about`: the version and build this process is.
pub(crate) fn about(
    call: &Call<Reader, (), ()>,
) -> Result<Json<suzatary_cards::wire::cards::AboutPayload>, Refusal> {
    let now = call.chain.caps.secretary.clock.now();
    call.state
        .stamp
        .about(&call.state.projects, &call.state.chains_read().set(now))
        .map(Json)
}

/// `GET /api/portal`: how this box wakes the Android portal.
pub(crate) fn portal(call: &Call<Reader, (), ()>) -> Json<wire::PortalPayload> {
    Json(wire::PortalPayload::from(call.state.portal.as_ref()))
}

/// `GET /api/releases`: the releases the plan store's plan declares.
pub(crate) fn releases(
    call: &Call<Reader, (), ()>,
) -> Result<Json<wire::ReleasesPayload>, Refusal> {
    let project = call
        .state
        .project()?
        .ok_or_else(|| Refusal::conflict("no chain this deployment mounts holds a release plan"))?;
    let now = call.chain.caps.secretary.clock.now();
    routes::releases(
        &project.chain,
        &project.plan,
        &call.state.chains_read().set(now),
    )
    .map(Json)
}

/// `POST /api/client-errors`: a tab's uncaught failure, logged as one `warn` line.
pub(crate) fn client_errors(call: &Call<Reporter, (), Report>) -> StatusCode {
    tracing::warn!("{}", call.body.line(&call.guard.0));
    StatusCode::NO_CONTENT
}

/// `POST /api/act/device`: the phone saying "wake me here"; nothing on the chain.
pub(crate) fn device(
    call: &Call<Actor, (), Registering>,
) -> Result<Json<suzatary_signatures::wire::signatures::DevicePayload>, Refusal> {
    let caps = call.chain.caps;
    devices::register_device(caps.devices, caps.secretary.clock, &call.body, &call.guard).map(Json)
}

pub(crate) struct DeviceKey(PublicKey);

impl Body for DeviceKey {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = wire::KeyAsked;

    fn parse(asked: wire::KeyAsked, _: &State) -> Result<Self, Refusal> {
        PublicKey::new(&asked.key)
            .map(Self)
            .map_err(|e| Refusal::bad_request(format!("key: {e}")))
    }
}

/// `POST /api/act/key`: files one Inbox card asking to trust a device as the gate's actor;
/// trusting is the tap on it from a device already trusted.
pub(crate) fn key(call: &Call<Asker, (), DeviceKey>) -> Result<Json<serde_json::Value>, Refusal> {
    // The request is signed by the key it asks for, so nobody files a card for a key they do
    // not hold.
    if call.guard.1 != call.body.0 {
        return Err(Refusal::forbidden(
            "a device asks for its own key: this request is signed by another",
        ));
    }
    let caps = call.state.first.caps;
    let index = crate::secretary::routes::ask_trust(caps, &call.guard.0, &call.body.0)?;
    call.state.heard(&DEFAULT);
    Ok(Json(serde_json::json!({ "proposed": index })))
}

pub(crate) struct PairingCode(Code);

impl Body for PairingCode {
    const CAP: usize = MAX_ACT_BYTES;

    type Wire = wire::Pairing;

    fn parse(pairing: wire::Pairing, _: &State) -> Result<Self, Refusal> {
        Code::typed(&pairing.code)
            .map_err(|refused| Refusal::bad_request(refused.to_string()))
            .map(Self)
    }
}

/// `POST /api/act/pair`: the key it trusts is the one that signed this request, never a field of
/// it, so a code trusts only the device that typed it; neither the code nor its hash is logged.
pub(crate) fn pair(
    call: &Call<Asker, (), PairingCode>,
) -> Result<Json<serde_json::Value>, Refusal> {
    let state = &call.state;
    let Asker(actor, device) = &call.guard;
    let pairings = &*state.pairings;
    let Some(keys) = state.keys.as_deref() else {
        return Err(Refusal::forbidden(
            "this box trusts no device by a code: it holds no root key to register one with",
        ));
    };
    let signatures::Keeping::Keyed(keyed) = state.keeping() else {
        return Err(Refusal::forbidden(
            "this box trusts no device by a code: every chain it mounts is public",
        ));
    };
    // Asked before the code is read, so a revoked key spends no try and no code.
    for chain in keyed.iter() {
        if signatures::revoked_for(chain.store, &keys.root().public(), actor, device)? {
            return Err(Refusal::forbidden(
                "This device's key was revoked, so no code can trust it again. Use a new key on \
                 this device (Settings, This device), then type the code.",
            ));
        }
    }
    let presented = signatures::present(pairings, &call.body.0, actor, (state.wall)())?;
    match presented {
        Presented::Refused(refused) => Err(Refusal::forbidden(refused.said())),
        Presented::Matched => {
            for chain in keyed.iter() {
                signatures::trust(chain.store, keys.root(), actor, device)?;
            }
            tracing::info!(
                "paired: trusted the device key {} for {actor}",
                device.as_str()
            );
            let caps = state.first.caps;
            match crate::secretary::routes::withdraw_trusted(
                caps.secretary.inbox,
                keyed.first().store,
                &keys.root().public(),
                caps.secretary.clock.now(),
            ) {
                Ok(withdrawn) if !withdrawn.is_empty() => {
                    state.heard(&DEFAULT);
                }
                Ok(_) => {}
                Err(failed) => tracing::warn!("paired: its card: {}", failed.message()),
            }
            Ok(Json(serde_json::json!({ "trusted": device.as_str() })))
        }
    }
}

/// A GET under `/api/act/` is 405, or the frontend's index would answer it.
pub(crate) async fn act_read(request: Request) -> Response {
    refused(&Refusal::method_not_allowed(format!(
        "{} is POST-only; a read is one of /api/chain, /api/fold, /api/todo/<id>",
        request.uri().path()
    )))
}
