# shellcheck shell=bash
# The triage agent's `claude -p` credential, sourced by scripts/hourly-triage.sh: resolved
# through secretspec into the environment so it never sits in a file the agent can read.
# Returns 1 when it is not set.
claude_auth() {
  local token
  token=$(secretspec get CLAUDE_CODE_OAUTH_TOKEN --reason "hourly triage: headless claude auth" 2>/dev/null || true)
  if [ -n "$token" ]; then
    export CLAUDE_CODE_OAUTH_TOKEN="$token"
    return 0
  fi
  return 1
}
