# shellcheck shell=bash
# What every web step reads, prepared once: the browser's two modules, the two static checks
# (scripts/web-static.sh) and the wire's TypeScript (scripts/wire.sh). Sourced, with `$root` the
# checkout, so the modules it resolves are exported to the caller and its children; a step under
# a caller that already prepared this checkout (the merge gate's `web-inputs`) prepares nothing.
#
# A module a caller passed is used as it is (the `suzatary-web` derivation passes all three);
# otherwise it is built here and rooted (scripts/nix-out.sh), and a build that fails fails this.
# The shell carries tools, not builds.
[[ "${SUZATARY_WEB_INPUTS:-}" != "$root" ]] || return 0

# shellcheck source=scripts/nix-out.sh
. "$root/scripts/nix-out.sh"

web_module() {
  local var="$1" out="$2" built
  if [[ -z "${!var:-}" ]]; then
    echo "==> resolving $out (nix build .#$out; no $var supplied)"
    built="$(nix_out "$out")" || return 1
    printf -v "$var" '%s' "$built"
  fi
  export "${var?}"
}

bash "$root/scripts/web-static.sh" &&
  web_module SUZATARY_PRODROME_WASM suzatary-wasm &&
  web_module SUZATARY_VIEWS_WASM suzatary-views-wasm &&
  web_module SUZATARY_TYPST_WASM suzatary-typst-wasm &&
  bash "$root/scripts/wire.sh" &&
  export SUZATARY_WEB_INPUTS="$root"
