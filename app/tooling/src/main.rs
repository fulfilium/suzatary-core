//! `train`: the verbs of the merge train that `scripts/train.sh` has handed to a typed program.
#![forbid(unsafe_code)]

use std::process::ExitCode;
use suzatary_tooling::command::{Command, Of};
use suzatary_tooling::gate::Gate;
use suzatary_tooling::instant::Instant;
use suzatary_tooling::{Ended, Error, Half, LandOrder, Row, Table, git};

fn main() -> ExitCode {
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
    let table = Table::at(&git::common_dir()?.join("train").join("gates"), &gate()?);
    match command {
        Command::Read { commit, of, seen } => {
            let tree = git::tree(&commit)?;
            let reading = match of {
                Of::Half(half) => table.read(half, seen, &tree)?,
                Of::Train => {
                    let [tree_half, live_half] =
                        Half::BOTH.map(|half| table.read(half, seen, &tree));
                    LandOrder::verdict([tree_half?, live_half?])
                }
            };
            println!("{reading}");
            Ok(0)
        }
        Command::Write {
            half,
            commit,
            ended,
            scope,
        } => {
            match ended {
                Ended::Judged(outcome) => {
                    let row = Row {
                        outcome,
                        commit: git::commit(&commit)?,
                        at: Instant::now(),
                    };
                    table.write(half, scope, &git::tree(&commit)?, &row)?;
                }
                Ended::DidNotStart => eprintln!(
                    "train: the {} half did not start, so it says nothing of the tree",
                    half.name()
                ),
            }
            Ok(ended.status())
        }
    }
}

/// The merge gate's table, or under the guards' seam a stub gate's, keyed by the stub's bytes.
#[cfg_attr(
    not(feature = "seams"),
    expect(clippy::unnecessary_wraps, reason = "only a stub's key asks git")
)]
fn gate() -> Result<Gate, Error> {
    #[cfg(feature = "seams")]
    if let Some(stub) = std::env::var_os("SUZATARY_TRAIN_GATE").filter(|path| !path.is_empty()) {
        return Ok(Gate::stub(&git::hash_object(&stub)?));
    }
    Ok(Gate::merge_gate())
}
