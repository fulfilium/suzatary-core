//! A page's view as data, the keyed diff that paints it, and the patches a painter applies.
#![forbid(unsafe_code)]

pub mod dom;
#[cfg(feature = "host")]
pub mod host;
pub mod kit;
pub mod node;
pub mod paint;
pub mod patch;
pub mod program;
pub mod subpatch;
pub mod subs;

pub use node::{Attr, Key, Kids, Listener, Node, On, Picked, Printed, Rows, Tag};
pub use paint::Painter;
pub use patch::Patch;
pub use program::{Frame, Program, Runtime, Stepped};
pub use subpatch::SubPatch;
pub use subs::Subs;
