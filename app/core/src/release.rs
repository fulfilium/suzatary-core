//! Suzatary's version, read off its project chain.
//!
//! A release is a todo on the project chain whose spec composes its items (`Ref`s): `2.0` is
//! `suzatarytwo`, and its fulfillment is the release's progress. [`Plan`] declares which todos
//! are releases and the `major.minor` each one ships as (`app/core/releases.toml`);
//! [`Reading::of`] is the one rule from the chain to the `major.minor` it reads, the last
//! completed release's, a pure and monotone function of the chain's objects; and
//! [`Version::next`] is the step from the last release to the next, the patch counting the
//! trains since `major.minor` was first tagged. `scripts/train.sh` asks `suzatary release` for that
//! step and writes the tag, the changelog and `app/release.toml` from its one answer; no crate
//! compiles that file, so a release moves no compiled input.

mod cut;
mod fragment;
mod plan;
mod reading;
mod released;
mod version;

pub use cut::{Cut, TreeError};
pub use fragment::{Fragment, FragmentError, Kind};
pub use plan::{Plan, PlanError, Release};
pub use reading::{Item, Lot, Named, Place, Reading, ReleaseReading, binding};
pub use released::{Completed, Completes, Released, ReleasedError};
pub use version::{Behind, Counted, NotAVersion, Version};
