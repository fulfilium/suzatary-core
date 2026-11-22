//! `suzatary up`: enable the fetcher's services and run the doctor.
//!
//! The fetcher lock lives in `scripts/up.sh`, hostname-based, since two live fetchers race pushes
//! and invalidate each other's sessions.

use std::path::Path;

use crate::instance::Instance;

pub fn run(path: &Path) -> anyhow::Result<()> {
    super::script(&Instance::open(path)?, "up.sh")
}
