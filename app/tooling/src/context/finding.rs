//! What the guard refuses of the tree alone: a plan that no longer matches the code, and a unit
//! whose USE crosses the budget.

use std::collections::BTreeSet;
use std::fmt;

use super::measure::Report;
use super::plan::{FILE, Plan, Shape};
use super::tree::Tree;

/// One refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A unit's `depends` is not what its manifest and witnesses show.
    Depends {
        unit: String,
        /// Named in `depends` and not shown.
        unshown: BTreeSet<String>,
        /// Shown and not named in `depends`.
        unnamed: BTreeSet<String>,
    },
    /// A unit not named in `budget.over_use` whose USE is over `budget.use`.
    OverUse {
        unit: String,
        tokens: u64,
        budget: u64,
    },
    /// A workspace member that no unit names.
    NoUnit {
        /// The member's directory.
        member: String,
    },
    /// A crate unit whose directory is no workspace member.
    NoMember { unit: String },
    /// A file the plan names that the tree does not hold.
    Missing {
        /// The unit or library that names it.
        by: String,
        path: String,
    },
    /// A library's recorded revision is not the one its file pins.
    Unpinned {
        /// Its source.
        library: String,
        /// The file that should hold the revision.
        file: String,
    },
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Depends {
                unit,
                unshown,
                unnamed,
            } => write!(
                f,
                "{unit}: {FILE} names {unshown:?} that its code does not use, and its code uses \
                 {unnamed:?} that {FILE} does not name"
            ),
            Self::OverUse {
                unit,
                tokens,
                budget,
            } => write!(
                f,
                "{unit}: its USE is {tokens} tokens, over the budget of {budget}: an agent cannot \
                 use it from its README and public interface; cut its interface or split the unit \
                 (AGENTS.md, rule 7)"
            ),
            Self::NoUnit { member } => {
                write!(f, "the workspace member {member} is no unit of {FILE}")
            }
            Self::NoMember { unit } => write!(f, "{unit}: its directory is no workspace member"),
            Self::Missing { by, path } => {
                write!(f, "{by}: {FILE} names {path}, which is not in the tree")
            }
            Self::Unpinned { library, file } => write!(
                f,
                "{library}: {file} no longer pins the revision {FILE} measured; write the new \
                 one as its `rev` and run `scripts/context.sh external <name> <checkout>` there \
                 for each of its parts"
            ),
        }
    }
}

/// Every refusal of `report`, measured over `tree` by `plan`.
#[must_use]
pub(crate) fn findings(plan: &Plan, tree: &Tree, report: &Report) -> Vec<Finding> {
    let mut found = Vec::new();
    for (unit, measured) in plan.units.iter().zip(&report.units) {
        if measured.uses != unit.depends {
            found.push(Finding::Depends {
                unit: unit.name.clone(),
                unshown: unit.depends.difference(&measured.uses).cloned().collect(),
                unnamed: measured.uses.difference(&unit.depends).cloned().collect(),
            });
        }
        if measured.figures.use_ > plan.budget.use_ && !plan.budget.over_use.contains(&unit.name) {
            found.push(Finding::OverUse {
                unit: unit.name.clone(),
                tokens: measured.figures.use_,
                budget: plan.budget.use_,
            });
        }
        let entries = match &unit.shape {
            Shape::Crate => &[][..],
            Shape::Web { entries } => entries.as_slice(),
        };
        for path in unit.scripts.iter().chain(entries) {
            if tree.get(path).is_none() {
                found.push(Finding::Missing {
                    by: unit.name.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    found.extend(
        plan.in_force
            .iter()
            .filter(|row| tree.get(&row.file).is_none())
            .map(|row| Finding::Missing {
                by: format!("the plan in force for {}", row.task),
                path: row.file.clone(),
            }),
    );
    found.extend(members(plan, tree));
    for library in &plan.libraries {
        if !tree
            .get(&library.pinned_in)
            .is_some_and(|text| text.contains(&library.rev))
        {
            found.push(Finding::Unpinned {
                library: library.source.clone(),
                file: library.pinned_in.clone(),
            });
        }
    }
    found
}

/// The crate units and the workspaces' members, matched both ways.
fn members(plan: &Plan, tree: &Tree) -> Vec<Finding> {
    let crates: Vec<_> = plan
        .units
        .iter()
        .filter(|unit| match unit.shape {
            Shape::Crate => true,
            Shape::Web { .. } => false,
        })
        .collect();
    let workspaces: BTreeSet<&str> = crates
        .iter()
        .map(|unit| unit.path.rsplit_once('/').map_or("", |(parent, _)| parent))
        .collect();
    let members: BTreeSet<String> = workspaces
        .iter()
        .flat_map(|dir| {
            tree.get(&format!("{dir}/Cargo.toml"))
                .and_then(|text| text.parse::<toml::Table>().ok())
                .and_then(|manifest| {
                    manifest
                        .get("workspace")
                        .and_then(|w| w.get("members"))
                        .and_then(toml::Value::as_array)
                        .cloned()
                })
                .unwrap_or_default()
                .into_iter()
                .filter_map(move |member| member.as_str().map(|member| format!("{dir}/{member}")))
        })
        .collect();
    let mut found: Vec<Finding> = members
        .iter()
        .filter(|member| crates.iter().all(|unit| &unit.path != *member))
        .map(|member| Finding::NoUnit {
            member: member.clone(),
        })
        .collect();
    found.extend(
        crates
            .iter()
            .filter(|unit| !members.contains(&unit.path))
            .map(|unit| Finding::NoMember {
                unit: unit.name.clone(),
            }),
    );
    found
}
