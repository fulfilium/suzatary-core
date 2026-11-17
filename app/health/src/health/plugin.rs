//! What a plugin is named and may name, and why the host refused it.

mod credential;
mod name;
mod origin;
mod refusal;
mod said;

pub use credential::CredentialName;
pub use name::PluginName;
pub use origin::{Origin, Scheme};
pub use refusal::Refusal;
pub use said::{MAX_SAID, Said};

/// The most fetches one call may make; past it each is refused unsent.
pub const MAX_FETCHES: usize = 64;

/// Why a `[[plugins]]` table was refused; it names the offending text, and a manifest holds no
/// secret to leak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// Not `[a-z][a-z0-9]{0,31}`.
    #[error("{0:?} is not a plugin name (a lowercase letter, then lowercase letters or digits)")]
    BadName(String),
    /// Not a relative `.wasm` path of plain segments.
    #[error("{0:?} is not a component path (relative, plain segments, ending in .wasm)")]
    BadComponent(String),
    /// Not a relative `.schema` path of plain segments.
    #[error("{0:?} is not a schema path (relative, plain segments, ending in .schema)")]
    BadSchema(String),
    /// Not `http(s)://host[:port]`.
    #[error("{0:?} is not an origin (http(s)://host[:port], nothing after it)")]
    BadOrigin(String),
    /// An origin named twice.
    #[error("{0} is allowed twice")]
    DuplicateOrigin(String),
    /// A credential outside `PLUGIN_<NAME>_`.
    #[error("{name:?} is not a credential this plugin may name: it must start with {prefix}")]
    CredentialOutsideNamespace {
        /// The name the manifest gave.
        name: String,
        /// The namespace it had to be in.
        prefix: String,
    },
    /// A credential declared twice.
    #[error("{0} is declared twice")]
    DuplicateCredential(String),
}
