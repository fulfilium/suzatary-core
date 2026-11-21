//! Static secrets: the environment first, then `secretspec get`. A lookup answers a value
//! (`Ok(Some)`), nothing holds the name (`Ok(None)`), or the store could not say
//! ([`SecretError`]); a locked GPG agent is never read as "not configured". A value is only ever
//! a [`Secret`], and only its name is ever printed.

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use suzatary_base::capability::Secret;
use suzatary_connect::secrets::{Answer, SecretState, Snapshot};
use suzatary_connect::settings::{Settable, Settables};

use crate::run::{Cmd, Ran, RunError, run, zero};

/// What the secret store's providers read of the environment beyond [`crate::run::BASE`]. Any
/// child that reaches the store (mbsync's `PassCmd`) passes these too.
pub const STORE: &[&str] = &[
    "SECRETSPEC_*",
    "PROTON_PASS_*",
    "PASSWORD_STORE_*",
    "GNUPGHOME",
    "GPG_AGENT_INFO",
    "GPG_TTY",
    "DBUS_SESSION_BUS_ADDRESS",
];

/// A locked provider (a `pass` GPG agent) must not hang whatever asked.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// How a caller resolves a secret, so a test can prove none was resolved.
pub trait Resolves {
    /// One secret's value, `None` when nothing holds it.
    fn get(&self, name: &str, reason: &str) -> Result<Option<Secret>, SecretError>;
}

impl Resolves for Secretspec {
    fn get(&self, name: &str, reason: &str) -> Result<Option<Secret>, SecretError> {
        Secretspec::get(self, name, reason)
    }
}

/// Where a secret stands, read from a lookup without keeping the value.
#[must_use]
pub fn state(found: &Result<Option<Secret>, SecretError>) -> SecretState {
    match found {
        Ok(Some(_)) => SecretState::Set,
        Ok(None) => SecretState::Missing,
        Err(failed) => failed.state(),
    }
}

/// `secretspec` over one `secretspec.toml`: a fixed file, or the file an instance's
/// [`Declarations`] last declared, written on the first call that needs it.
#[derive(Debug, Clone)]
pub struct Secretspec {
    program: PathBuf,
    spec: Spec,
    timeout: Duration,
}

#[derive(Debug, Clone)]
enum Spec {
    File(PathBuf),
    /// Written on the first call.
    Declared(Declarations),
}

impl Secretspec {
    #[must_use]
    pub fn new(spec: impl Into<PathBuf>) -> Self {
        Self::with_timeout(spec, DEFAULT_TIMEOUT)
    }

    /// `timeout` per `secretspec` call.
    fn with_timeout(spec: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            program: PathBuf::from("secretspec"),
            spec: Spec::File(spec.into()),
            timeout,
        }
    }

    /// One secret's value; `reason` is secretspec's audit trail.
    pub fn get(&self, name: &str, reason: &str) -> Result<Option<Secret>, SecretError> {
        let from_env = env::var(name).ok();
        Self::resolve(from_env.as_deref(), || self.asked(name, reason))
    }

    /// A non-empty environment value wins and `subprocess` never runs.
    fn resolve(
        env_value: Option<&str>,
        subprocess: impl FnOnce() -> Result<Option<Secret>, SecretError>,
    ) -> Result<Option<Secret>, SecretError> {
        if let Some(trimmed) = given(env_value) {
            return Ok(Some(Secret::new(trimmed)));
        }
        subprocess()
    }

    /// `names` as [`Secretspec::get`] reads each, in one `secretspec check`.
    #[must_use]
    pub fn snapshot(&self, names: &[Settable], reason: &str) -> Snapshot {
        use suzatary_lines::parts::part;
        let (from_env, asking): (Vec<&Settable>, Vec<&Settable>) = names.iter().partition(|name| {
            part("environment", || {
                given(env::var(name.name()).ok().as_deref()).is_some()
            })
        });
        let mut answers: std::collections::BTreeMap<Settable, Answer> = from_env
            .into_iter()
            .map(|name| (name.clone(), Answer::Set))
            .collect();
        if asking.is_empty() {
            return Snapshot::of(Ok(answers));
        }
        let asked = match part("store", || self.checked(reason, asking.len())) {
            Ok(report) => Ok(asking
                .into_iter()
                .map(|name| (name.clone(), report.answer(name.name())))
                .collect::<Vec<_>>()),
            Err(failed) => Answer::of(failed.state()).map(|answer| {
                asking
                    .into_iter()
                    .map(|name| (name.clone(), answer))
                    .collect()
            }),
        };
        Snapshot::of(asked.map(|asked| {
            answers.extend(asked);
            answers
        }))
    }

    /// `secretspec check --json`'s report. It exits non-zero when a required name is missing and
    /// still reports, so a report that parses is read whatever the exit; with none, the exit
    /// says why, as `get`'s does. It may take as long as `get` was given for each of the `names`
    /// it answers, since it decrypts each: a slow store must not read as locked.
    fn checked(&self, reason: &str, names: usize) -> Result<Report, SecretError> {
        const EVERY: &str = "every declared name";
        let spec = self.configured(EVERY)?;
        let cmd = Cmd::new(&self.program)
            .args(["check", "--json", "--file"])
            .arg(&spec)
            .args(["--reason", reason])
            .pass(STORE)
            .timeout(
                self.timeout
                    .saturating_mul(u32::try_from(names).unwrap_or(u32::MAX)),
            );
        let refused = |failed| SecretError::of(EVERY, Verb::Check, &failed);
        let ran = self.held(Verb::Check, &cmd).map_err(refused)?;
        if let Some(report) = Report::in_(&ran.stdout) {
            return Ok(report);
        }
        zero(&cmd, ran).map_err(refused)?;
        Err(SecretError::Unread {
            name: EVERY.to_owned(),
        })
    }

    /// The spec file to ask by, declared now if it was not yet. A missing one is a store to
    /// repair, never one to unlock.
    fn configured(&self, name: &str) -> Result<PathBuf, SecretError> {
        let spec = match &self.spec {
            Spec::File(file) => file.clone(),
            Spec::Declared(declarations) => {
                declarations
                    .opened()
                    .map(Declared::file)
                    .map_err(|failed| SecretError::Undeclared {
                        name: name.to_owned(),
                        why: failed.to_string(),
                    })?
            }
        };
        if spec.is_file() {
            Ok(spec)
        } else {
            Err(SecretError::NoSpec {
                name: name.to_owned(),
            })
        }
    }

    fn command(&self, spec: &Path, verb: &str, name: &str, reason: &str) -> Cmd {
        Cmd::new(&self.program)
            .args([verb, name, "--file"])
            .arg(spec)
            .args(["--reason", reason])
            .pass(STORE)
            .timeout(self.timeout)
    }

    /// Write a secret through `secretspec set`, the value on stdin because argv is
    /// world-readable in `/proc`. The error never repeats `secretspec`'s stderr, which could
    /// echo the value back.
    pub fn set(&self, name: &str, value: &Secret, reason: &str) -> Result<(), SecretError> {
        let spec = self.configured(name)?;
        let cmd = self
            .command(&spec, "set", name, reason)
            .stdin(format!("{}\n", value.expose()));
        let verb = Verb::Set(value);
        self.succeeded(verb, &cmd)
            .map(drop)
            .map_err(|failed| SecretError::of(name, verb, &failed))
    }

    pub fn delete(&self, name: &str, reason: &str) -> Result<(), SecretError> {
        let spec = self.configured(name)?;
        let cmd = self.command(&spec, "delete", name, reason);
        self.succeeded(Verb::Delete, &cmd)
            .map(drop)
            .map_err(|failed| SecretError::of(name, Verb::Delete, &failed))
    }

    /// The last line of `secretspec get`'s stdout: some providers print a banner first. A
    /// refusal is "unset" only when the store says the name is not found.
    fn asked(&self, name: &str, reason: &str) -> Result<Option<Secret>, SecretError> {
        let spec = self.configured(name)?;
        let cmd = self.command(&spec, "get", name, reason);
        let ran = match self.succeeded(Verb::Get, &cmd) {
            Ok(ran) => ran,
            Err(RunError::Exited { stderr, .. })
                if stderr
                    .as_str()
                    .contains(&format!("Secret '{name}' not found")) =>
            {
                return Ok(None);
            }
            Err(failed) => return Err(SecretError::of(name, Verb::Get, &failed)),
        };
        let stdout = String::from_utf8(ran.stdout).map_err(|_| SecretError::Unread {
            name: name.to_owned(),
        })?;
        Ok(stdout
            .trim()
            .lines()
            .next_back()
            .map(str::trim)
            .filter(|last| !last.is_empty())
            .map(Secret::new))
    }
}

/// One lock per store, keyed by the file every spec of it extends; each store call holds its
/// store's, a write alone: a dotenv store is rewritten whole and a `pass` store commits each
/// write to one git repository. It orders this process's calls only.
type Locks = std::collections::BTreeMap<PathBuf, Arc<parking_lot::RwLock<()>>>;
static STORES: parking_lot::Mutex<Locks> = parking_lot::const_mutex(Locks::new());

impl Secretspec {
    /// The store's identity: its file, or for a declared store the code root's file that each
    /// declared file extends (a widened declaration is a new file of the same store).
    fn store(&self) -> &Path {
        match &self.spec {
            Spec::File(file) => file,
            Spec::Declared(declarations) => &declarations.base,
        }
    }

    /// `cmd` run under its store's lock. A call waits for the lock at most its timeout, and is
    /// [`RunError::TimedOut`] if it could not start in it; a child that starts has its whole
    /// timeout, so a call takes at most twice its timeout.
    fn held(&self, verb: Verb<'_>, cmd: &Cmd) -> Result<Ran, RunError> {
        let after = cmd.timeout.unwrap_or(Duration::MAX);
        let timed_out = || RunError::TimedOut {
            program: cmd.program.to_string_lossy().into_owned(),
            after,
        };
        let lock = Arc::clone(STORES.lock().entry(self.store().to_path_buf()).or_default());
        match verb {
            Verb::Get | Verb::Check => {
                let _reading = lock.try_read_for(after).ok_or_else(timed_out)?;
                run(cmd)
            }
            Verb::Set(_) | Verb::Delete => {
                let _writing = lock.try_write_for(after).ok_or_else(timed_out)?;
                run(cmd)
            }
        }
    }

    /// [`Secretspec::held`], and a non-zero exit as [`RunError::Exited`].
    fn succeeded(&self, verb: Verb<'_>, cmd: &Cmd) -> Result<Ran, RunError> {
        self.held(verb, cmd).and_then(|ran| zero(cmd, ran))
    }
}

/// A non-empty value, trimmed.
fn given(env_value: Option<&str>) -> Option<&str> {
    env_value.map(str::trim).filter(|value| !value.is_empty())
}

/// `secretspec check --json`'s report: each declared name's status, never a value.
#[derive(Debug, serde::Deserialize)]
struct Report {
    secrets: Vec<Reported>,
}

#[derive(Debug, serde::Deserialize)]
struct Reported {
    name: String,
    status: Status,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Resolved,
    MissingOptional,
    MissingRequired,
    /// Neither set nor missing.
    #[serde(other)]
    Unknown,
}

impl Report {
    /// The report from the first line that opens it: some providers print a banner first, as
    /// `get`'s last-line reading allows for.
    fn in_(stdout: &[u8]) -> Option<Self> {
        let at = stdout
            .iter()
            .enumerate()
            .find(|&(at, byte)| *byte == b'{' && (at == 0 || stdout[at - 1] == b'\n'))?
            .0;
        serde_json::from_slice(&stdout[at..]).ok()
    }

    /// As `get` answers `name`: a name the report does not hold is one the store does not find.
    fn answer(&self, name: &str) -> Answer {
        self.secrets
            .iter()
            .find(|reported| reported.name == name)
            .map_or(Answer::Missing, |reported| match reported.status {
                Status::Resolved => Answer::Set,
                Status::MissingOptional | Status::MissingRequired => Answer::Missing,
                Status::Unknown => Answer::Failed,
            })
    }
}

/// Why the secret store could not answer for a name. It never carries the value, nor the
/// store's stderr, since that could echo the value back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("the secret store could not be started for {name}")]
    Spawn { name: String },
    #[error("the secret store has no spec file to read {name} by")]
    NoSpec { name: String },
    /// The file declaring the instance's names could not be written.
    #[error("the secret store's names could not be declared for {name}: {why}")]
    Undeclared { name: String, why: String },
    /// Killed at the timeout: a locked store waits for a passphrase nobody will type.
    #[error("the secret store did not answer for {name} in time (is it locked?)")]
    Timeout { name: String },
    /// A non-zero exit other than "not found": how it ended, and what its stderr says went wrong
    /// as one word ([`Refusal`]), never the stderr itself.
    #[error("the secret store refused {name} ({exit}: {refusal})")]
    Refused {
        name: String,
        exit: Exit,
        refusal: Refusal,
    },
    #[error("the secret store's answer for {name} did not read")]
    Unread { name: String },
    #[error("the secret store was started for {name} and could not be waited on")]
    Lost { name: String },
    #[error("the secret store was stopped before it answered for {name}")]
    Stopped { name: String },
}

/// How a refused call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    /// Stopped by a signal, with no code.
    Signal(i32),
}

impl std::fmt::Display for Exit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code(code) => write!(f, "exit {code}"),
            Self::Signal(signal) => write!(f, "signal {signal}"),
        }
    }
}

impl Exit {
    // On Unix a status with no code was ended by a signal.
    fn of(status: std::process::ExitStatus) -> Self {
        use std::os::unix::process::ExitStatusExt as _;
        status.code().map_or_else(
            || Self::Signal(status.signal().unwrap_or_default()),
            Self::Code,
        )
    }
}

/// What a refusal's stderr says went wrong, read as one of a few words so the stderr, which could
/// echo a value, is never repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The store's file did not parse, as a file another writer left half written reads.
    Unparsed,
    /// The store's file or its directory could not be read or written.
    Denied,
    /// A key or a passphrase was missing: the store is locked.
    Locked,
    /// Nothing the words above name.
    Unsaid,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unparsed => "its file did not parse",
            Self::Denied => "its file could not be read or written",
            Self::Locked => "is it locked?",
            Self::Unsaid => "it said nothing this box reads",
        })
    }
}

impl Refusal {
    fn of(stderr: &str) -> Self {
        let said = stderr.to_lowercase();
        let any = |words: &[&str]| words.iter().any(|word| said.contains(word));
        if any(&["permission denied", "read-only file system"]) {
            Self::Denied
        } else if any(&[
            "parse",
            "unterminated",
            "unexpected character",
            "invalid line",
        ]) {
            Self::Unparsed
        } else if any(&[
            "passphrase",
            "pinentry",
            "decryption failed",
            "no secret key",
            "locked",
        ]) {
            Self::Locked
        } else {
            Self::Unsaid
        }
    }
}

impl SecretError {
    /// Locked when an unlock may answer, failed when it cannot.
    #[must_use]
    pub fn state(&self) -> SecretState {
        match self {
            Self::Timeout { .. }
            | Self::Stopped { .. }
            | Self::Refused {
                refusal: Refusal::Locked | Refusal::Unsaid,
                ..
            } => SecretState::Locked,
            Self::Refused {
                refusal: Refusal::Unparsed | Refusal::Denied,
                ..
            }
            | Self::Spawn { .. }
            | Self::NoSpec { .. }
            | Self::Undeclared { .. }
            | Self::Unread { .. }
            | Self::Lost { .. } => SecretState::Failed,
        }
    }

    /// What the operator does about it.
    #[must_use]
    pub fn remedy(&self) -> &'static str {
        match self {
            Self::Timeout { .. }
            | Self::Refused {
                refusal: Refusal::Locked | Refusal::Unsaid,
                ..
            } => "unlock the secret store",
            Self::Refused {
                refusal: Refusal::Unparsed,
                ..
            } => "repair the secret store (its file does not parse)",
            Self::Refused {
                refusal: Refusal::Denied,
                ..
            } => "repair the secret store (can this user read and write its file?)",
            Self::Stopped { .. } => "ask again",
            Self::Spawn { .. } => "repair the secret store (is secretspec on PATH?)",
            Self::NoSpec { .. } => "repair the secret store (is its spec file there?)",
            Self::Undeclared { .. } => {
                "repair the state directory (can this user write its secretspec files?)"
            }
            Self::Unread { .. } | Self::Lost { .. } => {
                "repair the secret store (does secretspec answer at a terminal?)"
            }
        }
    }

    /// A refusal leaves [`Said`] in the log, at WARN when it is [`Refusal::Unsaid`].
    fn of(name: &str, verb: Verb<'_>, failed: &RunError) -> Self {
        let owned = name.to_owned();
        match failed {
            RunError::Spawn { .. } => Self::Spawn { name: owned },
            RunError::TimedOut { .. } => Self::Timeout { name: owned },
            RunError::Halted { .. } => Self::Stopped { name: owned },
            RunError::Wait { .. } => Self::Lost { name: owned },
            RunError::Exited { status, stderr, .. } => {
                let (exit, refusal) = (Exit::of(*status), Refusal::of(stderr.as_str()));
                let said = Said::of(stderr.as_str(), name, verb);
                match refusal {
                    Refusal::Unsaid => tracing::warn!(
                        verb = verb.word(), secret = name, %exit, %said,
                        "the secret store refused in words this box does not read"
                    ),
                    Refusal::Unparsed | Refusal::Denied | Refusal::Locked => tracing::debug!(
                        verb = verb.word(), secret = name, %exit, %refusal, %said,
                        "the secret store refused"
                    ),
                }
                #[cfg(feature = "fixture")]
                tracing::warn!(
                    verb = verb.word(), secret = name, %stderr,
                    "the secret store's stderr, which only the fixture build logs"
                );
                Self::Refused {
                    name: owned,
                    exit,
                    refusal,
                }
            }
        }
    }
}

/// A call of the store; a write holds the value [`Said`] leaves out.
#[derive(Clone, Copy)]
enum Verb<'a> {
    Get,
    Check,
    Set(&'a Secret),
    Delete,
}

impl Verb<'_> {
    fn word(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Check => "check",
            Self::Set(_) => "set",
            Self::Delete => "delete",
        }
    }
}

/// What a refusal's stderr said, as a log may quote it. The value a `set` wrote, its trimmed
/// whole and each of its lines are first put out of the stderr, wherever each occurs as a
/// substring, by a stand-in of its length; then the reduction, which never looks at the value
/// again, keeps the line and character counts, the name asked for, the words of [`WORDS`] and
/// the errno after `os error`, and makes everything else `…`. So a stderr that echoes the value
/// reads as one that echoes the stand-in, and one that does not contain the value, whole or a
/// line of it, reads the same whatever the value. A value or line of one or two characters is
/// put out of every word that contains it. The counts can tell an echoed value's length; for
/// `get`, `check` and `delete` no value is known, so a store that echoed another secret would
/// show it only if that secret were itself a word of the list. Its one constructor is that
/// reduction, so no log site of the shipped build can quote the stderr itself.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Said {
    lines: usize,
    chars: usize,
    words: String,
}

/// The words a store, its providers and the OS use to say why a call failed.
const WORDS: &str = "\
    a access already an and at audit available backend broken busy by call can cannot character \
    check commit config could create declared decryption defined delete denied device directory \
    disk dotenv empty encryption end entry eof error exists extended failed fatal file files for \
    found from get git gpg in index input interrupted invalid io is key left line lock locked \
    many missing no not note of on only open operation or os parse parsing pass passphrase \
    permission pinentry pipe profile project provider quota read reading required resource saved \
    secret secretspec set space spec store such system temporarily text the timed to toml too \
    unable unavailable unexpected unterminated utf valid value warning with write writing \
    written";

impl Said {
    fn of(stderr: &str, name: &str, verb: Verb<'_>) -> Self {
        let mut said = stderr.to_owned();
        if let Verb::Set(value) = verb {
            let value = value.expose();
            let mut whole: Vec<&str> = [value, value.trim()]
                .into_iter()
                .chain(value.lines().map(str::trim))
                .filter(|whole| !whole.is_empty())
                .collect();
            whole.sort_by_key(|whole| std::cmp::Reverse(whole.len()));
            for whole in whole {
                said = said.replace(whole, &"x".repeat(whole.chars().count()));
            }
        }
        let lines: Vec<String> = said
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let mut kept: Vec<&str> = Vec::new();
                for token in line
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .filter(|token| !token.is_empty())
                {
                    let lower = token.to_lowercase();
                    let errno = token.len() <= 3
                        && token.bytes().all(|b| b.is_ascii_digit())
                        && kept.ends_with(&["os", "error"]);
                    let word = if token == name
                        || WORDS.split_whitespace().any(|word| word == lower)
                        || errno
                    {
                        token
                    } else {
                        "…"
                    };
                    if !(word == "…" && kept.last() == Some(&"…")) {
                        kept.push(word);
                    }
                }
                kept.join(" ")
            })
            .collect();
        Self {
            lines: lines.len(),
            chars: said.chars().count(),
            words: lines.join(" | "),
        }
    }
}

impl std::fmt::Display for Said {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} line(s), {} chars: {}",
            self.lines, self.chars, self.words
        )
    }
}

impl suzatary_connect::settings::Writes for Secretspec {
    fn write_secret(
        &self,
        name: &suzatary_connect::settings::Settable,
        value: &suzatary_base::capability::Secret,
    ) -> Result<(), String> {
        self.set(
            name.name(),
            value,
            "the settings page: the owner set a secret",
        )
        .map_err(|failed| failed.to_string())
    }

    fn clear_secret(&self, name: &suzatary_connect::settings::Settable) -> Result<(), String> {
        self.delete(name.name(), "a connection the owner removed")
            .map_err(|failed| failed.to_string())
    }
}

/// The file the store reads a deployment's names from: the code root's `secretspec.toml`,
/// extended by a file in `dir` declaring every derived name it does not. Each file is named by
/// its text's digest and never rewritten, so no writer drops a name another declared.
#[derive(Debug, Clone)]
pub struct Declared {
    base: Base,
    dir: PathBuf,
    current: Arc<RwLock<PathBuf>>,
}

#[derive(Debug, Clone)]
enum Base {
    /// No file: no store, every name refused as [`SecretError::NoSpec`].
    Absent,
    Own(Own),
}

/// What the generated file takes from the code root's own.
#[derive(Debug, Clone)]
struct Own {
    path: PathBuf,
    /// As `extends` names it.
    directory: String,
    /// `[project] name`, under which every value is stored.
    name: String,
    revision: Option<String>,
    /// Its `[profiles.default]` names, which the generated file leaves to it.
    declared: BTreeSet<String>,
}

impl Base {
    fn read(path: &Path) -> Result<Self, DeclareError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Own::parse(path, &text).map(Self::Own),
            Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => Ok(Self::Absent),
            Err(failed) => Err(DeclareError::Read {
                path: path.to_path_buf(),
                said: failed.to_string(),
            }),
        }
    }
}

impl Own {
    fn parse(path: &Path, text: &str) -> Result<Self, DeclareError> {
        let refused = |reason: &str| DeclareError::Parse {
            path: path.to_path_buf(),
            reason: reason.to_owned(),
        };
        let parsed: toml::Table = text.parse().map_err(|_| refused("it is not TOML"))?;
        let project = parsed
            .get("project")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| refused("it has no [project]"))?;
        let name = project
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| refused("its [project] has no name"))?
            .to_owned();
        let revision = project
            .get("revision")
            .map(|revision| {
                revision
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| refused("its [project] revision is not a string"))
            })
            .transpose()?;
        let declared = parsed
            .get("profiles")
            .and_then(|profiles| profiles.get("default"))
            .and_then(toml::Value::as_table)
            .map(|default| default.keys().cloned().collect())
            .unwrap_or_default();
        let directory = path
            .parent()
            .and_then(Path::to_str)
            .ok_or_else(|| refused("its directory is not a UTF-8 path"))?
            .to_owned();
        Ok(Self {
            path: path.to_path_buf(),
            directory,
            name,
            revision,
            declared,
        })
    }

    /// The text of the file declaring every one of `names` this file does not. It keeps this
    /// file's project, since values are stored under the project's name and a new one would lose
    /// every secret already set. A pure function, so one set of names is one file.
    fn declaring(&self, names: &Settables) -> Result<String, DeclareError> {
        let mut project = toml::Table::new();
        project.insert("name".to_owned(), self.name.clone().into());
        if let Some(revision) = &self.revision {
            project.insert("revision".to_owned(), revision.clone().into());
        }
        project.insert(
            "extends".to_owned(),
            toml::Value::Array(vec![self.directory.clone().into()]),
        );
        let default: toml::Table = names
            .declarations()
            .filter(|(settable, _)| !self.declared.contains(settable.name()))
            .map(|(settable, by)| {
                let described: Vec<String> = by
                    .iter()
                    .map(suzatary_connect::settings::Declaration::sentence)
                    .collect();
                let entry = toml::Table::from_iter([
                    (
                        "description".to_owned(),
                        format!("Declared by {}.", described.join(" and ")).into(),
                    ),
                    ("required".to_owned(), false.into()),
                ]);
                (settable.name().to_owned(), toml::Value::Table(entry))
            })
            .collect();
        let file = toml::Table::from_iter([
            ("project".to_owned(), toml::Value::Table(project)),
            (
                "profiles".to_owned(),
                toml::Value::Table(toml::Table::from_iter([(
                    "default".to_owned(),
                    toml::Value::Table(default),
                )])),
            ),
        ]);
        let body = toml::to_string(&file).map_err(|_| DeclareError::Parse {
            path: self.path.clone(),
            reason: "its names do not print as TOML".to_owned(),
        })?;
        Ok(format!(
            "# Written by suzatary: the names this instance derives, beside {}.\n\n{body}",
            self.path.display()
        ))
    }
}

impl Declared {
    /// The file declaring `names` beside `base`, written in `dir`. With no `base`, the store
    /// refuses every name as [`SecretError::NoSpec`].
    pub fn of(base: PathBuf, dir: PathBuf, names: &Settables) -> Result<Self, DeclareError> {
        let declared = Self {
            base: Base::read(&base)?,
            current: Arc::new(RwLock::new(base)),
            dir,
        };
        declared.declare(names)?;
        Ok(declared)
    }

    /// Declare exactly `names`: the file is written before any store reads it.
    ///
    /// # Errors
    /// [`DeclareError`], the names declared before kept.
    pub fn declare(&self, names: &Settables) -> Result<(), DeclareError> {
        let own = match &self.base {
            Base::Absent => return Ok(()),
            Base::Own(own) => own,
        };
        let declaring = own.declaring(names)?;
        let file = self
            .dir
            .join(format!("secretspec-{}.toml", digest(&declaring)));
        write_once(&self.dir, &file, &declaring)?;
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = file;
        Ok(())
    }

    /// The file this last declared, which `secretspec set <NAME> --file` takes at a terminal.
    #[must_use]
    pub fn file(&self) -> PathBuf {
        self.current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// An instance's names as they will be declared, made with no I/O. The first call that needs
/// the file declares it ([`Declarations::opened`]); every clone shares that one [`Declared`].
#[derive(Debug, Clone)]
pub struct Declarations {
    base: PathBuf,
    dir: PathBuf,
    names: Settables,
    opened: Arc<std::sync::OnceLock<Declared>>,
}

impl Declarations {
    #[must_use]
    pub fn new(base: PathBuf, dir: PathBuf, names: Settables) -> Self {
        Self {
            base,
            dir,
            names,
            opened: Arc::default(),
        }
    }

    /// The declared file, written now if no call wrote it yet; a failure is retried next call.
    pub fn opened(&self) -> Result<&Declared, DeclareError> {
        if let Some(declared) = self.opened.get() {
            return Ok(declared);
        }
        let declared = Declared::of(self.base.clone(), self.dir.clone(), &self.names)?;
        // Two first calls write the same file, its name its text's digest; one is kept.
        Ok(self.opened.get_or_init(|| declared))
    }

    #[must_use]
    pub fn store(&self) -> Secretspec {
        Secretspec {
            spec: Spec::Declared(self.clone()),
            ..Secretspec::new(PathBuf::new())
        }
    }
}

impl suzatary_connect::settings::Declares for Declarations {
    fn declare(&self, names: &Settables) -> Result<(), String> {
        self.opened()
            .and_then(|declared| declared.declare(names))
            .map_err(|failed| failed.to_string())
    }
}

/// The first 16 hex digits of the sha256 of `text`: a file's name, never a proof.
fn digest(text: &str) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(text.as_bytes()))[..16].to_owned()
}

/// Written whole through a temporary, unless the file exists (its name is its text's digest).
fn write_once(dir: &Path, file: &Path, text: &str) -> Result<(), DeclareError> {
    let failed = |source: std::io::Error| DeclareError::Write {
        path: file.to_path_buf(),
        said: source.to_string(),
    };
    if file.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(failed)?;
    let mut temporary = tempfile::NamedTempFile::new_in(dir).map_err(failed)?;
    std::io::Write::write_all(&mut temporary, text.as_bytes()).map_err(failed)?;
    temporary
        .persist(file)
        .map(drop)
        .map_err(|persisting| failed(persisting.error))
}

/// Why a deployment's names could not be declared to the store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeclareError {
    #[error("the secret store's own file {path} did not read: {said}")]
    Read { path: PathBuf, said: String },
    #[error("the secret store's own file {path} is refused: {reason}")]
    Parse { path: PathBuf, reason: String },
    #[error("the file declaring this instance's secret names, {path}, was not written: {said}")]
    Write { path: PathBuf, said: String },
}

/// A fresh signing key's seed, as 64 lowercase hex: what `suzatary device init` stores under a
/// key's name, never shown.
pub fn fresh_seed() -> Result<Secret, String> {
    let bytes: [u8; 32] = ring::rand::generate(&ring::rand::SystemRandom::new())
        .map(ring::rand::Random::expose)
        .map_err(|_| "the system's random generator failed".to_owned())?;
    Ok(Secret::new(hex::encode(bytes)))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn fake(script: &str, timeout: Duration) -> (tempfile::TempDir, Secretspec) {
        let dir = suzatary_testkit::scratch("secrets");
        let program = suzatary_testkit::fake_program(
            dir.path(),
            "secretspec",
            &format!("#!/bin/sh\n{script}\n"),
        );
        std::fs::write(dir.path().join("secretspec.toml"), "").expect("a spec file");
        let spec = Secretspec {
            program,
            ..Secretspec::with_timeout(dir.path().join("secretspec.toml"), timeout)
        };
        (dir, spec)
    }

    const NOBODY: &str = "SUZATARY_TEST_SECRET_NOBODY_SETS";

    #[test]
    fn the_environment_wins_and_the_subprocess_is_never_asked() {
        let asked = Cell::new(false);
        let secret = Secretspec::resolve(Some("  from-env  "), || {
            asked.set(true);
            Ok(None)
        });
        assert_eq!(
            secret
                .expect("answered")
                .expect("an env value was given")
                .expose(),
            "from-env"
        );
        assert!(
            !asked.get(),
            "the subprocess must not run once the environment already answered"
        );
    }

    #[test]
    fn a_name_the_store_does_not_hold_is_unset() {
        let (_dir, spec) = fake(
            "echo \"Error: Secret '$2' not found\" >&2; exit 1",
            Duration::from_secs(5),
        );
        assert_eq!(spec.get(NOBODY, "test"), Ok(None));
    }

    /// A locked GPG agent waits for a passphrase; the timeout must not read as "not configured".
    #[test]
    fn a_store_that_does_not_answer_is_locked_never_unset() {
        let (_dir, spec) = fake("sleep 5", Duration::from_millis(200));
        let found = spec.get(NOBODY, "test");
        assert_eq!(
            found,
            Err(SecretError::Timeout {
                name: NOBODY.to_owned()
            })
        );
        assert_eq!(state(&found), SecretState::Locked);
    }

    #[test]
    fn any_other_refusal_is_the_store_s_failure_not_an_answer() {
        let (_dir, spec) = fake(
            "echo 'gpg: decryption failed: No secret key' >&2; exit 2",
            Duration::from_secs(5),
        );
        assert_eq!(
            spec.get(NOBODY, "test"),
            Err(SecretError::Refused {
                name: NOBODY.to_owned(),
                exit: Exit::Code(2),
                refusal: Refusal::Locked,
            })
        );
    }

    #[test]
    fn a_store_that_cannot_start_is_unavailable() {
        let dir = suzatary_testkit::scratch("secrets");
        std::fs::write(dir.path().join("secretspec.toml"), "").expect("a spec file");
        let spec = Secretspec {
            program: "/nonexistent/secretspec".into(),
            ..Secretspec::with_timeout(dir.path().join("secretspec.toml"), Duration::from_secs(5))
        };
        assert_eq!(
            spec.get(NOBODY, "test"),
            Err(SecretError::Spawn {
                name: NOBODY.to_owned()
            })
        );
    }

    #[test]
    fn a_store_with_no_spec_file_failed_and_was_never_run() {
        let spec = Secretspec {
            program: "/nonexistent/secretspec-must-not-run".into(),
            ..Secretspec::with_timeout("/nonexistent/secretspec.toml", Duration::from_secs(5))
        };
        let found = spec.get(NOBODY, "test");
        assert_eq!(
            found,
            Err(SecretError::NoSpec {
                name: NOBODY.to_owned()
            })
        );
        assert_eq!(state(&found), SecretState::Failed);
    }

    /// The store asked name by name.
    struct EachName<'a>(&'a Secretspec);

    impl suzatary_connect::secrets::Secrets for EachName<'_> {
        fn secret_state(&self, settable: &Settable) -> SecretState {
            state(&self.0.get(settable.name(), "test"))
        }
        fn mail_grants(
            &self,
            _: &suzatary_mail::mail::account::MailAccount,
        ) -> suzatary_connect::secrets::MailGrants {
            suzatary_connect::secrets::MailGrants::default()
        }
        fn google_grant(
            &self,
            _: &suzatary_mail::mail::address::EmailAddress,
        ) -> suzatary_google::google::Held {
            suzatary_google::google::Held::NotSignedIn
        }
    }

    /// Law (which cards are filed rests on it): one `check --json` reads every name as `get`
    /// reads each, on every kind of answer, a whole store failing included, and runs the store
    /// once where `get` runs it once per name. Its status words are secretspec 0.21.1's, as a
    /// `pass` store printed them: `resolved`, `missing_optional`, and `missing_required`, which
    /// exits 1 and still reports.
    #[test]
    fn one_check_reads_what_each_get_reads_in_one_run() {
        const SET: &str = "PLUGIN_ICS_TEST_SET";
        const UNSET: &str = "PLUGIN_ICS_TEST_UNSET";
        const REQUIRED: &str = "PLUGIN_ICS_TEST_REQUIRED";
        let declared = derived(&[SET, UNSET, REQUIRED]);
        let names: Vec<Settable> = [SET, UNSET, REQUIRED]
            .map(|name| Settable::mint(&declared, name).expect("declared"))
            .to_vec();
        let answering = format!(
            "case \"$1\" in\n\
             get) case \"$2\" in {SET}) echo hunter2;; *) echo \"Error: Secret '$2' not found\" >&2; exit 1;; esac;;\n\
             check) printf '{{\"secrets\": [\
               {{\"name\": \"{SET}\", \"status\": \"resolved\"}},\
               {{\"name\": \"{UNSET}\", \"status\": \"missing_optional\"}},\
               {{\"name\": \"{REQUIRED}\", \"status\": \"missing_required\"}}]}}'; exit 1;;\n\
             esac"
        );
        let bannered = format!("echo 'Using the pass provider'\n{answering}");
        for (store, script, timeout) in [
            ("answering", answering.as_str(), Duration::from_secs(5)),
            (
                "locked",
                "echo 'gpg: decryption failed: No secret key' >&2; exit 2",
                Duration::from_secs(5),
            ),
            (
                "unparsed",
                "echo 'Error: failed to parse .env at line 3' >&2; exit 1",
                Duration::from_secs(5),
            ),
            ("silent", "sleep 5", Duration::from_millis(200)),
            ("bannered", &bannered, Duration::from_secs(5)),
        ] {
            let (dir, spec) = fake(
                &format!("echo run >> \"$(dirname \"$0\")/runs\"\n{script}"),
                timeout,
            );
            let runs = || {
                std::fs::read_to_string(dir.path().join("runs"))
                    .map_or(0, |runs| runs.lines().count())
            };
            let each = suzatary_connect::secrets::Snapshot::each(&EachName(&spec), &names);
            let before = runs();
            assert_eq!(spec.snapshot(&names, "test"), each, "{store}");
            assert_eq!(runs() - before, 1, "{store}: one run of the store");
            assert_eq!(
                spec.snapshot(&[], "test"),
                suzatary_connect::secrets::Snapshot::of(Ok(std::collections::BTreeMap::new()))
            );
            assert_eq!(runs() - before, 1, "{store}: no run for no name");
        }
    }

    #[test]
    fn a_secret_never_prints_its_value() {
        let secret = Secret::new("hunter2-do-not-print-me");
        let shown = format!("{secret:?}");
        assert!(!shown.contains("hunter2"));
        assert_eq!(shown, "Secret(<redacted>)");
    }

    #[test]
    fn set_is_refused_when_the_store_refuses() {
        let (_dir, spec) = fake("cat >/dev/null; exit 1", Duration::from_secs(5));
        let failed = spec
            .set(NOBODY, &Secret::new("value"), "test")
            .expect_err("refused");
        assert!(matches!(failed, SecretError::Refused { .. }));
    }

    /// A refusal says how the call ended and what kind of failure its stderr names, in a word of
    /// this box's, and never repeats the stderr, which may echo the value: a store left half
    /// written by another writer reads as a file that did not parse, to repair, not to unlock.
    #[test]
    fn a_refusal_says_its_exit_and_class_and_never_the_stderr() {
        for (stderr, exit, refusal, state) in [
            (
                "Error: failed to parse .env at line 3: unterminated quote near hunter2",
                1,
                Refusal::Unparsed,
                SecretState::Failed,
            ),
            (
                "Error: Permission denied (os error 13) writing hunter2",
                3,
                Refusal::Denied,
                SecretState::Failed,
            ),
            (
                "gpg: decryption failed: No secret key hunter2",
                2,
                Refusal::Locked,
                SecretState::Locked,
            ),
            (
                "something else entirely: hunter2",
                4,
                Refusal::Unsaid,
                SecretState::Locked,
            ),
        ] {
            let (_dir, spec) = fake(
                &format!("cat >/dev/null; echo '{stderr}' >&2; exit {exit}"),
                Duration::from_secs(5),
            );
            let failed = spec
                .set(NOBODY, &Secret::new("hunter2"), "test")
                .expect_err("refused");
            assert_eq!(
                failed,
                SecretError::Refused {
                    name: NOBODY.to_owned(),
                    exit: Exit::Code(exit),
                    refusal
                },
                "{stderr}"
            );
            assert_eq!(failed.state(), state, "{stderr}");
            let shown = failed.to_string();
            assert!(shown.contains(&format!("exit {exit}")), "{shown}");
            assert!(!shown.contains("hunter2"), "{shown}");
        }
    }

    /// A value with separators, newlines and letters beyond ASCII.
    const VALUES: &str = "[a-zA-Z0-9 _:/.=+'\"\\-\néß中🔑]{1,40}";

    proptest::proptest! {
        /// Law (a refusal's log line rests on it), its first half: a stderr that repeats the value
        /// written reads as one that holds a stand-in of its length there instead; and the name
        /// and every listed word around it are kept.
        #[test]
        fn a_refusal_s_evidence_reads_an_echoed_value_as_its_stand_in(
            value in VALUES,
            words in proptest::collection::vec(proptest::sample::select(WORDS.split_whitespace().collect::<Vec<_>>()), 0..12),
            at in 0usize..12,
        ) {
            let said = |what: &str| {
                let mut said: Vec<&str> = words.clone();
                said.insert(at.min(said.len()), what);
                format!("{}\nError: {NOBODY} (os error 16) '{what}'", said.join(" "))
            };
            let template = said("");
            let whole = [value.as_str(), value.trim()].into_iter().chain(value.lines().map(str::trim));
            proptest::prop_assume!(whole.filter(|w| !w.is_empty()).all(|w| !template.contains(w)));
            let secret = Secret::new(value.clone());
            let evidence = |stderr: &str| Said::of(stderr, NOBODY, Verb::Set(&secret));
            let shown = evidence(&said(&value));
            proptest::prop_assert_eq!(&shown, &evidence(&said(&"x".repeat(value.chars().count()))));
            for word in &words {
                proptest::prop_assert!(shown.to_string().contains(word), "{word}: {shown}");
            }
        }

        /// Its second half: a stderr that does not contain the value, whole or a line of it, as a
        /// substring reads the same whatever the value written.
        #[test]
        fn a_refusal_s_evidence_of_a_stderr_without_the_value_is_the_same_for_every_value(
            one in VALUES,
            other in VALUES,
            words in proptest::collection::vec(proptest::sample::select(WORDS.split_whitespace().collect::<Vec<_>>()), 0..12),
        ) {
            let stderr = format!("{}\nError: {NOBODY} (os error 16)", words.join(" "));
            let absent = |value: &str| {
                [value, value.trim()].into_iter().chain(value.lines().map(str::trim)).filter(|w| !w.is_empty()).all(|w| !stderr.contains(w))
            };
            proptest::prop_assume!(absent(&one) && absent(&other));
            let evidence = |value: &str| Said::of(&stderr, NOBODY, Verb::Set(&Secret::new(value)));
            proptest::prop_assert_eq!(evidence(&one), evidence(&other));
        }
    }

    /// `secretspec set`'s stderr when the box handed it nothing, as an e2e run kept it.
    #[test]
    fn a_refusal_s_evidence_reads_as_the_store_said_it() {
        let stderr = format!(
            "Writing secret '{NOBODY}' to dotenv:/run/e2e/secrets-store.env (profile: default)\n  \
             target: item={NOBODY}\nError:   × Failed to set secret\n  \
             ╰─▶ Provider operation failed: Secret value cannot be empty"
        );
        assert_eq!(
            Said::of(&stderr, NOBODY, Verb::Set(&Secret::new("hunter2"))).to_string(),
            format!(
                "4 line(s), {} chars: Writing secret {NOBODY} to dotenv … store … \
                 profile … | … {NOBODY} | Error Failed to set secret | Provider operation failed \
                 Secret value cannot … empty",
                stderr.chars().count()
            )
        );
    }

    /// A call that cannot take its store's lock within its timeout is `Timeout`, as one that
    /// does not finish is: a set beside a slow read of the same store answers in its own time.
    #[test]
    fn a_set_beside_a_slow_read_times_out_in_its_own_time() {
        let script = "case \"$1\" in get) touch \"$(dirname \"$0\")/reading\"; sleep 2;; *) cat >/dev/null;; esac";
        let (dir, slow) = fake(script, Duration::from_secs(5));
        let quick = Secretspec {
            timeout: Duration::from_millis(300),
            ..slow.clone()
        };
        std::thread::scope(|both| {
            both.spawn(|| slow.get(NOBODY, "test"));
            while !dir.path().join("reading").exists() {
                std::thread::sleep(Duration::from_millis(10));
            }
            let started = std::time::Instant::now();
            let set = quick.set(NOBODY, &Secret::new("v"), "test");
            assert_eq!(
                set,
                Err(SecretError::Timeout {
                    name: NOBODY.to_owned()
                })
            );
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "{:?}",
                started.elapsed()
            );
        });
    }

    /// Law (the store's file rests on it): two sets at once both land, against a store that
    /// rewrites its whole file from what it read, as `secretspec`'s dotenv provider does.
    #[test]
    fn two_sets_at_once_both_land() {
        let (dir, spec) = fake(
            "value=$(cat); store=\"$(dirname \"$0\")/store\"; kept=$(cat \"$store\" 2>/dev/null); \
             sleep 0.3; printf '%s\\n%s=%s\\n' \"$kept\" \"$2\" \"$value\" > \"$store\"",
            Duration::from_secs(5),
        );
        std::thread::scope(|both| {
            for name in ["A_TOKEN", "B_TOKEN"] {
                let spec = &spec;
                both.spawn(move || spec.set(name, &Secret::new("v"), "test").expect("set"));
            }
        });
        let kept = std::fs::read_to_string(dir.path().join("store")).expect("the store");
        for name in ["A_TOKEN", "B_TOKEN"] {
            assert!(kept.contains(&format!("{name}=v")), "{kept}");
        }
    }

    #[test]
    fn set_hands_the_value_down_stdin() {
        let (dir, spec) = fake("cat > \"$(dirname \"$0\")/fed\"", Duration::from_secs(5));
        spec.set(NOBODY, &Secret::new("value"), "test")
            .expect("set");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("fed")).expect("fed"),
            "value\n"
        );
    }

    #[test]
    fn no_secret_error_ever_prints_a_value() {
        let name = || "NAME".to_owned();
        for error in [
            SecretError::Undeclared {
                name: name(),
                why: "the directory".to_owned(),
            },
            SecretError::Spawn { name: name() },
            SecretError::Timeout { name: name() },
            SecretError::Refused {
                name: name(),
                exit: Exit::Code(1),
                refusal: Refusal::Unparsed,
            },
            SecretError::Unread { name: name() },
            SecretError::Lost { name: name() },
            SecretError::Stopped { name: name() },
        ] {
            let shown = error.to_string();
            assert!(shown.contains("NAME"), "{shown}");
            assert!(!shown.contains("hunter2"), "{shown}");
        }
    }

    #[test]
    fn each_way_a_run_ends_is_its_own_cause() {
        let program = || "secretspec".to_owned();
        let os = || std::io::Error::other("the reason");
        let causes = [
            RunError::Spawn {
                program: program(),
                source: os(),
            },
            RunError::TimedOut {
                program: program(),
                after: Duration::from_secs(1),
            },
            RunError::Halted { program: program() },
            RunError::Wait {
                program: program(),
                source: os(),
            },
        ]
        .map(|failed| SecretError::of(NOBODY, Verb::Get, &failed));
        let name = || NOBODY.to_owned();
        assert_eq!(
            causes,
            [
                SecretError::Spawn { name: name() },
                SecretError::Timeout { name: name() },
                SecretError::Stopped { name: name() },
                SecretError::Lost { name: name() },
            ]
        );
    }

    const BASE: &str = r#"[project]
name = "suzatary"
revision = "1.0"

[profiles.default]
SUZATARY_DATA_PUSH_TOKEN = { description = "the data push", required = true }
SUZATARY_BOX_KEY = { description = "the box's key", required = false }
PROTON_BRIDGE_PASSWORD = { description = "the bridge", required = true }
"#;

    /// What a deployment with a calendar of `credentials` and an ntfy channel derives.
    fn derived(credentials: &[&str]) -> Settables {
        let manifest: suzatary_services::plugin::Manifest = toml::from_str(&format!(
            "name = \"ics\"\ncomponent = \"ics.wasm\"\nschema = \"ics.schema\"\n\
             credentials = [{}]\n",
            credentials
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .expect("a manifest");
        Settables::of_deployment(
            crate::model::TOKEN,
            [suzatary_connect::connect::Channel::Ntfy],
            None,
            (&[manifest], &[]),
            &suzatary_services::repos::Declared::default(),
        )
    }

    fn rooted() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = suzatary_testkit::scratch("secrets");
        let base = dir.path().join("code").join("secretspec.toml");
        std::fs::create_dir_all(base.parent().expect("a parent")).expect("a code root");
        std::fs::write(&base, BASE).expect("the code root's file");
        let state = dir.path().join("state");
        (dir, base, state)
    }

    fn names(table: &toml::Table) -> BTreeSet<String> {
        table["profiles"]["default"]
            .as_table()
            .expect("a default profile")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn the_file_declares_the_static_names_and_every_derived_name() {
        let (_dir, base, state) = rooted();
        let settables = derived(&["PLUGIN_ICS_WORK", "PLUGIN_ICS_HOME"]);
        let declared = Declared::of(base.clone(), state.clone(), &settables).expect("declared");
        let file = declared.file();
        assert_eq!(file.parent(), Some(state.as_path()));
        let written: toml::Table = std::fs::read_to_string(&file)
            .expect("the file")
            .parse()
            .expect("TOML");
        let own: toml::Table = BASE.parse().expect("TOML");
        let derived: BTreeSet<String> = settables.names().into_iter().map(str::to_owned).collect();
        assert_eq!(
            names(&written)
                .union(&names(&own))
                .cloned()
                .collect::<BTreeSet<_>>(),
            names(&own)
                .union(&derived)
                .cloned()
                .collect::<BTreeSet<_>>(),
        );
        assert!(
            names(&written).is_disjoint(&names(&own)),
            "the code root's entries stand as written: {written}"
        );
        for name in [
            "PLUGIN_ICS_WORK",
            "PLUGIN_ICS_HOME",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "NTFY_TOPIC",
        ] {
            assert!(names(&written).contains(name), "{name}: {written}");
        }
        let project = written["project"].as_table().expect("a project");
        assert_eq!(project["name"].as_str(), Some("suzatary"));
        assert_eq!(project["revision"].as_str(), Some("1.0"));
        assert_eq!(
            project["extends"],
            toml::Value::Array(vec![
                base.parent().and_then(Path::to_str).expect("UTF-8").into()
            ])
        );
        assert_eq!(
            written["profiles"]["default"]["PLUGIN_ICS_WORK"]["description"].as_str(),
            Some("Declared by the ics plugin's link.")
        );
    }

    #[test]
    fn declaring_again_is_the_same_file_and_every_store_follows_the_last() {
        use suzatary_connect::settings::Declares as _;
        let (_dir, base, state) = rooted();
        let declarations = Declarations::new(base, state.clone(), derived(&["PLUGIN_ICS_WORK"]));
        let store = declarations.store();
        let first = declarations.opened().expect("declared").file();
        declarations
            .declare(&derived(&["PLUGIN_ICS_WORK"]))
            .expect("declared again");
        assert_eq!(declarations.opened().expect("declared").file(), first);
        assert_eq!(std::fs::read_dir(&state).expect("the directory").count(), 1);
        declarations
            .declare(&derived(&["PLUGIN_ICS_WORK", "PLUGIN_ICS_HOME"]))
            .expect("widened");
        let now = declarations.opened().expect("declared").file();
        assert_ne!(now, first);
        assert_eq!(store.configured(NOBODY), Ok(now));
    }

    #[test]
    fn a_stale_writer_drops_no_name_a_newer_one_declared() {
        let (_dir, base, state) = rooted();
        let newer = Declared::of(
            base.clone(),
            state.clone(),
            &derived(&["PLUGIN_ICS_WORK", "PLUGIN_ICS_HOME"]),
        )
        .expect("declared");
        let older = Declared::of(base, state, &derived(&["PLUGIN_ICS_WORK"])).expect("declared");
        assert_ne!(older.file(), newer.file());
        let read = std::fs::read_to_string(newer.file()).expect("the newer file");
        assert!(read.contains("PLUGIN_ICS_HOME"), "{read}");
    }

    #[test]
    fn the_file_is_written_by_the_first_call_and_a_failed_write_fails_that_call() {
        let (dir, base, state) = rooted();
        let declarations = Declarations::new(base.clone(), state.clone(), derived(&[]));
        assert!(!state.exists(), "made with no I/O");
        assert!(declarations.store().configured(NOBODY).is_ok());
        assert!(state.exists(), "written by the first call");

        let blocked = dir.path().join("a-file");
        std::fs::write(&blocked, "").expect("a file where a directory would go");
        let found = Declarations::new(base, blocked.join("state"), derived(&[]))
            .store()
            .get(NOBODY, "test");
        assert!(
            matches!(&found, Err(SecretError::Undeclared { name, .. }) if name == NOBODY),
            "{found:?}"
        );
        assert_eq!(super::state(&found), SecretState::Failed);
    }

    #[test]
    fn a_code_root_with_no_file_declares_nothing_and_holds_no_store() {
        let dir = suzatary_testkit::scratch("secrets");
        let base = dir.path().join("secretspec.toml");
        let state = dir.path().join("state");
        let declared =
            Declared::of(base.clone(), state.clone(), &derived(&[])).expect("nothing to do");
        assert_eq!(declared.file(), base);
        assert!(!state.exists());
        assert_eq!(
            Declarations::new(base, state, derived(&[]))
                .store()
                .get(NOBODY, "test"),
            Err(SecretError::NoSpec {
                name: NOBODY.to_owned()
            })
        );
    }

    #[test]
    fn a_code_root_file_with_no_project_name_is_refused() {
        let (_dir, base, state) = rooted();
        std::fs::write(&base, "[profiles.default]\n").expect("a file with no project");
        assert!(matches!(
            Declared::of(base, state, &derived(&[])),
            Err(DeclareError::Parse { .. })
        ));
    }
}
