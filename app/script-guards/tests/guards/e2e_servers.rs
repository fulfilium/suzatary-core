//! `scripts/e2e-servers.sh`, the browser lane's server supervision, sourced as the lane sources
//! it and run against a stub server: an announced address is read, a port lost before the bind is
//! handed again, any other early end fails the wait at once with the server's words, a ready
//! answer counts only while the server runs, and a server never ready names its last answer.

use super::{Scratch, repo};
use std::process::Command;

/// The stub server, one behaviour per name; it counts its starts in `<name>.starts`.
const STUB: &str = r#"#!/bin/sh
work="$(dirname "$0")"
starts=$(($(cat "$work/$1.starts" 2>/dev/null || echo 0) + 1))
echo "$starts" >"$work/$1.starts"
in_use() { echo "Error: binding 127.0.0.1:41: Address already in use (os error 98)" >&2; exit 1; }
case "$1" in
  announces) echo '{"base":"http://127.0.0.1:41"}'; exec sleep 30 ;;
  taken-once) [ "$starts" -gt 1 ] || in_use; touch "$work/$1.ready"; exec sleep 30 ;;
  taken) in_use ;;
  taken-then-dies) [ "$starts" -gt 1 ] || in_use; echo "another cause" >&2; exit 3 ;;
  dies) echo "its last words" >&2; exit 3 ;;
  loses) echo "it lost the bind" >&2; exit 3 ;;
  never) exec sleep 30 ;;
esac
"#;

/// Sources the supervision, starts the stub as server `$3`, waits on `$4` with `launch` as its
/// restart, at most `$5` seconds when given, and prints the announced address; the trap stops the
/// server. `answered` is another process answering on the port once this server is gone; `refused`
/// probes a port nothing serves.
const DRIVER: &str = r#"set -euo pipefail
work="$1"
. "$2"
[ -z "${5:-}" ] || ready_within=$5
trap 'kill -- "-$(cat "$work/$3.pid")" 2>/dev/null || true' EXIT
ready() { [ -e "$work/$1.ready" ]; }
answered() { ! running "$(cat "$work/$1.pid")"; }
refused() { probe "$1" "http://127.0.0.1:9/"; }
printf '%s\0' "$work/stub" "$3" >"$work/$3.argv"
launch "$3"
up "$3" "$4" launch
announcement "$3"
"#;

/// The stub is another process, so a case it decides keeps the lane's bound, never reached.
#[test]
fn a_server_is_up_on_its_ready_or_its_announcement_and_down_at_its_end() {
    // (server, its readiness, its bound, whether the wait succeeds, its starts, what the lane says)
    let cases = [
        (
            "announces",
            "announced",
            None,
            true,
            1,
            "http://127.0.0.1:41",
        ),
        (
            "taken-once",
            "ready",
            None,
            true,
            2,
            "started again on a fresh one (try 2 of 4)",
        ),
        ("taken", "ready", None, false, 4, "Address already in use"),
        ("taken-then-dies", "ready", None, false, 2, "another cause"),
        ("dies", "ready", None, false, 1, "its last words"),
        ("loses", "answered", None, false, 1, "it lost the bind"),
        (
            "never",
            "refused",
            Some("0"),
            false,
            1,
            "its last probe was answered: ",
        ),
    ];
    for (name, ready, bound, succeeds, starts, says) in cases {
        let scratch = Scratch::new("e2e-servers");
        suzatary_testkit::write_program(&scratch.join("stub"), STUB.as_bytes());
        let out = Command::new("bash")
            .args(["-c", DRIVER, "driver"])
            .arg(&*scratch)
            .arg(repo().join("scripts/e2e-servers.sh"))
            .args([name, ready])
            .args(bound)
            .output()
            .expect("bash runs");
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.success(), succeeds, "{name}: {said}");
        assert!(said.contains(says), "{name} says {says:?}: {said}");
        let ended = said.contains("ended before it was ready");
        assert_eq!(ended, !succeeds && bound.is_none(), "{name}: {said}");
        assert_eq!(
            said.contains("was not ready in"),
            bound.is_some(),
            "{name}: {said}"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.join(format!("{name}.starts")))
                .expect("the stub ran")
                .trim(),
            starts.to_string(),
            "{name}: {said}"
        );
    }
}
