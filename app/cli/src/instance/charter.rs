//! A store's charter, `charter.toml` beside its objects: what the store says of itself.

use std::path::Path;

use anyhow::Context as _;

pub(crate) const FILE: &str = suzatary_io::ignores::CHARTER;

/// A store with no charter is private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Charter {
    Private,
    /// Everything it holds may be published.
    Public,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    public: bool,
}

impl Charter {
    /// The charter of the store at `store`: one `public = true|false`, and nothing else.
    pub(crate) fn read(store: &Path) -> anyhow::Result<Charter> {
        let path = store.join(FILE);
        let text = match std::fs::read_to_string(&path) {
            Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Charter::Private);
            }
            read => read?,
        };
        let File { public } =
            toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        Ok(if public {
            Charter::Public
        } else {
            Charter::Private
        })
    }

    pub(crate) fn write(self, store: &Path) -> anyhow::Result<()> {
        let path = store.join(FILE);
        let text = format!("public = {}\n", self == Charter::Public);
        suzatary_os::durable::replace(&path, text.as_bytes(), suzatary_os::durable::Mode::Readable)
            .with_context(|| format!("writing {}", path.display()))
    }
}
