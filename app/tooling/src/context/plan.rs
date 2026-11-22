//! `context.toml`, parsed once: the units of the repository, the libraries it uses from outside,
//! the budgets, the ratchet and the token ratios.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, bail};
use serde::Deserialize;

use super::tokens::Ratios;

/// The file's name at the repository's root.
pub const FILE: &str = "context.toml";

/// The whole file, every name it uses checked to be a unit or an external library.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub budget: Budget,
    pub ratchet: Ratchet,
    pub tokens: Tokens,
    /// What the repository's reading set holds beside the units.
    pub repository: Repository,
    /// The dated plans that guide an open roadmap task, read by that task's session alone.
    pub in_force: Vec<InForce>,
    /// The units, in the file's order.
    #[serde(rename = "unit")]
    pub units: Vec<Unit>,
    #[serde(rename = "library")]
    pub(crate) libraries: Vec<Library>,
}

/// The budgets, in tokens.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    #[serde(rename = "use")]
    pub use_: u64,
    pub develop: u64,
    pub tests: u64,
    /// How far a change may grow the reading set past the ratchet.
    pub allowance: u64,
}

/// The figures over budget as last recorded, which may only fall.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ratchet {
    pub reading: u64,
    /// The USE of each unit over `budget.use`; a unit absent here is held to the budget.
    #[serde(rename = "use", default)]
    pub use_: BTreeMap<String, u64>,
}

impl Ratchet {
    /// The most USE `unit` may have: its ratchet plus the allowance, or else the budget.
    #[must_use]
    pub fn use_limit(&self, unit: &str, budget: &Budget) -> u64 {
        self.use_
            .get(unit)
            .map_or(budget.use_, |recorded| recorded + budget.allowance)
    }
}

/// The ratchet of `text`, a `context.toml` whose other tables may be another version's.
///
/// # Errors
/// When it does not parse, or its ratchet does not.
pub(crate) fn ratchet(text: &str) -> anyhow::Result<Ratchet> {
    #[derive(Deserialize)]
    struct Ratcheted {
        ratchet: Ratchet,
    }
    Ok(toml::from_str::<Ratcheted>(text)
        .with_context(|| format!("{FILE} does not parse"))?
        .ratchet)
}

/// How a file's characters become tokens, and the measurement the ratios came from.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    /// The instant of the last calibration, in UTC.
    pub calibrated: String,
    pub ratio: Ratios,
    /// The files the ratios were measured over.
    pub sample: Vec<Sample>,
}

/// One file of the calibration's sample.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    /// Its path from the repository's root.
    pub path: String,
    pub characters: u64,
    /// Its tokens as the model counted them.
    pub tokens: u64,
}

/// The paths in the reading set that belong to no unit.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    /// Files and directories, from the repository's root.
    pub include: Vec<String>,
    /// Logs that the manuals cite as a place to append to or search, which no agent reads whole:
    /// never counted as a cited document.
    pub records: Vec<String>,
    /// The directory of the documents: a bare document name resolves under it, and its dated
    /// plans that nothing cites are listed.
    pub documents: String,
}

/// A dated plan in force: outside the reading set, and never listed as uncited.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InForce {
    /// Its path from the repository's root.
    pub file: String,
    /// The open roadmap task it guides.
    pub task: String,
}

/// One unit: a workspace member or the web app. (`serde` cannot deny unknown fields beside a
/// flattened sum.)
#[derive(Debug, Clone, Deserialize)]
pub struct Unit {
    /// Its name, as `depends` spells it.
    pub name: String,
    /// Its directory from the repository's root.
    pub path: String,
    /// What kind of code it is.
    #[serde(flatten)]
    pub shape: Shape,
    /// The units and external libraries its code uses.
    pub depends: BTreeSet<String>,
    /// The scripts an agent reads to build and test it.
    pub scripts: Vec<String>,
    /// Text whose presence in its production source shows a dependency: text → the unit.
    #[serde(default)]
    pub witnesses: BTreeMap<String, String>,
}

/// What kind of code a unit is, which decides its interface and its sources.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
pub enum Shape {
    /// A Rust crate: `src/` is its source, its `pub` items its interface.
    Crate,
    /// The web app: `src/`, `static/` and `typst/` are its source, the exports of its entries
    /// its interface.
    Web {
        /// The modules a page loads.
        entries: Vec<String>,
    },
}

/// A repository from outside this one, at one revision for every part of it used here.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Library {
    pub source: String,
    pub rev: String,
    /// The file of this repository that holds `rev`.
    pub pinned_in: String,
    #[serde(rename = "external")]
    pub externals: Vec<External>,
}

/// One part of a library, measured from its pinned source.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct External {
    /// A crate's package name.
    pub name: String,
    /// Its directory inside its repository; empty for the root.
    pub path: String,
    pub shape: ExternalShape,
    #[serde(default)]
    pub reexports: BTreeSet<String>,
    #[serde(rename = "use")]
    pub use_: u64,
}

/// What kind of library an external one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ExternalShape {
    /// A Rust crate.
    Crate,
    /// A Typst package: its `#let` definitions are its interface.
    Typst,
}

impl Plan {
    /// The file's text, parsed and its names checked.
    ///
    /// # Errors
    /// When it does not parse, or names a unit twice or one it does not declare.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let plan: Self = toml::from_str(text).with_context(|| format!("{FILE} does not parse"))?;
        let mut names = BTreeSet::new();
        for name in plan
            .units
            .iter()
            .map(|unit| &unit.name)
            .chain(plan.externals().map(|(_, external)| &external.name))
        {
            if !names.insert(name.as_str()) {
                bail!("{FILE} names {name} twice");
            }
        }
        if let Some((_, external)) = plan.externals().find(|(_, external)| {
            external
                .reexports
                .iter()
                .any(|name| !names.contains(name.as_str()))
        }) {
            bail!(
                "{FILE}: {} re-exports what is no external library",
                external.name
            );
        }
        let mut files = BTreeSet::new();
        if let Some(row) = plan.in_force.iter().find(|row| !files.insert(&row.file)) {
            bail!("{FILE}: in_force names {} twice", row.file);
        }
        if let Some(name) = plan
            .ratchet
            .use_
            .keys()
            .find(|name| !names.contains(name.as_str()))
        {
            bail!("{FILE}: ratchet.use names {name}, which is no unit");
        }
        for unit in &plan.units {
            if let Some(name) = unit
                .depends
                .iter()
                .chain(unit.witnesses.values())
                .find(|name| !names.contains(name.as_str()))
            {
                bail!(
                    "{FILE}: unit {} names {name}, which is no unit and no external library",
                    unit.name
                );
            }
        }
        Ok(plan)
    }

    pub(crate) fn externals(&self) -> impl Iterator<Item = (&Library, &External)> {
        self.libraries.iter().flat_map(|library| {
            library
                .externals
                .iter()
                .map(move |external| (library, external))
        })
    }

    #[must_use]
    pub(crate) fn external(&self, name: &str) -> Option<(&Library, &External)> {
        self.externals().find(|(_, external)| external.name == name)
    }

    /// `name` and the parts its interface re-exports: what a dependent of `name` reads.
    pub(crate) fn charged<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> {
        let reexports = self.external(name).map(|(_, external)| &external.reexports);
        std::iter::once(name).chain(reexports.into_iter().flatten().map(String::as_str))
    }
}
