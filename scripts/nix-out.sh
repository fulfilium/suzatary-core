# shellcheck shell=bash
# nix_out <output>: builds the flake output of `$root`, rooted under its gate-roots so a store
# collection keeps it, and prints its path.
nix_out() {
  local roots
  roots="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)/gate-roots" &&
    mkdir -p "$roots" &&
    nix build "$root#$1" --out-link "$roots/$1" --print-out-paths
}

# nix_held <output>: as nix_out when the store already holds the output, else fails; builds nothing.
nix_held() {
  local roots out
  roots="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)/gate-roots" &&
    out="$(nix eval --raw "$root#$1.outPath")" && [[ -e "$out" ]] &&
    mkdir -p "$roots" && nix-store --add-root "$roots/$1" --realise "$out" >/dev/null && echo "$out"
}

# nix_outs <output>...: builds the outputs in one `nix build`, in parallel; root each with nix_out.
nix_outs() {
  local outputs=("${@/#/$root#}")
  nix build --no-link "${outputs[@]}"
}

# root_environment <dir>: roots under <dir> (replacing it) every store path the environment
# names. `nix develop` roots nothing, so a collection can take the gate's own tools mid-run.
root_environment() {
  local dir="$1" paths
  mapfile -t paths < <(env | grep -o '/nix/store/[0-9a-z]\{32\}-[^/:"[:space:]]*' | sort -u |
    while read -r path; do [[ ! -e "$path" ]] || echo "$path"; done)
  rm -rf "$dir" && mkdir -p "$dir" &&
    { ((${#paths[@]} == 0)) || nix-store --add-root "$dir/path" --realise "${paths[@]}" >/dev/null; }
}

# missed <build argument>...: where the caller fetches alone (its `fetched` set, by --cached) and a
# `nix build --max-jobs 0` of those arguments failed, exits 75 (no verdict) when no binary cache
# holds what they name, since the tree is not judged by what a cache lacks; else returns 1, the
# build's own red.
missed() {
  local said
  ((${#fetched[@]})) || return 1
  said="$(nix build "$@" --dry-run 2>&1)" || return 1
  [[ "$said" == *'will be built'* ]] || return 1
  echo "nix: no binary cache holds $*, and this machine builds nothing for a gate on GitHub's runners: no verdict. Let the commit's publication run fill the cache (gh workflow run publication.yml), then gate again." >&2
  exit 75
}
