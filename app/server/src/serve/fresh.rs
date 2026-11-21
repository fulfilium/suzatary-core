//! The frontend's files are revalidated on every load, by content.
//!
//! Every Nix store file is dated 1970, so `ServeDir`'s `Last-Modified` earns years of
//! heuristic freshness and its time-plus-size `ETag` cannot tell two builds of one
//! length apart. So every answer says `no-cache`, the date is dropped, and the tag
//! names the served store path (a content hash). The request's conditions are removed
//! before `ServeDir` compares them to its 1970 labels, and compared here instead.

use std::sync::Arc;

use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// The part of every tag that changes with the directory served: its
/// resolved name (a content hash for a store path, `dist` for a checkout,
/// where `ServeDir`'s own times are real and carry the change instead).
#[must_use]
pub(crate) fn tag_of(dist: &std::path::Path) -> Arc<str> {
    let resolved = dist.canonicalize().unwrap_or_else(|_| dist.to_path_buf());
    resolved
        .file_name()
        .map_or_else(|| "dist".into(), |name| name.to_string_lossy().into())
}

/// Revalidate a frontend file: strip the request's conditions, serve, then
/// answer 304 when the browser already holds this exact tag.
pub(crate) async fn revalidated(tag: Arc<str>, mut request: Request, next: Next) -> Response {
    let held = request.headers_mut().remove(header::IF_NONE_MATCH);
    request.headers_mut().remove(header::IF_MODIFIED_SINCE);
    let mut response = next.run(request).await;
    if !response.status().is_success() {
        return response;
    }
    let inner = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim_matches('"')
        .to_owned();
    let Ok(etag) = HeaderValue::from_str(&format!("\"{tag}-{inner}\"")) else {
        return response;
    };
    if held.as_ref() == Some(&etag) {
        let mut unchanged = StatusCode::NOT_MODIFIED.into_response();
        unchanged.headers_mut().insert(header::ETAG, etag);
        unchanged
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return unchanged;
    }
    let headers = response.headers_mut();
    headers.remove(header::LAST_MODIFIED);
    headers.insert(header::ETAG, etag);
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}
