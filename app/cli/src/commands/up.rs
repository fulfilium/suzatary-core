//! `suzatary up`: enable the fetcher's services and run the doctor.
//!
//! The fetcher lock lives in `scripts/up.sh`: it refuses any box but the one `FETCHER` names,
//! since two live fetchers race pushes and invalidate each other's sessions. Hostname-based.

use std::path::Path;

use crate::instance::Instance;

/// `suzatary up`.
pub fn run(path: &Path) -> anyhow::Result<()> {
    super::script(&Instance::open(path)?, "up.sh")
}
