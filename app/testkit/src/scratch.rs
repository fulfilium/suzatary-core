//! A test's scratch directories. Each goes when its test ends, a failed test's too, unless
//! `SUZATARY_KEEP_SCRATCH` is set; each names itself in the test's output, which a failure shows.

use std::cell::OnceCell;
use std::io::ErrorKind;
use std::path::PathBuf;

use tempfile::TempDir;

const KEEP: &str = "SUZATARY_KEEP_SCRATCH";

const REMOVALS: usize = 100;

/// A fresh directory named after `name`, removed when the value drops.
///
/// # Panics
///
/// When the system's temporary directory cannot be written.
#[must_use]
pub fn scratch(name: &str) -> TempDir {
    let keep = kept();
    let dir = tempfile::Builder::new()
        .prefix(&format!("suzatary-{name}-"))
        .disable_cleanup(keep)
        .tempdir()
        .expect("a scratch directory");
    let fate = if keep {
        format!("kept ({KEEP} is set)")
    } else {
        format!("removed when the test ends; {KEEP}=1 keeps it")
    };
    eprintln!("scratch {}: {fate}", dir.path().display());
    dir
}

/// The running test's own directory, removed when the test's thread ends (libtest runs each test
/// on its own): the place for a path that a value outliving the test holds, as a test server
/// holds its fakes for the process. A thread the test spawns has a directory of its own.
#[must_use]
pub fn this_test() -> PathBuf {
    thread_local! {
        static DIR: OnceCell<ThisTest> = const { OnceCell::new() };
    }
    DIR.with(|dir| {
        dir.get_or_init(|| ThisTest(scratch("test")))
            .0
            .path()
            .to_path_buf()
    })
}

fn kept() -> bool {
    std::env::var_os(KEEP).is_some()
}

struct ThisTest(TempDir);

/// Removes the directory again, up to [`REMOVALS`] times, while it is not empty: a round the
/// test's server began can write as the test ends.
impl Drop for ThisTest {
    fn drop(&mut self) {
        if kept() {
            return;
        }
        for _ in 0..REMOVALS {
            let Err(e) = std::fs::remove_dir_all(self.0.path()) else {
                return;
            };
            if e.kind() != ErrorKind::DirectoryNotEmpty {
                return;
            }
        }
    }
}
