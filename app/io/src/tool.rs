//! A tool this box runs as a subprocess, resolved the same way everywhere: on `PATH`, else
//! through the code root's `triage` shell.

use std::path::Path;

use crate::run::Cmd;

/// `name` on `PATH`, else through the repository's `triage` devShell. The shell is named because
/// `nix develop` reads a bare store path as a derivation, not a flake.
#[must_use]
pub fn tool(repo: &Path, name: &str) -> Cmd {
    if on_path(name) {
        Cmd::new(name).pass(crate::run::TYPST)
    } else {
        Cmd::new("nix")
            .arg("develop")
            .arg(format!("{}#triage", repo.display()))
            .arg("--command")
            .arg(name)
            .pass(crate::run::TYPST)
            .pass(crate::run::NIX)
            .pass(crate::run::NETWORK)
    }
}

pub(crate) fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(name)))
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
    }
}
