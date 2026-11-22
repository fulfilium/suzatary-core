//! The frontend mount: `web/dist/` is the one directory served as files. Read as this box
//! throughout: this is about what is served, not who may see it.

use crate::common;

use common::Fakes;
use suzatary_cli::stamp::build as built;

fn with_dist(dist: &std::path::Path) -> String {
    let fakes = Fakes::new("frontend-built");
    let deployment = common::deployment([], []);
    common::Server::new(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    )
    .dist(dist)
    .start()
    .base
}

/// A `../` traversal never yields the escaped file's content; the answer is `index.html`, not
/// a 404, because navigation lives in the URL fragment and every unmatched path falls back to it.
#[test]
fn a_built_dist_is_served_and_nothing_outside_it_is() {
    let root = common::scratch("frontend-dist");
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist).expect("dist dir");
    std::fs::write(dist.join("index.html"), "<!doctype html><title>app</title>")
        .expect("writes index.html");
    std::fs::write(root.join("secret.txt"), "SSN 000-00-0000").expect("writes the secret");

    let base = with_dist(&dist);
    let mut response = common::get(&format!("{base}/"));
    assert_eq!(common::status(&response), 200);
    let body = response.body_mut().read_to_string().expect("a body");
    assert!(body.contains("<title>app</title>"));

    for escape in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/..%2Fsecret.txt",
        "/a/../../secret.txt",
    ] {
        let mut response = common::get(&format!("{base}{escape}"));
        let body = response.body_mut().read_to_string().unwrap_or_default();
        assert!(!body.contains("SSN"), "{escape}: {body}");
    }
}

/// `.well-known/assetlinks.json` carries the package name, the sha256 cert fingerprint, and the
/// `delegate_permission` relation.
#[test]
fn the_site_vouches_for_the_android_app() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("web")
        .join("static")
        .join(".well-known")
        .join("assetlinks.json");
    assert!(
        source.is_file(),
        "{} is what build-web.sh copies into web/dist/",
        source.display()
    );
    let bytes = std::fs::read(&source).expect("reads the statement");

    let root = common::scratch("frontend-assetlinks");
    let dist = root.join("dist");
    std::fs::create_dir_all(dist.join(".well-known")).expect("dist dir");
    std::fs::write(dist.join("index.html"), "<!doctype html>").expect("writes index.html");
    std::fs::write(dist.join(".well-known").join("assetlinks.json"), &bytes)
        .expect("writes the statement");

    let base = with_dist(&dist);
    let mut response = common::get(&format!("{base}/.well-known/assetlinks.json"));
    assert_eq!(common::status(&response), 200);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .split(';')
            .next(),
        Some("application/json")
    );
    let statements: serde_json::Value = common::body_json(&mut response);
    let statement = &statements[0];
    assert_eq!(
        statement["relation"],
        serde_json::json!(["delegate_permission/common.handle_all_urls"])
    );
    assert_eq!(statement["target"]["namespace"], "android_app");
    assert_eq!(statement["target"]["package_name"], "com.bmabsout.suzatary");
    assert!(
        statement["target"]["sha256_cert_fingerprints"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    );
}

/// A store path's files all date from 1970, so the answer is `no-cache`, no date and a tag for
/// a 304, and a 1970 `If-Modified-Since` is never answered "not modified".
#[test]
fn the_frontend_is_revalidated_by_content() {
    let root = common::scratch("frontend-fresh");
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist).expect("dist dir");
    std::fs::write(dist.join("app.js"), "console.log(1);").expect("writes app.js");
    let base = with_dist(&dist);

    let first = common::get(&format!("{base}/b/{}/app.js", built()));
    assert_eq!(common::status(&first), 200);
    let header = |name: &str| {
        first
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    assert_eq!(header("cache-control").as_deref(), Some("no-cache"));
    assert_eq!(header("last-modified"), None);
    let etag = header("etag").expect("a tag");
    assert!(etag.starts_with("\"dist-"), "{etag}");

    let held = common::agent()
        .get(&format!("{base}/b/{}/app.js", built()))
        .header("if-none-match", &etag)
        .call()
        .expect("the request completes");
    assert_eq!(common::status(&held), 304);

    let dated = common::agent()
        .get(&format!("{base}/b/{}/app.js", built()))
        .header("if-modified-since", "Thu, 01 Jan 1970 00:00:01 GMT")
        .call()
        .expect("the request completes");
    assert_eq!(
        common::status(&dated),
        200,
        "a 1970 date must not keep an old bundle"
    );
}

/// The `.br` is served with the file's own type, `Content-Encoding: br` and
/// `Vary: Accept-Encoding` to a request that accepts brotli, and the file itself to one that
/// does not.
#[test]
fn a_precompressed_file_is_served_as_its_br_and_nothing_is_compressed_per_request() {
    let root = common::scratch("frontend-br");
    let dist = root.join("dist");
    std::fs::create_dir_all(&dist).expect("dist dir");
    std::fs::write(dist.join("index.html"), "<!doctype html>").expect("writes index.html");
    let wasm: Vec<u8> = b"\0asm\x01\0\0\0".repeat(512);
    let br: &[u8] = b"brotli bytes, never decoded by the server";
    std::fs::write(dist.join("typst_bg.wasm"), &wasm).expect("writes the module");
    std::fs::write(dist.join("typst_bg.wasm.br"), br).expect("writes its br");
    let script = "console.log('the bundle');\n".repeat(200);
    std::fs::write(dist.join("app.js"), &script).expect("writes a file with no br");
    let base = with_dist(&dist);

    let fetch = |path: &str, accept: &str| {
        let mut response = common::agent()
            .get(format!("{base}{path}"))
            .header("accept-encoding", accept)
            .call()
            .expect("the request completes");
        assert_eq!(common::status(&response), 200, "{path} {accept}");
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let headers = (
            header("content-type"),
            header("content-encoding"),
            header("vary"),
            header("etag"),
        );
        let body = response
            .body_mut()
            .with_config()
            .limit(1 << 20)
            .read_to_vec()
            .expect("a body");
        (headers, body)
    };

    let ((kind, encoding, vary, compressed_tag), body) = fetch(
        &format!("/b/{}/typst_bg.wasm", built()),
        "gzip, deflate, br, zstd",
    );
    assert_eq!(body, br);
    assert_eq!(encoding.as_deref(), Some("br"));
    assert_eq!(kind.as_deref(), Some("application/wasm"));
    assert_eq!(vary.as_deref(), Some("accept-encoding"));

    for accept in ["gzip, deflate", "identity"] {
        let ((kind, encoding, vary, tag), body) =
            fetch(&format!("/b/{}/typst_bg.wasm", built()), accept);
        assert_eq!(body, wasm, "{accept}");
        assert_eq!(encoding, None, "{accept}");
        assert_eq!(kind.as_deref(), Some("application/wasm"), "{accept}");
        assert_eq!(vary.as_deref(), Some("accept-encoding"), "{accept}");
        assert_ne!(tag, compressed_tag, "two representations, two tags");
    }

    let ((_, encoding, _, _), body) = fetch(&format!("/b/{}/app.js", built()), "gzip, deflate, br");
    assert_eq!(
        encoding, None,
        "a file with no br is not compressed per request"
    );
    assert_eq!(body, script.as_bytes());
}
