//! The server-sent streams, which the adapter cannot express: an answer that never ends.

use std::collections::{BTreeMap, HashMap};

use axum::extract::{RawPathParams, State as AxumState};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use suzatary_core::conversation_id::ConversationId;
use suzatary_core::refusal::Refusal;
use suzatary_core::secretary::store::ConversationState;
use suzatary_core::store::StoreError;
use suzatary_core::wire;

use super::State;
use super::params::Asked;
use super::reply::refused;
use super::route::blocking;
use super::secretary::conversation_named;
use super::stores;

const POLL_SECONDS: f64 = 2.0;

/// Each key's value now and whenever it changes, as `frame` prints it; `read` runs on the blocking
/// pool, since a tips read scans a store under its lock.
fn polled<K, V, T: serde::Serialize>(
    read: impl Fn() -> Vec<(K, Result<V, StoreError>)> + Clone + Send + 'static,
    frame: impl Fn(K, V) -> T + Send + 'static,
) -> Response
where
    K: Ord + Clone + Send + std::fmt::Debug + 'static,
    V: PartialEq + Clone + Send + 'static,
{
    let stream = async_stream::stream! {
        let mut last = BTreeMap::new();
        loop {
            // A read that panicked is reported by the panic hook.
            for (key, now) in tokio::task::spawn_blocking(read.clone()).await.unwrap_or_default() {
                let now = now.map_err(|failed| failed.to_string());
                if last.get(&key) == Some(&now) {
                    continue;
                }
                let printed = match &now {
                    Ok(value) => serde_json::to_string(&frame(key.clone(), value.clone()))
                        .map_err(|failed| format!("a frame does not print: {failed}")),
                    Err(failed) => Err(format!("{key:?} does not read: {failed}")),
                };
                match printed {
                    Ok(data) => yield Ok::<_, std::convert::Infallible>(Event::default().data(data)),
                    Err(failed) => tracing::warn!("stream: {failed}"),
                }
                last.insert(key, now);
            }
            tokio::time::sleep(std::time::Duration::from_secs_f64(POLL_SECONDS)).await;
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `polled` over one chain's heads.
pub async fn events(AxumState(state): AxumState<State>, raw: RawPathParams) -> Response {
    match state.bound(Asked::of(&raw, HashMap::new()).chain()) {
        Ok(bound) => polled(
            move || {
                let tips = bound.caps.secretary.chain.tips();
                vec![(
                    (),
                    tips.map(|tips| tips.iter().map(|hash| hash.as_str().to_owned()).collect()),
                )]
            },
            |(), heads| wire::TipFrame { heads },
        ),
        Err(refusal) => refused(&refusal),
    }
}

/// `GET /api/stores/events`: `polled` over every store's heads.
pub async fn store_events(AxumState(state): AxumState<State>) -> Response {
    let caps = state.first.caps;
    polled(
        move || {
            wire::StoreName::ALL
                .into_iter()
                .map(|store| (store, stores::heads(store, caps)))
                .collect()
        },
        |store, heads| wire::StoreFrame { store, heads },
    )
}

/// `GET /api/secretary/conversations/<id>/stream`: the state, any partial, then live frames; a
/// lagging subscriber is told `turn`.
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
            &suzatary_core::secretary::conversations::read(caps.secretary.conversations, &id)?,
        ))
    })
    .await
    .unwrap_or(ConversationState::Idle)
}

fn frame_json(frame: &wire::Frame) -> String {
    serde_json::to_string(frame).unwrap_or_default()
}
