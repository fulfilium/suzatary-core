#!/usr/bin/env bash
# PreToolUse hook for the unattended triage session: default deny.
#
# `permissions.allow` is not an exclusive allowlist: under `dontAsk`, a tool neither allowed nor
# denied is still callable (`Monitor` ran a shell). A deny list of names cannot be finished, so
# this permits exactly one namespace and denies everything else, including tools that do not
# exist yet.
#
# Exit 0 allow, 2 block (stderr goes back to the agent). A parse failure denies.
set -uo pipefail

ALLOWED_PREFIX="mcp__suzatary__"

# Never deny ToolSearch: the suzatary verbs are deferred tools whose schemas load through it.
# It only fetches a schema; any resulting call comes back through this hook.
ALLOWED_EXACT="ToolSearch"

IN=$(cat)

# Every occurrence, not the first: `tool_input` carries email bodies and can forge a
# `"tool_name"` key. Requiring all matches in the namespace means a forgery can only turn an
# allow into a deny.
NAMES=$(printf '%s' "$IN" | grep -o '"tool_name"[[:space:]]*:[[:space:]]*"[^"]*"' \
  | sed 's/.*:[[:space:]]*"//; s/"$//')

if [ -z "$NAMES" ]; then
  echo "tool-guard: could not parse tool_name from the tool call — denying by default" >&2
  exit 2
fi

while IFS= read -r NAME; do
  [ -n "$NAME" ] || continue
  case "$NAME" in
    "$ALLOWED_PREFIX"*) ;;
    "$ALLOWED_EXACT") ;;
    *)
      echo "tool-guard: '$NAME' is not a suzatary verb — denied." >&2
      echo "tool-guard: this session may only call ${ALLOWED_PREFIX}* tools. Reading files," >&2
      echo "tool-guard: running commands and reaching the network are not available to you;" >&2
      echo "tool-guard: use the suzatary verbs instead (start with ${ALLOWED_PREFIX}context)." >&2
      exit 2
      ;;
  esac
done <<EOF
$NAMES
EOF

exit 0
