//! A page's view as data, the keyed diff that paints it, and the patches a painter applies.
//!
//! A program is a model, one pure `update` that answers a new model and the effects to perform,
//! and a `view` that is a function of the model alone ([`program::Program`]). The view is a tree
//! of [`node::Node`]s whose handlers are messages, never closures over state. [`paint::Painter`]
//! keeps the tree it last painted, gives each node an id, and answers the smallest list of
//! [`patch::Patch`]es that turns the painter's DOM into the new view; [`patch::encode`] prints
//! them as the bytes that cross to the painter. An event comes back as a node's id and what
//! happened, and the painter here turns it into the message the view put on that node.
#![forbid(unsafe_code)]

#[cfg(feature = "host")]
pub mod host;
pub mod kit;
pub mod node;
pub mod paint;
pub mod patch;
pub mod program;

pub use node::{Attr, Kids, Listener, Node, On, Tag};
pub use paint::Painter;
pub use patch::Patch;
pub use program::{Frame, Program, Runtime};
