#!/usr/bin/env bash
# PreToolUse hook for the unattended triage session: denies every Edit or Write.
# Edit(<glob>) argument scoping is not enforced by the permission system. A backstop
# behind scripts/tool-guard.sh, in case a settings file quietly reverts.
#
# Registered in .claude/triage-settings.json, never a project settings.json, which
# would stop every human session writing files.
#
# Hooks contract: 0 = allow, 2 = block (stderr shown to the agent). It never answers 0.
set -uo pipefail

cat >/dev/null
echo "edit-guard: the triage agent writes no file; todos go through todo_add/todo_update, state.json through mark." >&2
exit 2
