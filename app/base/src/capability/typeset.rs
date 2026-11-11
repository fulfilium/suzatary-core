//! Compiling authored markup in the closed world: the one effect the markup door asks of.

use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

/// Compiling one text, alone, in the closed world: a standard library with no `html` module
/// and no `plugin`, no file but the text itself, no package, no font and no clock.
///
/// The world is the containment, so what a text may produce is a fact about that library,
/// whoever wrote the text; the interpreter also bounds the compile in time.
pub trait Typeset: Sync {
    /// Typst's whole HTML document for `source`, or what typst said was wrong with it.
    fn closed(&self, source: &str) -> Result<String, Rejected>;
}

/// Whether `source` parses as typst markup, with typst's own parser: the half of the closed
/// world that needs no compiler, where a test double's [`Typeset::closed`] stops.
pub fn parsed(source: &str) -> Result<(), Rejected> {
    use typst_syntax::DiagSpanKind;
    let held = typst_syntax::Source::detached(source);
    let (errors, _) = held.root().errors_and_warnings();
    let diagnostics: Vec<Diagnostic> = errors
        .into_iter()
        .map(|e| Diagnostic {
            message: e.message.to_string(),
            hints: e.hints.iter().map(|h| h.v.to_string()).collect(),
            span: match e.span.get() {
                DiagSpanKind::Detached => None,
                DiagSpanKind::Number { num, sub_range, .. } => held.range(num, sub_range),
                DiagSpanKind::Range { range, .. } => Some(range),
            },
        })
        .collect();
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(Rejected { diagnostics })
    }
}

/// One thing typst said about a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Its words.
    pub message: String,
    /// What it suggests, if anything.
    #[serde(default)]
    pub hints: Vec<String>,
    /// The bytes of the source it is about, when it is about some.
    #[serde(default)]
    pub span: Option<Range<usize>>,
}

impl Diagnostic {
    /// A diagnostic about nothing in particular: a compile that did not finish, a world that
    /// could not be built.
    #[must_use]
    pub fn whole(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hints: Vec::new(),
            span: None,
        }
    }

    /// `line:column: message`, one-based, read against the source it is about.
    #[must_use]
    pub fn said(&self, source: &str) -> String {
        let hints = self.hints.iter().fold(String::new(), |mut out, h| {
            out.push_str(" (hint: ");
            out.push_str(h);
            out.push(')');
            out
        });
        match &self.span {
            Some(span) => {
                let (line, column) = position(source, span.start);
                format!("{line}:{column}: {}{hints}", self.message)
            }
            None => format!("{}{hints}", self.message),
        }
    }
}

/// The line and column (one-based, in characters) of a byte offset, clamped to the source.
fn position(source: &str, at: usize) -> (usize, usize) {
    let before = source.get(..at.min(source.len())).unwrap_or(source);
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

/// Typst refused a source: every error it gave, never empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rejected {
    /// The errors, in typst's order.
    pub diagnostics: Vec<Diagnostic>,
}

impl Rejected {
    /// A refusal with one diagnostic about the whole text.
    #[must_use]
    pub fn whole(message: impl Into<String>) -> Self {
        Self {
            diagnostics: vec![Diagnostic::whole(message)],
        }
    }

    /// Every diagnostic as [`Diagnostic::said`] spells it, joined by `; `.
    #[must_use]
    pub fn said(&self, source: &str) -> String {
        self.diagnostics
            .iter()
            .map(|d| d.said(source))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words: Vec<&str> = self
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        f.write_str(&words.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_alone_finds_a_syntax_error_and_where() {
        assert_eq!(parsed("Renew *passport*"), Ok(()));
        let refused = parsed("a ] b").expect_err("a stray bracket");
        assert!(
            refused
                .said("a ] b")
                .starts_with("1:3: unexpected closing bracket")
        );
    }
}
