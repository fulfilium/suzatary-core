//! Where a file of a plugin's bundle lives, relative to the directory the binary resolves it in.

use std::fmt;

/// A relative path of plain segments with a given extension: no `..`, no absolute path, no empty
/// segment, so a manifest cannot point outside the directory it is resolved against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundlePath(String);

impl BundlePath {
    /// Parse one ending in `.<extension>`, or `None`; the caller names what it was parsing.
    #[must_use]
    pub fn parse(text: &str, extension: &str) -> Option<Self> {
        let plain = |segment: &str| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        let shaped = text
            .rsplit_once('.')
            .is_some_and(|(_, found)| found == extension)
            && text.split('/').all(plain);
        shaped.then(|| Self(text.to_owned()))
    }

    /// The path, `/`-separated.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BundlePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_path_stays_under_its_directory() {
        assert!(BundlePath::parse("plugins/ics.wasm", "wasm").is_some());
        assert!(BundlePath::parse("ics.schema", "schema").is_some());
        for bad in [
            "/etc/ics.wasm",
            "../ics.wasm",
            "plugins/../../ics.wasm",
            "plugins//ics.wasm",
            "./ics.wasm",
            "plugins/ics.so",
            "plugins\\ics.wasm",
            "ics.wasm.txt",
            "",
        ] {
            assert!(BundlePath::parse(bad, "wasm").is_none(), "{bad:?}");
        }
    }
}
