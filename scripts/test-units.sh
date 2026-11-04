#!/usr/bin/env bash
# Cargo's test runner when the merge gate runs only the members in SUZATARY_GATE_UNITS: a member
# not named exits 0 without running; an executable cargo names no member for runs.
#
#   cargo test --workspace --config "target.<host>.runner=['scripts/test-units.sh']"
set -euo pipefail
if [[ -z "${CARGO_PKG_NAME:-}" || " ${SUZATARY_GATE_UNITS:-} " == *" $CARGO_PKG_NAME "* ]]; then
  exec "$@"
fi
echo "test-units: not run: the inputs of $CARGO_PKG_NAME's tests did not move since they passed"
