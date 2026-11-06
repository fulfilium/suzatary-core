//! Every path the box derives outside the instance file, from the instance and the environment.

use std::path::{Path, PathBuf};

use crate::instance::DataRoot;

/// The environment's part: home, the XDG roots and the exported code root. Read once at the
/// binary's entry, so a library call or a test never reads the process's environment.
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

    /// An environment of `home` and `runtime` alone, XDG roots defaulted, no code root.
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

    /// The same environment with `code` as the exported code root.
    #[must_use]
    pub fn running(self, code: Option<PathBuf>) -> Self {
        Self { code, ..self }
    }

    #[must_use]
    pub fn code(&self) -> Option<&Path> {
        self.code.as_deref()
    }

    /// The user repository's script a deployment's wrapper exported, if any.
    #[must_use]
    pub fn run(&self) -> Option<&str> {
        self.run.as_deref()
    }
}

/// The variable a deployment's wrapper names its user repository's script in.
pub const RUN_ENV: &str = "SUZATARY_RUN";
/// A directory name unique to the instance at `data`: its path with every byte but
/// `[A-Za-z0-9._-]` spelled `%XX`.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// The derived secret-name declarations (`suzatary_io::secrets::Declarations`): box state,
    /// per instance, since the code root may be read-only and the data root holds no derived file.
    pub secretspec: PathBuf,
    /// The Microsoft grants, one file per account.
    pub oauth: PathBuf,
    /// The IMAP sync state, one file per account and folder.
    pub imap_state: PathBuf,
    /// Where each listed message lives, by canonical hash; rebuilt by `suzatary locate-mail`.
    pub mail_hashes: PathBuf,
    /// The phones registered for push.
    pub devices: PathBuf,
    /// The answers a device's request was given, kept for its replay.
    pub answers: PathBuf,
    /// Each store's epoch, box-local so no rewrite of the data repository takes one back, and
    /// per instance so a scratch server never steps the live epochs.
    pub epochs: PathBuf,
    /// Each store's recorded frontiers, per instance; a lost one costs a replica a full pull.
    pub frontiers: PathBuf,
    /// The last pairing code's state, per instance and never pushed.
    pub pairing: PathBuf,
    /// Rendered markup, by the hash of its source.
    pub markup_cache: PathBuf,
    /// Bare mirrors of the owner's repositories.
    pub repos: PathBuf,
    /// When each GitHub token expires, as GitHub last said.
    pub github_tokens: PathBuf,
    /// The allowance endpoint's last good answer.
    pub usage_cache: PathBuf,
    /// The last mail sync's report: box state, never under a shared `/tmp`.
    pub last_sync: PathBuf,
    /// The compactor's working directory, kept empty so the CLI finds no `CLAUDE.md` there.
    pub compactor: PathBuf,
    /// The compactor's CLI configuration, apart from the box user's, so no user-level
    /// `CLAUDE.md` or settings reach it.
    pub compactor_config: PathBuf,
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
    #[must_use]
    pub fn of(data: &DataRoot, drive_cli: Option<&Path>, env: &Env) -> Self {
        let state = env.state.join("suzatary");
        let cache = env.cache.join("suzatary");
        Self {
            secretspec: state.join("secretspec").join(instance_key(data)),
            oauth: state.join("oauth"),
            imap_state: state.join("imap"),
            mail_hashes: state.join("mail-hashes").join(instance_key(data)),
            devices: state.join("devices"),
            answers: state.join("answers"),
            epochs: state.join("epochs").join(instance_key(data)),
            frontiers: state.join("frontiers").join(instance_key(data)),
            pairing: state
                .join("pairing")
                .join(format!("{}.json", instance_key(data))),
            markup_cache: cache.join("markup"),
            repos: cache.join("repos"),
            github_tokens: cache.join("github-tokens"),
            usage_cache: cache.join("usage"),
            last_sync: state.join("last-sync.json"),
            compactor: cache.join("compactor"),
            compactor_config: cache.join("compactor-config"),
            drive_cli: drive_cli.map_or_else(|| env.home.join("bin/proton-drive"), Path::to_owned),
            drive_scratch: env.runtime.join("suzatary-drive"),
            bridge_cert: env.home.join(".config/protonmail/bridge-cert.pem"),
            whois_socket: PathBuf::from(TAILSCALED_SOCKET),
        }
    }
}
