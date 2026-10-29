//! The server's whole HTTP surface, read from `surface.golden` and probed route by route.
//!
//! A guarded route refuses the local shell, a viewer who is not a writer, and a writer
//! without the CSRF triple, and lets a writer with it through; a read refuses none of
//! them. A path's unlisted method is axum's own 405, so a route gained is a route listed.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::{CAST, FakeWhois, Fakes, actor, login, v4};

const WRITER: &str = "writer@example.test";
const READER: &str = "reader@example.test";

/// One line of the golden file.
struct Line {
    method: &'static str,
    path: &'static str,
    guard: &'static str,
    asks: &'static str,
}

fn golden() -> Vec<Line> {
    include_str!("surface.golden")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let mut words = line.split(' ');
            let (Some(method), Some(path), Some(guard), Some(asks), None) = (
                words.next(),
                words.next(),
                words.next(),
                words.next(),
                words.next(),
            ) else {
                panic!("a golden line is `METHOD PATH GUARD ASKS`: {line:?}");
            };
            Line {
                method,
                path,
                guard,
                asks,
            }
        })
        .collect()
}

/// A pattern with every segment filled in with something the route will parse or refuse.
fn concrete(pattern: &str) -> String {
    pattern
        .replace("{chain}", "life")
        .replace("{conversation}", "default")
        .replace("{index}", "0")
        .replace("{sha}", "0")
        .replace("{date}", "2026-09-02")
        .replace("{name}", "x.pdf")
        .replace("{*verb}", "x")
        .replace("{id}", "1")
        .replace("{node}", &"0".repeat(64))
}

/// The local shell, a viewer who may not write, and a writer.
struct Servers {
    local: String,
    reader: String,
    writer: String,
}

fn servers() -> Servers {
    let deploying = || {
        common::deployment(
            [login(WRITER), login(READER)],
            [(login(WRITER), actor(CAST.owner.actor))],
        )
    };
    let named = |tag: &str, who: &str| {
        common::start(
            Arc::new(Fakes::new(tag).whois(FakeWhois::new().naming(v4(), login(who)))),
            deploying(),
            Vec::new(),
        )
    };
    Servers {
        local: common::start(
            Arc::new(Fakes::new("surface-local")),
            deploying(),
            common::loopback_is_this_box(),
        ),
        reader: named("surface-reader", READER),
        writer: named("surface-writer", WRITER),
    }
}

/// What came back: the status, and whether the body was the not-built page or axum's own
/// empty 405.
struct Answer {
    status: u16,
    fallback: bool,
    empty: bool,
}

fn ask(method: &str, url: &str, triple: bool) -> Answer {
    let mut response = if method == "GET" {
        common::agent().get(url).call()
    } else if triple {
        // Not JSON, so a write that passes its guard and the door stops at its body.
        Ok(common::act_bytes(url, b"not json"))
    } else {
        common::agent()
            .post(url)
            .header("content-type", "application/json")
            .send("not json")
    }
    .expect("the request completes");
    let status = common::status(&response);
    let streaming = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/event-stream"));
    if streaming {
        return Answer {
            status,
            fallback: false,
            empty: false,
        };
    }
    let body = response.body_mut().read_to_vec().unwrap_or_default();
    Answer {
        status,
        fallback: body.starts_with("suzatary serve \u{2014} the API is up".as_bytes()),
        empty: body.is_empty(),
    }
}

/// The file is sorted, parses, every POST carries a write guard, and every write asks for a
/// trusted device's signature but the three that cannot: a post of objects (each carries its
/// own), a device asking for its key, and an error report.
#[test]
fn the_golden_list_is_sorted_and_every_write_is_guarded() {
    let lines = golden();
    let keys: Vec<(&str, &str)> = lines.iter().map(|line| (line.path, line.method)).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        keys, sorted,
        "surface.golden is sorted by path, then method, once each"
    );
    for line in &lines {
        assert!(
            ["GET", "POST"].contains(&line.method),
            "{} {}",
            line.method,
            line.path
        );
        assert!(
            ["read", "write", "act", "tap"].contains(&line.guard),
            "{} {} names no guard",
            line.method,
            line.path
        );
        assert!(
            line.method != "POST" || line.guard != "read",
            "POST {} is a write with no write guard",
            line.path
        );
        let asks = match (line.method, line.path) {
            ("GET", _) | (_, "/api/objects" | "/api/{chain}/objects" | "/api/client-errors") => "-",
            (_, "/api/act/key" | "/api/act/pair") => "possession",
            _ => "signed",
        };
        assert_eq!(line.asks, asks, "{} {}", line.method, line.path);
    }
}

/// Every listed route answers its method, and its guard is the one listed.
#[test]
fn every_listed_route_answers_behind_its_guard() {
    let servers = servers();
    for line in golden() {
        let path = concrete(line.path);
        let shown = format!("{} {}", line.method, line.path);
        let as_writer = ask(line.method, &format!("{}{path}", servers.writer), true);
        assert!(!as_writer.fallback, "{shown} is not mounted");
        assert!(
            !(as_writer.status == 405 && as_writer.empty),
            "{shown} is not mounted for {}",
            line.method
        );
        assert_ne!(
            as_writer.status, 403,
            "{shown} refused a writer with the triple"
        );
        if line.guard == "read" {
            for base in [&servers.local, &servers.reader] {
                let answer = ask(line.method, &format!("{base}{path}"), false);
                assert_ne!(answer.status, 403, "{shown} is a read and refused a viewer");
            }
        } else {
            for (who, base, triple) in [
                ("the local shell", &servers.local, true),
                ("a viewer who may not write", &servers.reader, true),
                ("a writer without the triple", &servers.writer, false),
            ] {
                let answer = ask(line.method, &format!("{base}{path}"), triple);
                assert_eq!(answer.status, 403, "{shown} let {who} through");
            }
        }
    }
}

/// A listed path answers any other method with axum's own 405, never a handler.
#[test]
fn no_unlisted_method_answers_on_a_listed_path() {
    let servers = servers();
    let lines = golden();
    let listed: BTreeSet<(&str, &str)> =
        lines.iter().map(|line| (line.method, line.path)).collect();
    for line in &lines {
        for method in ["GET", "POST"] {
            if listed.contains(&(method, line.path)) {
                continue;
            }
            let path = concrete(line.path);
            let answer = ask(method, &format!("{}{path}", servers.local), true);
            assert_eq!(
                (answer.status, answer.empty),
                (405, true),
                "{method} {} answered, and is not listed",
                line.path
            );
        }
    }
}
