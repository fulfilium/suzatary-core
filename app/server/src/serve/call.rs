//! What a route's work is handed.

use suzatary_signatures::signatures::Admitted;

use super::{Chained, State};

/// One request, past its guard and parsed: everything a route's work reads.
pub(crate) struct Call<W, P, B> {
    pub state: State,
    /// The chain the path named, or the first; a box route is handed the first.
    pub chain: Chained,
    pub admitted: Admitted,
    pub guard: W,
    pub params: P,
    pub body: B,
}
