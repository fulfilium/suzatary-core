//! The processes a guard watches, read from `/proc`: by pid, never by a name. Both test targets
//! of guards mount this file, as they do scratch.rs.

/// Whether the process `pid` still runs, a zombie counted as gone.
#[must_use]
pub fn runs(pid: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit_once(") ")
            .is_some_and(|(_, rest)| !rest.starts_with('Z'))
    })
}

/// The children of the process `pid`, by pid.
///
/// # Panics
///
/// When `/proc` cannot be read.
#[must_use]
pub fn children(pid: &str) -> Vec<String> {
    std::fs::read_dir("/proc")
        .expect("/proc reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|child| {
            std::fs::read_to_string(format!("/proc/{child}/stat")).is_ok_and(|stat| {
                stat.rsplit_once(") ")
                    .and_then(|(_, rest)| rest.split(' ').nth(1))
                    .is_some_and(|ppid| ppid == pid)
            })
        })
        .collect()
}
