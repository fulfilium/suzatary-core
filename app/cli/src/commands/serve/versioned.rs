//! Shell files named by build: `/b/<build>/<file>` is `dist`'s `<file>` while this box serves
//! `<build>`, and `/b/current` names the build served.
//!
//! No page may run two builds' files: another build's file is a 404, never the new build's file
//! under the old name. The service worker caches each build's files under these names.

use axum::extract::{Request, State as Extract};
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::State;

/// The prefix of every path named by a build.
pub const PREFIX: &str = "/b/";

/// The path that names the build this box serves.
pub const CURRENT: &str = "/b/current";

/// The paths no build names, the same in every build.
pub const ROOT: [&str; 9] = [
    "/",
    "/index.html",
    "/boot.js",
    "/sw.js",
    "/manifest.webmanifest",
    "/icon.svg",
    "/icon-192.png",
    "/icon-512.png",
    "/.well-known/assetlinks.json",
];

/// What a request's path is to the frontend.
#[derive(Debug, PartialEq, Eq)]
pub enum Path {
    /// A path no build names, served as it is.
    Root,
    /// A path under [`PREFIX`], read against the build served ([`named`]).
    Named,
    /// Any other path: a build's file asked for by no build's name.
    Unnamed,
}

/// A path under [`PREFIX`], once the build served is known.
#[derive(Debug, PartialEq, Eq)]
pub enum Named<'a> {
    /// A file of the build this box serves, as `dist` names it (with its leading `/`).
    Own(&'a str),
    /// A file of another build, or no file.
    Other,
    /// The name of the build served.
    Current,
}

/// Reads `path` with no build: only a path under [`PREFIX`] needs one.
#[must_use]
pub fn read(path: &str) -> Path {
    if ROOT.contains(&path) {
        Path::Root
    } else if path.starts_with(PREFIX) {
        Path::Named
    } else {
        Path::Unnamed
    }
}

/// Reads a path under [`PREFIX`] against the build this box serves.
#[must_use]
pub fn named<'a>(path: &'a str, build: &str) -> Named<'a> {
    if path == CURRENT {
        return Named::Current;
    }
    let rest = path.strip_prefix(PREFIX).unwrap_or_default();
    match rest.split_once('/') {
        Some((b, file)) if b == build && !file.is_empty() => {
            Named::Own(&path[PREFIX.len() + b.len()..])
        }
        _ => Named::Other,
    }
}

/// Serves this build's file under its plain name, and refuses another build's or an unnamed one.
pub async fn serve(Extract(state): Extract<State>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    match read(&path) {
        Path::Root => return next.run(request).await,
        Path::Unnamed => return StatusCode::NOT_FOUND.into_response(),
        Path::Named => {}
    }
    let build = state.stamp.build();
    match named(&path, &build) {
        Named::Other => StatusCode::NOT_FOUND.into_response(),
        Named::Current => {
            let mut answer = build.into_response();
            answer
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            answer
        }
        Named::Own(file) => {
            let query = request
                .uri()
                .query()
                .map_or_else(String::new, |q| format!("?{q}"));
            match format!("{file}{query}").parse::<Uri>() {
                Ok(uri) => {
                    *request.uri_mut() = uri;
                    next.run(request).await
                }
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Named, Path, named, read};

    #[test]
    fn a_name_under_the_prefix_is_this_build_s_file_or_refused() {
        for (path, build, expected) in [
            ("/b/7crcansr/app.js", "7crcansr", Named::Own("/app.js")),
            (
                "/b/7crcansr/typst/kit.typ",
                "7crcansr",
                Named::Own("/typst/kit.typ"),
            ),
            ("/b/dev/app.js", "dev", Named::Own("/app.js")),
            ("/b/current", "7crcansr", Named::Current),
            ("/b/current", "current", Named::Current),
            ("/b/older/app.js", "7crcansr", Named::Other),
            ("/b/7crcansr/", "7crcansr", Named::Other),
            ("/b/7crcansr", "7crcansr", Named::Other),
            ("/b/", "7crcansr", Named::Other),
        ] {
            assert_eq!(named(path, build), expected, "{path} under {build}");
        }
    }

    #[test]
    fn only_a_path_under_the_prefix_needs_the_build() {
        for (path, expected) in [
            ("/b/current", Path::Named),
            ("/b/7crcansr/app.js", Path::Named),
            ("/", Path::Root),
            ("/index.html", Path::Root),
            ("/boot.js", Path::Root),
            ("/sw.js", Path::Root),
            ("/.well-known/assetlinks.json", Path::Root),
            ("/app.js", Path::Unnamed),
            ("/style.css", Path::Unnamed),
            ("/tokens.css", Path::Unnamed),
            ("/prodrome_bg.wasm", Path::Unnamed),
            ("/typst/kit.typ", Path::Unnamed),
            ("/fonts/x.woff2", Path::Unnamed),
            ("/bundle/app.js", Path::Unnamed),
        ] {
            assert_eq!(read(path), expected, "{path}");
        }
    }
}
