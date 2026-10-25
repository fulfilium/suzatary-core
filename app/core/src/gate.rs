//! Identity at the door: a listed viewer may read, a mapped actor may write as the mapping's
//! value, and this box's own node reads but never writes. An address is never an identity: every
//! peer, loopback included, is whatever the tailnet's whois says it is.

use std::net::IpAddr;

use prodrome_core::event::Actor;

use crate::capability::{Login, Peer, Whois, WhoisError};
use crate::deployment::Deployment;
use crate::refusal::{Refusal, shown};

/// Who got through the door; the box's own arm carries no login, so it can never write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admitted {
    /// A named tailnet peer, in this deployment's viewer roster.
    Peer(Login),
    /// This box's own node, as whois names it by its stable identity, and therefore nameless.
    Local,
}

impl Admitted {
    /// The login, for a log line; `None` for the local shell.
    #[must_use]
    pub fn login(&self) -> Option<&Login> {
        match self {
            Admitted::Peer(login) => Some(login),
            Admitted::Local => None,
        }
    }
}

/// The headers a proxy writes. A request carrying one came through something that is not its
/// peer, so the socket's address names the proxy and whois would answer for the wrong party.
pub const PROXY_HEADERS: [&str; 5] = [
    "tailscale-user-login",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-real-ip",
    "forwarded",
];

/// A host and the port it was written with, as an `app_url` or a request's `Host` names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Named<'a> {
    /// The host, as written.
    pub host: &'a str,
    /// The port, `None` where none was written.
    pub port: Option<u16>,
}

/// Where this app answers: `app_url`'s host and port. A name is the whole of what a browser keeps
/// an origin by, so a page another name resolves here (DNS rebinding) is not this app's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Door {
    host: String,
    port: u16,
    /// The port of the door's scheme, which a `Host` that writes none means.
    unwritten: u16,
}

impl Door {
    /// The door `named` names under a scheme whose port is `scheme_port`.
    #[must_use]
    pub fn new(named: Named<'_>, scheme_port: u16) -> Self {
        Self {
            host: named.host.to_ascii_lowercase(),
            port: named.port.unwrap_or(scheme_port),
            unwritten: scheme_port,
        }
    }

    /// The port the door answers at.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Was a request naming `named` addressed here: this host exactly, case aside, at this port?
    #[must_use]
    pub fn answers(&self, named: Named<'_>) -> bool {
        named.host.eq_ignore_ascii_case(&self.host)
            && named.port.unwrap_or(self.unwritten) == self.port
    }
}

impl std::fmt::Display for Door {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// What the door sees of a request before it asks who sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Knock<'a> {
    /// The address the socket says the request came from.
    pub peer: IpAddr,
    /// What the request was addressed to (`Host`, or HTTP/2's `:authority`), `None` when it named
    /// nothing readable.
    pub addressed: Option<Named<'a>>,
    /// The first of [`PROXY_HEADERS`] the request carries.
    pub forwarded: Option<&'static str>,
}

/// May this request read? Asked in order, the first two without a lookup: a request through a
/// proxy is refused, and so is one addressed anywhere but `door`; then only whois answers: a peer
/// that is this box's own node reads and never writes, whichever of its addresses it dialled; a
/// listed login reads; anything else, loopback included, is refused.
pub fn admit<W: Whois + ?Sized>(
    whois: &W,
    deployment: &Deployment,
    door: &Door,
    knock: &Knock<'_>,
) -> Result<Admitted, Refusal> {
    if let Some(header) = knock.forwarded {
        return Err(Refusal::forbidden(format!(
            "this app answers its peers directly, and this request came through a proxy ({header})"
        )));
    }
    let here = knock.addressed.is_some_and(|named| door.answers(named));
    if !here {
        return Err(Refusal::forbidden(
            "this app answers only at its own address",
        ));
    }
    match whois.peer(knock.peer) {
        Ok(Peer::ThisBox) => Ok(Admitted::Local),
        Ok(Peer::Known(login)) if deployment.may_view(&login) => Ok(Admitted::Peer(login)),
        // The login goes to the log, not the body, so a stranger learns no logins to claim.
        Ok(_) | Err(WhoisError::Unreachable { .. }) => {
            Err(Refusal::forbidden("not a viewer of this app"))
        }
    }
}

/// The actor this request writes as, from the deployment's mapping and never from the body.
pub fn actor_of(deployment: &Deployment, admitted: &Admitted) -> Result<Actor, Refusal> {
    let Some(login) = admitted.login() else {
        return Err(Refusal::forbidden(
            "a write must come from a named tailnet peer; loopback and the local shell use the CLI",
        ));
    };
    deployment
        .actor_of(login)
        .cloned()
        .ok_or_else(|| Refusal::forbidden(format!("{login} may read this app but not write to it")))
}

/// The app's own header, which forces a CORS preflight this server never answers.
pub const ACT_HEADER: &str = "x-suzatary-act";

/// What a request carries that the CSRF triple reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActHeaders<'a> {
    /// `X-Suzatary-Act`, absent as `None`.
    pub act: Option<&'a str>,
    /// `X-Suzatary-Client`, the write contract the page's build keeps, absent as `None`.
    pub client: Option<&'a str>,
    /// `Content-Type`, absent as `None`.
    pub content_type: Option<&'a str>,
    /// `Sec-Fetch-Site`, absent as `None`, which passes because non-browsers do not send it.
    pub site: Option<&'a str>,
}

/// The CSRF triple: did code this app served make this request.
pub fn check_csrf(headers: &ActHeaders<'_>) -> Result<(), Refusal> {
    if headers.act != Some("1") {
        return Err(Refusal::forbidden(format!(
            "a write must carry {ACT_HEADER}: 1"
        )));
    }
    // Parameters such as `charset` do not change the content type.
    let kind = headers
        .content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if kind != "application/json" {
        return Err(Refusal::forbidden(
            "a write must be Content-Type: application/json",
        ));
    }
    match headers.site {
        None => Ok(()),
        Some("same-origin") => current(headers.client),
        Some(labelled) => Err(Refusal::forbidden(format!(
            "a write must be same-origin; the browser called this {}",
            shown(labelled)
        ))),
    }
}

/// The header a page's build names its write contract in.
pub const CLIENT_HEADER: &str = "x-suzatary-client";

/// The oldest write contract the box takes from a page: 2 is the first whose instants are UTC,
/// 3 the first that signs every write with its device key (SPEC law 42), 4 the first that reads
/// the conversations from the lines with their keys and forgets what an erasure names, 5 the
/// first that reads a turn, a card's answer, a proposed object, a review and a stored object as
/// the tagged sums the box answers with, 6 the first that keeps the lines as a skeleton and
/// their content by hash, in a version of the device's database an older page cannot open, 7
/// the first that names what a replica holds by the frontier the box's last answer named, again
/// in a version of the database an older page cannot open. A page
/// older than this writes what the box cannot take (a stamp in the device's zone, a write no
/// device key proves) or acts on words it holds in the clear and never hears erased,
/// so it is refused before it writes and loads the box's shell instead, never told it is unsigned. Raise it in the
/// same series as a change to what a page writes; a client that is not a page (no
/// `Sec-Fetch-Site`) carries no bundle and is not asked.
pub const CLIENT_MIN: u32 = 7;

/// The contract this build's own page writes by, as its header says it: what a test writes as,
/// so raising [`CLIENT_MIN`] never leaves a test writing as a page the box refuses.
pub const CLIENT: &str = "7";

/// A page keeps [`CLIENT_MIN`] or later.
fn current(client: Option<&str>) -> Result<(), Refusal> {
    match client.and_then(|said| said.trim().parse::<u32>().ok()) {
        Some(contract) if contract >= CLIENT_MIN => Ok(()),
        said => Err(Refusal::new(
            crate::refusal::Status::UpgradeRequired,
            format!(
                "this page writes by contract {}, and the box takes {CLIENT_MIN} or later, so this act was not sent: the page says Suzatary <version> is ready with Reload, and it can be sent again after Reload",
                said.map_or_else(|| "none".to_owned(), |n| n.to_string())
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use prodrome_core::policy::Untrusted;

    use super::*;

    /// The build's own page writes by a contract its box takes.
    #[test]
    fn this_builds_page_keeps_the_contract_its_box_takes() {
        assert_eq!(current(Some(CLIENT)), Ok(()));
        assert_eq!(CLIENT.parse::<u32>().ok(), Some(CLIENT_MIN));
    }

    /// A whois that names one address, plus an address it cannot reach.
    struct Daemon;

    const HERS: &str = "her@example.test";
    const A_STRANGER: &str = "someone@else.test";

    /// This box's own tailnet address, which the daemon names as its owner's.
    const THIS_BOX: &str = "100.64.0.9";

    impl Whois for Daemon {
        fn peer(&self, peer: IpAddr) -> Result<Peer, WhoisError> {
            let named = |login: &str| Peer::Known(Login::from_whois(login));
            Ok(match peer.to_string().as_str() {
                "100.64.0.1" => named(HERS),
                THIS_BOX => Peer::ThisBox,
                "100.64.0.2" => named(A_STRANGER),
                "100.64.0.3" => Peer::Stranger,
                other => {
                    return Err(WhoisError::Unreachable {
                        peer,
                        reason: format!("no daemon for {other}"),
                    });
                }
            })
        }
    }

    fn actor(name: &str) -> Actor {
        Actor::new(name).expect("a roster name")
    }

    fn deployment() -> Deployment {
        Deployment {
            viewers: [
                Login::from_whois(HERS),
                Login::from_whois("reader@example.test"),
            ]
            .into_iter()
            .collect(),
            actors: BTreeMap::from([(Login::from_whois(HERS), actor("ada"))]),
            chains: crate::chains::Chains::of(vec![crate::chains::ChainPolicy {
                name: crate::chains::ChainName::new("life").expect("a slug"),
                untrusted: Untrusted::of([actor("triage")]),
                roster: BTreeSet::from([actor("ada"), actor("triage")]),
                root: crate::cast::keys::root(),
            }])
            .expect("one chain"),
            categories: BTreeMap::from([("home".to_owned(), 3)]),
            app_url: "https://example.test".to_owned(),
            coding: None,
            zone: crate::zone::Zone::UTC,
        }
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    /// This deployment's own door, `https://example.test`.
    fn door() -> Door {
        Door::new(
            Named {
                host: "example.test",
                port: None,
            },
            443,
        )
    }

    /// A request from `peer` addressed to the door, through no proxy.
    fn knock(peer: &str) -> Knock<'static> {
        Knock {
            peer: ip(peer),
            addressed: Some(Named {
                host: "example.test",
                port: None,
            }),
            forwarded: None,
        }
    }

    fn admit<W: Whois + ?Sized>(
        whois: &W,
        deployment: &Deployment,
        peer: &str,
    ) -> Result<Admitted, Refusal> {
        super::admit(whois, deployment, &door(), &knock(peer))
    }

    #[test]
    fn a_listed_login_passes_and_another_on_the_tailnet_does_not() {
        let deployed = deployment();
        assert_eq!(
            admit(&Daemon, &deployed, "100.64.0.1"),
            Ok(Admitted::Peer(Login::from_whois(HERS)))
        );
        let refused = admit(&Daemon, &deployed, "100.64.0.2").expect_err("a stranger is refused");
        assert_eq!(refused.message(), "not a viewer of this app");
    }

    /// A peer whois names no one at is refused: an unresolved peer, an unreachable daemon, and
    /// loopback, which is an address and not an identity, so a proxy on this box (`tailscale
    /// serve`, a funnel) reads nothing.
    #[test]
    fn a_peer_whois_names_no_one_at_is_refused() {
        let deployed = deployment();
        for address in ["100.64.0.3", "203.0.113.9", "127.0.0.1", "::1"] {
            let refused = admit(&Daemon, &deployed, address).expect_err("nameless is refused");
            assert_eq!(refused.status().code(), 403, "{address}");
        }
    }

    /// This box on its own tailnet address is named its owner by whois, and reads: admitted with
    /// no login, so it can never write.
    #[test]
    fn this_box_on_its_tailnet_address_reads_and_never_writes() {
        let deployed = deployment();
        let admitted = admit(&Daemon, &deployed, THIS_BOX).expect("it reads");
        assert_eq!(admitted, Admitted::Local);
        assert!(actor_of(&deployed, &admitted).is_err());
    }

    /// The local shell resolves like any peer: a whois that names loopback admits it as that login.
    #[test]
    fn the_local_shell_resolves_like_any_peer() {
        // A whois that names every address, like the end-to-end fixture's.
        struct Everything;
        impl Whois for Everything {
            fn peer(&self, _peer: IpAddr) -> Result<Peer, WhoisError> {
                Ok(Peer::Known(Login::from_whois(HERS)))
            }
        }
        let deployed = deployment();
        assert_eq!(
            admit(&Everything, &deployed, "127.0.0.1"),
            Ok(Admitted::Peer(Login::from_whois(HERS)))
        );
    }

    /// A request through a proxy is refused before any lookup, whoever the socket says it is.
    #[test]
    fn a_request_through_a_proxy_is_refused_before_whois_is_asked() {
        let deployed = deployment();
        for header in PROXY_HEADERS {
            let refused = super::admit(
                &Daemon,
                &deployed,
                &door(),
                &Knock {
                    forwarded: Some(header),
                    ..knock("100.64.0.1")
                },
            )
            .expect_err("proxied");
            assert_eq!(refused.status().code(), 403, "{header}");
            assert!(refused.message().contains(header), "{}", refused.message());
        }
    }

    /// The door answers only its own host and port: case aside, and a port written as the
    /// scheme's own is the port left unwritten.
    #[test]
    fn a_request_is_admitted_only_at_the_apps_own_address() {
        let deployed = deployment();
        let at = |host, port| {
            super::admit(
                &Daemon,
                &deployed,
                &door(),
                &Knock {
                    addressed: Some(Named { host, port }),
                    ..knock("100.64.0.1")
                },
            )
        };
        for (host, port) in [("example.test", Some(443)), ("EXAMPLE.test", None)] {
            assert!(at(host, port).is_ok(), "{host}:{port:?}");
        }
        for (host, port) in [
            ("example.test", Some(80)),
            ("example.test", Some(8787)),
            ("evil.example", None),
            ("100.64.0.9", None),
            ("example.test.evil.example", None),
            ("example.test.", None),
        ] {
            let refused = at(host, port).expect_err("elsewhere");
            assert_eq!(
                refused.message(),
                "this app answers only at its own address",
                "{host}:{port:?}"
            );
        }
        let nowhere = Knock {
            addressed: None,
            ..knock("100.64.0.1")
        };
        assert!(super::admit(&Daemon, &deployed, &door(), &nowhere).is_err());
    }

    /// The actor is the mapping's value, and the local shell has none.
    #[test]
    fn only_a_named_peer_in_the_actor_mapping_may_write() {
        let deployed = deployment();
        assert_eq!(
            actor_of(&deployed, &Admitted::Peer(Login::from_whois(HERS))),
            Ok(actor("ada"))
        );
        assert_eq!(
            actor_of(&deployed, &Admitted::Local).unwrap_err().message(),
            "a write must come from a named tailnet peer; loopback and the local shell use the CLI"
        );
        assert_eq!(
            actor_of(
                &deployed,
                &Admitted::Peer(Login::from_whois("reader@example.test"))
            )
            .unwrap_err()
            .message(),
            "reader@example.test may read this app but not write to it"
        );
    }

    #[test]
    fn the_csrf_triple_refuses_each_of_its_three_ways() {
        let good = ActHeaders {
            act: Some("1"),
            client: Some(CLIENT),
            content_type: Some("application/json"),
            site: Some("same-origin"),
        };
        assert_eq!(check_csrf(&good), Ok(()));

        let no_header = ActHeaders {
            act: None,
            ..good.clone()
        };
        assert_eq!(
            check_csrf(&no_header).unwrap_err().message(),
            "a write must carry x-suzatary-act: 1"
        );

        let wrong_type = ActHeaders {
            content_type: Some("text/plain"),
            ..good.clone()
        };
        assert_eq!(
            check_csrf(&wrong_type).unwrap_err().message(),
            "a write must be Content-Type: application/json"
        );

        let cross = ActHeaders {
            site: Some("cross-site"),
            ..good.clone()
        };
        assert_eq!(
            check_csrf(&cross).unwrap_err().message(),
            "a write must be same-origin; the browser called this 'cross-site'"
        );
    }

    /// Content-type parameters are ignored and a missing `Sec-Fetch-Site` passes.
    #[test]
    fn a_charset_is_still_json_and_an_absent_site_label_passes() {
        assert_eq!(
            check_csrf(&ActHeaders {
                act: Some("1"),
                client: None,
                content_type: Some("Application/JSON; charset=utf-8"),
                site: None,
            }),
            Ok(())
        );
    }

    /// A page older than the box's minimum is told to upgrade, never forbidden; a client that
    /// is not a page is not asked for a contract.
    #[test]
    fn a_page_writing_by_an_older_contract_is_told_to_load_the_new_one() {
        let page = |client| ActHeaders {
            act: Some("1"),
            client,
            content_type: Some("application/json"),
            site: Some("same-origin"),
        };
        for stale in [
            None,
            Some("1"),
            Some("2"),
            Some("3"),
            Some("4"),
            Some("5"),
            Some("6"),
            Some("not a number"),
        ] {
            let refused = check_csrf(&page(stale)).unwrap_err();
            assert_eq!(refused.status().code(), 426, "{stale:?}");
        }
        assert_eq!(
            check_csrf(&page(None)).unwrap_err().message(),
            "this page writes by contract none, and the box takes 7 or later, so this act was not sent: the page says Suzatary <version> is ready with Reload, and it can be sent again after Reload"
        );
        assert_eq!(check_csrf(&page(Some("7"))), Ok(()));
        assert_eq!(check_csrf(&page(Some("8"))), Ok(()));
        assert_eq!(
            check_csrf(&ActHeaders {
                site: None,
                ..page(None)
            }),
            Ok(()),
            "the Android shell's own registration carries no bundle"
        );
    }
}
