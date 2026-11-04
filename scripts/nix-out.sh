# shellcheck shell=bash
# nix_out <output>: builds the flake output of `$root`, rooted under its gate-roots so a store
# collection keeps it, and prints its path.
nix_out() {
  local roots
  roots="$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)/gate-roots" &&
    mkdir -p "$roots" &&
    nix build "$root#$1" --out-link "$roots/$1" --print-out-paths
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
