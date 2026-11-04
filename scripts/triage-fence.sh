#!/usr/bin/env bash
# scripts/triage-fence.sh <repo>: prints the triage agent's settings JSON (`claude --settings`)
# after proving its hook both denies and allows, or prints nothing and exits 1.
#
# .claude/triage-settings.json is a template; each placeholder is filled with the guard's
# absolute path, the same path probed, so what is checked is what runs (`${CLAUDE_PROJECT_DIR}`
# need not be the directory checked here).
#
# Fail closed: Claude Code blocks only on exit 2, and a missing or crashing hook exits otherwise,
# so the command is `'<guard>' || exit 2`.
#
# Both probes are required: a fence that denies nothing runs the agent uncontained, and one that
# denies everything (ToolSearch included) runs it blind while it narrates a triage it never did.
set -euo pipefail

repo="${1:?usage: triage-fence.sh <repo>}"
template="$repo/.claude/triage-settings.json"
tool_guard="$repo/scripts/tool-guard.sh"
edit_guard="$repo/scripts/edit-guard.sh"

die() { echo "triage-fence: $1 — refusing to run the agent" >&2; exit 1; }

[ -r "$template" ] || die "$template is missing"
for guard in "$tool_guard" "$edit_guard"; do
  [ -x "$guard" ] || die "$guard is missing or not executable"
  case "$guard" in
    /*) ;;
    *) die "$guard is not an absolute path" ;;
  esac
  case "$guard" in
    *[\'\"\\\|#]*) die "$guard holds a quote, a backslash, a bar or a hash, which the hook's command cannot carry" ;;
  esac
done

settings="$(sed \
  -e "s|@TOOL_GUARD@|'$tool_guard' \|\| exit 2|g" \
  -e "s|@EDIT_GUARD@|'$edit_guard' \|\| exit 2|g" \
  "$template")"
case "$settings" in
  *@*_GUARD@*) die "$template holds a placeholder this script does not fill" ;;
esac

# The catch-all hook's command, as the JSON registers it.
hook="$(printf '%s\n' "$settings" \
  | sed -n "s#.*\"command\": *\"\\('[^']*/tool-guard.sh' || exit 2\\)\".*#\\1#p")"
[ "$(printf '%s\n' "$hook" | grep -c .)" -eq 1 ] \
  || die "the settings register the tool guard $(printf '%s\n' "$hook" | grep -c .) times, not once"
[ "$hook" = "'$tool_guard' || exit 2" ] \
  || die "the settings register $hook, not the guard this script checks"

if printf '%s' '{"tool_name":"Bash","tool_input":{"command":"id"}}' | sh -c "$hook" >/dev/null 2>&1; then
  die "the registered hook ALLOWED a Bash call: the fence is broken"
fi
if ! printf '%s' '{"tool_name":"mcp__suzatary__context","tool_input":{}}' | sh -c "$hook" >/dev/null 2>&1; then
  die "the registered hook DENIED its own verbs: the agent would be blind"
fi

printf '%s\n' "$settings"
