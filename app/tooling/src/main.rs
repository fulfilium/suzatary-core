//! `train`: the verbs of the merge train that `scripts/train.sh` has handed to a typed program.
#![forbid(unsafe_code)]

use std::process::ExitCode;
use suzatary_tooling::admitted::Terminal;
use suzatary_tooling::command::{Command, Of};
use suzatary_tooling::core_rows::{CORE_MASTER, Core};

use suzatary_tooling::gate::Gate;
use suzatary_tooling::gate_run::Train;
use suzatary_tooling::land_order::Land;
use suzatary_tooling::matcher::Matcher;
use suzatary_tooling::public_tree::Script;
use suzatary_tooling::publication::{self, Guard};
use suzatary_tooling::{Error, Half, LandOrder, Seen, Table, git};

fn main() -> ExitCode {
    if let Err(e) = suzatary_tooling::supervisor::keep_children() {
        eprintln!("train: the children's statuses could not be kept for the waits: {e}");
        return ExitCode::from(3);
    }
    let args: Result<Vec<String>, Error> = std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|arg| Error::Usage(format!("{} is not UTF-8", arg.to_string_lossy())))
        })
        .collect();
    match args.and_then(|args| Command::parse(&args)).and_then(run) {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            eprintln!("train: {error}");
            ExitCode::from(error.status())
        }
    }
}

/// Runs `command` in the working directory's repository; the status to exit with.
fn run(command: Command) -> Result<u8, Error> {
    match command {
        Command::Read { commit, of, seen } => {
            let table = table()?;
            let tree = git::tree(&commit)?;
            let read = |half| table.read(half, seen, &tree);
            let reading = match of {
                Of::Half(half) => read(half)?,
                Of::Train => {
                    let [tree_half, live_half] = Half::BOTH.map(read);
                    LandOrder::verdict([tree_half?, live_half?])
                }
            };
            println!("{reading}");
            Ok(0)
        }
        Command::SetAside { half, task, words } => {
            let Some(terminal) = Terminal::stdin() else {
                return Err(Error::Said(
                    "a red row is set aside by a person at a terminal, and stdin is none.".into(),
                ));
            };
            let table = table()?;
            let commit = git::commit("train")?;
            let tree = git::tree(&commit.to_string())?;
            let Some((red, aside)) = table.set_aside(half, &tree, (task, words), &terminal)? else {
                return Err(Error::Said(format!(
                    "no red row of the {} half of the train {} stands where a land reads it.",
                    half.name(),
                    commit.short()
                )));
            };
            println!("set aside: the {} half's {red}", half.name());
            println!("because {}: {}", aside.task, aside.words);
            Ok(0)
        }
        Command::Land { commit } => {
            let table = table()?;
            let oid = git::commit(&commit)?;
            let tree = git::tree(&commit)?;
            let [tree_half, live_half] =
                Half::BOTH.map(|half| table.judged(half, Seen::Any, &tree));
            let land = Land {
                tree: tree_half?,
                live: live_half?,
            };
            if let Some(refusal) = land.refusal(oid.short()) {
                return Err(Error::Said(refusal));
            }
            for line in land.record() {
                println!("{line}");
            }
            Ok(0)
        }
        Command::Publish { skips, admits, rev } => {
            publication::publish(&guard()?, rev.as_deref(), &skips, admits)
        }
        Command::Publishable { commit } => {
            Ok(publication::gate_check(&guard()?, &commit)?.0.status())
        }
        Command::PushGate {
            commit,
            branch,
            subject,
            trailer,
        } => publication::push_gate(&guard()?, &commit, &branch, &subject, &trailer),
        Command::Gate { full, remote } => Train::new(table()?)?.gate(full, remote),
        Command::Bisect { remote } => Train::new(table()?)?.bisect(remote),
        Command::Nightly => Train::new(table()?)?.nightly(),
        Command::Check { pr } => Train::new(table()?)?.check(pr),
        Command::Locked { verb, command } => Train::new(table()?)?.locked(&verb, &command),
        Command::Context { repo, act } => suzatary_tooling::context::run(&repo, &act)
            .map(|()| 0)
            .map_err(|e| Error::Said(format!("{e:#}"))),
        Command::Published { rev } => {
            // The local name as `record` just fetched it: absent is a core with no master.
            let core = match git::commit_if(CORE_MASTER)? {
                None => Core::Unborn,
                Some(tip) => Core::read(tip)?,
            };
            if let Some(row) = core.rows().published(&git::commit(&rev)?) {
                println!("{row}");
            }
            Ok(0)
        }
    }
}

fn table() -> Result<Table, Error> {
    Ok(Table::at(
        &git::common_dir()?.join("train").join("gates"),
        &gate()?,
    ))
}

/// The merge gate's table, or under the guards' seam a stub gate's, keyed by the stub's bytes.
#[cfg_attr(
    not(feature = "seams"),
    expect(clippy::unnecessary_wraps, reason = "only a stub's key asks git")
)]
fn gate() -> Result<Gate, Error> {
    #[cfg(feature = "seams")]
    if let Some(stub) = seam("SUZATARY_TRAIN_GATE") {
        return Ok(Gate::stub(&git::hash_object(std::path::Path::new(&stub))?));
    }
    Ok(Gate::merge_gate())
}

/// The checkout's `public-tree.sh` and the deployed app, or under the guards' seams their stubs.
fn guard() -> Result<Guard, Error> {
    #[cfg(feature = "seams")]
    let (script, matcher) = (
        seam("SUZATARY_TRAIN_PUBLIC").map(|path| Script::stub(path.into())),
        seam("SUZATARY_TRAIN_BIN").map(|path| Matcher::stub(path.into())),
    );
    #[cfg(not(feature = "seams"))]
    let (script, matcher) = (None, None);
    Ok(Guard {
        script: script.map_or_else(Script::of_checkout, Ok)?,
        matcher: matcher.map_or_else(Matcher::deployed, Ok)?,
    })
}

#[cfg(feature = "seams")]
fn seam(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name).filter(|value| !value.is_empty())
}
