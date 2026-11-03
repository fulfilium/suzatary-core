//! A device's replica of one store the box serves: the nodes it holds, the box's words about
//! them and the frontier its next pull asks from, joined as a semilattice and read for what they
//! mean. Nothing here names a Suzatary store: a store gives its node type ([`held::Node`]) and,
//! where it locks words, the keys beside them ([`row::Keys`]).

pub mod frontier;
pub mod held;
pub mod pulling;
pub mod row;
pub mod word;
