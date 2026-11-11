//! How an answer goes on the wire, beyond a plain JSON 200.

use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use suzatary_base::refusal::{Refusal, Status};

/// What a route's work returns.
pub trait Outcome: Send + 'static {
    /// # Errors
    /// The work's own [`Refusal`].
    fn outcome(self) -> Result<Response, Refusal>;
}

impl<O: IntoResponse + Send + 'static> Outcome for Result<O, Refusal> {
    fn outcome(self) -> Result<Response, Refusal> {
        self.map(IntoResponse::into_response)
    }
}

impl<T: Serialize + Send + 'static> Outcome for axum::Json<T> {
    fn outcome(self) -> Result<Response, Refusal> {
        Ok(self.into_response())
    }
}

impl Outcome for StatusCode {
    fn outcome(self) -> Result<Response, Refusal> {
        Ok(self.into_response())
    }
}

/// A JSON 202: the work goes on after the answer.
pub struct Accepted<T>(pub T);

impl<T: Serialize> IntoResponse for Accepted<T> {
    fn into_response(self) -> Response {
        (StatusCode::ACCEPTED, axum::Json(self.0)).into_response()
    }
}

/// An answer that is a function of its URL alone, kept for good by the browser for this person
/// alone. [`kept`], outside the API's `no-store` layer, marks only this one, so every other
/// answer stays `no-store`. A wipe's `Clear-Site-Data` clears what was kept.
pub struct Immutable<T>(pub T);

/// A year, the longest a cache is asked to keep anything.
pub const IMMUTABLE: &str = "private, max-age=31536000, immutable";

#[derive(Debug, Clone, Copy)]
struct KeptForGood;

impl<T: IntoResponse> IntoResponse for Immutable<T> {
    fn into_response(self) -> Response {
        let mut response = self.0.into_response();
        response.extensions_mut().insert(KeptForGood);
        response
    }
}

/// Marks an [`Immutable`] answer to be kept for good, over the API layer's `no-store`.
pub async fn kept(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    if response.extensions().get::<KeptForGood>().is_some() {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static(IMMUTABLE),
        );
    }
    response
}

/// A refusal on the wire: one body shape, the server's own sentence.
pub fn refused(refusal: &Refusal) -> Response {
    let mut response = (
        StatusCode::from_u16(refusal.status().code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        axum::Json(refusal.payload()),
    )
        .into_response();
    match refusal.status() {
        Status::MethodNotAllowed => {
            response
                .headers_mut()
                .insert(header::ALLOW, header::HeaderValue::from_static("POST"));
        }
        // A 410 is cacheable by default, and a cached wipe would answer the new device the tab
        // comes back as. Clear only `cache` (the tab cannot reach it): not `storage`, which the
        // tab deletes itself, since a later 410 of the old page's would clear the new device's
        // key. `no-store` here too, since the gate sits outside the API's layer.
        Status::Wipe => {
            let headers = response.headers_mut();
            headers.insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("no-store"),
            );
            headers.insert(
                header::HeaderName::from_static("clear-site-data"),
                header::HeaderValue::from_static("\"cache\""),
            );
        }
        _ => {}
    }
    response
}
