//! What the guard refuses: a unit over its USE budget, a reading set grown past the ratchet, a
//! plan that no longer matches the code.

use std::collections::BTreeSet;
use std::fmt;

use super::measure::Report;
use super::plan::{FILE, Plan, Shape};
use super::tree::Tree;

/// One refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A unit's USE is over `budget.use`, or over its ratchet plus the allowance.
    OverUse {
        /// The unit.
        unit: String,
        /// Its USE.
        tokens: u64,
        /// The most it may be.
        limit: u64,
    },
    /// The reading set is over the ratchet plus the allowance.
    Grown {
        /// The reading set.
        reading: u64,
        /// The ratchet.
        ratchet: u64,
        /// The allowance.
        allowance: u64,
    },
    /// A unit's `depends` is not what its manifest and witnesses show.
    Depends {
        /// The unit.
        unit: String,
        /// Named in `depends` and not shown.
        unshown: BTreeSet<String>,
        /// Shown and not named in `depends`.
        unnamed: BTreeSet<String>,
    },
    /// A workspace member that no unit names.
    NoUnit {
        /// The member's directory.
        member: String,
    },
    /// A crate unit whose directory is no workspace member.
    NoMember {
        /// The unit.
        unit: String,
    },
    /// A file the plan names that the tree does not hold.
    Missing {
        /// The unit or library that names it.
        by: String,
        /// The file.
        path: String,
    },
    /// An external library's recorded revision is not the one its file pins.
    Unpinned {
        /// The library.
        external: String,
        /// The file that should hold the revision.
        file: String,
    },
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OverUse {
                unit,
                tokens,
                limit,
            } => write!(
                f,
                "{unit}: its USE is {tokens} tokens, over its limit of {limit}: shorten its README \
                 or its public interface"
            ),
            Self::Grown {
                reading,
                ratchet,
                allowance,
            } => write!(
                f,
                "the repository's reading set is {reading} tokens, over the ratchet of {ratchet} \
                 plus {allowance}: delete as much elsewhere in this change, or say in the pull \
                 request why the growth pays"
            ),
            Self::Depends {
                unit,
                unshown,
                unnamed,
            } => write!(
                f,
                "{unit}: {FILE} names {unshown:?} that its code does not use, and its code uses \
                 {unnamed:?} that {FILE} does not name"
            ),
            Self::NoUnit { member } => {
                write!(f, "the workspace member {member} is no unit of {FILE}")
            }
            Self::NoMember { unit } => write!(f, "{unit}: its directory is no workspace member"),
            Self::Missing { by, path } => {
                write!(f, "{by}: {FILE} names {path}, which is not in the tree")
            }
            Self::Unpinned { external, file } => write!(
                f,
                "{external}: {file} no longer pins the revision {FILE} measured; run \
                 `scripts/context.sh external {external} <checkout>` at the new revision"
            ),
        }
    }
}

/// Every refusal of `report`, measured over `tree` by `plan`.
#[must_use]
pub fn findings(plan: &Plan, tree: &Tree, report: &Report) -> Vec<Finding> {
    let mut found = Vec::new();
    for (unit, measured) in plan.units.iter().zip(&report.units) {
        let limit = plan.ratchet.use_limit(&unit.name, &plan.budget);
        if measured.figures.use_ > limit {
            found.push(Finding::OverUse {
                unit: unit.name.clone(),
                tokens: measured.figures.use_,
                limit,
            });
        }
        if measured.uses != unit.depends {
            found.push(Finding::Depends {
                unit: unit.name.clone(),
                unshown: unit.depends.difference(&measured.uses).cloned().collect(),
                unnamed: measured.uses.difference(&unit.depends).cloned().collect(),
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
    if report.reading.total() > plan.ratchet.reading + plan.budget.allowance {
        found.push(Finding::Grown {
            reading: report.reading.total(),
            ratchet: plan.ratchet.reading,
            allowance: plan.budget.allowance,
        });
    }
    found.extend(members(plan, tree));
    for external in &plan.externals {
        if !tree
            .get(&external.pinned_in)
            .is_some_and(|text| text.contains(&external.rev))
        {
            found.push(Finding::Unpinned {
                external: external.name.clone(),
                file: external.pinned_in.clone(),
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
