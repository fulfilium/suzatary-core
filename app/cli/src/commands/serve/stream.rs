//! The server-sent streams, which the adapter cannot express: an answer that never ends.

use std::collections::HashMap;

use axum::extract::{RawPathParams, State as AxumState};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use suzatary_core::capability::Server;
use suzatary_core::refusal::Refusal;
use suzatary_core::secretary::store::{ConversationId, ConversationState};
use suzatary_core::store::StoreError;
use suzatary_core::wire;

use super::State;
use super::params::Asked;
use super::reply::refused;
use super::route::blocking;
use super::secretary::conversation_named;
use super::stores;

const POLL_SECONDS: f64 = 2.0;

/// The heads now, and again whenever they change, per chain.
///
/// Polled, not watched: comparing the head set every two seconds cannot miss a change,
/// and a watch can silently stop delivering.
pub async fn events(AxumState(state): AxumState<State>, raw: RawPathParams) -> Response {
    let bound = match state.bound(Asked::of(&raw, HashMap::new()).chain()) {
        Ok(bound) => bound,
        Err(refusal) => return refused(&refusal),
    };
    let stream = async_stream::stream! {
        // The head set decides whether something moved, since an adopt may only add a head.
        let mut heads = None;
        loop {
            if let Ok(now) = head_set(bound.caps)
                && heads.as_ref() != Some(&now)
                && let Ok(data) = frame(&now)
            {
                heads = Some(now);
                yield Ok::<_, std::convert::Infallible>(Event::default().data(data));
            }
            tokio::time::sleep(std::time::Duration::from_secs_f64(POLL_SECONDS)).await;
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/stores/events`: each store's heads now and whenever they change, one frame per
/// store that moved, on one stream for every store.
pub async fn store_events(AxumState(state): AxumState<State>) -> Response {
    let caps = state.first.caps;
    let stream = async_stream::stream! {
        let mut said: HashMap<wire::StoreName, Vec<String>> = HashMap::new();
        loop {
            for store in wire::StoreName::ALL {
                if let Ok(now) = stores::heads(store, caps)
                    && said.get(&store) != Some(&now)
                    && let Ok(data) = serde_json::to_string(&wire::StoreFrame {
                        store,
                        heads: now.clone(),
                    })
                {
                    said.insert(store, now);
                    yield Ok::<_, std::convert::Infallible>(Event::default().data(data));
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs_f64(POLL_SECONDS)).await;
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

fn head_set(caps: &Server<'_>) -> Result<Vec<String>, StoreError> {
    Ok(caps
        .secretary
        .chain
        .tips()?
        .iter()
        .map(|hash| hash.as_str().to_owned())
        .collect())
}

fn frame(heads: &[String]) -> Result<String, serde_json::Error> {
    serde_json::to_string(&suzatary_core::wire::TipFrame {
        heads: heads.to_vec(),
    })
}

/// `GET /api/secretary/conversations/<id>/stream`: the live frames behind a round.
///
/// It sends the state, then any partial as one `delta`, then live frames; a subscriber
/// that lags is told `turn` and refetches rather than being handed a gap.
pub async fn conversation(AxumState(state): AxumState<State>, raw: RawPathParams) -> Response {
    let id = match conversation_named(&Asked::of(&raw, HashMap::new())) {
        Ok(id) => id,
        Err(refusal) => return refused(&refusal),
    };
    let (mut frames, replay) = state.workers.subscribe(&id);
    let reported = if replay.is_empty() {
        transcript_state(&state, id).await
    } else {
        ConversationState::Answering
    };
    let sse = async_stream::stream! {
        yield Ok::<_, std::convert::Infallible>(Event::default().data(frame_json(
            &wire::Frame::State { state: reported },
        )));
        for frame in replay {
            yield Ok(Event::default().data(frame_json(&frame)));
        }
        loop {
            match frames.recv().await {
                Ok(frame) => yield Ok(Event::default().data(frame_json(&frame))),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    yield Ok(Event::default().data(frame_json(&wire::Frame::Turn)));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(sse)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// What the transcript says the conversation is doing; idle when it cannot be read.
async fn transcript_state(state: &State, id: ConversationId) -> ConversationState {
    let caps = state.first.caps;
    blocking(move || {
        Ok::<_, Refusal>(suzatary_core::secretary::state(
            &suzatary_core::secretary::read(caps.secretary.conversations, &id)?,
        ))
    })
    .await
    .unwrap_or(ConversationState::Idle)
}

fn frame_json(frame: &wire::Frame) -> String {
    serde_json::to_string(frame).unwrap_or_default()
}
