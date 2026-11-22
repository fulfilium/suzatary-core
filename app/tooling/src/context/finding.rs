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
        unit: String,
        tokens: u64,
        /// The most it may be.
        limit: u64,
    },
    /// The reading set is over what is allowed.
    Grown { reading: u64, limit: u64 },
    /// The ratchet is written, with another figure than the tree's.
    Written { written: u64, measured: u64 },
    /// A unit's `depends` is not what its manifest and witnesses show.
    Depends {
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
            Self::OverUse {
                unit,
                tokens,
                limit,
            } => write!(
                f,
                "{unit}: its USE is {tokens} tokens, over its limit of {limit}: shorten its README \
                 or its public interface"
            ),
            Self::Grown { reading, limit } => write!(
                f,
                "the reading set is {reading} tokens, over the {limit} its merge base allows: \
                 delete as much, or say why it pays on a line `Reading-growth: +{}: <reason>` in \
                 the pull request's description, which the train copies into its merge, and \
                 in a commit message of the branch, which its own gate reads",
                reading - limit
            ),
            Self::Written { written, measured } => write!(
                f,
                "{FILE} says `reading = {written}`, the tree reads {measured}: a pull request \
                 writes no `reading`, the train's build does"
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

/// What the reading set may be: the merge base's ratchet, one allowance per merge since, and
/// each `Reading-growth: +<tokens>: <reason>` line since.
#[derive(Debug)]
pub(crate) struct Allowed {
    ratchet: u64,
    allowance: u64,
    declared: u64,
}

impl Allowed {
    /// Over `ratchet`, for `merges` merges and the `messages` since it; a line not in its form
    /// declares nothing.
    #[must_use]
    pub(crate) fn of(ratchet: u64, allowance: u64, merges: u64, declared: &[String]) -> Self {
        let declared = declared.iter().filter_map(|value| tokens(value)).sum();
        Self {
            ratchet,
            allowance: allowance * merges.max(1),
            declared,
        }
    }

    const fn limit(&self) -> u64 {
        self.ratchet + self.allowance + self.declared
    }
}

/// The tokens a `Reading-growth: +<tokens>: <reason>` value declares, or none when it is not in
/// that form.
fn tokens(value: &str) -> Option<u64> {
    let (tokens, why) = value.trim().strip_prefix('+')?.split_once(':')?;
    (!why.trim().is_empty()).then_some(())?;
    tokens.trim().parse().ok()
}

/// Each declared value not in the form, refused, so a declaration is never read as what it does
/// not say.
pub(crate) fn malformed(declared: &[String]) -> Vec<String> {
    declared
        .iter()
        .filter(|value| tokens(value).is_none())
        .map(|value| {
            format!("`Reading-growth: {value}` is not `Reading-growth: +<tokens>: <reason>`")
        })
        .collect()
}

/// The refusals of a reading set of `measured` tokens whose ratchet says `written`.
#[must_use]
pub(crate) fn reading(written: u64, measured: u64, allowed: &Allowed) -> Vec<Finding> {
    let mut found = Vec::new();
    if measured > allowed.limit() {
        found.push(Finding::Grown {
            reading: measured,
            limit: allowed.limit(),
        });
    }
    if written != allowed.ratchet && written != measured {
        found.push(Finding::Written { written, measured });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{Allowed, malformed, reading};

    /// A declaration not in its form declares nothing and is refused, so it is never read as what
    /// it does not say.
    #[test]
    fn a_malformed_growth_is_refused() {
        let said = |values: &[&str]| -> Vec<String> {
            malformed(&values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>())
        };
        assert!(said(&["+600: the laws"]).is_empty());
        for wrong in ["600 tokens", "+600:", "+six hundred: the laws"] {
            assert_eq!(said(&[wrong]).len(), 1, "{wrong}");
        }
    }

    /// Over a merge base's ratchet of 1,000 and 2,000 for each merge: a growth past it with no
    /// declared line is refused and names the line to write; a declared one goes through; a
    /// pull request that writes `reading` is refused unless it wrote the tree's own figure; and
    /// a line not in its form declares nothing, and neither does a line of prose that begins with
    /// the key, which git reads as no trailer.
    #[test]
    fn a_growth_is_declared_and_reading_is_the_builds() {
        let said = |found: Vec<super::Finding>| -> Vec<String> {
            found.iter().map(ToString::to_string).collect()
        };
        let declared =
            |message: &str| crate::git::trailers("Reading-growth", message).expect("git reads");
        let one = Allowed::of(1_000, 2_000, 1, &[]);
        assert!(
            reading(1_000, 3_000, &one).is_empty(),
            "within one allowance"
        );
        assert!(reading(1_000, 5_000, &Allowed::of(1_000, 2_000, 2, &[])).is_empty());
        let refused = said(reading(1_000, 3_500, &one));
        assert!(
            refused.len() == 1 && refused[0].contains("`Reading-growth: +500: <reason>`"),
            "{refused:?}"
        );
        let lines = declared("x\n\nprose.\n\nReading-growth: +600: the laws\n");
        assert!(reading(1_000, 3_500, &Allowed::of(1_000, 2_000, 1, &lines)).is_empty());
        let prose =
            declared("x\n\nIts lines ride beside\nReading-growth: +600: as said.\nMore prose.\n");
        assert_eq!(prose, Vec::<String>::new(), "a line of prose is no trailer");
        let written = said(reading(3_000, 2_900, &one));
        assert!(
            written.len() == 1 && written[0].contains("a pull request writes no `reading`"),
            "{written:?}"
        );
        assert!(
            reading(2_900, 2_900, &one).is_empty(),
            "the build's own figure"
        );
        let unread = Allowed::of(
            1_000,
            2_000,
            1,
            &declared("x\n\nReading-growth: 600 tokens\n"),
        );
        assert_eq!(said(reading(1_000, 3_500, &unread)).len(), 1);
    }
}
