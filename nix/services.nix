# What a Suzatary service needs and its executable; nix/nixos-module.nix says when and under
# whom it runs. Every wrapper but `roots` execs its script, read-only, from the tree `roots`
# links at `$SUZATARY_ROOTS/app`.
{ pkgs }:

let
  inherit (pkgs) lib;

  # Every program door 2 runs, by the name its scripts call it, so the hourly pass can push a
  # public store's clone; app/cli/tests/guards/door_two.rs holds this set to the scripts.
  doorTwo = with pkgs; {
    awk = gawk;
    basename = coreutils;
    cat = coreutils;
    comm = coreutils;
    cut = coreutils;
    dirname = coreutils;
    find = findutils;
    flock = util-linux;
    git = git;
    gitleaks = gitleaks;
    grep = gnugrep;
    head = coreutils;
    ls = coreutils;
    mkdir = coreutils;
    mktemp = coreutils;
    mv = coreutils;
    nix = nix;
    nix-store = nix;
    paste = coreutils;
    realpath = coreutils;
    rm = coreutils;
    sed = gnused;
    sha256sum = coreutils;
    sort = coreutils;
    stat = coreutils;
    tar = gnutar;
    tr = coreutils;
    wc = coreutils;
  };

  # Everything the wrapper scripts shell out to; `nix` re-enters this repo's pinned flake.
  runtimeDeps = lib.unique (lib.attrValues doorTwo ++ (with pkgs; [
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
    # safe-push.sh's credential helper needs secretspec and its provider, pass+gnupg;
    # the agent, with no Bash, reaches none of them.
    secretspec
    pass
    gnupg
  ]));

  # A nix evaluation before DNS is up does not fail: it builds from source. So a service that
  # evaluates the flake at run time waits for the substituter first.
  waitForNetwork = pkgs.writeShellScript "suzatary-wait-for-substituter" ''
    for _ in $(seq 1 60); do
      ${pkgs.curl}/bin/curl -sf --max-time 5 -o /dev/null \
        https://cache.nixos.org/nix-cache-info && exit 0
      sleep 5
    done
    # Five minutes is an outage, not a slow boot: start, visibly, rather than never.
    echo "suzatary: no substituter reachable after 5min, starting anyway" >&2
  '';

  # The units' root links: the module sets SUZATARY_ROOTS; a shell without it
  # uses `roots-dev`, as scripts/app-bin.sh does.
  rootsDir = ''
    roots="''${SUZATARY_ROOTS:-''${XDG_STATE_HOME:-$HOME/.local/state}/suzatary/roots-dev}"
  '';

  # ROOTED FOR THIS UNIT under its own name (scripts/root-serving.sh): the shared link moves on
  # the next deploy and the daemon collects what nothing roots, while the unit still reads it.
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

  # `runsTheApp` marks it for nix/module-check.nix, which finds a unit that runs one by its ExecStart.
  appService = { name, script, args ? [ ], waitForNet ? true }:
    pkgs.writeShellScriptBin name ''
      set -euo pipefail
      export PATH="${lib.makeBinPath runtimeDeps}:''${PATH:-}"
      ${resolveApp name}
      ${lib.optionalString waitForNet "${waitForNetwork}"}
      exec "$app/${script}" ${lib.escapeShellArgs args} "$@"
    '' // { runsTheApp = true; };
in
rec {
  inherit doorTwo runtimeDeps waitForNetwork;

  # THE ONE READER OF THE LOCK. It builds only a COMMITTED lock naming a revision of the branch
  # this box follows; it roots the tree before building it; it refuses a tree without
  # nix/app-input.nix, whose scripts would write under their own read-only code root; it builds
  # under names only it moves (`lock-`); and it moves `$SUZATARY_ROOTS/app` LAST, so a failed
  # run leaves every unit on the app it ran. It runs the app's `scripts/app-builds.sh`, which
  # `scripts/deploy.sh` ran on the same tree, and links `app` only after every build succeeds.
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
    # Only what went through the gate and the operator chose: a revision a deploy here pinned from
    # the followed branch's first-parent chain (or the pin before), whoever committed the lock.
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

  # IMAP IDLE per Microsoft account: it wakes the `triage` unit early, debounced, and never
  # syncs itself; the 5-minute timer stays the fallback.
  mailIdle = appService {
    name = "suzatary-mail-idle";
    script = "scripts/mail-idle.sh";
    waitForNet = true;
  };

  plugins = appService {
    name = "suzatary-plugins";
    script = "scripts/plugin-sync.sh";
  };

  # Waits for the network: the timer's catch-up after a boot would otherwise run first, and a
  # refused fetch is recorded where every reading of the list shows it until the next one.
  follows = appService {
    name = "suzatary-follows";
    script = "scripts/app-bin.sh";
    args = [ "--deployed" "follow" "fetch" ];
  };

  web = appService {
    name = "suzatary-web";
    script = "scripts/serve-web.sh";
    waitForNet = true;
  };

  # Bridge runs the .#triage shell's own package, by store path, so a start evaluates nothing.
  # pass/gnupg stay on PATH: Bridge shells out to `pass` for its keyring, since
  # `keyring` needs a DBus secret-service and dies headless.
  protonBridge = pkgs.writeShellScriptBin "suzatary-proton-bridge" ''
    set -euo pipefail
    export PATH="${lib.makeBinPath [ pkgs.pass pkgs.gnupg ]}:''${PATH:-}"
    exec ${pkgs.protonmail-bridge}/bin/protonmail-bridge --noninteractive "$@"
  '';
}
