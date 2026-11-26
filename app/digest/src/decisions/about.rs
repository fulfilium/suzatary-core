use std::cmp::Reverse;

use suzatary_todo::decisions::{Area, Decision};

/// A path or a word that a task names, which the decisions about it are shown in full for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// As the task wrote it, without a leading `./` or a trailing `/`.
    said: String,
    /// What `said` is when it is a path: the part of the tree it points at.
    path: Option<Area>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("--about names a path or a word, and this names nothing")]
pub struct NothingAbout;

/// How a decision came to be about a task, the stronger first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reach {
    /// The decision governs an area that the task's path lies in or holds, or its text names a path
    /// of two or more directories that the task names; the deeper the path, the stronger.
    Path(Reverse<usize>),
    /// The decision's title, body or rationale holds the task's words.
    Words,
}

impl About {
    pub fn new(text: &str) -> Result<Self, NothingAbout> {
        let said = text.trim().trim_start_matches("./").trim_end_matches('/');
        if said.is_empty() {
            return Err(NothingAbout);
        }
        Ok(Self {
            said: said.to_owned(),
            path: Area::new(said).ok(),
        })
    }

    fn reach(&self, decision: &Decision, words: &str) -> Option<Reach> {
        let overlaps = |area: &Area| {
            area.covers(&self.said)
                || self
                    .path
                    .as_ref()
                    .is_some_and(|path| path.covers(area.as_str()))
        };
        decision
            .areas
            .iter()
            .filter(|area| overlaps(area))
            .map(|area| Reach::Path(Reverse(area.depth())))
            .min()
            .or_else(|| {
                words
                    .contains(&self.said.to_lowercase())
                    .then(|| match &self.path {
                        Some(path) if path.depth() >= 2 => Reach::Path(Reverse(path.depth())),
                        Some(_) | None => Reach::Words,
                    })
            })
    }
}

fn said(decision: &Decision) -> String {
    let statement = decision.statement();
    format!(
        "{} {} {}",
        statement.title(),
        statement.body().as_str(),
        statement.rationale()
    )
    .to_lowercase()
}

pub(crate) fn reach(abouts: &[About], decision: &Decision) -> Option<Reach> {
    let words = said(decision);
    abouts
        .iter()
        .filter_map(|about| about.reach(decision, &words))
        .min()
}
