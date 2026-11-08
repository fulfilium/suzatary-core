//! A coding session in the cloud, started by the owner's tap: the box never runs a shell over
//! a repository for the secretary, a routine with no route to the tailnet does.
//!
//! The task is data and the brief is fixed: the routine's saved prompt is the containment, so
//! the payload is the [`Task`] text alone and the box adds nothing to it. A session's life is
//! the closed [`Session`], stepped only by [`Session::step`].

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::coding_report::{FileStat, Report};
use crate::repo_ids::{CommitId, RepoName, RepoPath};

/// The secretspec name the routine's bearer token lives under; never read by this crate.
pub const ROUTINE_TOKEN: &str = "SUZATARY_ROUTINE_TOKEN";

/// The most a task may be, in bytes.
pub(crate) const MAX_TASK_BYTES: usize = 4096;

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
    /// Over `MAX_TASK_BYTES`.
    #[error("a task is at most {MAX_TASK_BYTES} bytes; this one is {0}")]
    LongTask(usize),
    /// A control character other than a newline or a tab.
    #[error("a task is plain text: no control characters but newlines and tabs")]
    ControlInTask,
    /// Not a routine id.
    #[error("a routine id is letters, digits, '_' and '-', got {0:?}")]
    Routine(String),
    /// Not a session id.
    #[error(
        "a session id is cse_ or session_ then up to 256 letters, digits, '_' and '-', got {0:?}"
    )]
    Session(String),
    /// Not a claude.ai URL.
    #[error("a session URL is https://claude.ai/…, got {0:?}")]
    Url(String),
    /// Not a `claude/` branch.
    #[error("a session's branch is claude/…, a plain git ref, got {0:?}")]
    Branch(String),
}

/// What a session is asked to do: plain text, trimmed, at most
/// `MAX_TASK_BYTES`, with no control character but `\n` and `\t`.
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

/// The two prefixes one session's id is spelled with: the routine API answers `cse_…`, and the
/// session names itself `session_…`, both with the same body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spelling {
    /// `cse_`, as the fire answers.
    Cse,
    /// `session_`, as the session writes in its report.
    Session,
}

impl Spelling {
    const ALL: [Self; 2] = [Self::Cse, Self::Session];

    const fn prefix(self) -> &'static str {
        match self {
            Self::Cse => "cse_",
            Self::Session => "session_",
        }
    }
}

/// Whether `c` can be part of a session id's body.
fn id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
}

/// The body of a session id: what both spellings share, and all that says which session it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionKey(String);

/// A session's id in either spelling. Two ids are the same session exactly when their bodies
/// are equal; the spelling is kept only to print the id back as it came.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId {
    spelling: Spelling,
    key: SessionKey,
}

impl SessionId {
    /// Whether `other` names the same session, in whichever spelling.
    #[must_use]
    pub fn is(&self, other: &Self) -> bool {
        self.key == other.key
    }

    /// The body: what both spellings share.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.key.0
    }
}

impl TryFrom<String> for SessionId {
    type Error = CodingError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let parsed = Spelling::ALL.into_iter().find_map(|spelling| {
            text.strip_prefix(spelling.prefix())
                .filter(|body| !body.is_empty() && body.len() <= 256 && body.chars().all(id_char))
                .map(|body| Self {
                    spelling,
                    key: SessionKey(body.to_owned()),
                })
        });
        parsed.ok_or(CodingError::Session(text))
    }
}

impl std::str::FromStr for SessionId {
    type Err = CodingError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::try_from(text.to_owned())
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> Self {
        id.to_string()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.spelling.prefix(), self.key.0)
    }
}

/// Where a session's page is. A report names its session by this page's URL, so the poll
/// reads only ids that follow it.
pub const SESSION_PAGE: &str = "https://claude.ai/code/";

/// Every session `text` names by its page's URL. The id is the whole run of id characters
/// after the page, so `…_1` is never read out of `…_12`, and a URL that goes on with `/`
/// names some other page.
fn sessions_named(text: &str) -> Vec<SessionId> {
    text.match_indices(SESSION_PAGE)
        .filter_map(|(at, page)| {
            let rest = &text[at + page.len()..];
            let end = rest.find(|c| !id_char(c)).unwrap_or(rest.len());
            if rest[end..].starts_with('/') {
                None
            } else {
                rest[..end].parse().ok()
            }
        })
        .collect()
}

parsed!(
    /// `claude_code_session_url`: an https URL on claude.ai, one token, where the session can
    /// be watched.
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

/// `[coding]` in the instance file, parsed: which routine a tap fires and which repository its
/// branches come back to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coding {
    /// The routine created at claude.ai/code/routines.
    pub routine: RoutineId,
    /// The repository its saved prompt names; the poll reads its branches.
    pub repo: RepoName,
}

/// What a fire answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    /// The session's id.
    pub id: SessionId,
    /// Where it can be watched.
    pub url: SessionUrl,
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
    pub reports: Vec<ReportFile>,
    /// Its diff against the default branch.
    pub stat: Vec<FileStat>,
}

/// One report file as a branch carries it, and the sessions its text names, read once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportFile {
    path: RepoPath,
    text: String,
    names: Vec<SessionId>,
}

impl ReportFile {
    /// The file at `path` saying `text`.
    #[must_use]
    pub fn read(path: RepoPath, text: String) -> Self {
        let names = sessions_named(&text);
        Self { path, text, names }
    }

    /// The file.
    #[must_use]
    pub const fn path(&self) -> &RepoPath {
        &self.path
    }

    /// Its text, whole.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the text names session `id`, in either spelling.
    #[must_use]
    pub(crate) fn speaks_for(&self, id: &SessionId) -> bool {
        self.names.iter().any(|named| named.is(id))
    }

    /// Every session the text names, as it spelled each.
    #[must_use]
    pub fn names(&self) -> &[SessionId] {
        &self.names
    }
}

/// How the poll reads one session against the branches it saw: the step it takes, or why it
/// takes none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading<'a> {
    /// A fired session's report is on this branch.
    Reported {
        /// The branch.
        found: &'a Seen,
        /// The report naming the session.
        file: &'a ReportFile,
    },
    /// A reported session's branch is on the default branch.
    Merged {
        /// The branch.
        found: &'a Seen,
    },
    /// A fired session, and no branch the poll saw carries a report.
    NoReport,
    /// A fired session, and every report the poll saw names other sessions or none.
    OtherSessions,
    /// A reported session whose branch the poll did not see.
    NotSeen,
    /// A reported session whose branch is not on the default branch yet.
    NotMerged,
    /// A merged or dropped session, which the poll never moves.
    Ended,
}

impl fmt::Display for Reading<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reported { found, file } => {
                write!(f, "reported by {} in {}", found.branch, file.path.as_str())
            }
            Self::Merged { found } => {
                write!(f, "merged: {} is on the default branch", found.branch)
            }
            Self::NoReport => {
                f.write_str(&format!("no branch carries a report under {REPORTS_DIR}"))
            }
            Self::OtherSessions => f.write_str("every report names another session, or none"),
            Self::NotSeen => f.write_str("its branch is not among the branches seen"),
            Self::NotMerged => f.write_str("its branch is not on the default branch yet"),
            Self::Ended => f.write_str("it has ended, so the poll leaves it"),
        }
    }
}

/// How the poll reads `session` against `seen`: a fired one is reported by the first branch
/// whose report names its id; a reported one is merged once its own branch's tip is on the
/// default branch.
#[must_use]
pub fn read<'a>(session: &Session, seen: &'a [Seen]) -> Reading<'a> {
    match session {
        Session::Fired { id, .. } => seen
            .iter()
            .find_map(|found| {
                found
                    .reports
                    .iter()
                    .find(|file| file.speaks_for(id))
                    .map(|file| Reading::Reported { found, file })
            })
            .unwrap_or_else(|| {
                if seen.iter().all(|found| found.reports.is_empty()) {
                    Reading::NoReport
                } else {
                    Reading::OtherSessions
                }
            }),
        Session::Reported { branch, .. } => match seen.iter().find(|found| &found.branch == branch)
        {
            None => Reading::NotSeen,
            Some(found) if found.merged => Reading::Merged { found },
            Some(_) => Reading::NotMerged,
        },
        Session::Merged { .. } | Session::Dropped { .. } => Reading::Ended,
    }
}

/// The step the poll takes for one session, as [`read`] reads it.
#[must_use]
pub fn next(session: &Session, seen: &[Seen]) -> Option<Step> {
    match read(session, seen) {
        Reading::Reported { found, file } => Some(Step::Report {
            branch: found.branch.clone(),
            report: Report::new(file.path.clone(), &file.text, found.stat.clone()),
        }),
        Reading::Merged { found } => Some(Step::Merge {
            sha: found.tip.clone(),
        }),
        Reading::NoReport
        | Reading::OtherSessions
        | Reading::NotSeen
        | Reading::NotMerged
        | Reading::Ended => None,
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
                lines: crate::coding_report::Lines::Text {
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

    fn file(text: &str) -> ReportFile {
        ReportFile::read(
            RepoPath::try_from("docs/reports/x.md".to_owned()).expect("a path"),
            text.to_owned(),
        )
    }

    fn id(text: &str) -> SessionId {
        text.parse().expect("a session id")
    }

    /// The routine API and the session spell one id two ways, and a report in either spelling
    /// speaks for the session; the spelling still prints back as it came.
    #[test]
    fn both_spellings_of_an_id_are_one_session() {
        let answered = id("cse_01T8xs3hEcBw7tQfi7Hzu3mG");
        let written = id("session_01T8xs3hEcBw7tQfi7Hzu3mG");
        assert!(answered.is(&written) && written.is(&answered));
        assert_ne!(answered, written);
        assert_eq!(answered.to_string(), "cse_01T8xs3hEcBw7tQfi7Hzu3mG");
        let line = serde_json::to_string(&written).expect("serialises");
        assert_eq!(line, "\"session_01T8xs3hEcBw7tQfi7Hzu3mG\"");
        assert_eq!(
            serde_json::from_str::<SessionId>(&line).expect("reads"),
            written
        );
        for bad in [
            "01abc",
            "cse_",
            "session_",
            "sess_01abc",
            "cse_01 abc",
            "cse_01/abc",
        ] {
            assert!(bad.parse::<SessionId>().is_err(), "{bad}");
        }
    }

    /// A report names a session only by its page's URL with the id whole, and punctuation
    /// after it still names it.
    #[test]
    fn a_report_names_a_session_only_whole() {
        let one = id("cse_1");
        assert!(file("Session: https://claude.ai/code/session_1").speaks_for(&one));
        assert!(file("(https://claude.ai/code/cse_1).").speaks_for(&one));
        assert!(!file("https://claude.ai/code/session_12").speaks_for(&one));
        assert!(!file("https://claude.ai/code/session_1/other").speaks_for(&one));
        assert!(!file("Session: session_1").speaks_for(&one));
        assert!(!file("https://claude.ai/code/session_2").speaks_for(&one));
        assert!(
            file("https://claude.ai/code/session_12 https://claude.ai/code/session_1")
                .speaks_for(&one)
        );
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
            reports: vec![file(report)],
            stat: Vec::new(),
        }
    }

    /// The poll reports a fired session only by a report naming its id, in either spelling,
    /// and merges a reported one only on its own branch's merge.
    #[test]
    fn the_poll_reports_by_the_id_and_merges_by_the_branch() {
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

    /// As on the live box: the fire answered `cse_…`, the session wrote `session_…`, and the
    /// reverse; a report naming another id reports nothing.
    #[test]
    fn a_report_in_the_other_spelling_reports_the_session() {
        let fired_as = |spelling: &str| Session::Fired {
            id: id(&format!("{spelling}01T8")),
            url: format!("https://claude.ai/code/{spelling}01T8")
                .parse()
                .expect("a url"),
            at: "2026-11-01T09:00:00".to_owned(),
        };
        for (answered, written) in [("cse_", "session_"), ("session_", "cse_")] {
            let back = seen(
                "claude/changelog-typo",
                false,
                &format!("Session: https://claude.ai/code/{written}01T8\n"),
            );
            assert!(
                matches!(
                    next(&fired_as(answered), std::slice::from_ref(&back)),
                    Some(Step::Report { .. })
                ),
                "{answered} fired, {written} written"
            );
            let elsewhere = seen(
                "claude/other",
                false,
                &format!("Session: https://claude.ai/code/{written}01T9\n"),
            );
            assert_eq!(next(&fired_as(answered), &[elsewhere]), None);
        }
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
