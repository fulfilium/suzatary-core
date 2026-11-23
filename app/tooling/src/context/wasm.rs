//! `web/wasm-budget`: for each shipped wasm module, the bytes of the tree the train started from and
//! the most the train's tree may ship; `scripts/build-web.sh` refuses a module over its limit.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::path::PathBuf;

use anyhow::{Context as _, bail};

use super::figures::Figure;

pub(crate) const BUDGET: &str = "web/wasm-budget";

/// The bytes a module may grow by for each merged pull request, undeclared.
pub(crate) const ALLOWANCE: u64 = 16_000;

const HEADER: &str = "\
# <module> <system> <measured> <limit>, written by the train (AGENTS.md, \"Derived figures\"):
# `measured` is the module's bytes after wasm-opt in the tree the train started from, one train
# behind; `limit` adds the allowance for each merged pull request and what they declare.
";

/// One module's line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) measured: u64,
    pub(crate) limit: u64,
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "measured {}, limit {}", self.measured, self.limit)
    }
}

/// The lines, by module and system.
pub(crate) type Budget = BTreeMap<Figure, Line>;

/// A wasm module's file name.
fn is_module(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension == "wasm")
}

/// The file's text, parsed.
pub(crate) fn read(text: &str) -> anyhow::Result<Budget> {
    let number = |word: &str| {
        word.parse::<u64>()
            .with_context(|| format!("{BUDGET}: `{word}` is not a number of bytes"))
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_whitespace().collect::<Vec<_>>()[..] {
            [module, system, measured, limit] if is_module(module) => Ok((
                Figure::Wasm {
                    module: module.to_owned(),
                    system: system.to_owned(),
                },
                Line {
                    measured: number(measured)?,
                    limit: number(limit)?,
                },
            )),
            _ => bail!("{BUDGET}: `{l}` is not `<module>.wasm <system> <measured> <limit>`"),
        })
        .collect()
}

/// The file's text for `budget`.
pub(crate) fn print(budget: &Budget) -> String {
    let mut out = HEADER.to_owned();
    for (figure, line) in budget {
        match figure {
            Figure::Wasm { module, system } => {
                let _ = writeln!(out, "{module} {system} {} {}", line.measured, line.limit);
            }
            Figure::Reading | Figure::Use(_) => {}
        }
    }
    out
}

/// A build's modules, as `scripts/wasm-modules.sh` prints them: the system that built them, and
/// each module's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modules {
    pub system: String,
    pub files: Vec<PathBuf>,
}

impl Modules {
    /// Each module's bytes, by its file's name.
    pub(crate) fn measure(&self) -> anyhow::Result<BTreeMap<Figure, u64>> {
        self.files
            .iter()
            .map(|file| {
                let module = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| is_module(name))
                    .with_context(|| format!("{} is not a wasm module", file.display()))?;
                let bytes = std::fs::metadata(file)
                    .with_context(|| format!("measuring {}", file.display()))?
                    .len();
                Ok((
                    Figure::Wasm {
                        module: module.to_owned(),
                        system: self.system.clone(),
                    },
                    bytes,
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{print, read};

    /// The file the train writes reads back as the lines it wrote.
    #[test]
    fn the_printed_file_reads_back() {
        let budget = read("views_bg.wasm aarch64-linux 3760404 3776404\n").expect("it reads");
        assert_eq!(read(&print(&budget)).expect("it reads"), budget);
        assert!(read("views_bg.wasm aarch64-linux 3760404\n").is_err());
        assert!(read("views.js aarch64-linux 1 2\n").is_err());
    }
}
