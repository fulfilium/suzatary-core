//! What `train` was asked to do, parsed once from its arguments.

use crate::{Ended, Error, Half, Scope, Seen};

pub const USAGE: &str = "usage: train table read <commit> [--half tree|live] [--full]
       train table write <half> <commit> <status> [--full]";

/// One verb of the tool, its arguments parsed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Command {
    /// One word on stdout: the reading `of` the commit's tree.
    Read { commit: String, of: Of, seen: Seen },
    /// The row of a half's run that ended with an exit status; no row when it did not start.
    Write {
        half: Half,
        commit: String,
        ended: Ended,
        scope: Scope,
    },
}

/// What a reading answers for: one half, or the train, whose verdict weighs both.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Of {
    Half(Half),
    Train,
}

impl Command {
    pub fn parse(args: &[String]) -> Result<Command, Error> {
        let usage = |why: &str| Error::Usage(format!("{why}\n{USAGE}"));
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        let (full, words): (Vec<&str>, Vec<&str>) = words.into_iter().partition(|w| *w == "--full");
        if full.len() > 1 {
            return Err(usage("--full is given once"));
        }
        let full = !full.is_empty();
        match words[..] {
            ["table", "read", commit] => Ok(Command::Read {
                commit: commit.to_owned(),
                of: Of::Train,
                seen: if full { Seen::Full } else { Seen::Any },
            }),
            ["table", "read", commit, "--half", half] => Ok(Command::Read {
                commit: commit.to_owned(),
                of: Of::Half(Half::parse(half)?),
                seen: if full { Seen::Full } else { Seen::Any },
            }),
            ["table", "write", half, commit, status] => Ok(Command::Write {
                half: Half::parse(half)?,
                commit: commit.to_owned(),
                ended: Ended::of_status(
                    status
                        .parse()
                        .map_err(|_| usage(&format!("\"{status}\" is no exit status")))?,
                ),
                scope: if full { Scope::Full } else { Scope::Selective },
            }),
            _ => Err(usage("unknown arguments")),
        }
    }
}
