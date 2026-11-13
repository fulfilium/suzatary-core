//! The measures of every unit and of the repository, in tokens, over one tree.

use std::collections::{BTreeMap, BTreeSet};

use super::interface;
use super::plan::{Plan, Repository, Shape, Unit};
use super::tokens::Kind;
use super::tree::Tree;

/// The documents every agent reads, whichever unit it works on.
pub const MANUALS: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

/// The directories of a unit whose code is tests, and whose other files are fixtures.
const TEST_DIRECTORIES: [&str; 5] = ["tests", "test", "e2e", "benches", "examples"];

/// One unit's measures, in tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Figures {
    /// What a dependent reads: the README and the public interface.
    pub use_: u64,
    /// What an agent reads to change it: its production source, README, scripts, the documents
    /// they and the manuals cite, the manuals, and each dependency's USE.
    pub develop: u64,
    /// Its tests' code.
    pub tests: u64,
    /// Its tests' other files.
    pub fixtures: u64,
}

/// One unit as measured.
#[derive(Debug, Clone)]
pub struct Measured {
    pub name: String,
    pub figures: Figures,
    /// The files of its USE, and the part of each read.
    pub use_files: Reading,
    /// The files of its DEVELOP, and each dependency's USE.
    pub develop_files: Reading,
    /// The units and libraries its manifest and its witnesses show it uses.
    pub uses: BTreeSet<String>,
}

/// Every unit and the repository, measured over one tree.
#[derive(Debug, Clone)]
pub struct Report {
    /// The units, in the plan's order.
    pub units: Vec<Measured>,
    /// The repository's reading set: every unit's production source, README and scripts, the
    /// paths of `repository.include`, and every document any of them cites.
    pub reading: Reading,
    /// The dated plans of `repository.documents` that nothing in the reading set cites.
    pub uncited_plans: Vec<String>,
}

impl Report {
    /// The unit named `name`.
    #[must_use]
    pub fn unit(&self, name: &str) -> Option<&Measured> {
        self.units.iter().find(|unit| unit.name == name)
    }
}

/// What a set of files costs: each file once, by path, with the tokens of the part read.
#[derive(Debug, Clone, Default)]
pub struct Reading(pub BTreeMap<String, u64>);

impl Reading {
    fn add(&mut self, key: &str, tokens: u64) {
        self.0.insert(key.to_owned(), tokens);
    }

    /// Its tokens.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.0.values().sum()
    }
}

/// One unit's files, sorted by the part of the unit each belongs to.
#[derive(Debug, Default)]
struct Parts<'a> {
    /// Production source: path → the text read (a Rust file's part before its tests).
    source: Vec<(&'a str, &'a str)>,
    /// The tests inside source files, and the code of the test directories.
    tests: Vec<(&'a str, &'a str)>,
    fixtures: Vec<(&'a str, &'a str)>,
    readme: Option<(&'a str, &'a str)>,
}

/// Measure every unit of `plan` and the repository over `tree`.
#[must_use]
pub fn measure(plan: &Plan, tree: &Tree) -> Report {
    let tokens = |path: &str, text: &str| plan.tokens.ratio.tokens(Kind::of(path), text);
    let parts: Vec<Parts<'_>> = plan.units.iter().map(|unit| parts(unit, tree)).collect();
    let use_files: Vec<Reading> = plan
        .units
        .iter()
        .zip(&parts)
        .map(|(unit, parts)| {
            let mut reading = Reading::default();
            if let Some((path, text)) = parts.readme {
                reading.add(path, tokens(path, text));
            }
            for (path, text) in interface_of(unit, parts, tree) {
                reading.add(path, tokens(path, &text));
            }
            reading
        })
        .collect();
    let uses: BTreeMap<&str, u64> = plan
        .units
        .iter()
        .zip(&use_files)
        .map(|(unit, reading)| (unit.name.as_str(), reading.total()))
        .chain(
            plan.externals
                .iter()
                .map(|external| (external.name.as_str(), external.use_)),
        )
        .collect();
    let manuals: Vec<(&str, &str)> = MANUALS
        .iter()
        .filter_map(|path| tree.get(path).map(|text| (*path, text)))
        .collect();
    let units = plan
        .units
        .iter()
        .zip(parts.iter().zip(use_files))
        .map(|(unit, (parts, use_files))| {
            let read: Vec<(&str, &str)> = parts
                .source
                .iter()
                .copied()
                .chain(parts.readme)
                .chain(scripts(unit, tree))
                .chain(manuals.iter().copied())
                .collect();
            let mut develop = with_cited(&read, plan, tree);
            for name in &unit.depends {
                develop.add(&format!("the use of {name}"), uses[name.as_str()]);
            }
            let sum = |files: &[(&str, &str)]| -> u64 {
                files.iter().map(|(path, text)| tokens(path, text)).sum()
            };
            Measured {
                name: unit.name.clone(),
                figures: Figures {
                    use_: uses[unit.name.as_str()],
                    develop: develop.total(),
                    tests: sum(&parts.tests),
                    fixtures: sum(&parts.fixtures),
                },
                uses: uses_of(unit, parts, plan, tree),
                use_files,
                develop_files: develop,
            }
        })
        .collect();
    let mut read: Vec<(&str, &str)> = Vec::new();
    for (unit, parts) in plan.units.iter().zip(&parts) {
        read.extend(parts.source.iter().copied().chain(parts.readme));
        read.extend(scripts(unit, tree));
    }
    for path in &plan.repository.include {
        read.extend(tree.under(path).map(|(file, text)| {
            if Kind::of(file) == Kind::Rust {
                (file, interface::production(text))
            } else {
                (file, text)
            }
        }));
    }
    let reading = with_cited(&read, plan, tree);
    let uncited_plans = tree
        .under(&plan.repository.documents)
        .map(|(path, _)| path)
        .filter(|path| is_plan(path, &plan.repository.documents) && !reading.0.contains_key(*path))
        .map(str::to_owned)
        .collect();
    Report {
        units,
        reading,
        uncited_plans,
    }
}

/// The files of `read`, and every document they cite.
fn with_cited(read: &[(&str, &str)], plan: &Plan, tree: &Tree) -> Reading {
    let tokens = |path: &str, text: &str| plan.tokens.ratio.tokens(Kind::of(path), text);
    let mut reading = Reading::default();
    for (path, text) in read {
        reading.add(path, tokens(path, text));
    }
    for path in cited(read.iter().map(|(_, text)| *text), tree, &plan.repository) {
        if let Some(text) = tree.get(&path) {
            reading.add(&path, tokens(&path, text));
        }
    }
    reading
}

/// Whether `path` is a dated plan: `<documents>/<NAME>-PLAN-<date>.md`.
fn is_plan(path: &str, documents: &str) -> bool {
    path.strip_prefix(documents)
        .and_then(|rest| rest.strip_prefix('/'))
        .is_some_and(|name| {
            !name.contains('/') && name.contains("-PLAN-") && Kind::of(name) == Kind::Markdown
        })
}

/// The files of `unit`, sorted into its parts.
fn parts<'a>(unit: &'a Unit, tree: &'a Tree) -> Parts<'a> {
    let mut parts = Parts::default();
    let readme = format!("{}/README.md", unit.path);
    for (path, text) in tree.under(&unit.path) {
        let inside = &path[unit.path.len() + 1..];
        let tested = inside
            .split_once('/')
            .is_some_and(|(top, _)| TEST_DIRECTORIES.contains(&top));
        let code = Kind::of(path).is_code();
        if path == readme {
            parts.readme = Some((path, text));
        } else if tested && code {
            parts.tests.push((path, text));
        } else if tested {
            parts.fixtures.push((path, text));
        } else if Kind::of(path) == Kind::Rust {
            parts.source.push((path, interface::production(text)));
            let tests = interface::tests(text);
            if !tests.is_empty() {
                parts.tests.push((path, tests));
            }
        } else {
            parts.source.push((path, text));
        }
    }
    parts
}

/// The public interface of `unit`, file by file.
fn interface_of<'a>(unit: &Unit, parts: &Parts<'a>, tree: &'a Tree) -> Vec<(&'a str, String)> {
    match &unit.shape {
        Shape::Crate => {
            let src = format!("{}/src/", unit.path);
            parts
                .source
                .iter()
                .filter(|(path, _)| path.starts_with(&src) && Kind::of(path) == Kind::Rust)
                .map(|(path, text)| (*path, interface::rust(text)))
                .collect()
        }
        Shape::Web { entries } => tree
            .under("")
            .filter(|(path, _)| entries.iter().any(|entry| entry == path))
            .map(|(path, text)| (path, interface::typescript(text)))
            .collect(),
    }
}

/// The scripts `unit` names that the tree holds.
fn scripts<'a>(unit: &'a Unit, tree: &'a Tree) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
    unit.scripts
        .iter()
        .filter_map(|path| tree.get(path).map(|text| (path.as_str(), text)))
}

/// Every document of `tree` that one of `texts` cites by its path from the root, or by its name
/// under `repository.documents`, except the `records`.
fn cited<'a>(
    texts: impl Iterator<Item = &'a str>,
    tree: &Tree,
    repository: &Repository,
) -> BTreeSet<String> {
    let (documents, records) = (&repository.documents, &repository.records);
    let mut found = BTreeSet::new();
    for text in texts {
        for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || "_.-/".contains(c))) {
            let word = word
                .trim_end_matches('.')
                .trim_start_matches("./")
                .trim_start_matches('/');
            if Kind::of(word) != Kind::Markdown {
                continue;
            }
            for path in [word.to_owned(), format!("{documents}/{word}")] {
                if tree.get(&path).is_some() {
                    if !records.contains(&path) {
                        found.insert(path);
                    }
                    break;
                }
            }
        }
    }
    found
}

/// The units and libraries `unit` uses: its manifest's dependencies that are units or external
/// libraries, and the units its witnesses' texts show.
fn uses_of(unit: &Unit, parts: &Parts<'_>, plan: &Plan, tree: &Tree) -> BTreeSet<String> {
    let mut uses: BTreeSet<String> = unit
        .witnesses
        .iter()
        .filter(|(text, _)| {
            parts
                .source
                .iter()
                .any(|(_, source)| source.contains(text.as_str()))
        })
        .map(|(_, name)| name.clone())
        .collect();
    match unit.shape {
        Shape::Crate => uses.extend(manifest_uses(unit, plan, tree)),
        Shape::Web { .. } => {}
    }
    uses
}

/// The dependencies of a crate's `Cargo.toml` that are units or external libraries: a path
/// dependency by its directory, a git one by its package, and a workspace one by the workspace's
/// entry.
fn manifest_uses(unit: &Unit, plan: &Plan, tree: &Tree) -> BTreeSet<String> {
    let parse = |path: &str| {
        tree.get(path)
            .and_then(|text| text.parse::<toml::Table>().ok())
            .unwrap_or_default()
    };
    let manifest = parse(&format!("{}/Cargo.toml", unit.path));
    let workspace_dir = unit.path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let workspace = parse(&format!("{workspace_dir}/Cargo.toml"));
    let shared = workspace
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(toml::Value::as_table);
    let by_path = |dir: &str, relative: &str| -> Option<String> {
        let mut segments: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
        for segment in relative.split('/') {
            match segment {
                "." | "" => {}
                ".." => {
                    segments.pop();
                }
                name => segments.push(name),
            }
        }
        let path = segments.join("/");
        plan.units
            .iter()
            .find(|unit| unit.path == path)
            .map(|unit| unit.name.clone())
    };
    let resolve = |key: &str, spec: &toml::Value, dir: &str| -> Option<String> {
        if let Some(path) = spec.get("path").and_then(toml::Value::as_str) {
            return by_path(dir, path);
        }
        if spec.get("git").is_some() {
            let package = spec
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key);
            return plan.external(package).map(|external| external.name.clone());
        }
        None
    };
    let mut tables: Vec<&toml::Table> = ["dependencies", "build-dependencies"]
        .iter()
        .filter_map(|name| manifest.get(*name).and_then(toml::Value::as_table))
        .collect();
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        tables.extend(
            targets
                .values()
                .filter_map(|target| target.get("dependencies").and_then(toml::Value::as_table)),
        );
    }
    tables
        .into_iter()
        .flatten()
        .filter_map(|(key, spec)| {
            if spec.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                shared
                    .and_then(|shared| shared.get(key))
                    .and_then(|entry| resolve(key, entry, workspace_dir))
            } else {
                resolve(key, spec, &unit.path)
            }
        })
        .filter(|name| name != &unit.name)
        .collect()
}
