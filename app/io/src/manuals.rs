//! The manuals, as `suzatary_core::capability::Manuals`. No method takes a path, so the agent
//! holds no file read. A missing file is `None`, never an error.

use std::path::PathBuf;

use suzatary_core::capability::{Manual, Manuals};
use suzatary_core::persona::Persona;

/// The repository's manuals.
#[derive(Debug)]
pub struct Handbook {
    repo: PathBuf,
    persona: Persona,
}

impl Handbook {
    /// The manuals of `repo`; [`Manual::Operator`] and [`Manual::Secretary`] have their
    /// `{{persona.*}}` placeholders filled from `persona` at read time, never written back.
    #[must_use]
    pub fn new(repo: impl Into<PathBuf>, persona: Persona) -> Self {
        Self {
            repo: repo.into(),
            persona,
        }
    }
}

impl Manuals for Handbook {
    fn document(&self, which: Manual) -> Option<String> {
        let path = match which {
            Manual::Constitution => self.repo.join("CLAUDE.md"),
            Manual::Operator => self.repo.join("OPERATOR.md"),
            Manual::Secretary => self.repo.join("SECRETARY.md"),
        };
        let text = std::fs::read_to_string(path).ok()?;
        Some(match which {
            Manual::Operator | Manual::Secretary => self.persona.render(&text),
            Manual::Constitution => text,
        })
    }
}
