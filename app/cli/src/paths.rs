//! Every path the box derives outside the instance file, derived once from
//! the instance and the environment it runs in.

use std::path::{Path, PathBuf};

use crate::instance::DataRoot;

/// The environment's part: the home directory, the XDG roots, and the code root a deployment's
/// wrapper exported. Read once, at the binary's entry, and handed to [`crate::instance::Instance::read`],
/// so a library call or a test never reads the process's environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Env {
    home: PathBuf,
    state: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
    code: Option<PathBuf>,
    run: Option<String>,
}

impl Env {
    /// Read `HOME`, the XDG variables and [`crate::instance::CODE_ENV`]; an empty one is unset.
    ///
    /// # Errors
    /// `HOME` or `XDG_RUNTIME_DIR` unset: without the one every box path is under `/`, and
    /// without the other a Drive download would land in a directory every user shares.
    pub fn read() -> anyhow::Result<Self> {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let required = |name: &'static str| {
            var(name).map(PathBuf::from).ok_or_else(|| {
                anyhow::anyhow!(
                    "{name} is unset, so this box's paths have nowhere to be: run from a login \
                     session or a unit, which set it"
                )
            })
        };
        let under = Self::under(required("HOME")?, required("XDG_RUNTIME_DIR")?);
        Ok(Self {
            state: var("XDG_STATE_HOME").map_or(under.state, PathBuf::from),
            cache: var("XDG_CACHE_HOME").map_or(under.cache, PathBuf::from),
            code: var(crate::instance::CODE_ENV).map(PathBuf::from),
            run: var(RUN_ENV).and_then(|script| script.into_string().ok()),
            ..under
        })
    }

    /// An environment of `home` and `runtime` alone, the XDG roots their defaults beneath
    /// `home` and no code root exported: what a test or a scratch deployment reads in.
    #[must_use]
    pub fn under(home: impl Into<PathBuf>, runtime: impl Into<PathBuf>) -> Self {
        let home = home.into();
        Self {
            state: home.join(".local").join("state"),
            cache: home.join(".cache"),
            runtime: runtime.into(),
            code: None,
            run: None,
            home,
        }
    }

    /// The same environment with `code` the code root a wrapper exported, which the instance
    /// file's `repo` then gives way to.
    #[must_use]
    pub fn running(self, code: Option<PathBuf>) -> Self {
        Self { code, ..self }
    }

    /// The code root a deployment's wrapper exported, if any.
    #[must_use]
    pub fn code(&self) -> Option<&Path> {
        self.code.as_deref()
    }

    /// The user repository's script a deployment's wrapper exported, if any: how the person
    /// reaches the app from a terminal.
    #[must_use]
    pub fn run(&self) -> Option<&str> {
        self.run.as_deref()
    }
}

/// The variable a deployment's wrapper names its user repository's script in.
pub const RUN_ENV: &str = "SUZATARY_RUN";
/// A directory name for the instance whose data is at `data`: its path with every byte but a
/// letter, a digit, `.`, `_` and `-` spelled `%XX`, so two instances never share one and the same
/// instance always finds its own.
fn instance_key(data: &DataRoot) -> String {
    data.as_path()
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .map(|&byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Where tailscaled's socket is on NixOS.
const TAILSCALED_SOCKET: &str = "/run/tailscale/tailscaled.sock";

/// The paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// The directory of the files declaring the secret names this instance derives beside the
    /// code root's own `secretspec.toml` (`suzatary_io::secrets::Declarations`): box state, the
    /// instance's own as `epochs` is, since a deployment's code root is read-only and the data
    /// root is the checkpoint, which a derived file is not part of.
    pub secretspec: PathBuf,
    /// The Microsoft grants, one file per account.
    pub oauth: PathBuf,
    /// The IMAP sync state, one file per account and folder.
    pub imap_state: PathBuf,
    /// The phones registered for push.
    pub devices: PathBuf,
    /// The answers a device's request was given, kept for its replay while it could be one.
    pub answers: PathBuf,
    /// Each store's epoch, box-local so no rewrite of the data repository takes one back, and
    /// the instance's own, so a server over a scratch instance never steps the live epochs.
    pub epochs: PathBuf,
    /// Each store's recorded frontiers, box-local and the instance's own as the epochs are: a
    /// lost one costs a replica one pull from nothing.
    pub frontiers: PathBuf,
    /// The last pairing code's state (`suzatary device pair`): box state, the instance's own,
    /// and never pushed, since a code trusts a device of this box alone.
    pub pairing: PathBuf,
    /// Rendered markup, by the hash of its source.
    pub markup_cache: PathBuf,
    /// Bare mirrors of the owner's repositories.
    pub repos: PathBuf,
    /// The allowance endpoint's last good answer.
    pub usage_cache: PathBuf,
    /// The last mail sync's report, which the sync writes and the gate, the briefing and the
    /// agent read: box state, never the instance's to place, and never under a shared `/tmp`.
    pub last_sync: PathBuf,
    /// The compactor's working directory, kept empty so the CLI finds no `CLAUDE.md` there.
    pub compactor: PathBuf,
    /// The compactor's CLI configuration, kept apart from the box user's, so no user-level
    /// `CLAUDE.md` or settings reach it.
    pub compactor_config: PathBuf,
    /// Claude Code's transcripts.
    pub transcripts: PathBuf,
    /// The home directory the transcripts' project names are relative to.
    pub home: PathBuf,
    /// The Proton Drive CLI.
    pub drive_cli: PathBuf,
    /// Where a Drive download lands for the length of one read.
    pub drive_scratch: PathBuf,
    /// The Proton Bridge's self-signed certificate, pinned for SMTP.
    pub bridge_cert: PathBuf,
    /// tailscaled's local API.
    pub whois_socket: PathBuf,
}

impl Paths {
    /// The paths of the instance whose data is at `data`, with the Drive CLI the instance names, under `env`.
    #[must_use]
    pub fn of(data: &DataRoot, drive_cli: Option<&Path>, env: &Env) -> Self {
        let state = env.state.join("suzatary");
        let cache = env.cache.join("suzatary");
        Self {
            secretspec: state.join("secretspec").join(instance_key(data)),
            oauth: state.join("oauth"),
            imap_state: state.join("imap"),
            devices: state.join("devices"),
            answers: state.join("answers"),
            epochs: state.join("epochs").join(instance_key(data)),
            frontiers: state.join("frontiers").join(instance_key(data)),
            pairing: state
                .join("pairing")
                .join(format!("{}.json", instance_key(data))),
            markup_cache: cache.join("markup"),
            repos: cache.join("repos"),
            usage_cache: cache.join("usage"),
            last_sync: state.join("last-sync.json"),
            compactor: cache.join("compactor"),
            compactor_config: cache.join("compactor-config"),
            transcripts: env.home.join(".claude").join("projects"),
            home: env.home.clone(),
            drive_cli: drive_cli.map_or_else(|| env.home.join("bin/proton-drive"), Path::to_owned),
            drive_scratch: env.runtime.join("suzatary-drive"),
            bridge_cert: env.home.join(".config/protonmail/bridge-cert.pem"),
            whois_socket: PathBuf::from(TAILSCALED_SOCKET),
        }
    }
}
