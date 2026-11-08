//! Identity at the door, over a real socket: the gate's wiring in front of every route, reading
//! `ConnectInfo` and refusing in the documented shape (the decision itself is pinned in
//! `app/core/src/gate.rs`).
//!
//! The whois cache is process-wide (see `common`'s header), so [`common::v4`] always resolves
//! to [`PEER`], and [`common::v6`] is kept for a dead daemon, an `Err` the cache never stores.
//! Another login and an unresolved peer refuse with the same body, so one test stands for both.

mod common;

use std::net::IpAddr;

use common::Fakes;

/// The login `v4` resolves to in every server here, since the cache remembers the first answer.
fn peer() -> suzatary_core::capability::Login {
    common::login("peer@example.ts.net")
}

fn start(viewers_include_peer: bool) -> String {
    let fakes = Fakes::new("gate").whois(common::FakeWhois::new().naming(common::v4(), peer()));
    let viewers: Vec<_> = if viewers_include_peer {
        vec![peer()]
    } else {
        vec![common::login("somebody-else@example.ts.net")]
    };
    let deployment = common::deployment(viewers, []);
    common::start(std::sync::Arc::new(fakes), deployment, vec![])
}

/// A listed login passes.
#[test]
fn a_listed_login_passes() {
    let base = start(true);
    let response = common::get(&format!("{base}/api/chain"));
    assert_eq!(common::status(&response), 200);
}

/// Another tailnet login is refused, with the body an unresolved peer gets too.
#[test]
fn another_tailnet_login_is_refused() {
    let base = start(false);
    let mut response = common::get(&format!("{base}/api/chain"));
    assert_eq!(common::status(&response), 403);
    assert_eq!(
        common::body_json(&mut response),
        serde_json::json!({"error": "not a viewer of this app"})
    );
}

/// A dead daemon is refused too, on `v6`, since an `Err` answer is never cached.
#[test]
fn a_dead_daemon_is_refused_too() {
    let fakes =
        Fakes::new("gate-dead").whois(common::FakeWhois::new().unreachable_at(common::v6()));
    let deployment = common::deployment([peer()], []);
    let base = common::Server::new(std::sync::Arc::new(fakes), deployment, vec![])
        .v6()
        .start()
        .base;
    let mut response = common::get(&format!("{base}/api/chain"));
    assert_eq!(common::status(&response), 403);
    assert_eq!(
        common::body_json(&mut response),
        serde_json::json!({"error": "not a viewer of this app"})
    );
}

/// This box on its own tailnet address, which whois names as the owner and the writer, reads
/// like loopback and may not write.
#[test]
fn this_box_on_its_tailnet_address_reads_and_never_writes() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), peer())
        .this_box_at(common::v4());
    let fakes = Fakes::new("gate-this-box").whois(whois);
    let deployment = common::deployment([peer()], [(peer(), common::actor("ada"))]);
    let base = common::start(std::sync::Arc::new(fakes), deployment, vec![]);
    let response = common::get(&format!("{base}/api/chain"));
    assert_eq!(common::status(&response), 200);
    let mut response = common::act(
        &format!("{base}/api/act/device"),
        &serde_json::json!({"platform": "android", "token": "t"}),
    );
    assert_eq!(common::status(&response), 403);
    assert_eq!(
        common::body_json(&mut response)["error"],
        "a write must come from a named tailnet peer; loopback and the local shell use the CLI"
    );
}

/// Knowing the box's own node takes nothing from the owner's other devices: with the box's
/// address set, the owner's phone, the same login on another node, writes.
#[test]
fn the_owner_s_phone_writes_while_the_box_is_known() {
    let the_box: std::net::IpAddr = "100.64.0.9".parse().expect("an address");
    let whois = common::FakeWhois::new()
        .naming(common::v4(), peer())
        .naming(the_box, peer())
        .this_box_at(the_box);
    let fakes = Fakes::new("gate-phone-writes").whois(whois);
    let deployment = common::deployment([peer()], [(peer(), common::actor("ada"))]);
    let base = common::start(std::sync::Arc::new(fakes), deployment, vec![]);
    let response = common::act(
        &format!("{base}/api/act/device"),
        &serde_json::json!({"platform": "android", "token": "t"}),
    );
    assert_eq!(common::status(&response), 200);
}

/// Loopback is an address, not an identity: with a daemon that names no one there, a request from
/// this box's loopback (a `tailscale serve` or a reverse proxy pointed at the port) reads nothing.
#[test]
fn loopback_reads_nothing_the_daemon_does_not_name() {
    let fakes = Fakes::new("gate-loopback").whois(common::FakeWhois::new());
    let deployment = common::deployment([peer()], []);
    let base = common::start(std::sync::Arc::new(fakes), deployment, vec![]);
    for path in ["/api/chain", "/api/mail/inbox", "/"] {
        let mut response = common::get(&format!("{base}{path}"));
        assert_eq!(common::status(&response), 403, "{path}");
        assert_eq!(
            common::body_json(&mut response),
            serde_json::json!({"error": "not a viewer of this app"}),
            "{path}"
        );
    }
}

/// The mail is behind the same door as the todos: an unlisted peer is refused on a mail route
/// exactly as on `/api/chain`.
#[test]
fn the_mail_is_behind_the_same_door_as_the_todos() {
    let base = start(false);
    for path in ["/api/mail/inbox", "/api/mail/search?q=permit"] {
        let mut response = common::get(&format!("{base}{path}"));
        assert_eq!(common::status(&response), 403, "{path}");
        assert_eq!(
            common::body_json(&mut response),
            serde_json::json!({"error": "not a viewer of this app"}),
            "{path}"
        );
    }
}

/// The local shell is resolved through whois like any other peer and must still be a viewer;
/// every server in this suite that is not this box relies on it.
#[test]
fn the_local_shell_resolves_like_any_peer() {
    let whois = common::FakeWhois::new().naming(common::v4(), peer());
    let fakes = Fakes::new("gate-local-shell").whois(whois);
    let deployment = common::deployment([peer()], []);
    let base = common::start(std::sync::Arc::new(fakes), deployment, Vec::<IpAddr>::new());
    let response = common::get(&format!("{base}/api/chain"));
    assert_eq!(common::status(&response), 200);
}

/// A request through a proxy is refused whoever the socket says sent it: behind `tailscale serve`
/// or a reverse proxy the socket's address is the proxy's, not the reader's.
#[test]
fn a_request_through_a_proxy_is_refused() {
    let base = start(true);
    for header in suzatary_core::gate::PROXY_HEADERS {
        let mut response = common::agent()
            .get(&format!("{base}/api/chain"))
            .header(header, "100.64.0.1")
            .call()
            .expect("the request completes");
        assert_eq!(common::status(&response), 403, "{header}");
        let said = common::body_json(&mut response)["error"].to_string();
        assert!(said.contains(header), "{header}: {said}");
    }
}

/// A request addressed to any name but the app's own is refused, so a page another name resolves
/// to this box (DNS rebinding) reads nothing, mail included.
#[test]
fn a_request_addressed_elsewhere_is_refused() {
    let base = start(true);
    let port = base.rsplit(':').next().expect("a port");
    for host in [
        "rebound.example".to_owned(),
        format!("rebound.example:{port}"),
        "127.0.0.1".to_owned(),
    ] {
        for path in ["/api/chain", "/api/mail/inbox"] {
            let mut response = common::agent()
                .get(&format!("{base}{path}"))
                .header("host", &host)
                .call()
                .expect("the request completes");
            assert_eq!(common::status(&response), 403, "{host}{path}");
            assert_eq!(
                common::body_json(&mut response),
                serde_json::json!({"error": "this app answers only at its own address"}),
                "{host}{path}"
            );
        }
    }
}

/// Law: no page but a writer's quiets the phone. A viewer's ping and the box's own ping to
/// `GET /api/presence` are refused, so neither can keep a decision from ringing the owner.
#[test]
fn only_a_writer_s_page_quiets_the_phone() {
    let viewer = common::deployment([peer()], []);
    let writer = common::deployment([peer()], [(peer(), common::actor("ada"))]);
    let the_box = common::FakeWhois::new()
        .naming(common::v4(), peer())
        .this_box_at(common::v4());
    let cases = [
        (
            "a viewer",
            Fakes::new("presence-viewer")
                .whois(common::FakeWhois::new().naming(common::v4(), peer())),
            viewer,
            403,
        ),
        (
            "this box",
            Fakes::new("presence-box").whois(the_box),
            writer.clone(),
            403,
        ),
        (
            "a writer",
            Fakes::new("presence-writer")
                .whois(common::FakeWhois::new().naming(common::v4(), peer())),
            writer,
            204,
        ),
    ];
    for (who, fakes, deployment, status) in cases {
        let base = common::start(std::sync::Arc::new(fakes), deployment, vec![]);
        let response = common::get(&format!("{base}/api/presence"));
        assert_eq!(common::status(&response), status, "{who}");
    }
}
