//! A conversation line without its words: what is pushed in the clear, and what every reading
//! of what a line IS (the fold's bookkeeping, taint, the digest's labels) reads.
//!
//! [`split`] and [`join`] are a lawful pair over [`Entry`]: `join(split(e).0, at, kept(split(e).1))
//! == e` for every entry whose words are all here (`line_shape` laws in this file's tests), and a
//! slot that does not open joins as words gone, never as a sentinel.

use std::collections::BTreeSet;

use crate::capability::SessionId;
use crate::coding::{Branch, Session, SessionUrl};
use crate::repos::CommitId;
use crate::secretary::erasure::Erasure;
use crate::secretary::inbox::ProposalId;
use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::KeyName;
use crate::secretary::stamp::Stamp;
use crate::secretary::store::{ConversationId, DEFAULT, Decision, Entry, Kind, Namer, Scope};
use crate::secretary::turn_shape::{self, TurnShape};
use crate::secretary::words::{Gone, Words};

/// One line's shape, by kind; the instant is the line's head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineShape {
    /// A conversation opened: which model, and what for. Words: its title.
    Opened {
        /// The model's name for itself.
        model: String,
        /// What it is for; `None` on a line written before a kind was stored, whose kind
        /// [`unkinded`] reads off its conversation's id.
        kind: Option<Kind>,
    },
    /// Named. Words: the name.
    Titled {
        /// Who named it.
        by: Namer,
    },
    /// Something said. Words: the turn's slots.
    Said {
        /// The turn's shape.
        turn: TurnShape,
        /// Who stamped it.
        by: Option<Stamp>,
    },
    /// A proposal made.
    Proposed {
        /// The inbox object.
        object: ProposalId,
        /// The proposal it replaces.
        revises: Option<usize>,
    },
    /// A triage card withdrawn or held. Words: why.
    Reviewed {
        /// The chain.
        chain: String,
        /// What was reviewed: a card's reference, or an older batch's head.
        batch: String,
        /// What was decided, and the objects it settled.
        decision: Decision,
    },
    /// A review that could not run. Words: why.
    ReviewFailed {
        /// How many cards it left open.
        standing: usize,
    },
    /// The round stopped.
    Stopped {
        /// By whom.
        by: String,
    },
    /// Asked again.
    Rewound {
        /// The first turn set aside.
        to: usize,
        /// By whom.
        by: String,
    },
    /// The memory's version told.
    MemorySeen {
        /// The version.
        version: u64,
    },
    /// A session opened.
    Session {
        /// Its id.
        session: SessionId,
        /// Its model.
        model: String,
        /// Its directory.
        cwd: String,
        /// Its surface digest.
        surface: Option<String>,
    },
    /// A turn a session did not hear.
    Unheard {
        /// The session.
        session: SessionId,
        /// Who said it.
        model: String,
    },
    /// A session kept from turn `from`.
    Kept {
        /// Its id.
        session: SessionId,
        /// Its model.
        model: String,
        /// Its directory.
        cwd: String,
        /// Its surface digest.
        surface: Option<String>,
        /// The first turn it was sent.
        from: usize,
    },
    /// How the asks send the conversation.
    Scoped {
        /// The scope.
        scope: Scope,
    },
    /// A coding session moved on. Words: its report, or why it was dropped.
    Coding {
        /// The proposal that started it.
        proposal: usize,
        /// The session's shape.
        session: CodingShape,
    },
    /// Lines of this conversation whose words were erased, by the names of their keys.
    Erased {
        /// The keys.
        keys: BTreeSet<KeyName>,
        /// Why.
        erasure: Erasure,
    },
    /// One slot's words sealed again under the current redaction rules. Words: the slot's.
    Resealed {
        /// The key of the slot as first written.
        of: KeyName,
        /// The key of the seal it was sealed again from.
        from: KeyName,
    },
}

/// A coding session without its words.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum CodingShape {
    /// Fired.
    Fired {
        /// The session.
        id: crate::coding::SessionId,
        /// Where it is watched.
        url: SessionUrl,
        /// When.
        at: String,
    },
    /// Reported on a branch.
    Reported {
        /// The session.
        id: crate::coding::SessionId,
        /// The branch.
        branch: Branch,
    },
    /// Merged.
    Merged {
        /// The session.
        id: crate::coding::SessionId,
        /// The tip merged.
        sha: CommitId,
    },
    /// Dropped.
    Dropped {
        /// The session.
        id: crate::coding::SessionId,
    },
}

impl LineShape {
    /// How many slots of words a line of this shape holds.
    #[must_use]
    pub fn slots(&self) -> usize {
        match self {
            LineShape::Said { turn, .. } => turn.slots(),
            LineShape::Opened { .. }
            | LineShape::Titled { .. }
            | LineShape::Reviewed { .. }
            | LineShape::ReviewFailed { .. }
            | LineShape::Coding {
                session: CodingShape::Reported { .. } | CodingShape::Dropped { .. },
                ..
            }
            | LineShape::Resealed { .. } => 1,
            LineShape::Proposed { .. }
            | LineShape::Stopped { .. }
            | LineShape::Rewound { .. }
            | LineShape::MemorySeen { .. }
            | LineShape::Session { .. }
            | LineShape::Kept { .. }
            | LineShape::Unheard { .. }
            | LineShape::Scoped { .. }
            | LineShape::Coding { .. }
            | LineShape::Erased { .. } => 0,
        }
    }

    /// The constructor a line of this shape is stored under.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            LineShape::Opened { .. } => "Opened",
            LineShape::Titled { .. } => "Titled",
            LineShape::Said { .. } => "Said",
            LineShape::Proposed { .. } => "Proposed",
            LineShape::Reviewed { .. } => "Reviewed",
            LineShape::ReviewFailed { .. } => "ReviewFailed",
            LineShape::Stopped { .. } => "Stopped",
            LineShape::Rewound { .. } => "Rewound",
            LineShape::MemorySeen { .. } => "MemorySeen",
            LineShape::Session { .. } => "Session",
            LineShape::Kept { .. } => "Kept",
            LineShape::Unheard { .. } => "Unheard",
            LineShape::Scoped { .. } => "Scoped",
            LineShape::Coding { .. } => "Coding",
            LineShape::Erased { .. } => "Erased",
            LineShape::Resealed { .. } => "Resealed",
        }
    }
}

/// A line's shape and its words, one per slot; no words for a line some of whose words are
/// already gone, which is no line to write.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per kind of line")]
pub fn split(entry: &Entry) -> (LineShape, Option<Vec<LineWords>>) {
    let one = |words: Option<LineWords>| words.map(|words| vec![words]);
    match entry {
        Entry::Opened {
            title, model, kind, ..
        } => (
            LineShape::Opened {
                model: model.clone(),
                kind: Some(*kind),
            },
            one(title.get().map(|title| LineWords::Title {
                title: title.clone(),
            })),
        ),
        Entry::Titled { title, by, .. } => (
            LineShape::Titled { by: by.clone() },
            one(title.get().map(|title| LineWords::Title {
                title: title.as_str().to_owned(),
            })),
        ),
        Entry::Said { turn, by, .. } => {
            let (turn, words) = turn_shape::split(turn);
            (
                LineShape::Said {
                    turn,
                    by: by.clone(),
                },
                words,
            )
        }
        Entry::Proposed {
            object, revises, ..
        } => (
            LineShape::Proposed {
                object: object.clone(),
                revises: *revises,
            },
            Some(Vec::new()),
        ),
        Entry::Reviewed {
            chain,
            batch,
            decision,
            why,
            ..
        } => (
            LineShape::Reviewed {
                chain: chain.clone(),
                batch: batch.clone(),
                decision: decision.clone(),
            },
            one(why.get().map(|why| LineWords::Why { why: why.clone() })),
        ),
        Entry::ReviewFailed {
            reason, standing, ..
        } => (
            LineShape::ReviewFailed {
                standing: *standing,
            },
            one(reason.get().map(|reason| LineWords::Reason {
                reason: reason.clone(),
            })),
        ),
        Entry::Stopped { by, .. } => (LineShape::Stopped { by: by.clone() }, Some(Vec::new())),
        Entry::Rewound { to, by, .. } => (
            LineShape::Rewound {
                to: *to,
                by: by.clone(),
            },
            Some(Vec::new()),
        ),
        Entry::MemorySeen { version, .. } => (
            LineShape::MemorySeen { version: *version },
            Some(Vec::new()),
        ),
        Entry::Session {
            session,
            model,
            cwd,
            surface,
            ..
        } => (
            LineShape::Session {
                session: session.clone(),
                model: model.clone(),
                cwd: cwd.clone(),
                surface: surface.clone(),
            },
            Some(Vec::new()),
        ),
        Entry::Unheard { session, model, .. } => (
            LineShape::Unheard {
                session: session.clone(),
                model: model.clone(),
            },
            Some(Vec::new()),
        ),
        Entry::Kept {
            session,
            model,
            cwd,
            surface,
            from,
            ..
        } => (
            LineShape::Kept {
                session: session.clone(),
                model: model.clone(),
                cwd: cwd.clone(),
                surface: surface.clone(),
                from: *from,
            },
            Some(Vec::new()),
        ),
        Entry::Scoped { scope, .. } => (LineShape::Scoped { scope: *scope }, Some(Vec::new())),
        Entry::Coding {
            proposal, session, ..
        } => {
            let (shape, words) = coding(session);
            (
                LineShape::Coding {
                    proposal: *proposal,
                    session: shape,
                },
                words,
            )
        }
        Entry::Erased { keys, erasure, .. } => (
            LineShape::Erased {
                keys: keys.clone(),
                erasure: erasure.clone(),
            },
            Some(Vec::new()),
        ),
        // Its words are another slot's, so it is sealed by `LineEvent::reseal` alone.
        Entry::Resealed { of, from, .. } => (
            LineShape::Resealed {
                of: of.clone(),
                from: from.clone(),
            },
            None,
        ),
    }
}

/// A coding session's shape and its words.
fn coding(session: &Session) -> (CodingShape, Option<Vec<LineWords>>) {
    match session {
        Session::Fired { id, url, at } => (
            CodingShape::Fired {
                id: id.clone(),
                url: url.clone(),
                at: at.clone(),
            },
            Some(Vec::new()),
        ),
        Session::Reported { id, branch, report } => (
            CodingShape::Reported {
                id: id.clone(),
                branch: branch.clone(),
            },
            report.get().map(|report| {
                vec![LineWords::Report {
                    report: report.clone(),
                }]
            }),
        ),
        Session::Merged { id, sha } => (
            CodingShape::Merged {
                id: id.clone(),
                sha: sha.clone(),
            },
            Some(Vec::new()),
        ),
        Session::Dropped { id, reason } => (
            CodingShape::Dropped { id: id.clone() },
            reason.get().map(|reason| {
                vec![LineWords::Dropped {
                    reason: reason.clone(),
                }]
            }),
        ),
    }
}

/// What a conversation opened before kinds were stored is for, read off its id: the box's rounds
/// are `<day>-review` and `<day>-sweep`, and any other but [`DEFAULT`] a person opened. Only the
/// readers of such a line ask; every line written since prints its kind.
#[must_use]
pub fn unkinded(id: &ConversationId) -> Kind {
    if id == &*DEFAULT {
        Kind::Default
    } else if id.as_str().ends_with("-review") || id.as_str().ends_with("-sweep") {
        Kind::Review
    } else {
        Kind::Topic
    }
}

/// The entry a shape of conversation `id`'s, its instant and its slots' words make: a slot whose
/// words are not of its kind reads as [`Gone::Missing`], since no writer made it.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per kind of line")]
pub fn join(
    id: &ConversationId,
    shape: LineShape,
    at: crate::capability::Datetime,
    mut words: Vec<Words<LineWords>>,
) -> Entry {
    words.resize_with(shape.slots(), || Gone::Missing.into());
    let mut slot = words.into_iter();
    match shape {
        LineShape::Opened { model, kind } => Entry::Opened {
            at,
            title: take(&mut slot, |words| match words {
                LineWords::Title { title } => Some(title),
                _ => None,
            }),
            model,
            kind: kind.unwrap_or_else(|| unkinded(id)),
        },
        LineShape::Titled { by } => Entry::Titled {
            at,
            title: take(&mut slot, |words| match words {
                LineWords::Title { title } => Some(title),
                _ => None,
            })
            .into_kept()
            .map_or_else(
                || Gone::Missing.into(),
                |title| {
                    crate::secretary::store::Title::new(title)
                        .map_or_else(|_| Gone::Missing.into(), Words::kept)
                },
            ),
            by,
        },
        LineShape::Said { turn, by } => Entry::Said {
            at,
            turn: turn_shape::join(turn, slot.collect()),
            by,
        },
        LineShape::Proposed { object, revises } => Entry::Proposed {
            at,
            object,
            revises,
        },
        LineShape::Reviewed {
            chain,
            batch,
            decision,
        } => Entry::Reviewed {
            at,
            chain,
            batch,
            decision,
            why: take(&mut slot, |words| match words {
                LineWords::Why { why } => Some(why),
                _ => None,
            }),
        },
        LineShape::ReviewFailed { standing } => Entry::ReviewFailed {
            at,
            reason: take(&mut slot, |words| match words {
                LineWords::Reason { reason } => Some(reason),
                _ => None,
            }),
            standing,
        },
        LineShape::Stopped { by } => Entry::Stopped { at, by },
        LineShape::Rewound { to, by } => Entry::Rewound { to, at, by },
        LineShape::MemorySeen { version } => Entry::MemorySeen { at, version },
        LineShape::Session {
            session,
            model,
            cwd,
            surface,
        } => Entry::Session {
            at,
            session,
            model,
            cwd,
            surface,
        },
        LineShape::Unheard { session, model } => Entry::Unheard { at, session, model },
        LineShape::Kept {
            session,
            model,
            cwd,
            surface,
            from,
        } => Entry::Kept {
            at,
            session,
            model,
            cwd,
            surface,
            from,
        },
        LineShape::Scoped { scope } => Entry::Scoped { at, scope },
        LineShape::Coding { proposal, session } => Entry::Coding {
            at,
            proposal,
            session: match session {
                CodingShape::Fired { id, url, at } => Session::Fired { id, url, at },
                CodingShape::Reported { id, branch } => Session::Reported {
                    id,
                    branch,
                    report: take(&mut slot, |words| match words {
                        LineWords::Report { report } => Some(report),
                        _ => None,
                    }),
                },
                CodingShape::Merged { id, sha } => Session::Merged { id, sha },
                CodingShape::Dropped { id } => Session::Dropped {
                    id,
                    reason: take(&mut slot, |words| match words {
                        LineWords::Dropped { reason } => Some(reason),
                        _ => None,
                    }),
                },
            },
        },
        LineShape::Erased { keys, erasure } => Entry::Erased { at, keys, erasure },
        LineShape::Resealed { of, from } => Entry::Resealed { at, of, from },
    }
}

/// The next slot's words, when they are of the kind `fits` reads.
fn take<T>(
    slot: &mut std::vec::IntoIter<Words<LineWords>>,
    fits: fn(LineWords) -> Option<T>,
) -> Words<T> {
    match slot.next().unwrap_or_else(|| Gone::Missing.into()) {
        Words::Kept { kept } => fits(kept).map_or_else(|| Gone::Missing.into(), Words::kept),
        Words::Gone { gone } => gone.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line written before kinds were stored reads its kind off its conversation's id; every
    /// line written since prints the kind it was opened with.
    #[test]
    fn an_unkinded_opening_reads_its_kind_off_its_id() {
        let opened = |id: &str| {
            let id = ConversationId::new(id).expect("a slug");
            let shape = LineShape::Opened {
                model: "m".to_owned(),
                kind: None,
            };
            let title = Words::kept(LineWords::Title {
                title: "t".to_owned(),
            });
            match join(
                &id,
                shape,
                crate::capability::Datetime::new(2026, 10, 11, 9, 0, 0, 0).expect("an instant"),
                vec![title],
            ) {
                Entry::Opened { kind, .. } => kind,
                other => panic!("an opening joins as one, got {other:?}"),
            }
        };
        assert_eq!(opened("default"), Kind::Default);
        assert_eq!(opened("2026-10-11-review"), Kind::Review);
        assert_eq!(opened("2026-10-11-sweep"), Kind::Review);
        assert_eq!(opened("2026-10-11-1"), Kind::Topic);
        let (shape, _) = split(&Entry::Opened {
            at: crate::capability::Datetime::new(2026, 10, 11, 9, 0, 0, 0).expect("an instant"),
            title: Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: Kind::Topic,
        });
        assert!(matches!(
            shape,
            LineShape::Opened {
                kind: Some(Kind::Topic),
                ..
            }
        ));
    }
}
