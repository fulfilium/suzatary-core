# Suzatary's background jobs, declared.
#
# WHY THIS EXISTS. Until 2026-08-08 these four units were hand-written files in
# ~/.config/systemd/user/ that existed in no repository, plus a fifth (Proton
# Bridge) conjured at boot by `systemd-run` from the host config's suzatary-up.sh. Three
# problems, in ascending order of severity:
#
#   1. A fresh machine got none of it. "Set up Suzatary on a new box" meant
#      remembering four unit files nobody had written down.
#   2. `systemd-run --unit=proton-bridge` makes a *transient* unit: invisible to
#      nixos-rebuild, absent from any config, gone on reboot until the bring-up
#      script re-ran it.
#   3. The units pinned claude-code by raw store path
#      (/nix/store/2lyv...-claude-code-2.1.191/bin). Nothing held a GC root on
#      it, and the host's configuration.nix now runs `nix.gc` weekly with
#      --delete-older-than 30d. That path was scheduled to be collected out from
#      under the triage timer, which would have failed the way the July 26 auth
#      lapse failed: quietly, hourly, until somebody read the journal.
#
# ⚠️ THIS MODULE CAN BE SILENTLY OVERRIDDEN. `~/.config/systemd/user/` takes
# PRECEDENCE over the `/etc/systemd/user/` units NixOS writes from here. The
# hand-enabled leftovers from problem 1 above were still there — symlinks made
# 2026-08-29 15:28 pointing at that day's store paths, which are immutable, so
# they froze the units at that generation forever. Every rebuild between then
# and 2026-09-01 updated /etc and changed nothing that ran: the 5-minute triage
# cadence was committed on 08-30 and the box kept running hourly (24-29 starts
# a day in the journal, never ~288) until the shadows were removed. A rebuild
# reporting success is NOT evidence that a unit changed. Check the thing that
# actually runs:
#
#   systemctl --user show -p FragmentPath suzatary-triage.timer
#
# It must be under /etc/systemd/user. If it is under ~/.config or /nix/store,
# this file is decorative for that unit.
#
# WHAT THE UNITS RUN IS THE APP THE USER REPOSITORY'S LOCK NAMES
# (docs/USERREPO-PLAN-2026-10-01.md stage 2): one option, `userRepo`, names the
# user checkout, and everything else is derived from it. The roots refresh
# reads its flake.lock and links the app's store path; every unit runs the
# app's scripts from there, and they commit and push the DATA tree, never the
# code. A deploy is a commit that moves the lock (scripts/deploy.sh).
{ config, lib, pkgs, ... }:

let
  cfg = config.services.suzatary;

  # What these jobs need in order to run now lives in nix/services.nix, next to
  # the executables it produces, so a machine without this module can still get
  # it. This module is left with the part that is genuinely systemd's business:
  # when to run, under whose manager, and what happens on failure.
  services = import ./services.nix { inherit pkgs; };
  inherit (services) runtimeDeps;

  # Common shape for every Suzatary user unit. ConditionUser is load-bearing,
  # not decoration: NixOS renders systemd.user.* into EVERY user's manager, and
  # this box has `services.getty.autologinUser = "root"`. Without the condition,
  # logging in on the console would start a second Proton Bridge as root, which
  # would fight over vault.enc. It also keeps the units from firing under any
  # future user.
  #
  # Do not replace the substituter gate with `after = [ "network-online.target"
  # ]`. That target does not exist in a USER manager — `systemctl --user`
  # reports it not-found — so systemd silently drops the ordering. It looks like
  # a gate and is not one.
  #
  # The gate itself now lives INSIDE the wrappers (nix/services.nix), because it
  # is a property of "this job re-enters the flake at run time", not of "systemd
  # started this job" — and something running a wrapper by hand on
  # another box needs it just as much. There is no ExecStartPre here any more.
  #
  # `path` stays even though the wrappers set their own PATH. It is what a
  # process started under this unit inherits BEFORE the wrapper prepends to it,
  # and Proton Bridge in particular used to get the whole .#triage devShell on
  # PATH; narrowing that to exactly what its wrapper names would be a guess
  # about a daemon that has historically been the flakiest piece here.
  baseUnit = description: serviceConfig: {
    inherit description;
    path = runtimeDeps;
    unitConfig.ConditionUser = cfg.user;
    # The user repository: its flake.lock names the app (read by the roots
    # refresh alone), and its instance file the chains, state.json and the
    # briefings, which safe-push commits into it. SUZATARY_REPO is not set
    # here: each wrapper sets it to the app's store path (nix/services.nix).
    environment.SUZATARY_USER_REPO = cfg.userRepo;
    environment.SUZATARY_INSTANCE = "${cfg.userRepo}/instance.toml";
    # The units' own root links: `app` (the tree the lock names, once built),
    # `app-bin`, `web-dist`, and each unit's own roots of what it runs.
    # scripts/app-bin.sh, web-dist.sh and root-serving.sh link here, and into
    # `roots-dev` when this is unset, so a dev shell's build never repoints
    # what these units run.
    environment.SUZATARY_ROOTS = cfg.rootsDir;
    # THE JUNK SWEEP'S CHURN SEED (2026-09-27, `app/core/src/mail/junk.rs`):
    # `cfg.junkSeed`'s `domains.txt`, or unset when the option is null — a dev
    # shell or a host that has not wired it, which the binary reads as no
    # seed (a real state, not an error; the digest says `seed: none`). Every
    # unit gets it, on `baseUnit`'s usual grounds ("in baseUnit or on the
    # units that serve and review", 2026-09-27): only `suzatary-web` and the
    # secretary's `review` pass ever build a `ChurnList` from it, and an
    # unused environment variable on the rest is harmless.
    environment.SUZATARY_JUNK_SEED =
      if cfg.junkSeed == null then null else "${cfg.junkSeed}/domains.txt";
    # EVERY UNIT'S FILES ARE ITS USER'S, AND ITS /tmp ITS OWN. These jobs write
    # what quotes mail (a sync's report, an agent's run, their logs), so a file
    # is created 0600 whatever a script forgets, and nothing lands in a /tmp
    # another user or unit shares. Nothing here reads another unit's /tmp: the
    # sync's report and every log are in the state directory, Bridge serves
    # IMAP and SMTP on loopback TCP, and the vault stages in its own mktemp. In
    # a user manager PrivateTmp implies PrivateUsers, which maps every uid but
    # this one to nobody; nothing here changes uid, and a socket's peer is
    # still seen as its real uid by tailscaled, the nix daemon and the bus.
    serviceConfig = { PrivateTmp = true; UMask = "0077"; } // serviceConfig;
  };

  # A unit that runs the app for as long as it is up, rather than one job: it
  # read the app at its start and goes on running that app until it restarts.
  # So it is PartOf the target, whose ConsistsOf is the list scripts/deploy.sh
  # try-restarts once the roots run links a new app; a unit joins the list
  # here, where it is declared. nix/module-check.nix fails on a long-running
  # unit that runs one of the app's wrappers and is not on it.
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

    # SUNSET 2026-08-18: no `voice` option. The voice secretary is not declared
    # here any more — see the note at the bottom of this file.
  };

  config = lib.mkIf cfg.enable {
    # Linger: the whole point of these being user units is that they inherit the
    # user's keyring/GPG/pass environment, and a user manager only exists at
    # boot if the user lingers. This was previously set by hand with
    # `loginctl enable-linger` and was therefore one `nixos-install` away from
    # being forgotten on a rebuild.
    users.users.${cfg.user}.linger = true;

    boot.kernel.sysctl = lib.mkIf cfg.web.unprivilegedPorts {
      "net.ipv4.ip_unprivileged_port_start" = lib.mkDefault 443;
    };

    # The certificate is minted and renewed by ROOT (tailscale's socket is
    # root's), handed to the user, and the user's web unit is restarted so
    # the server reads the new files — it does not reload them on its own.
    # `tailscale cert` is idempotent: it returns the cached certificate until
    # renewal is due, so weekly is a check, not a fetch.
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

    # Also system-wide, not just on the unit PATH. The git credential helper
    # runs wherever git runs — including an ordinary interactive `git push` from
    # a login shell that is not inside `nix develop` — and a credential helper
    # that only resolves inside a dev shell is a credential helper that fails at
    # the worst moment.
    # muchsync/notmuch are system-wide for the same reason: a laptop running
    # `muchsync <fetcher>` over SSH spawns muchsync in a plain login shell on
    # this end, outside any dev shell (the server half of mail replicas,
    # 2026-08-29).
    environment.systemPackages = with pkgs; [ secretspec pass gnupg muchsync notmuch ];

    systemd.user.services.proton-bridge = lib.mkIf cfg.protonBridge.enable (baseUnit
      "Proton Mail Bridge (localhost IMAP 1143 / SMTP 1025)"
      {
        # Bridge is Proton's only IMAP path, so every other mail job is
        # downstream of it staying up. It has historically been the flakiest
        # piece — hence always, not on-failure.
        Restart = "always";
        RestartSec = "10";
        # Was `nix develop <repo>#triage --command protonmail-bridge`,
        # which evaluated the flake on every start — at boot, before DNS, which
        # is exactly the 2026-08-10 failure where this unit reported "active
        # (running)" for minutes while compiling bootstrap-tools instead of
        # serving IMAP. A store path has nothing left to evaluate. Same
        # derivation the .#triage devShell hands out, so the bridge version
        # vault.enc was written by is unchanged.
        ExecStart = "${services.protonBridge}/bin/suzatary-proton-bridge";
      } // {
        wantedBy = [ "default.target" ];
      });

    systemd.user.services.suzatary-triage = baseUnit
      "Suzatary mail sync + triage (every 5 min)"
      {
        Type = "oneshot";
        # The wrapper supplies PATH and the substituter gate, then execs the
        # script from the app's store path; it commits and pushes the data
        # tree.
        ExecStart = "${services.triage}/bin/suzatary-triage";
      };

    systemd.user.timers.suzatary-triage = {
      # The description is load-bearing evidence, not decoration: the journal
      # line "Starting Suzatary hourly ..." is what proved, on 2026-09-01, that
      # the deployed unit was still the Aug 29 one. Keep it in step with
      # OnCalendar so the journal cannot lie about the cadence.
      description = "Run Suzatary triage every 5 minutes";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # Every 5 minutes since 2026-08-30 (was hourly — a default, never a
        # decision). Cheap because the wrapper already gates the LLM on
        # queue-nonempty: empty ticks cost one sync sweep and no tokens.
        # Bursts still batch: a running oneshot delays the next tick rather
        # than overlapping it. THIS STAYS THE FALLBACK even now that
        # suzatary-mail-idle (below) exists: IDLE lowers the latency between
        # a real message arriving and the pipeline seeing it, it does not
        # become a second source of truth — an account IDLE does not watch
        # (Gmail, Proton) or a dropped IDLE connection still needs this timer
        # to be the one thing nothing else depends on staying up.
        OnCalendar = "*:0/5";
        # Catch up after downtime rather than silently skipping the missed hour.
        Persistent = true;
      };
    };

    # THE PLUGINS' SOURCES (docs/PLUGINS-PLAN-2026-10-02.md stage 2): every
    # `[[plugins]]` source run into the cache tier, the calendar first. A
    # sync is monotone and idempotent, so it needs no lock and no ordering
    # with anything else here; a run that changes nothing writes nothing.
    systemd.user.services.suzatary-plugins = baseUnit
      "Suzatary plugin sync (calendars into the cache tier, every 15 min)"
      {
        Type = "oneshot";
        # Nothing waits on it, so it never competes with the triage pass.
        Nice = 10;
        # The wrapper supplies PATH and the substituter gate, then execs
        # scripts/plugin-sync.sh, which roots the binary and the components
        # and prints one journal line per plugin.
        ExecStart = "${services.plugins}/bin/suzatary-plugins";
      };

    systemd.user.timers.suzatary-plugins = {
      # Kept in step with OnCalendar, as the triage timer's is: the journal's
      # "Starting …" line is the evidence of the cadence.
      description = "Run Suzatary's plugin sync every 15 minutes";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # Every 15 minutes: a calendar link is a published file the providers
        # themselves refresh on the order of minutes to hours, and a meeting
        # booked this quarter-hour reaches the briefing and the secretary at
        # most one tick late. Odd minutes, off the 5-minute triage ticks.
        OnCalendar = "*:02/15";
        Persistent = true;
      };
    };

    # IMAP IDLE: one connection per Microsoft account, watching for new mail
    # and starting suzatary-triage.service on a debounced wake — a LONG-
    # RUNNING watcher, not a periodic job, rather than the oneshot-under-a-
    # timer shape above. It never syncs itself: the 5-minute timer above
    # stays the fallback source of truth, and this unit only lowers the
    # latency.
    #
    # WHAT IT WATCHES IS THE INSTANCE'S, read by the binary at start: a watch
    # that ends is an error (the verb bails so the whole watch restarts), and
    # a roster with no Microsoft account is the empty watch, which exits 0.
    # So on-failure, not always: the first restarts, the second stays down,
    # and a box without such an account runs no watcher. A roster that gains
    # one is read on the unit's next start.
    systemd.user.services.suzatary-mail-idle = servingUnit
      "Suzatary IMAP IDLE watcher (lowers triage latency; the 5-minute timer stays the fallback)"
      {
        Restart = "on-failure";
        RestartSec = "10";
        # The wrapper supplies PATH and the substituter gate, then execs the
        # app's scripts/mail-idle.sh, which execs `suzatary mail idle` out of
        # the pure `.#suzatary` derivation.
        ExecStart = "${services.mailIdle}/bin/suzatary-mail-idle";
      };

    # The long-running units that run the app: each `servingUnit` is part of
    # it. Nothing starts it; it is the list a deploy restarts. Stopping it
    # would stop every one of them, so it refuses a manual start or stop.
    systemd.user.targets.suzatary-app = {
      description = "Suzatary: the long-running units that run the app the lock names";
      unitConfig = { RefuseManualStart = true; RefuseManualStop = true; };
    };

    # BUILD WHEN THE LOCK MOVES, NOT WHEN THE BOX IS ASKED
    # (docs/LAZYBUILDSTALL-PLAN-2026-09-21.md; the lock since
    # docs/USERREPO-PLAN-2026-10-01.md stage 2). The user repository's
    # flake.lock IS the deployment's version: scripts/deploy.sh is the one
    # thing that moves it, in a commit, so a stamp of history says which code
    # ran when. Its service is the one reader of the lock (nix/services.nix
    # `roots`): it runs exactly the app's two scripts, so there is one
    # spelling of what a root is, and links the app only after both succeed.
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
        # Never starves the triage timer or an agent's own cargo build — the
        # whole point is that this refresh moves off the blocking path
        # rather than moving the stall (plan §3, §5).
        Nice = 10;
        IOSchedulingClass = "idle";
        # A oneshot triggered by a `.path` unit does not queue a second
        # start while the first is still running (systemd coalesces
        # PathChanged activations against an already-active unit), so this
        # never overlaps itself; no lock file is needed. Each script prints
        # its store path, one journal line each: the evidence a deploy cites.
        ExecStart = "${services.roots}/bin/suzatary-roots";
      };

    systemd.user.services.suzatary-web = servingUnit
      "Suzatary web app on the tailnet (https on web.port)"
      {
        # A server, not a job: Type=simple, and on-failure rather than always
        # — a crash loop here should be visible as a FAILED unit, not papered
        # over. Bridge gets `always` because every mail job is downstream of
        # it; nothing is downstream of this one.
        Type = "simple";
        Restart = "on-failure";
        RestartSec = "10";
        # The wrapper supplies PATH and the substituter gate, then execs the
        # app's scripts/serve-web.sh. That script resolves the
        # tailnet address and refuses to start without it or a certificate —
        # see its header for why a bind address is half this app's access
        # control.
        #
        # WHAT IT RUNS IS THE RUST BINARY since stage 2 of
        # docs/ARCHITECTURE.md: `suzatary serve`, out of the pure `.#suzatary`
        # derivation (scripts/app-bin.sh resolves and runs it on demand, not
        # a `.#triage` shell package — see that shell's comment in flake.nix),
        # reading the user repository's `instance.toml` for the roster, the
        # trust policy and the paths. The two variables below stay because
        # they are the two things this MODULE decides and that file cannot —
        # where the certificate lands, and which port an option set.
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
        # Encrypts to the vault PUBLIC key (private half is off-box with
        # the owner), so this unit carries no secret. Needs the Drive CLI session
        # in the pass store (proton-drive auth login, one-time per box).
        ExecStart = "${services.vault}/bin/suzatary-vault";
      });

    systemd.user.timers.suzatary-vault = lib.mkIf cfg.vault.enable {
      description = "Daily encrypted state snapshot to Proton Drive";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        # Daily, because a key or a text written since the last snapshot is
        # lost with the box, and the snapshots are pruned to a week, so a
        # forgotten fact's words leave the backups within seven days. 04:17,
        # an odd minute, so it never lands on a triage tick (every five).
        # A failed upload surfaces as a FAILED unit.
        OnCalendar = "*-*-* 04:17:00";
        Persistent = true;
      };
    };

    # The voice secretary used to be declared here. Sunset 2026-08-18 (the owner:
    # "i think lets sunset the voice thing for now").
    #
    # It was not a hard call, because the thing had already stopped being real:
    # the unit had been up 8 days and served ZERO requests, and the reason is
    # that `tailscaled` is inactive on this box with no tailscale binary in the
    # system profile. The server was listening on 127.0.0.1:8787 with no route
    # to it from anywhere — the phone could not have reached it. It was costing
    # a whisper-cpp/ffmpeg/gtk4 closure and a boot-time flake evaluation to
    # answer nobody.
    #
    # THE CODE IS NOW DELETED TOO (2026-09-08). It was Python — `suzatary/voice/`,
    # `scripts/voice-server.sh` and the `.#voice` devShell — and it went with
    # the rest of the Python in stage 6 of docs/ARCHITECTURE.md, three weeks
    # after its last request. Reviving it means writing it again, against the
    # Rust binary, AND making the tailnet exist for it: real HTTPS certificates
    # are what phone browsers require for getUserMedia, which is why
    # `tailscale serve` was the transport. `git log -- suzatary/voice` is the
    # design, and the entry above is why it was not worth keeping warm.
  };
}
