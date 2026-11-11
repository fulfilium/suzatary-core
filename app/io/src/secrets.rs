//! Static secrets: the environment first, then `secretspec get`. A lookup answers a value
//! (`Ok(Some)`), nothing holds the name (`Ok(None)`), or the store could not say
//! ([`SecretError`]); a locked GPG agent is never read as "not configured". A value is only ever
//! a [`Secret`], and only its name is ever printed.

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use suzatary_core::capability::Secret;
use suzatary_core::secrets::SecretState;
use suzatary_core::settings::Settables;

use crate::run::{Cmd, RunError, succeed};

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
    #[must_use]
    pub fn with_timeout(spec: impl Into<PathBuf>, timeout: Duration) -> Self {
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
        if let Some(trimmed) = env_value.map(str::trim).filter(|value| !value.is_empty()) {
            return Ok(Some(Secret::new(trimmed)));
        }
        subprocess()
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
        succeed(&cmd)
            .map(drop)
            .map_err(|failed| SecretError::of(name, &failed))
    }

    pub fn delete(&self, name: &str, reason: &str) -> Result<(), SecretError> {
        let spec = self.configured(name)?;
        succeed(&self.command(&spec, "delete", name, reason))
            .map(drop)
            .map_err(|failed| SecretError::of(name, &failed))
    }

    /// The last line of `secretspec get`'s stdout: some providers print a banner first. A
    /// refusal is "unset" only when the store says the name is not found.
    fn asked(&self, name: &str, reason: &str) -> Result<Option<Secret>, SecretError> {
        let spec = self.configured(name)?;
        let ran = match succeed(&self.command(&spec, "get", name, reason)) {
            Ok(ran) => ran,
            Err(RunError::Exited { stderr, .. })
                if stderr
                    .as_str()
                    .contains(&format!("Secret '{name}' not found")) =>
            {
                return Ok(None);
            }
            Err(failed) => return Err(SecretError::of(name, &failed)),
        };
        let stdout = String::from_utf8(ran.stdout).map_err(|_| SecretError::NotText {
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
    #[error("the secret store answered for {name} with bytes that are not text")]
    NotText { name: String },
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
            | Self::NotText { .. }
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
            Self::NotText { .. } | Self::Lost { .. } => {
                "repair the secret store (does secretspec answer at a terminal?)"
            }
        }
    }

    fn of(name: &str, failed: &RunError) -> Self {
        let name = name.to_owned();
        match failed {
            RunError::Spawn { .. } => Self::Spawn { name },
            RunError::TimedOut { .. } => Self::Timeout { name },
            RunError::Halted { .. } => Self::Stopped { name },
            RunError::Wait { .. } => Self::Lost { name },
            RunError::Exited { status, stderr, .. } => {
                let (exit, refusal) = (Exit::of(*status), Refusal::of(stderr.as_str()));
                Self::Refused {
                    name,
                    exit,
                    refusal,
                }
            }
        }
    }
}

impl suzatary_core::settings::Writes for Secretspec {
    fn write_secret(
        &self,
        name: &suzatary_core::settings::Settable,
        value: &suzatary_core::capability::Secret,
    ) -> Result<(), String> {
        self.set(
            name.name(),
            value,
            "the settings page: the owner set a secret",
        )
        .map_err(|failed| failed.to_string())
    }

    fn clear_secret(&self, name: &suzatary_core::settings::Settable) -> Result<(), String> {
        self.delete(name.name(), "a connection the owner removed")
            .map_err(|failed| failed.to_string())
    }
}

/// The file the store reads a deployment's names from: the code root's own `secretspec.toml`
/// (`base`, read-only), extended by a file in `dir` declaring every derived name `base` does not.
///
/// Each file is named by the digest of its text and never rewritten, so a stale process writes
/// beside a newer file, never over it: no writer drops a name another declared.
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
                    .map(suzatary_core::settings::Declaration::sentence)
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

impl suzatary_core::settings::Declares for Declarations {
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
        let dir = tempfile::tempdir().expect("a scratch directory");
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
        let dir = tempfile::tempdir().expect("a scratch directory");
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
            SecretError::NotText { name: name() },
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
        .map(|failed| SecretError::of(NOBODY, &failed));
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
        let manifest: suzatary_core::plugin::Manifest = toml::from_str(&format!(
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
            [suzatary_core::connect::Channel::Ntfy],
            None,
            (&[manifest], &[]),
            &suzatary_core::repos::Declared::default(),
        )
    }

    fn rooted() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch directory");
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
        use suzatary_core::settings::Declares as _;
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
        let dir = tempfile::tempdir().expect("a scratch directory");
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
