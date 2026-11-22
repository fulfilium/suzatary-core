//! Compile-time includes and the flake's test builds holding them: a build in Nix reads only its
//! fileset, so an include from outside the fileset compiles everywhere but in Nix.

use super::scratch::on_path;
use super::{Scratch, repo};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One `include_str!`, `include_bytes!` or `include!` of a string literal.
pub(super) struct Include {
    pub(super) from: PathBuf,
    /// The literal, a path from the directory of `from`.
    pub(super) literal: String,
}

impl Include {
    /// The path it names, beside the file that includes it, as rustc resolves it.
    pub(super) fn target(&self) -> PathBuf {
        self.from
            .parent()
            .expect("a file's directory")
            .join(&self.literal)
    }
}

/// Every include of a literal under `dir`; one of a non-literal (`concat!`, `env!`) is not read.
pub(super) fn includes(dir: &Path) -> Vec<Include> {
    let mut found = Vec::new();
    walk(dir, &mut found);
    found
}

fn walk(dir: &Path, into: &mut Vec<Include>) {
    for entry in std::fs::read_dir(dir)
        .expect("a readable directory")
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "target" && name != "node_modules" {
                walk(&path, into);
            }
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a readable source");
        for macro_ in ["include_str!", "include_bytes!", "include!"] {
            for (at, _) in text.match_indices(macro_) {
                let rest = text[at + macro_.len()..].trim_start();
                let Some(rest) = rest.strip_prefix('(').map(str::trim_start) else {
                    continue;
                };
                let Some(literal) = rest.strip_prefix('"').and_then(|r| r.split('"').next()) else {
                    continue;
                };
                into.push(Include {
                    from: path.clone(),
                    literal: literal.to_owned(),
                });
            }
        }
    }
}

/// The includes of the sources under `source` that name a file `source` does not hold, each as
/// the including file and the path it names, both from `source`.
fn unheld(source: &Path) -> Vec<String> {
    includes(source)
        .into_iter()
        .filter(|include| !include.target().is_file())
        .map(|include| {
            let from = include.from.strip_prefix(source).expect("under the source");
            let named = lexical(&from.parent().expect("a directory").join(&include.literal));
            format!(
                "{} includes {:?}, which is {}",
                from.display(),
                include.literal,
                named.display()
            )
        })
        .collect()
}

/// `path` with its `..` and `.` taken out by name, as the path from the source's root it names.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The store copies of the sources of the flake's builds that compile tests, each with its
/// outputs: the tree each build sees, not a reading of `flake.nix`'s text (a dependency build's
/// crane dummy holds no include).
fn test_sources() -> BTreeMap<PathBuf, Vec<String>> {
    let system = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
    let selects = [
        ("checks", "true"),
        (
            "packages",
            "(o.doCheck or false) || builtins.match \".*cargo test.*\" (o.buildPhase or \"\") != null",
        ),
    ];
    let mut sources: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for (outputs, select) in selects {
        let apply = format!(
            "outputs: let copied = src: builtins.all (c: c.path or false) \
             (builtins.attrValues (builtins.getContext (toString src))); in \
             builtins.mapAttrs (name: o: if o ? src && copied o.src && ({select}) \
             then toString o.src else null) outputs"
        );
        let out = Command::new("nix")
            .args(["eval", "--json", "--apply", &apply])
            .arg(format!("{}#{outputs}.{system}", repo().display()))
            .output()
            .expect("nix eval runs");
        assert!(out.status.success(), "nix eval {outputs}: {out:?}");
        let named: BTreeMap<String, Option<PathBuf>> =
            serde_json::from_slice(&out.stdout).expect("a set of sources");
        for (name, source) in named
            .into_iter()
            .filter_map(|(name, source)| Some((name, source?)))
        {
            sources
                .entry(source)
                .or_default()
                .push(format!("{outputs}.{name}"));
        }
    }
    sources
}

#[test]
fn every_build_of_tests_holds_what_its_sources_include() {
    if !on_path("nix") || std::env::var_os("IN_NIX_SHELL").is_none() {
        eprintln!("every_build_of_tests_holds_what_its_sources_include: not in a Nix shell");
        return;
    }
    let sources = test_sources();
    assert!(
        sources
            .values()
            .flatten()
            .any(|o| o == "packages.suzatary-wire-ts"),
        "the wire's build, which runs the core's unit tests, is one of them: {sources:?}"
    );
    let unheld: Vec<String> = sources
        .iter()
        .flat_map(|(source, outputs)| {
            unheld(source)
                .into_iter()
                .map(move |line| format!("{}: {line}", outputs.join(", ")))
        })
        .collect();
    assert!(
        unheld.is_empty(),
        "these builds' filesets in flake.nix do not hold what their sources include (add the \
         file to the fileset, or move it under one the fileset holds):\n{}",
        unheld.join("\n")
    );
}

#[test]
fn an_include_outside_the_source_is_named() {
    let source = Scratch::new("includes-unheld");
    let write = |path: &str, text: &str| {
        let path = source.join(path);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("a directory");
        std::fs::write(path, text).expect("a file");
    };
    write("app/secretary/releases.toml", "");
    write(
        "app/secretary/src/release/plan.rs",
        "const DECLARED: &str = include_str!(\"../../releases.toml\");\n",
    );
    assert!(unheld(&source).is_empty(), "{:?}", unheld(&source));
    write(
        "app/secretary/src/secretary/line_event.rs",
        "#[cfg(test)]\nconst REPLICAS: &str = include_str!(\n    \"../../tests/web/replicas.json\");\n",
    );
    assert_eq!(
        unheld(&source),
        [
            "app/secretary/src/secretary/line_event.rs includes \"../../tests/web/replicas.json\", \
          which is app/secretary/tests/web/replicas.json"
        ]
    );
}
