//! Where the owner asked from: one of his devices, or the box's own terminal.

use crate::gate::Admitted;

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
