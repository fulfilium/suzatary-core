//! Identity at the door: a listed viewer may read, a mapped actor may write as the mapping's
//! value, and this box's own node reads but never writes. An address is never an identity: every
//! peer, loopback included, is whatever the tailnet's whois says it is.

use std::net::IpAddr;

use prodrome_core::event::Actor;

use suzatary_base::capability::{Peer, Whois, WhoisError};
use suzatary_base::refusal::{Refusal, shown};
use suzatary_core::deployment::Deployment;
use suzatary_signatures::signatures::Admitted;

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
    /// The key and actor the request names as the operator's, `None` where it names no actor.
    pub operator: Option<Claim<'a>>,
}

/// The header in which a request from this box names the actor its operator writes as.
pub const OPERATOR_HEADER: &str = "x-suzatary-operator";

/// What a request from this box names to be read as its operator's: the key that signs it, and
/// the actor it writes as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim<'a> {
    /// The key the request names, as its header carries it.
    pub key: &'a str,
    /// The actor it writes as, as its header carries it.
    pub actor: &'a str,
}

/// May this request read? Asked in order, the first two without a lookup: a request through a
/// proxy is refused, and so is one addressed anywhere but `door`; then only whois answers: a peer
/// that is this box's own node reads and never writes, whichever of its addresses it dialled; a
/// listed login reads; anything else, loopback included, is refused. A request from this box's
/// own node that claims the operator, by the root key and a writer's actor, is admitted as
/// [`Admitted::Operator`].
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
        Ok(Peer::ThisBox) => operator(deployment, knock.operator),
        Ok(Peer::Known(login)) if deployment.may_view(&login) => Ok(Admitted::Peer(login)),
        // The login goes to the log, not the body, so a stranger learns no logins to claim.
        Ok(_) | Err(WhoisError::Unreachable { .. }) => {
            Err(Refusal::forbidden("not a viewer of this app"))
        }
    }
}

/// This box's own node, as its operator where `claim` names the deployment's root key: refused
/// when it names an actor no login writes as, so the operator never writes as the agent.
fn operator(deployment: &Deployment, claim: Option<Claim<'_>>) -> Result<Admitted, Refusal> {
    let Some(claim) = claim else {
        return Ok(Admitted::Local);
    };
    if claim.key != deployment.chains.first().root.as_str() {
        return Ok(Admitted::Local);
    }
    match Actor::new(claim.actor) {
        Ok(actor) if deployment.writers().contains(&actor) => Ok(Admitted::Operator(actor)),
        Ok(_) | Err(_) => Err(Refusal::forbidden(format!(
            "the operator writes as an actor a device writes as, and {} is not one",
            shown(claim.actor)
        ))),
    }
}

/// The actor a write's door proves the request's signature for: the mapping's value for a
/// peer, and the claimed actor for the operator, whose signature must be the root's.
pub fn writer_of(deployment: &Deployment, admitted: &Admitted) -> Result<Actor, Refusal> {
    match admitted {
        Admitted::Operator(actor) => Ok(actor.clone()),
        Admitted::Peer(_) | Admitted::Local => actor_of(deployment, admitted),
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use prodrome_core::policy::Untrusted;

    use super::*;
    use suzatary_base::capability::Login;

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
            chains: suzatary_base::chains::Chains::of(vec![suzatary_base::chains::ChainPolicy {
                name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
                untrusted: Untrusted::of([actor("triage")]),
                roster: BTreeSet::from([actor("ada"), actor("triage")]),
                root: suzatary_core::cast::keys::root(),
            }])
            .expect("one chain"),
            categories: BTreeMap::from([("home".to_owned(), 3)]),
            app_url: "https://example.test".to_owned(),
            coding: None,
            zone: suzatary_base::zone::Zone::UTC,
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
            operator: None,
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

    /// This box's own node is its operator only where the request names the root key and an
    /// actor a login writes as; the claim gives a write's door its actor and a read route none.
    #[test]
    fn this_box_is_its_operator_only_by_the_root_key_and_a_writers_actor() {
        let deployed = deployment();
        let root = suzatary_core::cast::keys::root();
        let claiming = |key: &str, actor: &str, peer: &str| {
            super::admit(
                &Daemon,
                &deployed,
                &door(),
                &Knock {
                    operator: Some(Claim { key, actor }),
                    ..knock(peer)
                },
            )
        };
        let operator = claiming(root.as_str(), "ada", THIS_BOX).expect("the operator");
        assert_eq!(operator, Admitted::Operator(actor("ada")));
        assert_eq!(writer_of(&deployed, &operator), Ok(actor("ada")));
        assert!(actor_of(&deployed, &operator).is_err());
        assert_eq!(
            claiming(&"1".repeat(64), "ada", THIS_BOX),
            Ok(Admitted::Local),
            "another key is no claim"
        );
        for actor in ["triage", "Not An Actor"] {
            let refused = claiming(root.as_str(), actor, THIS_BOX).expect_err("no writer");
            assert_eq!(refused.status().code(), 403, "{actor}");
        }
        assert_eq!(
            claiming(root.as_str(), "ada", "100.64.0.1"),
            Ok(Admitted::Peer(Login::from_whois(HERS))),
            "a peer's claim is ignored"
        );
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
}
