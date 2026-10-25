//! Wall-clock timings of `/api/fold` and `/api/chain` over the live chain and render
//! cache, `#[ignore]`d so the test gate never times a machine nobody chose. Run with:
//!
//!   nix develop .#rust -c cargo test --release -p suzatary \
//!     --test `perf_fold_chain` -- --ignored --nocapture
//!
//! The live chain is only read, never appended to, adopted into or merged.

#![allow(dead_code)]

mod common;

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use flate2::Compression;
use flate2::write::GzEncoder;
use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;
use prodrome_core::{fpl, view as folds};
use sha2::{Digest, Sha256};
use suzatary_core::capability::{Datetime, Secretary, Server};
use suzatary_core::record::EventStore;
use suzatary_core::routes;
use suzatary_core::store::Reads as _;
use suzatary_io::render::Typst;

/// The repository root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("app/cli has a parent")
        .parent()
        .expect("app has a parent")
        .to_path_buf()
}

/// The real chain and render cache, over `common::Fakes` for everything else.
struct Bench {
    fakes: common::Fakes,
    render: Typst,
}

impl Bench {
    /// `None` when no instance file locates the live chain.
    fn real(tag: &str) -> Option<Self> {
        let deployment = crate::this_box::instance()?;
        let untrusted: Untrusted = Untrusted::of([Actor::new("triage").expect("valid")]);
        let events = EventStore::new(deployment.first().events.clone(), untrusted);
        let fakes = common::Fakes::with_store(tag, events);
        let render = Typst::new(
            deployment.code().to_path_buf(),
            deployment.paths.markup_cache.clone(),
            suzatary_io::typeset::InProcess,
        );
        Some(Self { fakes, render })
    }
}

fn timed<T>(f: impl Fn() -> T) -> (T, f64) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed().as_secs_f64() * 1000.0)
}

/// `n` timed runs of `f`, keeping the last value and every reading.
///
/// The minimum is reported beside the mean because scheduler noise only adds time.
fn repeated<T>(n: usize, f: impl Fn() -> T) -> (T, Vec<f64>) {
    let mut times = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let (out, ms) = timed(&f);
        times.push(ms);
        last = Some(out);
    }
    (last.expect("n > 0"), times)
}

fn min_ms(times: &[f64]) -> f64 {
    times.iter().copied().fold(f64::INFINITY, f64::min)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "ITERATIONS is 15; a f64 mantissa holds every usize this harness ever counts with"
)]
fn mean_ms(times: &[f64]) -> f64 {
    times.iter().sum::<f64>() / times.len() as f64
}

fn gzip(bytes: &[u8], level: u32) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(bytes).expect("writes to memory");
    encoder.finish().expect("finishes")
}

/// `part` of `whole`, as a percentage.
#[allow(
    clippy::cast_precision_loss,
    reason = "bodies here are under 1 MB; a f64 mantissa holds that exactly"
)]
fn percent(part: usize, whole: usize) -> f64 {
    100.0 * part as f64 / whole as f64
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

const ITERATIONS: usize = 15;
/// The compression layer's default gzip level (6), then the cheaper candidates.
const GZIP_LEVELS: [u32; 4] = [6, 3, 2, 1];

/// A fixed instant after every event, so the body hash compares across runs.
fn far_future() -> Datetime {
    Datetime::new(2099, 1, 1, 0, 0, 0, 0).expect("a real instant")
}

#[test]
#[ignore = "wall-clock timing against the real chain; run explicitly, see this file's header"]
fn fold_breakdown() {
    let Some(bench) = Bench::real("perf-fold") else {
        eprintln!(
            "skipped: no instance file to locate the live chain (SUZATARY_INSTANCE unset \
             and no instance.toml beside the repository)"
        );
        return;
    };
    let untrusted: Untrusted = Untrusted::of([Actor::new("triage").expect("valid")]);
    let policy = suzatary_core::chains::ChainPolicy {
        name: suzatary_core::chains::ChainName::new("life").expect("a slug"),
        untrusted: untrusted.clone(),
        roster: std::collections::BTreeSet::new(),
        root: suzatary_core::cast::keys::root(),
    };
    let chain: &dyn suzatary_core::store::Prodrome<suzatary_core::record::TodoEvent> =
        &bench.fakes.chain;
    let at = far_future();

    // Warm the head-set memo, which a route pays once per process.
    let objects = bench.fakes.chain.objects().expect("reads");
    let nodes = bench.fakes.chain.nodes().expect("reads");
    println!("chain: {} objects, {} nodes", objects.len(), nodes.len());

    let (_, core_times) = repeated(ITERATIONS, || {
        folds::entries(&nodes, at, &untrusted).expect("folds")
    });
    println!(
        "core fold   (prodrome_core::view::entries, {} nodes): min {:.3} ms  mean {:.3} ms",
        nodes.len(),
        min_ms(&core_times),
        mean_ms(&core_times)
    );

    let (payload, full_times) = repeated(ITERATIONS, || {
        // The whole fold: a limited one builds the same entries and drops most.
        let read = suzatary_core::chain_set::Read::of_stores([(&policy, chain)]);
        routes::fold(
            &bench.render,
            &policy.name,
            &policy.untrusted,
            &read.set(at),
            None,
        )
        .expect("route answers")
    });
    println!(
        "full route  (suzatary_core::routes::fold, memo warm): min {:.3} ms  mean {:.3} ms  ({} entries)",
        min_ms(&full_times),
        mean_ms(&full_times),
        payload.entries.len()
    );
    println!(
        "  -> wire build (inferred: full route minus core fold): {:.3} ms",
        (min_ms(&full_times) - min_ms(&core_times)).max(0.0)
    );

    let (json, serde_times) = repeated(ITERATIONS, || {
        serde_json::to_vec(&payload).expect("serialises")
    });
    println!(
        "serde       (serde_json::to_vec): min {:.3} ms  mean {:.3} ms  {} bytes",
        min_ms(&serde_times),
        mean_ms(&serde_times),
        json.len()
    );
    println!(
        "sha256 of the uncompressed /api/fold body at t={}: {}",
        fpl::iso(fpl::instant_of(at)),
        sha256_hex(&json)
    );

    for level in GZIP_LEVELS {
        let (compressed, gzip_times) = repeated(ITERATIONS, || gzip(&json, level));
        println!(
            "gzip level {level}  : min {:.3} ms  mean {:.3} ms  {} -> {} bytes ({:.1}%)",
            min_ms(&gzip_times),
            mean_ms(&gzip_times),
            json.len(),
            compressed.len(),
            percent(compressed.len(), json.len())
        );
    }
}

#[test]
#[ignore = "wall-clock timing against the real chain; run explicitly, see this file's header"]
fn chain_breakdown() {
    let Some(bench) = Bench::real("perf-chain") else {
        eprintln!(
            "skipped: no instance file to locate the live chain (SUZATARY_INSTANCE unset \
             and no instance.toml beside the repository)"
        );
        return;
    };
    let chain = common::chain();

    let objects = bench.fakes.chain.objects().expect("reads"); // warms the memo
    println!("chain: {} objects", objects.len());

    // A fresh `Bench` has an unwarmed memo, so this is the once-per-process cost.
    let cold = Bench::real("perf-chain-cold").expect("the instance file just read still reads");
    let (_, cold_verify_ms) = timed(|| cold.fakes.chain.verify());
    println!("verify, COLD (fresh process, first call)   : {cold_verify_ms:.3} ms");

    let (_, warm_verify_times) = repeated(ITERATIONS, || bench.fakes.chain.verify());
    println!(
        "verify, WARM (memo hit)                    : min {:.3} ms  mean {:.3} ms",
        min_ms(&warm_verify_times),
        mean_ms(&warm_verify_times)
    );

    let (payload, full_times) = repeated(ITERATIONS, || {
        routes::chain(&bench.fakes.chain, &chain).expect("route answers")
    });
    println!(
        "full route  (suzatary_core::routes::chain, memo warm): min {:.3} ms  mean {:.3} ms  ({} links)",
        min_ms(&full_times),
        mean_ms(&full_times),
        payload.links.len()
    );

    let (json, serde_times) = repeated(ITERATIONS, || {
        serde_json::to_vec(&payload).expect("serialises")
    });
    println!(
        "serde       (serde_json::to_vec): min {:.3} ms  mean {:.3} ms  {} bytes",
        min_ms(&serde_times),
        mean_ms(&serde_times),
        json.len()
    );
    println!(
        "sha256 of the uncompressed /api/chain body: {}",
        sha256_hex(&json)
    );

    for level in GZIP_LEVELS {
        let (compressed, gzip_times) = repeated(ITERATIONS, || gzip(&json, level));
        println!(
            "gzip level {level}  : min {:.3} ms  mean {:.3} ms  {} -> {} bytes ({:.1}%)",
            min_ms(&gzip_times),
            mean_ms(&gzip_times),
            json.len(),
            compressed.len(),
            percent(compressed.len(), json.len())
        );
    }
}

/// Read as this box's own node, because this measures latency, not access.
#[test]
#[ignore = "wall-clock timing against the real chain; run explicitly, see this file's header"]
fn http_round_trip_with_and_without_compression() {
    let Some(bench) = Bench::real("perf-http") else {
        eprintln!(
            "skipped: no instance file to locate the live chain (SUZATARY_INSTANCE unset \
             and no instance.toml beside the repository)"
        );
        return;
    };
    let Bench { fakes, render } = bench;
    let render: &'static Typst = Box::leak(Box::new(render));
    let deployment = common::deployment([], []);
    let base = common::Server::new(Arc::new(fakes), deployment, common::loopback_is_this_box())
        .effects(move |fakes| Server {
            secretary: Secretary {
                render,
                ..fakes.secretary
            },
            ..fakes
        })
        .start()
        .base;

    let t = fpl::iso(fpl::instant_of(far_future())).replace(':', "%3A");
    let routes: [(&str, String); 2] = [
        ("chain", "/api/chain".to_owned()),
        ("fold", format!("/api/fold?t={t}")),
    ];
    for (name, path) in routes {
        for encoding in ["identity", "gzip"] {
            let url = format!("{base}{path}");
            let start = Instant::now();
            let mut response = common::agent()
                .get(&url)
                .header("Accept-Encoding", encoding)
                .call()
                .expect("the request completes");
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            let sent_encoding = response
                .headers()
                .get("content-encoding")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("(none)")
                .to_owned();
            let bytes = response.body_mut().read_to_vec().expect("a body");
            println!(
                "{name:<5} Accept-Encoding={encoding:<8}: {elapsed:6.3} ms  {:>7} bytes over the wire  content-encoding={sent_encoding}",
                bytes.len()
            );
        }
    }
}
