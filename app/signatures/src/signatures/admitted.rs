use prodrome_data::event::Actor;

use suzatary_base::capability::Login;

/// Who got through the door; the box's own arm carries no login, so it can never write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admitted {
    /// A named tailnet peer, in this deployment's viewer roster.
    Peer(Login),
    /// This box's own node, named by whois by its stable identity; it carries no login.
    Local,
    /// This box's node naming the root key and an actor: the operator at its terminal. The gate
    /// does not prove the claim; the door proves the root's signature before any write
    /// (`suzatary_server::gate::writer_of`), and `actor_of` refuses it for reads.
    Operator(Actor),
}

impl Admitted {
    /// The login, for a log line.
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
    /// A trusted device on the tailnet.
    Device,
    /// The box's terminal, signing with the root key.
    Operator,
}

impl Seat {
    #[must_use]
    pub fn of(admitted: &Admitted) -> Self {
        match admitted {
            Admitted::Operator(_) => Seat::Operator,
            Admitted::Peer(_) | Admitted::Local => Seat::Device,
        }
    }
}
