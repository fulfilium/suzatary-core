//! What `train` was asked to do, parsed once from its arguments.

use crate::gate_branch::GateBranch;
use crate::repo_path::RepoPath;
use crate::{Error, Half, Seen};

pub const USAGE: &str = "usage: train table read <commit> [--half tree|live] [--full]
       train publish [--skip <sha>]... [--admit <path>]... [<rev>]
       train publishable <commit>
       train push-gate <commit> gate/<name> <subject> <trailer>
       train published <rev>
       train gate [--full] [--remote github]
       train bisect [--remote github]
       train nightly
       train check <pr>
       train locked <verb> -- <command>...
       train context [--repo <dir>] [files [<unit>] | record [--merged] | external <name> <dir>]";

/// One verb of the tool, its arguments parsed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    /// The context budget of the repository at `repo`.
    Context {
        repo: std::path::PathBuf,
        act: crate::context::Act,
    },
    /// One word on stdout: the reading `of` the commit's tree.
    Read {
        commit: String,
        of: Of,
        seen: Seen,
    },
    /// Core's master gains the public tree of each landed commit up to `rev`, origin's master
    /// where it is `None`.
    Publish {
        skips: Vec<String>,
        admits: Vec<RepoPath>,
        rev: Option<String>,
    },
    /// Whether the commit's public tree may go to a gate branch.
    Publishable {
        commit: String,
    },
    /// The commit's public tree, if publishable, pushed to a gate branch.
    PushGate {
        commit: String,
        branch: GateBranch,
        subject: String,
        trailer: String,
    },
    /// The row of core's master that publishes `rev`, or nothing.
    Published {
        rev: String,
    },
    /// Both halves of the train, on the box or with the tree half on GitHub's runners.
    Gate {
        full: bool,
        remote: bool,
    },
    Bisect {
        remote: bool,
    },
    Nightly,
    Check {
        pr: u32,
    },
    /// A command of `train.sh` run under the train's lock.
    Locked {
        verb: String,
        command: Vec<String>,
    },
}

/// What a reading answers for: one half, or the train, whose verdict weighs both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Of {
    Half(Half),
    Train,
}

/// `--remote github`, or nothing: where the tree half runs.
fn remote(words: &[&str]) -> Result<bool, Error> {
    match words {
        [] => Ok(false),
        ["--remote", "github"] => Ok(true),
        _ => Err(usage("unknown arguments")),
    }
}

fn usage(why: &str) -> Error {
    Error::Usage(format!("{why}\n{USAGE}"))
}

impl Command {
    pub fn parse(args: &[String]) -> Result<Command, Error> {
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        match words[..] {
            ["table", ref rest @ ..] => Command::table(rest),
            ["publish", ref rest @ ..] => Command::publish(rest),
            ["publishable", commit] => Ok(Command::Publishable {
                commit: commit.to_owned(),
            }),
            ["push-gate", commit, branch, subject, trailer] => Ok(Command::PushGate {
                commit: commit.to_owned(),
                branch: GateBranch::parse(branch)?,
                subject: subject.to_owned(),
                trailer: trailer.to_owned(),
            }),
            ["published", rev] => Ok(Command::Published {
                rev: rev.to_owned(),
            }),
            ["gate", ref rest @ ..] => {
                let (full, rest) = match rest {
                    ["--full", rest @ ..] => (true, rest),
                    rest => (false, rest),
                };
                Ok(Command::Gate {
                    full,
                    remote: remote(rest)?,
                })
            }
            ["bisect", ref rest @ ..] => Ok(Command::Bisect {
                remote: remote(rest)?,
            }),
            ["nightly"] => Ok(Command::Nightly),
            ["check", n] => match n.parse() {
                Ok(pr) if !n.starts_with(['0', '+']) => Ok(Command::Check { pr }),
                _ => Err(usage(&format!("\"{n}\" is not a pull request number"))),
            },
            ["context", "--repo", repo, ref rest @ ..] => Command::context(repo, rest),
            ["context", ref rest @ ..] => Command::context(".", rest),
            ["locked", verb, "--", ref command @ ..] if !command.is_empty() => {
                Ok(Command::Locked {
                    verb: verb.to_owned(),
                    command: command.iter().map(|word| (*word).to_owned()).collect(),
                })
            }
            _ => Err(usage("unknown arguments")),
        }
    }

    fn context(repo: &str, words: &[&str]) -> Result<Command, Error> {
        use crate::context::{Act, Recording};
        let act = match words {
            [] => Act::Show,
            ["files"] => Act::Files(None),
            ["files", unit] => Act::Files(Some((*unit).to_owned())),
            ["record"] => Act::Record(Recording::Lowered),
            ["record", "--merged"] => Act::Record(Recording::Merged),
            ["external", name, checkout] => Act::External {
                name: (*name).to_owned(),
                checkout: checkout.into(),
            },
            _ => return Err(usage("unknown arguments")),
        };
        Ok(Command::Context {
            repo: repo.into(),
            act,
        })
    }

    fn table(words: &[&str]) -> Result<Command, Error> {
        let (full, words): (Vec<&str>, Vec<&str>) = words.iter().partition(|w| **w == "--full");
        if full.len() > 1 {
            return Err(usage("--full is given once"));
        }
        let full = !full.is_empty();
        let seen = if full { Seen::Full } else { Seen::Any };
        match words[..] {
            ["read", commit] => Ok(Command::Read {
                commit: commit.to_owned(),
                of: Of::Train,
                seen,
            }),
            ["read", commit, "--half", half] => Ok(Command::Read {
                commit: commit.to_owned(),
                of: Of::Half(Half::parse(half)?),
                seen,
            }),
            _ => Err(usage("unknown arguments")),
        }
    }

    /// An empty `<rev>` is refused, never read as master's tip, which would leave the rev meant
    /// below the table for good.
    fn publish(mut words: &[&str]) -> Result<Command, Error> {
        let (mut skips, mut admits) = (Vec::new(), Vec::new());
        loop {
            match words {
                ["--skip", skip, rest @ ..] => {
                    skips.push((*skip).to_owned());
                    words = rest;
                }
                ["--admit", path, rest @ ..] => {
                    admits.push(RepoPath::parse(path)?);
                    words = rest;
                }
                [] => {
                    return Ok(Command::Publish {
                        skips,
                        admits,
                        rev: None,
                    });
                }
                [""] => return Err(crate::refusal::Refusal::EmptyRev.into()),
                [rev] if !rev.starts_with("--") => {
                    return Ok(Command::Publish {
                        skips,
                        admits,
                        rev: Some((*rev).to_owned()),
                    });
                }
                _ => return Err(usage("unknown arguments")),
            }
        }
    }
}
