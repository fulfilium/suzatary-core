# shellcheck shell=bash
# What every web step reads, prepared once: the wasm modules, the static checks and the wire's
# TypeScript. Sourced with `$root` the checkout; exports the modules, and prepares nothing when
# this checkout is already prepared. A module the caller passed is used as it is.
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
