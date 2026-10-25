# shellcheck shell=bash
# The credential `claude -p` runs the triage agent under, for scripts/hourly-triage.sh to
# source: the year-long token `claude setup-token` mints, resolved through secretspec so it
# never sits in a file the agent can read. claude reads it from its environment, and
# `suzatary init` prints the step that sets it. Returns 1 when it is not set.
claude_auth() {
  local token
  token=$(secretspec get CLAUDE_CODE_OAUTH_TOKEN --reason "hourly triage: headless claude auth" 2>/dev/null || true)
  if [ -n "$token" ]; then
    export CLAUDE_CODE_OAUTH_TOKEN="$token"
    return 0
  fi
  return 1
}
