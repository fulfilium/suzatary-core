//! A program the tool runs and reads.

use crate::Error;
use std::process::{Command, Output, Stdio};

/// The command as a person would type it, for an error that names it.
fn shown(cmd: &Command) -> String {
    std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn failed(cmd: &Command, why: String) -> Error {
    Error::Run {
        command: shown(cmd),
        why,
    }
}

/// The command's output whatever its status; only a program that did not start is an error.
pub fn output(cmd: &mut Command) -> Result<Output, Error> {
    cmd.output().map_err(|e| failed(cmd, e.to_string()))
}

/// The command's stdout without its last newline; a failure names the command and its words.
pub fn out(cmd: &mut Command) -> Result<String, Error> {
    let out = output(cmd)?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(failed(
            cmd,
            if said.is_empty() {
                format!("exited {}", out.status)
            } else {
                said
            },
        ));
    }
    let text =
        String::from_utf8(out.stdout).map_err(|_| failed(cmd, "its answer is not UTF-8".into()))?;
    Ok(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

/// Whether the command exits 0, its stdout dropped and its stderr where `stderr` sends it.
pub fn succeeds(cmd: &mut Command, stderr: Stdio) -> Result<bool, Error> {
    cmd.stdout(Stdio::null())
        .stderr(stderr)
        .status()
        .map(|status| status.success())
        .map_err(|e| failed(cmd, e.to_string()))
}
