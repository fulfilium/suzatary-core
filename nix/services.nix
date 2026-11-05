# What a Suzatary service IS, independent of how it gets started.
#
# This file says what a service needs and builds its executable; nix/nixos-module.nix says when
# and under whom it runs. The `roots` service reads `$SUZATARY_USER_REPO/flake.lock`, builds the
# tree it names and links it at `$SUZATARY_ROOTS/app`; every other wrapper execs its script from
# there, read-only.
{ pkgs }:

let
  inherit (pkgs) lib;

  # Everything the wrapper scripts shell out to; `nix` re-enters this repo's pinned flake.
  runtimeDeps = with pkgs; [
    bash
    coreutils     # df, date, cat, cp, rm, mktemp, sleep
    gnugrep
    gawk          # the COUNTS parse in hourly-triage.sh
    gnused
    gnutar gzip   # vault-backup.sh: tar -z shells out to gzip — the maiden timer run died on tar, the second on gzip (2026-08-30)
    git           # safe-push.sh
    nix
    util-linux    # flock, for safe-push.sh's push serialization
    claude-code   # the whole point: version-tracked and GC-rooted
    openssh       # git push over ssh, if the remote ever moves off https
    # The git push in safe-push.sh authenticates through
    # scripts/git-credential-secretspec.sh, which needs all three: secretspec to
    # resolve the declared secret, pass+gnupg because `pass` is the provider.
    # The unattended agent cannot reach any of them — it has no Bash — but the
    # trusted wrapper that pushes on its behalf must.
    secretspec
    pass
    gnupg
  ];

  # A nix evaluation before DNS is up does not fail: it builds from source. So a service that
  # evaluates the flake at run time waits for the substituter first.
  waitForNetwork = pkgs.writeShellScript "suzatary-wait-for-substituter" ''
    for _ in $(seq 1 60); do
      ${pkgs.curl}/bin/curl -sf --max-time 5 -o /dev/null \
        https://cache.nixos.org/nix-cache-info && exit 0
      sleep 5
    done
    # Five minutes with no substituter is an outage, not a slow boot. Start
    # anyway: a degraded start visible in the journal beats never coming up.
    echo "suzatary: no substituter reachable after 5min, starting anyway" >&2
  '';

  # The units' root links: the module sets SUZATARY_ROOTS; a shell without it
  # uses `roots-dev`, as scripts/app-bin.sh does.
  rootsDir = ''
    roots="''${SUZATARY_ROOTS:-''${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
  '';

  # The app the roots refresh last built, ROOTED FOR THIS UNIT under its own
  # name (scripts/root-serving.sh): the shared link moves on the next deploy and
  # the daemon collects what nothing roots, while a long-running unit reads its
  # files from this tree for as long as it runs.
  resolveApp = name: ''
    ${rootsDir}
    app="$(readlink -f "$roots/app" || true)"
    if [ -z "$app" ] || [ ! -e "$app/flake.nix" ]; then
      echo "suzatary: no app linked at $roots/app." >&2
      echo "  The roots refresh links it from the user repository's flake.lock:" >&2
      echo "  systemctl --user start suzatary-roots (or nix run .#roots)." >&2
      exit 75
    fi
    "$app/scripts/root-serving.sh" "${name}-app" "$app"
    export SUZATARY_REPO="$app"
    if [ -z "''${SUZATARY_INSTANCE:-}" ] || [ ! -f "$SUZATARY_INSTANCE" ]; then
      echo "suzatary: no instance file at ''${SUZATARY_INSTANCE:-(SUZATARY_INSTANCE unset)}" >&2
      echo "  Set SUZATARY_INSTANCE=/path/to/instance.toml (the user repository's)." >&2
      exit 64
    fi
    export SUZATARY_INSTANCE
  '';

  # A service that runs one of the app's scripts with the environment it needs.
  # `runsTheApp` marks it for nix/module-check.nix, which finds a unit that
  # runs one by its ExecStart.
  appService = { name, script, waitForNet ? true }:
    pkgs.writeShellScriptBin name ''
      set -euo pipefail
      export PATH="${lib.makeBinPath runtimeDeps}:''${PATH:-}"
      ${resolveApp name}
      ${lib.optionalString waitForNet "${waitForNetwork}"}
      exec "$app/${script}" "$@"
    '' // { runsTheApp = true; };
in
rec {
  inherit runtimeDeps waitForNetwork;

  # THE ONE READER OF THE LOCK, started by the module's path unit when
  # flake.lock changes and by scripts/deploy.sh. It reads only a COMMITTED lock
  # (the working tree's lock equal to HEAD's), so a lock written and not
  # committed builds nothing; it builds only a revision of the branch this box
  # follows (scripts/followed.sh, deploy's own check); it roots the tree before
  # building it; it refuses
  # a tree without the lock interface (nix/app-input.nix), whose scripts would
  # write under their own read-only code root; it builds, under names only it
  # moves (`lock-`), the app's builds (scripts/app-builds.sh, which
  # scripts/deploy.sh ran on the same tree before committing the lock, so
  # this run normally finds them all in the store); and it moves
  # `$SUZATARY_ROOTS/app` LAST, so a failed run leaves every unit on the app
  # it ran. The shell profiles keep their current generation only.
  rootsScript = ''
    set -euo pipefail
    export PATH="${lib.makeBinPath runtimeDeps}:''${PATH:-}"
    ${rootsDir}
    user="''${SUZATARY_USER_REPO:?set SUZATARY_USER_REPO to the user repository}"
    lock="$user/flake.lock"
    if [ "$(git -C "$user" show HEAD:flake.lock)" != "$(cat "$lock")" ]; then
      echo "suzatary-roots: $lock is not the committed lock; only a commit is a version (scripts/deploy.sh makes one). Nothing built." >&2
      exit 1
    fi
    # The revision went through the gate and the operator chose it: one a
    # deploy here pinned from the branch this box follows (or the pin before,
    # a revert's way back), on that branch's first-parent chain as the deploy
    # fetched it, from the remote it follows, whoever committed the lock
    # (scripts/followed.sh).
    locked() { nix eval --raw --file ${./app-input.nix} --argstr lock "$lock" "$1"; }
    bash ${../scripts/followed.sh} admit "$(git -C "$user" rev-parse --absolute-git-dir)/suzatary-followed.git" \
      "$(locked lockedUrl)" "$(locked lockedRef)" "$(locked revision)" || {
      echo "suzatary-roots: $lock names a revision this box does not follow; only scripts/deploy.sh moves it. Nothing built." >&2
      exit 1
    }
    ${waitForNetwork}
    app="$(locked source.outPath)"
    echo "suzatary-roots: $lock names $app" >&2
    bash ${../scripts/app-builds.sh} "$app" "$roots/lock-"
    [ ! -e "$roots/triage-profile" ] || nix profile wipe-history --profile "$roots/triage-profile"
    nix-store --add-root "$roots/app" --indirect -r "$app" >/dev/null
    echo "suzatary-roots: linked $app" >&2
  '';
  roots = pkgs.writeShellScriptBin "suzatary-roots" rootsScript;

  triage = appService {
    name = "suzatary-triage";
    script = "scripts/hourly-triage.sh";
  };

  vault = appService {
    name = "suzatary-vault";
    script = "scripts/vault-backup.sh";
  };

  # IMAP IDLE: one connection per Microsoft account, triggering the SAME
  # `triage` unit above on a debounced wake rather than running the sync
  # itself — the 5-minute timer stays the fallback, this only lowers the
  # latency between a real mail arriving and the pipeline seeing it. A
  # long-running watcher, not a periodic job, so `waitForNet` matters the same
  # way `web`'s does: the wrapper re-enters `nix develop .#triage` at start.
  mailIdle = appService {
    name = "suzatary-mail-idle";
    script = "scripts/mail-idle.sh";
    waitForNet = true;
  };

  # The read-only web app over the chain (`suzatary serve`).
  # A long-running server, not a periodic job — the NixOS module runs it as a
  # Type=simple unit with no timer. waitForNet is on for the usual reason: the
  # script re-enters `nix develop .#triage` at start, and an evaluation that
  # begins before DNS concludes the substituter is unreachable and starts
  # building from source (2026-08-10).
  plugins = appService {
    name = "suzatary-plugins";
    script = "scripts/plugin-sync.sh";
  };

  web = appService {
    name = "suzatary-web";
    script = "scripts/serve-web.sh";
    waitForNet = true;
  };

  # Bridge runs the .#triage shell's own package, by store path, so a start evaluates nothing.
  #
  # pass/gnupg stay on PATH: Bridge shells out to `pass` for its keyring, since
  # `keyring` needs a DBus secret-service and dies headless.
  protonBridge = pkgs.writeShellScriptBin "suzatary-proton-bridge" ''
    set -euo pipefail
    export PATH="${lib.makeBinPath [ pkgs.pass pkgs.gnupg ]}:''${PATH:-}"
    exec ${pkgs.protonmail-bridge}/bin/protonmail-bridge --noninteractive "$@"
  '';
}
