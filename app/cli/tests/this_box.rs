//! This box's own deployment, as the live checks read it: `SUZATARY_INSTANCE`, else the
//! repository's `instance.toml`.

use std::path::{Path, PathBuf};

use suzatary_cli::instance::Instance;

pub(crate) fn path() -> PathBuf {
    std::env::var(suzatary_cli::instance::ENV)
        .ok()
        .filter(|named| !named.is_empty())
        .map_or_else(
            || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../instance.toml"),
            PathBuf::from,
        )
}

/// The deployment, or `None` only where no instance file exists: a file that does not open
/// fails the run, because a live check that cannot read the box has not passed.
pub(crate) fn instance() -> Option<Instance> {
    let path = path();
    path.exists().then(|| {
        Instance::open(&path).unwrap_or_else(|failed| {
            panic!(
                "{} is this box's and does not open: {failed:#}",
                path.display()
            )
        })
    })
}
