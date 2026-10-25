//! A plugin's grant, as data: the origins its manifest allows and the credentials it names.
//!
//! A plugin is a WebAssembly component of `app/wit/plugin.wit`'s `source` world: one function,
//! `sync`, whose one power is `fetch`. A [`Manifest`] is parsed once at the boundary into a value
//! that cannot hold an origin or a credential name it did not declare, and [`Target::admit`] is
//! the fence over each request, a function of that value. The runtime is `suzatary-plugin`'s.

mod bundle_path;
mod credential;
mod fetch;
mod item;
mod ledger;
mod manifest;
mod name;
mod origin;
mod refusal;
mod said;
mod url;

pub use bundle_path::BundlePath;
pub use credential::CredentialName;
pub use fetch::{MAX_FETCHES, MAX_RESPONSE, Outgoing, Target};
pub use item::{ItemRefusal, MAX_ITEM, admit};
pub use ledger::{Answer, Came, LastRun, Ledger};
pub use manifest::{Manifest, ManifestError};
pub use name::PluginName;
pub use origin::{Origin, Scheme};
pub use refusal::Refusal;
pub use said::{MAX_SAID, Said};
pub use url::{MAX_URL, PluginUrl};
