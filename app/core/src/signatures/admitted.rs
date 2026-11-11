//! Whom the gate admitted, and where the owner asked from: one of his devices, or the box's own
//! terminal.

use prodrome_core::event::Actor;

use suzatary_base::capability::Login;

/// Who got through the door; the box's own arm carries no login, so it can never write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admitted {
    /// A named tailnet peer, in this deployment's viewer roster.
    Peer(Login),
    /// This box's own node, as whois names it by its stable identity, and therefore nameless.
    Local,
    /// This box's own node on a request that names the deployment's root key and an actor some
    /// login writes as: the box's operator at its terminal. The gate does not prove the claim;
    /// the door proves the root's signature before any write ([`crate::gate::writer_of`]), and [`crate::gate::actor_of`]
    /// refuses it, so a read route never takes an actor from it.
    Operator(Actor),
}

impl Admitted {
    /// The login, for a log line; `None` for the local shell.
    #[must_use]
    pub fn login(&self) -> Option<&Login> {
        match self {
            Admitted::Peer(login) => Some(login),
            Admitted::Local | Admitted::Operator(_) => None,
        }
    }
}

/// Where a request the gate admitted was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seat {
    /// A trusted device of the owner's, on the tailnet.
    Device,
    /// The box's operator, at the box's terminal, signing with the root key.
    Operator,
}

impl Seat {
    /// The seat the gate's answer names.
    #[must_use]
    pub fn of(admitted: &Admitted) -> Self {
        match admitted {
            Admitted::Operator(_) => Seat::Operator,
            Admitted::Peer(_) | Admitted::Local => Seat::Device,
        }
    }
}
