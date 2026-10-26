//! `scripts/git-credential-secretspec.sh`: the token comes from secretspec, and the username is
//! the remote URL's when git hands one over, else GitHub's generic token user, never a name of
//! one deployment. Run as the real script against a `secretspec` stand-in on `PATH`.

use super::{Scratch, repo};
use std::io::Write as _;
use std::process::{Command, Stdio};

fn answer(scratch: &Scratch, stdin: &str) -> String {
    let script = repo().join("scripts/git-credential-secretspec.sh");
    let mut child = Command::new(&script)
        .arg("get")
        // In front of the inherited PATH, never instead of it: on NixOS `env` finds bash
        // only there.
        .env(
            "PATH",
            format!(
                "{}:{}",
                scratch.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("SUZATARY_REPO", repo())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the helper runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("written");
    let out = child.wait_with_output().expect("it ends");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).expect("text")
}

#[test]
fn the_helper_names_the_remotes_user_or_the_generic_one() {
    let scratch = Scratch::new("credential");
    let _ = suzatary_testkit::fake_program(&scratch, "secretspec", "#!/bin/sh\necho t0ken\n");

    let bare = answer(&scratch, "protocol=https\nhost=github.com\n\n");
    assert_eq!(bare, "username=x-access-token\npassword=t0ken\n");
    let named = answer(
        &scratch,
        "protocol=https\nhost=github.com\nusername=ada\n\n",
    );
    assert_eq!(named, "username=ada\npassword=t0ken\n");
}
