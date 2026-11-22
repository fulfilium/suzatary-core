//! A device's replica of one store the box serves, joined as a semilattice.
//! Nothing here names a Suzatary store: a store gives its node type ([`held::Node`]).

pub mod form;
pub mod frontier;
pub mod held;
pub mod objects;
pub mod pulling;
pub mod row;
pub mod word;
pub mod writes;
