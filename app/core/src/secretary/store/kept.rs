//! A session a conversation keeps, and the first turn it holds.

use super::Entry;
use crate::capability::{Datetime, Session, SessionId};

/// A session this conversation keeps, and the first turn it holds: its identity, and what it was
/// sent when it opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The session.
    pub session: Session,
    /// The first turn its opening ask sent it; it has been sent every live turn since.
    pub from: usize,
}

impl Kept {
    pub(super) fn of(
        id: &SessionId,
        model: &str,
        cwd: &str,
        at: Datetime,
        surface: Option<String>,
        from: usize,
    ) -> Self {
        let at = crate::secretary::stamp::stamp(at);
        Self {
            session: Session {
                id: id.clone(),
                model: model.to_owned(),
                cwd: cwd.to_owned(),
                created: at.clone(),
                last: at,
                surface,
            },
            from,
        }
    }

    /// The line that records it at `at`.
    #[must_use]
    pub fn line(&self, at: Datetime) -> Entry {
        Entry::Kept {
            at,
            session: self.session.id.clone(),
            model: self.session.model.clone(),
            cwd: self.session.cwd.clone(),
            surface: self.session.surface.clone(),
            from: self.from,
        }
    }
}
