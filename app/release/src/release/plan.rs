//! Which todos are releases: the declaration `releases.toml` holds, beside the store whose todos
//! they are.

use std::collections::BTreeSet;

use prodrome_data::event::{Hash, TodoId};
use serde::Deserialize;

use super::Counted;

/// The releases the version counts toward, in the order they ship, in one store.
///
/// Only [`Plan::new`] makes one, so every plan has distinct todos whose versions strictly
/// increase, the first above `base.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    genesis: Hash,
    base: u32,
    releases: Vec<Release>,
}

/// One release: the todo whose spec composes its items, and the `major.minor` its completion
/// moves the version to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The release's own todo.
    pub todo: TodoId,
    /// The version's `major.minor` once it is completed.
    pub version: Counted,
}

/// Why a declaration is not a plan.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    /// Not the file's shape.
    #[error("releases.toml: {0}")]
    Unread(String),
    /// Its `genesis` is not an object's name.
    #[error("releases.toml: genesis {0:?} is not an object's name")]
    NotAGenesis(String),
    /// A release's todo is not a todo id.
    #[error("releases.toml: {0:?} is not a todo id")]
    NotATodo(String),
    /// Two releases name one todo.
    #[error("releases.toml: {0} is declared twice")]
    Twice(String),
    /// A release's version is not above the one before it, or the first not above `base.0`.
    #[error("releases.toml: {todo} is {version}, not above {floor}")]
    NotAbove {
        /// The release.
        todo: String,
        /// Its version.
        version: Counted,
        /// What it must be above.
        floor: Counted,
    },
}

/// The file as written, before [`Plan::new`] reads it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declared {
    genesis: String,
    base: u32,
    #[serde(default)]
    release: Vec<DeclaredRelease>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredRelease {
    todo: String,
    major: u32,
    minor: u32,
}

impl Plan {
    /// The plan of `releases` in the store begun by `genesis`, the version being `base.0` until
    /// one completes.
    ///
    /// The versions strictly increase, so the last completed release in the plan's order is
    /// also the greatest, and completing any release never lowers what the chain reads.
    ///
    /// # Errors
    /// A todo twice, or a version not above the one before it (`base.0` for the first).
    pub fn new(genesis: Hash, base: u32, releases: Vec<Release>) -> Result<Plan, PlanError> {
        let mut seen = BTreeSet::new();
        let mut floor = Counted {
            major: base,
            minor: 0,
        };
        for release in &releases {
            if !seen.insert(release.todo.as_str()) {
                return Err(PlanError::Twice(release.todo.as_str().to_owned()));
            }
            if release.version <= floor {
                return Err(PlanError::NotAbove {
                    todo: release.todo.as_str().to_owned(),
                    version: release.version,
                    floor,
                });
            }
            floor = release.version;
        }
        Ok(Plan {
            genesis,
            base,
            releases,
        })
    }

    /// The plan `text` declares, in `releases.toml`'s shape.
    ///
    /// # Errors
    /// Not that shape, or not a plan.
    pub fn parse(text: &str) -> Result<Plan, PlanError> {
        let declared: Declared =
            toml::from_str(text).map_err(|e| PlanError::Unread(e.message().to_owned()))?;
        let releases = declared
            .release
            .into_iter()
            .map(|r| {
                TodoId::new(r.todo.clone())
                    .map(|todo| Release {
                        todo,
                        version: Counted {
                            major: r.major,
                            minor: r.minor,
                        },
                    })
                    .map_err(|_| PlanError::NotATodo(r.todo))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let genesis = Hash::new(declared.genesis.clone())
            .map_err(|_| PlanError::NotAGenesis(declared.genesis))?;
        Plan::new(genesis, declared.base, releases)
    }

    /// The genesis of the store whose todos the releases are: that store's name wherever a
    /// deployment mounts it.
    #[must_use]
    pub fn genesis(&self) -> &Hash {
        &self.genesis
    }

    /// The major before any release completes, whose minor is 0.
    #[must_use]
    pub fn base(&self) -> u32 {
        self.base
    }

    /// The releases, in the order they ship.
    #[must_use]
    pub fn releases(&self) -> &[Release] {
        &self.releases
    }
}
