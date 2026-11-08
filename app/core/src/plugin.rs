//! A plugin's grant, as data: a [`Manifest`] holds only the origins and credentials it declared,
//! and [`Target::admit`] is the fence over each request. The runtime is `suzatary-plugin`'s.

mod bundle_path;
mod fetch;
mod item;
mod manifest;
mod run;
mod url;

pub(crate) use bundle_path::BundlePath;
pub use fetch::{MAX_RESPONSE, Outgoing, Target};
pub use item::admit;
pub use manifest::Manifest;
pub use run::{Answer, Came, LastRun};
pub use url::PluginUrl;
