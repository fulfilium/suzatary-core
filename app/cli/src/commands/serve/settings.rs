//! The settings page's route: what is connected. It writes nothing: every change is a card the
//! owner taps, and no answer carries a secret's value.

use std::collections::BTreeMap;
use std::path::PathBuf;
use suzatary_io::present::Present as _;

use axum::Json;
use prodrome_core::store::Replica as _;
use suzatary_core::calendar::Occurrence;
use suzatary_core::plugin::PluginName;
use suzatary_core::refusal::Refusal;
use suzatary_core::routes;
use suzatary_core::settings::overview::{Asking, Declared, Registers, Storage, Store};
use suzatary_core::settings::remote::{self, Pushed, Tip};
use suzatary_core::wire;
use suzatary_io::plugin_store::PluginStore;

use super::State;
use super::call::Call;
use super::guard::Reader;

/// `GET /api/overview`: what is connected, by group, as the stores read now.
///
/// Each part is timed, and the parts are logged under `suzatary::overview`: at `info` when the
/// whole took a second or more, at `debug` otherwise.
///
/// # Errors
/// A health register that cannot be read, which is not one with nothing in it.
pub fn overview(call: &Call<Reader, (), ()>) -> Result<Json<wire::Overview>, Refusal> {
    let state = &call.state;
    let caps = call.chain.caps;
    let mut timed = Timed::start();
    let super::Live {
        settables,
        channels,
        accounts,
        plugins,
    } = state.live();
    let health = timed.part("sources", || {
        caps.secretary
            .health
            .healths()
            .map_err(|failed| Refusal::internal(format!("the sources' health register: {failed}")))
    })?;
    let answered = timed.part("models", || {
        state
            .answered
            .healths()
            .map_err(|failed| Refusal::internal(format!("the model health register: {failed}")))
    })?;
    let drive = timed.part("drive", || caps.secretary.drive.allowed_folders());
    let build = timed.part("about", || {
        state
            .stamp
            .about(&state.chains_read().set(caps.secretary.clock.now()))
    })?;
    let storage = timed.part("storage", || storage(state));
    let asking = timed.part("inbox", || {
        suzatary_core::store::memoised(
            &state.asking,
            b"settings::overview::Asking::open of the inbox's fold",
            caps.secretary.inbox,
            |inbox| {
                Ok(Asking::open(&suzatary_core::secretary::inbox::Fold::of(
                    &inbox.nodes()?,
                )))
            },
        )
    })?;
    let stores = timed.part("plugins", || stores(state, &plugins));
    let pushes = timed.part("pushes", || pushes(state.data.as_path()));
    let jobs = state.models.jobs();
    let declared = Declared {
        deployment: &state.deployment,
        settables: &settables,
        channels: suzatary_core::nonempty::NonEmpty::listed(channels.as_ref()),
        accounts: accounts.accounts(),
        jobs: &jobs,
        plugins: &plugins,
        drive: &drive,
        storage: &storage,
        build: &build,
        reach: &state.reach,
    };
    let secrets = Stopwatch::over(caps.secretary.secrets);
    let at = std::time::Instant::now();
    let overview = routes::overview(
        &declared,
        &secrets,
        &Registers {
            health: &health,
            answered: &answered,
            plugins: &stores,
            pushes: pushes.as_ref(),
        },
        &asking,
    );
    timed.apart(("drawn", at.elapsed()), ("secrets", secrets.took()));
    timed.said();
    Ok(Json(overview))
}

/// How long each part of one overview took, in the order it was read.
struct Timed {
    started: std::time::Instant,
    parts: Vec<(&'static str, std::time::Duration)>,
}

impl Timed {
    fn start() -> Self {
        Self {
            started: std::time::Instant::now(),
            parts: Vec::new(),
        }
    }

    fn part<T>(&mut self, name: &'static str, work: impl FnOnce() -> T) -> T {
        let at = std::time::Instant::now();
        let value = work();
        self.parts.push((name, at.elapsed()));
        value
    }

    // A part that took `whole` with `inner` inside it, said as the two apart.
    fn apart(
        &mut self,
        (name, whole): (&'static str, std::time::Duration),
        (inner, took): (&'static str, std::time::Duration),
    ) {
        self.parts.push((inner, took));
        self.parts.push((name, whole.saturating_sub(took)));
    }

    fn said(&self) {
        let total = self.started.elapsed();
        let parts = self
            .parts
            .iter()
            .map(|(name, took)| format!("{name}={}ms", took.as_millis()))
            .collect::<Vec<_>>()
            .join(" ");
        if total >= std::time::Duration::from_secs(1) {
            tracing::info!(target: "suzatary::overview", "overview took {}ms: {parts}", total.as_millis());
        } else {
            tracing::debug!(target: "suzatary::overview", "overview took {}ms: {parts}", total.as_millis());
        }
    }
}

/// The secrets' store, asked by the overview, the time each question took added up.
struct Stopwatch<'a> {
    secrets: &'a dyn suzatary_core::capability::Secrets,
    took: std::sync::Mutex<std::time::Duration>,
}

impl<'a> Stopwatch<'a> {
    fn over(secrets: &'a dyn suzatary_core::capability::Secrets) -> Self {
        Self {
            secrets,
            took: std::sync::Mutex::new(std::time::Duration::ZERO),
        }
    }

    fn timed<T>(&self, ask: impl FnOnce() -> T) -> T {
        let at = std::time::Instant::now();
        let answer = ask();
        *super::locks::held(&self.took) += at.elapsed();
        answer
    }

    fn took(&self) -> std::time::Duration {
        *super::locks::held(&self.took)
    }
}

impl suzatary_core::capability::Secrets for Stopwatch<'_> {
    fn secret_state(
        &self,
        settable: &suzatary_core::settings::Settable,
    ) -> suzatary_core::capability::SecretState {
        self.timed(|| self.secrets.secret_state(settable))
    }

    fn mail_grants(
        &self,
        account: &suzatary_core::mail::account::MailAccount,
    ) -> suzatary_core::capability::MailGrants {
        self.timed(|| self.secrets.mail_grants(account))
    }

    fn google_grant(
        &self,
        account: &suzatary_core::mail::address::EmailAddress,
    ) -> suzatary_core::google::Held {
        self.timed(|| self.secrets.google_grant(account))
    }
}

/// The data repository and the `origin` its git config names, read now.
fn storage(state: &State) -> Storage {
    Storage {
        pushed: pushed(state),
        root: state.data.as_path().display().to_string(),
    }
}

/// Where the data repository pushes, read from its git configuration now.
pub(super) fn pushed(state: &State) -> Pushed {
    match git_config(state.data.as_path()).present() {
        Ok(Some(config)) => Pushed::of_git_config(&config),
        Ok(None) => Pushed::Nowhere,
        Err(error) => Pushed::Unread(error.to_string()),
    }
}

/// The repository at `root`'s own git directory and the one it shares with its worktrees,
/// through a `.git` file's `gitdir` (a worktree's names its own directory, whose `commondir`
/// holds the shared one).
fn git_dirs(root: &std::path::Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let dot_git = root.join(".git");
    if !dot_git.is_file() {
        return Ok((dot_git.clone(), dot_git));
    }
    let text = std::fs::read_to_string(&dot_git)?;
    let named = Pushed::gitdir(&text).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, ".git names no gitdir")
    })?;
    let dir = root.join(named);
    let common = match std::fs::read_to_string(dir.join("commondir")).present()? {
        Some(common) => dir.join(common.trim()),
        None => dir.clone(),
    };
    Ok((dir, common))
}

/// The git configuration of the repository at `root`.
fn git_config(root: &std::path::Path) -> std::io::Result<String> {
    std::fs::read_to_string(git_dirs(root)?.1.join("config"))
}

/// What the data repository's pushes read, from its reflogs: where its branch last moved, and
/// where `origin`'s copy of it last moved, which a push moves. `None` when either log is not
/// there to read: a repository on no branch, or one never pushed.
fn pushes(root: &std::path::Path) -> Option<suzatary_core::health::Health> {
    let (dir, common) = git_dirs(root).ok()?;
    let head = std::fs::read_to_string(dir.join("HEAD")).ok()?;
    let branch = head.trim().strip_prefix("ref: refs/heads/")?;
    let tip = |log: PathBuf| Tip::of_reflog(&std::fs::read_to_string(log).ok()?);
    let upstream = tip(common.join("logs/refs/remotes/origin").join(branch));
    remote::pushes(&tip(dir.join("logs/HEAD"))?, upstream.as_ref())
}

/// Each declared plugin's store as the schema beside it reads, a store that cannot be read
/// saying why.
fn stores(
    state: &State,
    manifests: &[suzatary_core::plugin::Manifest],
) -> BTreeMap<PluginName, Store> {
    state
        .plugins
        .as_ref()
        .map_or_else(BTreeMap::new, |plugins| {
            manifests
                .iter()
                .map(|manifest| {
                    let name = manifest.name();
                    let store = match PluginStore::at(plugins.store(name)).read() {
                        Ok(None) => Store::NotBegun,
                        Ok(Some(store)) if Occurrence::reads(store.inner().schema()) => {
                            Store::Calendar
                        }
                        Ok(Some(_)) => Store::Other,
                        Err(failed) => Store::Unreadable(failed.to_string()),
                    };
                    (name.clone(), store)
                })
                .collect()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worktree's `.git` is a file: its configuration is the main checkout's, through the
    /// worktree's `gitdir` and its `commondir`.
    #[test]
    fn a_worktree_reads_the_configuration_its_gitdir_shares() {
        let main = tempfile::tempdir().expect("a scratch directory");
        let worktree = tempfile::tempdir().expect("a scratch directory");
        let gitdir = main.path().join(".git").join("worktrees").join("w");
        std::fs::create_dir_all(&gitdir).expect("a gitdir");
        std::fs::write(gitdir.join("commondir"), "../..\n").expect("a commondir");
        std::fs::write(
            main.path().join(".git").join("config"),
            "[remote \"origin\"]\n\turl = git@example.test:ada/data.git\n",
        )
        .expect("a config");
        std::fs::write(
            worktree.path().join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .expect("a .git file");
        assert_eq!(
            git_config(worktree.path())
                .map(|config| Pushed::of_git_config(&config))
                .ok(),
            Some(Pushed::of_git_config(
                "[remote \"origin\"]\n\turl = git@example.test:ada/data.git\n"
            ))
        );
        assert_eq!(
            git_config(main.path())
                .map(|config| Pushed::of_git_config(&config))
                .ok(),
            git_config(worktree.path())
                .map(|config| Pushed::of_git_config(&config))
                .ok()
        );
    }

    /// A repository whose branch is where `origin`'s copy of it last moved was pushed then; one
    /// whose branch moved on since is failing since that push; one never pushed observes nothing.
    #[test]
    fn the_pushes_are_read_from_the_reflogs() {
        use suzatary_core::health::Reading;
        let root = tempfile::tempdir().expect("a scratch directory");
        let git = root.path().join(".git");
        std::fs::create_dir_all(git.join("logs/refs/remotes/origin")).expect("the logs");
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("a head");
        let line = |commit: &str, seconds: i64| {
            format!("0000 {commit} Box <box@example.test> {seconds} +0000\tcommit\n")
        };
        std::fs::write(git.join("logs/HEAD"), line("aaaa", 1_791_363_600)).expect("a log");
        assert_eq!(pushes(root.path()), None, "never pushed");
        std::fs::write(
            git.join("logs/refs/remotes/origin/main"),
            line("aaaa", 1_791_367_200),
        )
        .expect("a log");
        assert!(matches!(
            pushes(root.path()).map(|health| health.reading()),
            Some(Reading::Ok { .. })
        ));
        std::fs::write(git.join("logs/HEAD"), line("bbbb", 1_791_370_800)).expect("a log");
        assert!(matches!(
            pushes(root.path()).map(|health| health.reading()),
            Some(Reading::Failing { .. })
        ));
    }
}
