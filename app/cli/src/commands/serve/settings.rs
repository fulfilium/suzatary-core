//! The settings route writes nothing: a change is a card the owner taps, and no answer carries a
//! secret's value.

use std::collections::BTreeMap;
use std::path::PathBuf;
use suzatary_io::present::Present as _;

use axum::Json;
use prodrome_core::store::Replica as _;
use suzatary_core::calendar::Occurrence;
use suzatary_core::plugin::PluginName;
use suzatary_core::refusal::Refusal;
use suzatary_core::routes;
use suzatary_core::settings::overview::{
    Asked, Asking, Build, Declared, Registers, Storage, Store,
};
use suzatary_core::settings::remote::{self, Pushed, Tip};
use suzatary_core::store::StoreError;
use suzatary_core::timings::{Part, Phase, timed};
use suzatary_core::wire;
use suzatary_io::plugin_store::PluginStore;

use super::State;
use super::call::Call;
use super::guard::Reader;

/// `GET /api/overview`: what is connected, by group.
pub fn overview(call: &Call<Reader, (), ()>) -> Result<Json<wire::Overview>, Refusal> {
    timed(Phase::Overview(Part::Whole), || drawn(call))
}

fn drawn(call: &Call<Reader, (), ()>) -> Result<Json<wire::Overview>, Refusal> {
    let state = &call.state;
    let caps = call.chain.caps;
    let part = |part| Phase::Overview(part);
    let super::Live {
        settables,
        channels,
        accounts,
        plugins,
    } = state.live();
    let health = timed(part(Part::Sources), || {
        caps.secretary
            .health
            .healths()
            .map_err(|failed| Refusal::internal(format!("the sources' health register: {failed}")))
    })?;
    let answered = timed(part(Part::Models), || {
        state
            .answered
            .healths()
            .map_err(|failed| Refusal::internal(format!("the model health register: {failed}")))
    })?;
    let drive = timed(part(Part::Drive), || caps.secretary.drive.allowed_folders());
    let build = timed(part(Part::Build), || {
        build(state, caps.secretary.clock.now())
    })?;
    let storage = timed(part(Part::Storage), || storage(state));
    let asking = timed(part(Part::Inbox), || {
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
    let stores = timed(part(Part::Plugins), || stores(state, &plugins));
    let pushes = timed(part(Part::Pushes), || pushes(state.data.as_path()));
    let asked = timed(part(Part::Secrets), || {
        Asked::of(&state.secrets(caps), &settables, accounts.accounts())
    });
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
    Ok(Json(timed(part(Part::Drawn), || {
        routes::overview(
            &declared,
            &asked,
            Registers {
                health: &health,
                answered: &answered,
                plugins: &stores,
                pushes: pushes.as_ref(),
            },
            &asking,
        )
    })))
}

fn build(state: &State, now: suzatary_core::capability::Datetime) -> Result<Build, Refusal> {
    let about = || {
        state
            .stamp
            .about(&state.chains_read().set(now))
            .map(|about| Build::of(&about))
    };
    let plan = suzatary_core::release::Plan::declared()
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let Some(project) = state.chains.get(plan.chain()) else {
        return about();
    };
    let function = format!(
        "settings::overview::Build::of GET /api/about, built {}",
        state.stamp.build()
    );
    Ok(suzatary_core::store::memoised(
        &state.built,
        function.as_bytes(),
        project.caps.secretary.chain,
        |_| about().map_err(|refused| StoreError::unreadable("chain", refused.message())),
    )?)
}

fn storage(state: &State) -> Storage {
    Storage {
        pushed: pushed(state),
        root: state.data.as_path().display().to_string(),
    }
}

pub(super) fn pushed(state: &State) -> Pushed {
    pushed_at(state.data.as_path())
}

/// Where the data repository at `root` pushes.
#[must_use]
pub fn pushed_at(root: &std::path::Path) -> Pushed {
    match git_config(root).present() {
        Ok(Some(config)) => Pushed::of_git_config(&config),
        Ok(None) => Pushed::Nowhere,
        Err(error) => Pushed::Unread(error.to_string()),
    }
}

/// A worktree's `.git` file names its `gitdir`, whose `commondir` holds the shared one.
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

fn git_config(root: &std::path::Path) -> std::io::Result<String> {
    std::fs::read_to_string(git_dirs(root)?.1.join("config"))
}

/// `None` on no branch, or never pushed.
fn pushes(root: &std::path::Path) -> Option<suzatary_core::health::Health> {
    let (dir, common) = git_dirs(root).ok()?;
    let head = std::fs::read_to_string(dir.join("HEAD")).ok()?;
    let branch = head.trim().strip_prefix("ref: refs/heads/")?;
    let upstream = tip(&common.join("logs/refs/remotes/origin").join(branch));
    remote::pushes(&tip(&dir.join("logs/HEAD"))?, upstream.as_ref())
}

/// Bytes read from a reflog's end.
const REFLOG_TAIL: u64 = 64 * 1024;

fn tip(log: &std::path::Path) -> Option<Tip> {
    use std::io::{Read as _, Seek as _};
    let mut file = std::fs::File::open(log).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(std::io::SeekFrom::Start(length.saturating_sub(REFLOG_TAIL)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    Tip::of_reflog(&String::from_utf8_lossy(&tail))
}

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
        // A log longer than the end that is read: its last line is still its tip.
        let long = format!(
            "{}{}",
            line("aaaa", 1_791_363_600).repeat(2_000),
            line("bbbb", 1_791_370_800)
        );
        assert!(u64::try_from(long.len()).is_ok_and(|length| length > REFLOG_TAIL));
        std::fs::write(git.join("logs/HEAD"), long).expect("a log");
        assert!(matches!(
            pushes(root.path()).map(|health| health.reading()),
            Some(Reading::Failing { .. })
        ));
    }
}
