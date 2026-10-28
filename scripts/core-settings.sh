#!/usr/bin/env bash
# THE SETTINGS OF CORE THAT LET IT RUN THE GATE, set once by the operator at a terminal
# (docs/OPERATOR.md). Core is the public repository the train publishes to and runs the tree half
# in (`scripts/train.sh gate --remote github`, .github/workflows/gate.yml); the train never runs
# this, and nothing here is read by a gate.
#
#   scripts/core-settings.sh [<owner>/<repo>]   (default: the remote `core` of this checkout)
#
# Each setting is idempotent, so running it again sets the same values:
#   - Actions on, running only GitHub's own actions, `cachix/install-nix-action` and
#     `nix-community/cache-nix-action`, which the workflows below pin;
#   - gate.yml, gate-store.yml and publication.yml on, and every other workflow off: the public
#     tree carries this repository's other workflows, which are for the private repository
#     (quick.yml would run on every publication, update-flake-lock.yml on its schedule);
#   - the token a run is handed reads the repository and writes nothing, and cannot approve a
#     pull request;
#   - a run a pull request from any outside contributor starts waits for an approval (gate.yml
#     names no pull request event, so this covers a workflow added later);
#   - a ruleset under which only a repository administrator may move master, so no workflow
#     (whose token is never an administrator) can move it, though a run may ask for
#     `contents: write`: a workflow a gate branch carries could otherwise move master and so run
#     code of its own in publication.yml, beside the cache's token;
#   - the environment `cachix`, which only a run on master may enter, and in it the secret
#     CACHIX_AUTH_TOKEN, the bmabsout cache's push token, which publication.yml alone names, read
#     from secretspec and handed to `gh secret set` on its stdin, so it is in no argument, file or
#     line printed; a repository-wide copy of it is deleted.
#
# It needs `gh` logged in as an administrator of core, and secretspec holding CACHIX_AUTH_TOKEN.
set -euo pipefail

die() {
  printf 'core-settings: %s\n' "$*" >&2
  exit 1
}
[[ -t 0 ]] || die "the operator runs this at a terminal, once; it is never run by a script."
cd "$(dirname "${BASH_SOURCE[0]}")/.."

if (($#)); then
  repo="$1"
else
  url="$(git config --get remote.core.url)" || die "this checkout has no remote \`core\`; name the repository: scripts/core-settings.sh <owner>/<repo>"
  [[ "$url" =~ github\.com[:/]([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)$ ]] || die "core ($url) is not a GitHub repository"
  repo="${BASH_REMATCH[1]%.git}"
fi
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die "\"$repo\" is not <owner>/<repo>"
[[ "$(gh repo view "$repo" --json visibility -q .visibility)" == PUBLIC ]] \
  || die "$repo is not public, so its runners are not free: nothing set"

echo "core-settings: Actions on in $repo, GitHub's actions and the two Nix actions alone"
gh api --silent -X PUT "repos/$repo/actions/permissions" -F enabled=true -f allowed_actions=selected
gh api --silent -X PUT "repos/$repo/actions/permissions/selected-actions" \
  -F github_owned_allowed=true -F verified_allowed=false \
  -f 'patterns_allowed[]=cachix/install-nix-action@*' \
  -f 'patterns_allowed[]=nix-community/cache-nix-action/restore@*' \
  -f 'patterns_allowed[]=nix-community/cache-nix-action/save@*'

echo "core-settings: the runs' token reads only"
gh api --silent -X PUT "repos/$repo/actions/permissions/workflow" \
  -f default_workflow_permissions=read -F can_approve_pull_request_reviews=false

echo "core-settings: an outside contributor's run waits for an approval"
gh api --silent -X PUT "repos/$repo/actions/permissions/fork-pr-contributor-approval" \
  -f approval_policy=all_external_contributors

echo "core-settings: gate.yml, gate-store.yml and publication.yml on, every other workflow off"
while IFS=$'\t' read -r id path state; do
  case "$path" in
    .github/workflows/gate.yml | .github/workflows/gate-store.yml | .github/workflows/publication.yml)
      [[ "$state" != active ]] || continue
      gh api --silent -X PUT "repos/$repo/actions/workflows/$id/enable"
      echo "core-settings:   $path on" ;;
    *)
      [[ "$state" == active ]] || continue
      gh api --silent -X PUT "repos/$repo/actions/workflows/$id/disable"
      echo "core-settings:   $path off" ;;
  esac
done < <(gh api --paginate "repos/$repo/actions/workflows" --jq '.workflows[] | "\(.id)\t\(.path)\t\(.state)"')

# Only an administrator moves master: the train's publish pushes as the owner's login, which is
# one, and a run's token is never one. The rule sits beside core's fast-forward ruleset, which
# this leaves as it is.
update_rules="master-updates-admin-only"
echo "core-settings: the ruleset $update_rules, under which only an administrator may move master"
ruleset="$(cat <<JSON
{"name": "$update_rules", "target": "branch", "enforcement": "active",
 "conditions": {"ref_name": {"include": ["refs/heads/master"], "exclude": []}},
 "rules": [{"type": "update"}],
 "bypass_actors": [{"actor_type": "RepositoryRole", "actor_id": 5, "bypass_mode": "always"}]}
JSON
)"
id="$(gh api "repos/$repo/rulesets" --jq ".[] | select(.name == \"$update_rules\") | .id")"
if [[ -n "$id" ]]; then
  gh api --silent -X PUT "repos/$repo/rulesets/$id" --input - <<<"$ruleset"
else
  gh api --silent -X POST "repos/$repo/rulesets" --input - <<<"$ruleset"
fi

# The token is the environment's, not the repository's: a gate branch runs the gate.yml of its
# own commit, so a repository secret is one any workflow file a pushed commit carries can name.
echo "core-settings: the environment cachix, which a run on master alone enters"
gh api --silent -X PUT "repos/$repo/environments/cachix" --input - <<'JSON'
{"deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
master=""
while IFS=$'\t' read -r id type name; do
  if [[ "$type" == branch && "$name" == master ]]; then
    master="$id"
  else
    gh api --silent -X DELETE "repos/$repo/environments/cachix/deployment-branch-policies/$id"
    echo "core-settings:   the $type $name may no longer enter it"
  fi
done < <(gh api "repos/$repo/environments/cachix/deployment-branch-policies" \
  --jq '.branch_policies[] | "\(.id)\t\(.type // "branch")\t\(.name)"')
[[ -n "$master" ]] ||
  gh api --silent -X POST "repos/$repo/environments/cachix/deployment-branch-policies" -f name=master -f type=branch

echo "core-settings: the secret CACHIX_AUTH_TOKEN, from secretspec, in the environment cachix"
token="$(secretspec get CACHIX_AUTH_TOKEN --file secretspec.toml --reason "core-settings: the publication's Cachix push token")" \
  || die "secretspec has no CACHIX_AUTH_TOKEN: secretspec set CACHIX_AUTH_TOKEN, then run this again"
[[ -n "$token" ]] || die "secretspec's CACHIX_AUTH_TOKEN is empty: nothing set"
printf '%s' "$token" | gh secret set CACHIX_AUTH_TOKEN --env cachix --repo "$repo"
if gh secret list --app actions --repo "$repo" --json name --jq '.[].name' | grep -qx CACHIX_AUTH_TOKEN; then
  gh secret delete CACHIX_AUTH_TOKEN --app actions --repo "$repo"
  echo "core-settings:   the repository's own copy deleted"
fi
echo "core-settings: done"
