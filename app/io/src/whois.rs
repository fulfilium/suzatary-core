//! Who is on the other end, asked of tailscaled's local API over its unix socket with one
//! hand-rolled HTTP/1.0 GET.
//!
//! HTTP/1.0 because its server may not answer it chunked: the body is the bytes after the head,
//! read to the close, so no transfer coding is decoded here.
//!
//! It fails closed: a peer without both a login and a node name is not admitted, and a
//! [`Peer::Stranger`] is kept apart from a daemon that did not answer (`Err`).

use std::io::{Read, Write};
use std::net::IpAddr;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use suzatary_core::capability::{Login, Peer, Whois, WhoisError};

/// Long enough for a slow netmap read, short enough that a wedged daemon does not hang a request.
const TIMEOUT: Duration = Duration::from_secs(5);

/// [`Whois`], against tailscaled's local API over its unix socket.
#[derive(Debug)]
pub struct Tailscaled {
    socket: PathBuf,
    timeout: Duration,
}

impl Tailscaled {
    /// Talk to the daemon over `socket`.
    #[must_use]
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
            timeout: TIMEOUT,
        }
    }
}

impl Whois for Tailscaled {
    /// The peer's login, and whether its node is this box's by the node's stable identity,
    /// never by address: an address can move to another node.
    fn peer(&self, address: IpAddr) -> Result<Peer, WhoisError> {
        // The local API wants `IP:PORT` and ignores the port; an `IpAddr` needs no escaping.
        let whois = self.get(&format!("/localapi/v0/whois?addr={address}:0"), address)?;
        let Some(login) = parse(&whois) else {
            return Ok(Peer::Stranger);
        };
        let broken = |what: &str| WhoisError::Unreachable {
            peer: address,
            reason: format!("the daemon's answer names no {what}"),
        };
        let node = node_of(&whois).ok_or_else(|| broken("node identity"))?;
        let status = self.get("/localapi/v0/status?peers=false", address)?;
        let own = own_node(&status).ok_or_else(|| broken("identity of this box"))?;
        Ok(if node == own {
            Peer::ThisBox
        } else {
            Peer::Known(login)
        })
    }
}

impl Tailscaled {
    /// The daemon's response body to `path`; any failure is `Unreachable` about `peer`.
    fn get(&self, path: &str, peer: IpAddr) -> Result<String, WhoisError> {
        let fail = |reason: String| WhoisError::Unreachable { peer, reason };

        let mut stream = UnixStream::connect(&self.socket)
            .map_err(|e| fail(format!("connecting to {}: {e}", self.socket.display())))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| fail(format!("setting a read timeout: {e}")))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|e| fail(format!("setting a write timeout: {e}")))?;

        let request = format!("GET {path} HTTP/1.0\r\nHost: local-tailscaled.sock\r\n\r\n");
        stream
            .write_all(request.as_bytes())
            .map_err(|e| fail(format!("writing the request: {e}")))?;

        // An HTTP/1.0 server closes after one response, so reading to EOF reads exactly it.
        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| fail(format!("reading the response: {e}")))?;

        let (head, body) = split_head_body(&raw).ok_or_else(|| {
            fail("the response had no blank line separating header from body".to_string())
        })?;
        if !status_ok(head) {
            let status_line = head.lines().next().unwrap_or("(empty)");
            return Err(fail(format!("the daemon answered {status_line}")));
        }
        String::from_utf8(body.to_vec())
            .map_err(|e| fail(format!("the response body was not utf-8: {e}")))
    }
}

fn split_head_body(raw: &[u8]) -> Option<(&str, &[u8])> {
    let boundary = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..boundary]).ok()?;
    Some((head, &raw[boundary + 4..]))
}

/// Whether the status is 200, whichever version the server answered in; anything else is a
/// broken box, not a stranger.
fn status_ok(head: &str) -> bool {
    head.lines().next().and_then(|line| line.split(' ').nth(1)) == Some("200")
}

/// A whois answer, `None` unless it has a non-empty login and a node name; the node is checked
/// and not kept, since a profile with no device behind it is a half-resolved peer.
fn parse(body: &str) -> Option<Login> {
    let data: serde_json::Value = serde_json::from_str(body).ok()?;
    let object = data.as_object()?;
    let profile = object.get("UserProfile")?.as_object()?;
    let node = object.get("Node")?.as_object()?;
    let login = profile.get("LoginName")?.as_str()?;
    if login.is_empty() {
        return None;
    }
    node.get("Name")?.as_str()?;
    Some(Login::from_whois(login))
}

/// The stable identity of the node a whois answer is about: `Node.StableID`.
fn node_of(body: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(body).ok()?;
    let id = data.get("Node")?.get("StableID")?.as_str()?;
    (!id.is_empty()).then(|| id.to_owned())
}

/// This box's own node's stable identity, from the daemon's status: `Self.ID`.
fn own_node(body: &str) -> Option<String> {
    let data: serde_json::Value = serde_json::from_str(body).ok()?;
    let id = data.get("Self")?.get("ID")?.as_str()?;
    (!id.is_empty()).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::os::unix::net::UnixListener;

    use super::*;

    /// Terse, because a unix socket path is limited to about 108 bytes.
    fn scratch_socket(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ts-{tag}-{}-{:?}.sock",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn a_node_s_identity_is_read_from_whois_and_this_box_s_from_the_status() {
        let whois = r#"{"UserProfile":{"LoginName":"a@example.com"},"Node":{"Name":"box.tailnet.ts.net.","StableID":"nBOX"}}"#;
        assert_eq!(node_of(whois), Some("nBOX".to_owned()));
        assert_eq!(
            own_node(r#"{"Self":{"ID":"nBOX","TailscaleIPs":["100.64.0.9"]}}"#),
            Some("nBOX".to_owned())
        );
        for broken in [r#"{"Self":{"ID":""}}"#, r#"{"Self":{}}"#, "not json"] {
            assert_eq!(own_node(broken), None, "{broken}");
        }
        assert_eq!(node_of(r#"{"Node":{"Name":"x"}}"#), None);
    }

    #[test]
    fn parse_reads_a_full_answer() {
        let body = r#"{"UserProfile":{"LoginName":"ada.login@example.com"},"Node":{"Name":"box.tailnet.ts.net."}}"#;
        assert_eq!(
            parse(body),
            Some(Login::from_whois("ada.login@example.com"))
        );
    }

    #[test]
    fn parse_refuses_what_names_no_login_on_a_node() {
        for body in [
            r#"{"Node":{"Name":"box.tailnet.ts.net."}}"#,
            r#"{"UserProfile":{"LoginName":""},"Node":{"Name":"box.tailnet.ts.net."}}"#,
            r#"{"UserProfile":{"LoginName":"ada.login@example.com"}}"#,
            "{not json",
            r#"["UserProfile","Node"]"#,
        ] {
            assert_eq!(parse(body), None, "{body}");
        }
    }

    #[test]
    fn a_status_is_read_in_either_version() {
        assert!(status_ok(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json"
        ));
        assert!(status_ok("HTTP/1.1 200 OK"));
        assert!(!status_ok("HTTP/1.0 403 Forbidden"));
        assert!(!status_ok("HTTP/1.0 2000"));
        assert!(!status_ok(""));
    }

    /// A fake tailscaled answering one connection per body, in order, each with a canned 200;
    /// it returns the request lines it was sent.
    fn daemon(
        path: &std::path::Path,
        bodies: Vec<&'static str>,
    ) -> std::thread::JoinHandle<Vec<String>> {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path).expect("binds a scratch socket");
        std::thread::spawn(move || {
            bodies
                .into_iter()
                .map(|json| {
                    let (mut conn, _) = listener.accept().expect("accepts a connection");
                    let mut request = Vec::new();
                    let mut buf = [0_u8; 512];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        let n = conn.read(&mut buf).expect("reads the request");
                        assert_ne!(n, 0, "the client closed before a whole request");
                        request.extend_from_slice(&buf[..n]);
                    }
                    let text = String::from_utf8(request).expect("the request is utf-8");
                    assert!(text.contains("Host: local-tailscaled.sock"), "{text}");
                    assert!(
                        text.starts_with("GET ") && text.contains(" HTTP/1.0\r\n"),
                        "{text}"
                    );
                    let response =
                        format!("HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{json}");
                    conn.write_all(response.as_bytes())
                        .expect("writes the response");
                    text.lines().next().unwrap_or_default().to_owned()
                })
                .collect()
        })
    }

    const WHOIS_BOX: &str = r#"{"UserProfile":{"LoginName":"ada.login@example.com"},"Node":{"Name":"box.tailnet.ts.net.","StableID":"nBOX"}}"#;
    const WHOIS_PHONE: &str = r#"{"UserProfile":{"LoginName":"ada.login@example.com"},"Node":{"Name":"phone.tailnet.ts.net.","StableID":"nPHONE"}}"#;
    const STATUS: &str = r#"{"Self":{"ID":"nBOX","TailscaleIPs":["100.64.0.9"]}}"#;

    /// The owner's phone, the same login on another node, is not the box.
    #[test]
    fn a_peer_is_this_box_by_its_node_s_identity_not_its_address() {
        let owner = || Peer::Known(Login::from_whois("ada.login@example.com"));
        for (whois, expected) in [(WHOIS_BOX, Peer::ThisBox), (WHOIS_PHONE, owner())] {
            let path = scratch_socket(if expected == Peer::ThisBox {
                "box"
            } else {
                "phone"
            });
            let server = daemon(&path, vec![whois, STATUS]);
            let peer = Tailscaled::new(path.clone())
                .peer(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)))
                .expect("asks the fake daemon");
            assert_eq!(peer, expected);
            let asked = server.join().expect("the fake daemon finishes");
            assert!(
                asked[0].contains("/localapi/v0/whois?addr=100.64.0.1:0"),
                "{asked:?}"
            );
            assert!(asked[1].starts_with("GET /localapi/v0/status"), "{asked:?}");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn a_stranger_is_answered_on_whois_alone() {
        let path = scratch_socket("stranger");
        let server = daemon(&path, vec![r#"{"Node":{"Name":"x","StableID":"nX"}}"#]);
        let peer = Tailscaled::new(path.clone())
            .peer(IpAddr::V4(Ipv4Addr::new(100, 64, 0, 3)))
            .expect("asks");
        assert_eq!(peer, Peer::Stranger);
        assert_eq!(server.join().expect("finishes").len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn login_of_reports_unreachable_when_the_daemon_is_not_there() {
        let path = scratch_socket("missing");
        let _ = std::fs::remove_file(&path);
        let client = Tailscaled::new(path);
        let peer = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 2));
        match client.peer(peer) {
            Err(WhoisError::Unreachable { peer: got, .. }) => assert_eq!(got, peer),
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }
}
