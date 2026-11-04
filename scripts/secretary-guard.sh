#!/usr/bin/env bash
# PreToolUse hook for the secretary's model subprocess: deny everything, including tools that do
# not exist yet. Its verbs run through `suzatary_core::secretary::converse` from the JSON
# envelope it prints, so it needs no tools.
#
# ToolSearch stays out of every `.claude/` deny list (app/cli/tests/guards.rs checks), since the
# triage agent's verbs arrive through it; this hook refuses it anyway.
#
# Exit 2 = block (stderr goes back to the model). This never exits 0.
set -uo pipefail

# No parser: a guard with no branch cannot be talked past by a payload.
cat >/dev/null

echo "secretary-guard: this session holds no tools at all." >&2
echo "secretary-guard: your verbs are not tools here — ask for one by printing" >&2
echo "secretary-guard: {\"say\": \"...\", \"calls\": [{\"verb\": \"...\", \"args\": {...}}]}" >&2
echo "secretary-guard: and the box will run it and hand you the answer." >&2
exit 2
