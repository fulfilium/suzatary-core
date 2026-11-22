//! `scripts/e2e-servers.sh`, the browser lane's server supervision, sourced as the lane sources
//! it and run against a stub server: an announced address is read, a port lost before the bind is
//! handed again, any other early end fails the wait within seconds with the server's words, a
//! ready answer counts only while the server runs, and a server never ready names its last answer.

use super::{Scratch, repo};
use std::process::Command;
use std::time::{Duration, Instant};

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
/// restart, and prints the announced address; the trap stops the server. `answered` is another
/// process answering on the port once this server is gone; `refused` probes a port nothing serves.
const DRIVER: &str = r#"set -euo pipefail
work="$1"
. "$2"
ready_within=2
trap 'kill -- "-$(cat "$work/$3.pid")" 2>/dev/null || true' EXIT
ready() { [ -e "$work/$1.ready" ]; }
answered() { ! running "$(cat "$work/$1.pid")"; }
refused() { probe "$1" "http://127.0.0.1:9/"; }
printf '%s\0' "$work/stub" "$3" >"$work/$3.argv"
launch "$3"
up "$3" "$4" launch
announcement "$3"
"#;

#[test]
fn a_server_is_up_on_its_ready_or_its_announcement_and_down_at_its_end() {
    // (server, its readiness, whether the wait succeeds, its starts, what the lane says)
    let cases = [
        ("announces", "announced", true, 1, "http://127.0.0.1:41"),
        (
            "taken-once",
            "ready",
            true,
            2,
            "started again on a fresh one (try 2 of 4)",
        ),
        ("taken", "ready", false, 4, "Address already in use"),
        ("taken-then-dies", "ready", false, 2, "another cause"),
        ("dies", "ready", false, 1, "its last words"),
        ("loses", "answered", false, 1, "ended before it was ready"),
        (
            "never",
            "refused",
            false,
            1,
            "its last probe was answered: ",
        ),
    ];
    for (name, ready, succeeds, starts, says) in cases {
        let scratch = Scratch::new("e2e-servers");
        suzatary_testkit::write_program(&scratch.join("stub"), STUB.as_bytes());
        let started = Instant::now();
        let out = Command::new("bash")
            .args(["-c", DRIVER, "driver"])
            .arg(&*scratch)
            .arg(repo().join("scripts/e2e-servers.sh"))
            .args([name, ready])
            .output()
            .expect("bash runs");
        let took = started.elapsed();
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.success(), succeeds, "{name}: {said}");
        assert!(said.contains(says), "{name} says {says:?}: {said}");
        assert_eq!(
            std::fs::read_to_string(scratch.join(format!("{name}.starts")))
                .expect("the stub ran")
                .trim(),
            starts.to_string(),
            "{name}: {said}"
        );
        assert!(
            took < Duration::from_secs(60),
            "{name} took {took:?}: {said}"
        );
    }
}
