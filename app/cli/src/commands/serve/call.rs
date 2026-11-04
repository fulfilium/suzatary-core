//! What a route's work is handed.

use suzatary_core::gate::Admitted;

use super::{Chained, State};

/// One request, past its guard and parsed: everything a route's work reads.
pub struct Call<W, P, B> {
    pub state: State,
    /// The chain the path named, or the first; a box route is handed the first.
    pub chain: Chained,
    pub admitted: Admitted,
    /// What the route's guard minted.
    pub guard: W,
    pub params: P,
    pub body: B,
    /// The device key whose signature the request carried, as its guard asked; `None` where it
    /// asked nothing or the deployment signs nothing.
    pub device: Option<prodrome_core::sign::PublicKey>,
}
