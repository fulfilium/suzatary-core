//! The box-local tier on disk: a directory whose own `.gitignore` keeps it out of every push.

use std::path::Path;

const IGNORED: &str = "*\n";

/// Make `dir` a tier: the directory, and its `.gitignore` written once.
///
/// # Errors
/// The directory or the file could not be written.
pub fn ignored(dir: &Path) -> std::io::Result<()> {
    let path = dir.join(".gitignore");
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(path, IGNORED)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Git reads the file as ignoring everything beside it, the file included.
    #[test]
    fn the_tier_ignores_everything_in_it_and_is_written_once() {
        let dir = tempfile::tempdir().expect("scratch");
        let tier = dir.path().join("local");
        ignored(&tier).expect("written");
        assert_eq!(
            std::fs::read_to_string(tier.join(".gitignore")).expect("reads"),
            "*\n"
        );
        std::fs::write(tier.join(".gitignore"), "*\n# kept\n").expect("an owner's edit");
        ignored(&tier).expect("again");
        assert_eq!(
            std::fs::read_to_string(tier.join(".gitignore")).expect("reads"),
            "*\n# kept\n"
        );
    }
}
