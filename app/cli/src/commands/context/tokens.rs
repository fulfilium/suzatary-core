//! A file's size in tokens, estimated offline from its characters by a ratio per kind of file.

use serde::{Deserialize, Serialize};

/// A kind of file, by its extension: each kind has its own characters-per-token ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// `.rs`.
    Rust,
    /// `.ts`.
    TypeScript,
    /// `.md`.
    Markdown,
    /// `.sh` and the extensionless hooks.
    Shell,
    /// `.nix`.
    Nix,
    /// `.toml`.
    Toml,
    /// `.typ`.
    Typst,
    /// Every other text file: JavaScript, CSS, JSON, YAML, Java, WIT, HTML.
    Other,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Self; 8] = [
        Self::Rust,
        Self::TypeScript,
        Self::Markdown,
        Self::Shell,
        Self::Nix,
        Self::Toml,
        Self::Typst,
        Self::Other,
    ];

    /// Whether a file of this kind is code, which a test directory holds as a test rather than
    /// as a fixture.
    #[must_use]
    pub const fn is_code(self) -> bool {
        match self {
            Self::Rust | Self::TypeScript => true,
            Self::Markdown | Self::Shell | Self::Nix | Self::Toml | Self::Typst | Self::Other => {
                false
            }
        }
    }

    /// The kind of the file at `path`.
    #[must_use]
    pub fn of(path: &str) -> Self {
        match path.rsplit_once('.').map(|(_, extension)| extension) {
            Some("rs") => Self::Rust,
            Some("ts") => Self::TypeScript,
            Some("md") => Self::Markdown,
            Some("sh") => Self::Shell,
            Some("nix") => Self::Nix,
            Some("toml") => Self::Toml,
            Some("typ") => Self::Typst,
            None if path.contains("/hooks/") => Self::Shell,
            Some(_) | None => Self::Other,
        }
    }
}

/// Characters per token for each kind, as `context.toml`'s `[tokens.ratio]` holds them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ratios {
    /// Rust.
    pub rust: f64,
    /// TypeScript.
    pub typescript: f64,
    /// Markdown.
    pub markdown: f64,
    /// Shell.
    pub shell: f64,
    /// Nix.
    pub nix: f64,
    /// TOML.
    pub toml: f64,
    /// Typst.
    pub typst: f64,
    /// Every other text file.
    pub other: f64,
}

impl Ratios {
    /// The ratio of `kind`.
    #[must_use]
    pub const fn of(&self, kind: Kind) -> f64 {
        match kind {
            Kind::Rust => self.rust,
            Kind::TypeScript => self.typescript,
            Kind::Markdown => self.markdown,
            Kind::Shell => self.shell,
            Kind::Nix => self.nix,
            Kind::Toml => self.toml,
            Kind::Typst => self.typst,
            Kind::Other => self.other,
        }
    }

    /// The ratio of `kind` set to `ratio`.
    pub const fn set(&mut self, kind: Kind, ratio: f64) {
        match kind {
            Kind::Rust => self.rust = ratio,
            Kind::TypeScript => self.typescript = ratio,
            Kind::Markdown => self.markdown = ratio,
            Kind::Shell => self.shell = ratio,
            Kind::Nix => self.nix = ratio,
            Kind::Toml => self.toml = ratio,
            Kind::Typst => self.typst = ratio,
            Kind::Other => self.other = ratio,
        }
    }

    /// The tokens of `text`, a file of `kind`: its characters over the kind's ratio, rounded.
    #[must_use]
    pub fn tokens(&self, kind: Kind, text: &str) -> u64 {
        let characters = text.chars().count();
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a file holds far fewer than 2^52 characters, and the quotient is positive"
        )]
        let tokens = (characters as f64 / self.of(kind)).round() as u64;
        tokens
    }
}
