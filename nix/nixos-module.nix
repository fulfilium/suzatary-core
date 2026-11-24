# Suzatary's background jobs, declared.
#
# ⚠️ `~/.config/systemd/user/` takes precedence over the `/etc/systemd/user/` units written here,
# and a rebuild reporting success is no evidence a unit changed. Check what runs:
#
#   systemctl --user show -p FragmentPath suzatary-triage.timer
#
# It must be under /etc/systemd/user.
#
# The units run the app the user repository's flake.lock names (`userRepo`); a deploy is a
# commit that moves the lock (scripts/deploy.sh).
{ config, lib, pkgs, ... }:

let
  cfg = config.services.suzatary;

  services = import ./services.nix { inherit pkgs; };
  inherit (services) runtimeDeps;

  # ConditionUser: NixOS renders systemd.user.* into every user's manager, and root autologs in
  # on the console, where a second Proton Bridge would fight over vault.enc. The substituter gate
  # lives in the wrappers (nix/services.nix): `network-online.target` does not exist in a user
  # manager.
  baseUnit = description: serviceConfig: {
    inherit description;
    path = runtimeDeps;
    unitConfig.ConditionUser = cfg.user;
    # SUZATARY_REPO is each wrapper's own: the app's store path (nix/services.nix).
    environment.SUZATARY_USER_REPO = cfg.userRepo;
    environment.SUZATARY_INSTANCE = "${cfg.userRepo}/instance.toml";
    # The units' root links; unset, scripts link into `roots-dev`, so a dev build never repoints them.
    environment.SUZATARY_ROOTS = cfg.rootsDir;
    # The junk sweep's churn seed; unset when the option is null, which the binary reads as no seed.
    environment.SUZATARY_JUNK_SEED =
      if cfg.junkSeed == null then null else "${cfg.junkSeed}/domains.txt";
    # These jobs write what quotes mail: files 0600 whatever a script forgets, and a /tmp of their
    # own.
    serviceConfig = { PrivateTmp = true; UMask = "0077"; } // serviceConfig;
  };

  # A long-running unit runs the app it started with, so it is PartOf the target scripts/deploy.sh
  # try-restarts; nix/module-check.nix fails on one that is not.
  servingUnit = description: serviceConfig: baseUnit description serviceConfig // {
    partOf = [ "suzatary-app.target" ];
    wantedBy = [ "default.target" ];
  };
in
{
  options.services.suzatary = {
    enable = lib.mkEnableOption "the Suzatary mail-triage and daily-notes background jobs";

    user = lib.mkOption {
      type = lib.types.str;
      description = ''
        The single user whose systemd manager runs these jobs. This is not a
        multi-user system: the mail store, the notmuch DB, the Proton Bridge
        vault, the pass/GPG keyring and the Claude credentials all live in one
        home directory and are meaningless outside it.
      '';
    };

    userRepo = lib.mkOption {
      type = lib.types.pathWith { inStore = false; absolute = true; };
      # A path literal stays a path: never copied into the store.
      apply = toString;
      example = "/home/ada/suzatary-data";
      description = ''
        The checkout of the USER REPOSITORY (docs/USERREPO-PLAN-2026-10-01.md):
        a mutable working tree of its own git repository holding
        `instance.toml` and everything it names (the stores, `state.json`,
        the briefings), which the jobs commit and push, its `flake.nix`,
        whose `suzatary` input declares the app's publication (suzatary-core's
        master), and `flake.lock`, whose commit of that input is the app every
        unit runs. The roots refresh
        fires when that lock changes; `scripts/deploy.sh` is what changes it.
      '';
    };

    rootsDir = lib.mkOption {
      type = lib.types.str;
      default = "${config.users.users.${cfg.user}.home}/.local/state/suzatary/roots";
      defaultText = lib.literalExpression
        ''"''${config.users.users.''${config.services.suzatary.user}.home}/.local/state/suzatary/roots"'';
      description = ''
        Where the units' GC-rooted links live (`app`, `app-bin`, `web-dist` and
        each long-running unit's serving roots), set as `SUZATARY_ROOTS` for
        every unit. A shell without it links into `roots-dev` beside it, so
        a dev build cannot repoint what the units run.
      '';
    };

    junkSeed = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        The `suzatary-junk-seed` derivation's store path — the consuming
        flake's `self.packages.<system>.suzatary-junk-seed` (this module has
        no `self` of its own: every flake output it needs arrives as a plain
        option the consumer sets). When set, every unit's `SUZATARY_JUNK_SEED` points at its
        `domains.txt`, so a Junk digest's churn list is a maintained public
        seed unioned with this deployment's own `[mail.junk] churn`, minus
        `[mail.junk] keep`. Null (the default: a dev shell, or a host that
        has not wired this yet) leaves the variable unset, which the binary
        reads as no seed — a real state, not an error.
      '';
    };

    web.tlsDomain = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        This box's MagicDNS name (e.g. `<your-box>.<tailnet>.ts.net`). When set,
        a root unit mints and weekly renews a Let's Encrypt certificate for it
        through `tailscale cert` into `web.tlsDir`, and the web app serves
        https. Null mints nothing, and the web app refuses to start until a
        certificate is in `web.tlsDir`: it never serves plain http on a
        tailnet address. The tailnet must have HTTPS certificates enabled in
        its admin console.
      '';
    };

    web.port = lib.mkOption {
      type = lib.types.port;
      default = 443;
      description = ''
        The web app's port. 443 by default since 2026-09-05: Android verifies
        an app's claim to a site's links only over https on 443, and a URL
        without a port is what a person types. The user unit can bind it
        because the host sets `net.ipv4.ip_unprivileged_port_start` to 443
        (see `web.unprivilegedPorts`).
      '';
    };

    web.unprivilegedPorts = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = ''
        Let unprivileged processes bind ports from 443 up (a sysctl), so the
        user unit serves 443 without capabilities a user manager cannot grant.
        On a single-person box that is the whole threat model; a host that
        objects sets this false and `web.port` above 1024.
      '';
    };

    web.tlsDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/suzatary/tls";
      description = "Where cert.pem and key.pem live; owned by `user`, key mode 600.";
    };

    protonBridge.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Run Proton Mail Bridge (`proton-bridge.service`), the local IMAP and
        SMTP that every `sync = "proton"` mail account reads and sends
        through. Set it exactly when the instance file declares such an
        account: the module cannot read the instance, which lives in the user
        repository outside the store, and a bridge nobody logged into
        restarts every ten seconds forever.
      '';
    };

    vault.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Run the daily encrypted snapshot of the box's irreplaceable state
        to Proton Drive (`suzatary-vault.timer`, scripts/vault-backup.sh),
        which keeps the last seven and deletes the rest. It
        needs the user repository's `VAULT_RECIPIENT` (the public key it
        encrypts to) and a Proton Drive CLI signed in at `~/bin/proton-drive`;
        set it once both exist, since a declared vault that cannot upload is
        a FAILED unit every day, which is the point.
      '';
    };

    nightlyGate.checkout = lib.mkOption {
      type = lib.types.nullOr (lib.types.pathWith { inStore = false; absolute = true; });
      apply = path: if path == null then null else toString path;
      default = null;
      example = "/home/ada/src/suzatary";
      description = ''
        The code checkout where `scripts/train.sh nightly` runs each night.
        Null runs no nightly gate.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # User units inherit the user's keyring/GPG/pass environment, and a user manager exists at
    # boot only if the user lingers.
    users.users.${cfg.user}.linger = true;

    boot.kernel.sysctl = lib.mkIf cfg.web.unprivilegedPorts {
      "net.ipv4.ip_unprivileged_port_start" = lib.mkDefault 443;
    };

    # Minted by ROOT (tailscale's socket is root's) and handed to the user, whose web unit is
    # restarted because the server does not reload the files; `tailscale cert` returns the cached
    # certificate until renewal is due, so weekly is a check, not a fetch.
    systemd.services.suzatary-tls = lib.mkIf (cfg.web.tlsDomain != null) {
      description = "Suzatary: mint or renew the tailnet TLS certificate for the web app";
      after = [ "tailscaled.service" "network-online.target" ];
      wants = [ "tailscaled.service" "network-online.target" ];
      serviceConfig.Type = "oneshot";
      path = [ config.services.tailscale.package pkgs.coreutils pkgs.systemd ];
      script = ''
        set -euo pipefail
        install -d -m 700 -o ${cfg.user} "${cfg.web.tlsDir}"
        tailscale cert --cert-file "${cfg.web.tlsDir}/cert.pem" --key-file "${cfg.web.tlsDir}/key.pem" "${cfg.web.tlsDomain}"
        chown ${cfg.user} "${cfg.web.tlsDir}/cert.pem" "${cfg.web.tlsDir}/key.pem"
        chmod 600 "${cfg.web.tlsDir}/key.pem"
        systemctl --machine=${cfg.user}@.host --user restart suzatary-web.service || true
      '';
    };

    systemd.timers.suzatary-tls = lib.mkIf (cfg.web.tlsDomain != null) {
      description = "Weekly renewal check of the web app's tailnet certificate";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = "weekly";
        Persistent = true;
        RandomizedDelaySec = "1h";
      };
    };

    # System-wide too: the git credential helper runs wherever git runs, a login shell's
    # `git push` outside `nix develop` included.
    environment.systemPackages = with pkgs; [ secretspec pass gnupg notmuch ];

    systemd.user.services.proton-bridge = lib.mkIf cfg.protonBridge.enable (baseUnit
      "Proton Mail Bridge (localhost IMAP 1143 / SMTP 1025)"
      {
        # Bridge is Proton's only IMAP path, so every other mail job is downstream of it: always.
        Restart = "always";
        RestartSec = "10";
        # A store path, so a start evaluates nothing; the .#triage shell's bridge, which wrote vault.enc.
        ExecStart = "${services.protonBridge}/bin/suzatary-proton-bridge";
      } // {
        wantedBy = [ "default.target" ];
      });

    systemd.user.services.suzatary-triage = baseUnit
      "Suzatary mail sync + triage (every 5 min)"
      {
        Type = "oneshot";
        ExecStart = "${services.triage}/bin/suzatary-triage";
      };

    systemd.user.timers.suzatary-triage = {
      # The journal's "Starting …" line is the evidence of the cadence: keep it in step with OnCalendar.
      description = "Run Suzatary triage every 5 minutes";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # The fallback beside the IDLE watcher, which does not watch every account and can drop.
        OnCalendar = "*:0/5";
        # Catch up after downtime rather than silently skipping the missed hour.
        Persistent = true;
      };
    };

    # A plugin sync is monotone and idempotent, so it needs no lock and no ordering with anything here.
    systemd.user.services.suzatary-plugins = baseUnit
      "Suzatary plugin sync (calendars into the cache tier, every 15 min)"
      {
        Type = "oneshot";
        # Nothing waits on it, so it never competes with the triage pass.
        Nice = 10;
        ExecStart = "${services.plugins}/bin/suzatary-plugins";
      };

    systemd.user.timers.suzatary-plugins = {
      description = "Run Suzatary's plugin sync every 15 minutes";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # Odd minutes, off the 5-minute triage ticks.
        OnCalendar = "*:02/15";
        Persistent = true;
      };
    };

    # A fetch only gains objects, under the copy's own lock, so it needs no ordering with anything here.
    systemd.user.services.suzatary-follows = baseUnit
      "Suzatary followed stores' fetch (every hour)"
      {
        Type = "oneshot";
        Nice = 10;
        ExecStart = "${services.follows}/bin/suzatary-follows";
      };

    systemd.user.timers.suzatary-follows = {
      description = "Fetch Suzatary's followed stores every hour";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # An hour bounds how stale a followed store's rows read; every reading dates the copy.
        OnCalendar = "*:37";
        Persistent = true;
      };
    };

    # IMAP IDLE for each Microsoft account, starting the triage on a debounced wake; it never syncs.
    # A watch that ends fails and restarts; a roster with no such account exits 0 and stays down.
    systemd.user.services.suzatary-mail-idle = servingUnit
      "Suzatary IMAP IDLE watcher (lowers triage latency; the 5-minute timer stays the fallback)"
      {
        Restart = "on-failure";
        RestartSec = "10";
        ExecStart = "${services.mailIdle}/bin/suzatary-mail-idle";
      };

    # Nothing starts it; it is the list a deploy restarts, and stopping it would stop every unit on it.
    systemd.user.targets.suzatary-app = {
      description = "Suzatary: the long-running units that run the app the lock names";
      unitConfig = { RefuseManualStart = true; RefuseManualStop = true; };
    };

    # BUILD WHEN THE LOCK MOVES, NOT WHEN THE BOX IS ASKED: the user repository's flake.lock IS
    # the deployment's version, moved only by scripts/deploy.sh, in a commit.
    systemd.user.paths.suzatary-roots = {
      description = "Suzatary roots: build the app the user repository's lock names";
      unitConfig.ConditionUser = cfg.user;
      pathConfig.PathChanged = [ "${cfg.userRepo}/flake.lock" ];
      wantedBy = [ "paths.target" ];
    };

    systemd.user.services.suzatary-roots = baseUnit
      "Suzatary roots: build the app the user repository's lock names"
      {
        Type = "oneshot";
        # Never starves the triage timer or an agent's own cargo build.
        Nice = 10;
        IOSchedulingClass = "idle";
        # systemd coalesces PathChanged activations against an already-active unit, so this never
        # overlaps itself and needs no lock file.
        ExecStart = "${services.roots}/bin/suzatary-roots";
      };

    systemd.user.services.suzatary-web = servingUnit
      "Suzatary web app on the tailnet (https on web.port)"
      {
        # On-failure, not always: nothing is downstream of it, and a crash loop shows as a FAILED unit.
        Type = "simple";
        Restart = "on-failure";
        RestartSec = "10";
        # scripts/serve-web.sh refuses to start without the tailnet address or a certificate; the
        # two variables are what the module decides and the instance file cannot.
        ExecStart = "${services.web}/bin/suzatary-web";
        Environment = [
          "SUZATARY_TLS_DIR=${cfg.web.tlsDir}"
          "SUZATARY_WEB_PORT=${toString cfg.web.port}"
        ];
      };

    systemd.user.services.suzatary-vault = lib.mkIf cfg.vault.enable (baseUnit
      "Encrypted state snapshot to Proton Drive"
      {
        Type = "oneshot";
        # Encrypts to the vault PUBLIC key (the private half is off-box), so this unit carries no
        # secret; it needs the Drive CLI session in the pass store (proton-drive auth login).
        ExecStart = "${services.vault}/bin/suzatary-vault";
      });

    systemd.user.timers.suzatary-vault = lib.mkIf cfg.vault.enable {
      description = "Daily encrypted state snapshot to Proton Drive";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # Daily, because what was written since the last snapshot is lost with the box; pruned to a
        # week, so a forgotten fact's words leave the backups within seven days; an odd minute, so
        # it never lands on a triage tick.
        OnCalendar = "*-*-* 04:17:00";
        Persistent = true;
      };
    };

    # No baseUnit: the train refuses an inherited variable the gate reads, so this sets the
    # instance alone, never SUZATARY_ROOTS.
    systemd.user.services.suzatary-nightly-gate = lib.mkIf (cfg.nightlyGate.checkout != null) {
      description = "Suzatary nightly gate: both halves, full, on origin/master";
      unitConfig.ConditionUser = cfg.user;
      path = [ config.nix.package pkgs.git pkgs.bash pkgs.coreutils pkgs.util-linux pkgs.openssh pkgs.secretspec ];
      environment.SUZATARY_INSTANCE = "${cfg.userRepo}/instance.toml";
      serviceConfig = {
        Type = "oneshot";
        WorkingDirectory = cfg.nightlyGate.checkout;
        ExecStart = "${config.nix.package}/bin/nix develop .#triage --command scripts/train.sh nightly";
        # Behind the triage timer and the web, as the roots refresh is.
        Nice = 10;
        IOSchedulingClass = "idle";
        # A night's gate, and the wait for a train before it, ends by morning.
        TimeoutStartSec = "8h";
        PrivateTmp = true;
        UMask = "0077";
      };
    };

    systemd.user.timers.suzatary-nightly-gate = lib.mkIf (cfg.nightlyGate.checkout != null) {
      description = "Run Suzatary's nightly gate at 01:43";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # An odd minute, so it never lands on a triage tick, and long before the vault's 04:17.
        OnCalendar = "*-*-* 01:43:00";
        # A night missed is not run in the day, beside the trains.
        Persistent = false;
      };
    };
  };
}
