//! A coding session in the cloud, started by the owner's tap: the box never runs a shell over
//! a repository for the secretary, a routine with no route to the tailnet does.
//!
//! The task is data and the brief is fixed: the routine's saved prompt is the containment, so
//! the payload is the [`Task`] text alone and the box adds nothing to it. A session's life is
//! the closed [`Session`], stepped only by [`Session::step`].

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::repos::{CommitId, RepoName, RepoPath};

/// The secretspec name the routine's bearer token lives under; never read by this crate.
pub const ROUTINE_TOKEN: &str = "SUZATARY_ROUTINE_TOKEN";

/// The most a task may be, in bytes.
pub const MAX_TASK_BYTES: usize = 4096;

/// The most of a report a card carries, in bytes; the rest is cut and says so.
pub const MAX_REPORT_BYTES: usize = 64 * 1024;

/// The prefix every branch a routine may push carries.
pub const BRANCH_PREFIX: &str = "claude/";

/// The directory a session writes its report into.
pub const REPORTS_DIR: &str = "docs/reports";

/// Why a name or a task was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodingError {
    /// Empty once trimmed.
    #[error("a task says what to do, in plain text; this one is empty")]
    EmptyTask,
    /// Over [`MAX_TASK_BYTES`].
    #[error("a task is at most {MAX_TASK_BYTES} bytes; this one is {0}")]
    LongTask(usize),
    /// A control character other than a newline or a tab.
    #[error("a task is plain text: no control characters but newlines and tabs")]
    ControlInTask,
    /// Not a routine id.
    #[error("a routine id is letters, digits, '_' and '-', got {0:?}")]
    Routine(String),
    /// Not a session id.
    #[error("a session id is up to 256 characters with no whitespace, got {0:?}")]
    Session(String),
    /// Not a claude.ai URL.
    #[error("a session URL is https://claude.ai/…, got {0:?}")]
    Url(String),
    /// Not a `claude/` branch.
    #[error("a session's branch is claude/…, a plain git ref, got {0:?}")]
    Branch(String),
}

/// What a session is asked to do: plain text, trimmed, at most
/// [`MAX_TASK_BYTES`], with no control character but `\n` and `\t`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Task(String);

impl Task {
    /// The text, exactly as the fire sends it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Its first line, for a sentence.
    #[must_use]
    pub fn headline(&self) -> &str {
        self.0.lines().next().unwrap_or_default()
    }
}

impl TryFrom<String> for Task {
    type Error = CodingError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            Err(CodingError::EmptyTask)
        } else if trimmed.len() > MAX_TASK_BYTES {
            Err(CodingError::LongTask(trimmed.len()))
        } else if trimmed
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            Err(CodingError::ControlInTask)
        } else {
            Ok(Self(trimmed.to_owned()))
        }
    }
}

impl From<Task> for String {
    fn from(task: Task) -> Self {
        task.0
    }
}

/// Whether `text` is one token: non-empty, at most `max` characters, and no
/// whitespace or control character in it.
fn token(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.chars().count() <= max
        && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Declares a newtype over `String` that parses through `$check`.
macro_rules! parsed {
    ($(#[$doc:meta])* $name:ident, $error:ident, $check:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// The text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = CodingError;
            fn try_from(text: String) -> Result<Self, Self::Error> {
                let check: fn(&str) -> bool = $check;
                if check(&text) {
                    Ok(Self(text))
                } else {
                    Err(CodingError::$error(text))
                }
            }
        }

        impl std::str::FromStr for $name {
            type Err = CodingError;
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(text.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

parsed!(
    /// The routine's id, as `[coding] routine` names it: it goes into the
    /// fire URL's path, so nothing but letters, digits, `_` and `-`.
    RoutineId,
    Routine,
    |text| {
        !text.is_empty()
            && text.len() <= 128
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    }
);

impl RoutineId {
    /// The routine's own page, where its API trigger's token is generated; the id is letters,
    /// digits, `_` and `-`, so it is a path segment as it stands.
    #[must_use]
    pub fn page(&self) -> String {
        format!("https://claude.ai/code/routines/{}", self.0)
    }
}

parsed!(
    /// `claude_code_session_id`, as the fire answered it: only "one token", because a fire
    /// that went must never be lost to a stricter guess at an undocumented shape.
    SessionId,
    Session,
    |text| token(text, 256)
);

parsed!(
    /// `claude_code_session_url`: an https URL on claude.ai, one token. The
    /// report a session writes names it, which is how its branch is found.
    SessionUrl,
    Url,
    |text| text.starts_with("https://claude.ai/") && token(text, 512)
);

parsed!(
    /// A branch a routine pushed: `claude/…`, a ref `git` reads as a name and
    /// never as an option or a range.
    Branch,
    Branch,
    |text| {
        text.len() <= 200
            && text
                .strip_prefix(BRANCH_PREFIX)
                .is_some_and(|rest| !rest.is_empty())
            && !text.contains("..")
            && text
                .split('/')
                .all(|part| !part.is_empty() && !part.starts_with('.') && part.strip_suffix(".lock").is_none())
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
    }
);

impl SessionUrl {
    /// Whether `text` names this URL whole, so `…/session_1` is not named by a report about
    /// `…/session_12`.
    #[must_use]
    pub fn named_in(&self, text: &str) -> bool {
        text.match_indices(self.as_str()).any(|(at, url)| {
            text[at + url.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/')))
        })
    }
}

/// `[coding]` in the instance file, parsed: which routine a tap fires, which
/// repository its branches come back to, and which chain a hand-over lands on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coding {
    /// The routine created at claude.ai/code/routines.
    pub routine: RoutineId,
    /// The repository its saved prompt names; the poll reads its branches.
    pub repo: RepoName,
    /// The project chain "Hand to the main session" writes its todo on.
    pub chain: crate::chains::ChainName,
}

/// The todo a hand-over writes, derived from the proposal, so a second hand-over of the same
/// session is the same id and is refused. `merge<hex>`, since a todo id is letters and digits.
#[must_use]
pub fn handed_todo(conversation: &crate::secretary::store::ConversationId, index: usize) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(format!("proposal:{conversation}/{index}"));
    format!("merge{}", &hex::encode(digest)[..12])
}

/// What a fire answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    /// The session's id.
    pub id: SessionId,
    /// Where it can be watched.
    pub url: SessionUrl,
}

/// How many lines one file of a branch changed, as `git diff --numstat`
/// says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Lines {
    /// A text file.
    Text {
        /// Lines added.
        added: u32,
        /// Lines removed.
        removed: u32,
    },
    /// A binary file, which has no lines.
    Binary,
}

/// One file of a branch's diff against the default branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    /// The file.
    pub path: RepoPath,
    /// What changed in it.
    pub lines: Lines,
}

/// A session's report, as the branch carries it: the file, its text (cut at
/// [`MAX_REPORT_BYTES`], and saying so), and the branch's diff stat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// `docs/reports/<slug>.md`.
    pub path: RepoPath,
    /// What the session wrote: commits, gates, open questions.
    pub text: String,
    /// Every file the branch changes.
    pub stat: Vec<FileStat>,
}

impl Report {
    /// A report, its text cut to [`MAX_REPORT_BYTES`] at a character
    /// boundary with a line saying where.
    #[must_use]
    pub fn new(path: RepoPath, text: &str, stat: Vec<FileStat>) -> Self {
        let text = if text.len() <= MAX_REPORT_BYTES {
            text.to_owned()
        } else {
            let end = (0..=MAX_REPORT_BYTES)
                .rev()
                .find(|at| text.is_char_boundary(*at))
                .unwrap_or_default();
            format!(
                "{}\n\n[cut: the report is {} bytes; the rest is on the branch]",
                &text[..end],
                text.len()
            )
        };
        Self { path, text, stat }
    }
}

/// One session's life, closed. Stored beside the proposal that started it; nothing infers it
/// from branch names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Session {
    /// The tap fired the routine and the session is running.
    Fired {
        /// The session.
        id: SessionId,
        /// Where it can be watched.
        url: SessionUrl,
        /// When it was fired.
        at: String,
    },
    /// A branch came back with a report naming the session.
    Reported {
        /// The session.
        id: SessionId,
        /// The branch it pushed.
        branch: Branch,
        /// What it wrote there, while its words are kept.
        report: crate::secretary::words::Words<Report>,
    },
    /// The branch is on the default branch.
    Merged {
        /// The session.
        id: SessionId,
        /// The branch's tip, as merged.
        sha: CommitId,
    },
    /// The owner let it go, and why.
    Dropped {
        /// The session.
        id: SessionId,
        /// His words, while they are kept.
        reason: crate::secretary::words::Words<String>,
    },
}

/// What can happen to a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The poll found its report.
    Report {
        /// On this branch.
        branch: Branch,
        /// Saying this.
        report: Report,
    },
    /// The poll found its branch merged.
    Merge {
        /// At this commit.
        sha: CommitId,
    },
    /// The owner dropped it.
    Drop {
        /// Why.
        reason: String,
    },
}

/// A step the session's state does not admit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a {state} session cannot be {step}")]
pub struct NotAStep {
    /// The state it is in.
    pub state: &'static str,
    /// What was asked of it.
    pub step: &'static str,
}

impl Session {
    /// The session, whatever became of it.
    #[must_use]
    pub fn id(&self) -> &SessionId {
        match self {
            Self::Fired { id, .. }
            | Self::Reported { id, .. }
            | Self::Merged { id, .. }
            | Self::Dropped { id, .. } => id,
        }
    }

    /// The state's one word, as the wire and a refusal say it.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Fired { .. } => "fired",
            Self::Reported { .. } => "reported",
            Self::Merged { .. } => "merged",
            Self::Dropped { .. } => "dropped",
        }
    }

    /// The one transition. Fired reports or drops; Reported merges or drops; Merged and
    /// Dropped are final. The id is carried, never replaced.
    ///
    /// # Errors
    /// [`NotAStep`] for any other pair.
    pub fn step(self, step: Step) -> Result<Self, NotAStep> {
        match (self, step) {
            (Self::Fired { id, .. }, Step::Report { branch, report }) => Ok(Self::Reported {
                id,
                branch,
                report: crate::secretary::words::Words::kept(report),
            }),
            (Self::Reported { id, .. }, Step::Merge { sha }) => Ok(Self::Merged { id, sha }),
            (Self::Fired { id, .. } | Self::Reported { id, .. }, Step::Drop { reason }) => {
                Ok(Self::Dropped {
                    id,
                    reason: crate::secretary::words::Words::kept(reason),
                })
            }
            (session, step) => Err(NotAStep {
                state: session.word(),
                step: match step {
                    Step::Report { .. } => "reported",
                    Step::Merge { .. } => "merged",
                    Step::Drop { .. } => "dropped",
                },
            }),
        }
    }
}

/// One `claude/` branch as the poll saw it on the repository's mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// The branch.
    pub branch: Branch,
    /// Its tip.
    pub tip: CommitId,
    /// Whether the tip is on the default branch.
    pub merged: bool,
    /// Every report the branch adds or changes under [`REPORTS_DIR`], whole.
    pub reports: Vec<(RepoPath, String)>,
    /// Its diff against the default branch.
    pub stat: Vec<FileStat>,
}

/// What the poll does to one session: a fired one is reported by the first branch whose report
/// names its URL; a reported one is merged once its own branch's tip is on the default branch.
#[must_use]
pub fn next(session: &Session, seen: &[Seen]) -> Option<Step> {
    match session {
        Session::Fired { url, .. } => seen.iter().find_map(|found| {
            found
                .reports
                .iter()
                .find(|(_, text)| url.named_in(text))
                .map(|(path, text)| Step::Report {
                    branch: found.branch.clone(),
                    report: Report::new(path.clone(), text, found.stat.clone()),
                })
        }),
        Session::Reported { branch, .. } => seen
            .iter()
            .find(|found| &found.branch == branch && found.merged)
            .map(|found| Step::Merge {
                sha: found.tip.clone(),
            }),
        Session::Merged { .. } | Session::Dropped { .. } => None,
    }
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    Task => |task| String::from(task.clone()),
);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn task(text: &str) -> Result<Task, CodingError> {
        Task::try_from(text.to_owned())
    }

    fn fired() -> Session {
        Session::Fired {
            id: "session_01abc".parse().expect("an id"),
            url: "https://claude.ai/code/session_01abc"
                .parse()
                .expect("a url"),
            at: "2026-09-27T09:00:00".to_owned(),
        }
    }

    fn report() -> Report {
        Report::new(
            RepoPath::try_from("docs/reports/readme.md".to_owned()).expect("a path"),
            "Session: https://claude.ai/code/session_01abc\nGates: green",
            vec![FileStat {
                path: RepoPath::try_from("README.md".to_owned()).expect("a path"),
                lines: Lines::Text {
                    added: 3,
                    removed: 1,
                },
            }],
        )
    }

    fn branch() -> Branch {
        "claude/readme-typo".parse().expect("a branch")
    }

    fn sha() -> CommitId {
        CommitId::try_from("a".repeat(40)).expect("a commit")
    }

    /// A task is trimmed once, and parsing its text again is the same task.
    #[test]
    fn a_task_is_trimmed_and_parsing_it_again_changes_nothing() {
        let once = task("  Fix the README's typo.\n\n").expect("a task");
        assert_eq!(once.as_str(), "Fix the README's typo.");
        assert_eq!(task(once.as_str()), Ok(once.clone()));
        let line = serde_json::to_string(&once).expect("serialises");
        assert_eq!(
            serde_json::from_str::<Task>(&line).expect("reads back"),
            once
        );
    }

    /// The cap is in bytes, and its edge is exact.
    #[test]
    fn a_task_is_at_most_four_kilobytes() {
        assert!(task(&"x".repeat(MAX_TASK_BYTES)).is_ok());
        assert_eq!(
            task(&"x".repeat(MAX_TASK_BYTES + 1)),
            Err(CodingError::LongTask(MAX_TASK_BYTES + 1))
        );
        // Two bytes a character: the count is bytes, not characters.
        assert!(task(&"é".repeat(MAX_TASK_BYTES / 2 + 1)).is_err());
    }

    /// Plain text: newlines and tabs, no other control character, not empty.
    #[test]
    fn a_task_is_plain_text() {
        assert!(task("one\n\ttwo").is_ok());
        assert_eq!(task(" \n "), Err(CodingError::EmptyTask));
        assert_eq!(task("a\u{1b}[2Jb"), Err(CodingError::ControlInTask));
        assert_eq!(task("a\rb"), Err(CodingError::ControlInTask));
        assert!(serde_json::from_str::<Task>("\"\"").is_err());
    }

    #[test]
    fn names_refuse_what_git_or_a_url_path_could_misread() {
        assert!("trig_01XyZ-2".parse::<RoutineId>().is_ok());
        assert!("../fire".parse::<RoutineId>().is_err());
        assert!("a b".parse::<SessionId>().is_err());
        assert!("https://evil.test/code/x".parse::<SessionUrl>().is_err());
        assert!("claude/fix-typo.2".parse::<Branch>().is_ok());
        for bad in [
            "claude/",
            "main",
            "claude/a..b",
            "claude/a/",
            "claude/a.lock",
            "claude/-x y",
            "secretary/x",
        ] {
            assert!(bad.parse::<Branch>().is_err(), "{bad}");
        }
    }

    /// A report names a URL only whole, and punctuation after it still names it.
    #[test]
    fn a_url_is_named_only_whole() {
        let url: SessionUrl = "https://claude.ai/code/session_1".parse().expect("a url");
        assert!(url.named_in("Session: https://claude.ai/code/session_1"));
        assert!(url.named_in("(https://claude.ai/code/session_1)."));
        assert!(!url.named_in("https://claude.ai/code/session_12"));
        assert!(!url.named_in("https://claude.ai/code/session_1/other"));
        assert!(url.named_in("https://claude.ai/code/session_12 https://claude.ai/code/session_1"));
    }

    #[test]
    fn a_session_round_trips_and_says_its_state() {
        for session in [
            fired(),
            fired()
                .step(Step::Report {
                    branch: branch(),
                    report: report(),
                })
                .expect("reports"),
        ] {
            let line = serde_json::to_string(&session).expect("serialises");
            assert!(
                line.contains(&format!("\"state\":\"{}\"", session.word())),
                "{line}"
            );
            assert_eq!(
                serde_json::from_str::<Session>(&line).expect("reads"),
                session
            );
        }
    }

    fn seen(branch: &str, merged: bool, report: &str) -> Seen {
        Seen {
            branch: branch.parse().expect("a branch"),
            tip: sha(),
            merged,
            reports: vec![(
                RepoPath::try_from("docs/reports/x.md".to_owned()).expect("a path"),
                report.to_owned(),
            )],
            stat: Vec::new(),
        }
    }

    /// The poll reports a fired session only by a report naming its URL, and merges a
    /// reported one only on its own branch's merge.
    #[test]
    fn the_poll_reports_by_the_url_and_merges_by_the_branch() {
        let other = seen(
            "claude/other",
            false,
            "https://claude.ai/code/session_01abcd",
        );
        let ours = seen(
            "claude/readme-typo",
            false,
            "Session https://claude.ai/code/session_01abc",
        );
        assert_eq!(next(&fired(), std::slice::from_ref(&other)), None);
        let Some(Step::Report { branch: found, .. }) = next(&fired(), &[other.clone(), ours])
        else {
            panic!("the naming branch reports it");
        };
        assert_eq!(found, branch());
        let reported = fired()
            .step(Step::Report {
                branch: branch(),
                report: report(),
            })
            .expect("reports");
        let merged_elsewhere = seen("claude/other", true, "");
        assert_eq!(next(&reported, &[merged_elsewhere]), None);
        assert_eq!(
            next(&reported, &[seen("claude/readme-typo", true, "")]),
            Some(Step::Merge { sha: sha() })
        );
    }

    fn any_step() -> impl Strategy<Value = Step> {
        prop_oneof![
            Just(Step::Report {
                branch: branch(),
                report: report()
            }),
            Just(Step::Merge { sha: sha() }),
            "[a-z ]{1,20}".prop_map(|reason| Step::Drop { reason }),
        ]
    }

    proptest! {
        /// Laws over any sequence of steps: the id never changes, a refused
        /// step leaves the session as it was, and Merged and Dropped are final.
        #[test]
        fn steps_keep_the_id_and_the_final_states_absorb(steps in proptest::collection::vec(any_step(), 0..8)) {
            let mut session = fired();
            let id = session.id().clone();
            for step in steps {
                let before = session.clone();
                match session.step(step) {
                    Ok(next) => {
                        let live = !matches!(before, Session::Merged { .. } | Session::Dropped { .. });
                        prop_assert!(live, "a final state stepped");
                        session = next;
                    }
                    Err(_) => session = before,
                }
                prop_assert_eq!(session.id(), &id);
            }
        }
    }
}
