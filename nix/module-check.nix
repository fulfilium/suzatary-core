# The NixOS module, EVALUATED: a host's configuration built from it, and the
# facts a deploy rests on read off the result rather than off the module's text.
# A failed fact fails evaluation with its name; the derivation is the witness.
#
#   nix build .#checks.<system>.suzatary-module
{ nixpkgs, system, module }:
let
  inherit (nixpkgs) lib;
  pkgs = nixpkgs.legacyPackages.${system};

  userRepo = "/home/ada/suzatary-data";

  host = extra: nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      module
      {
        boot.isContainer = true;
        system.stateVersion = "26.05";
        nixpkgs.config.allowUnfreePredicate = pkg: lib.getName pkg == "claude-code";
        users.users.ada = { isNormalUser = true; home = "/home/ada"; };
        services.suzatary = { enable = true; user = "ada"; inherit userRepo; };
      }
      extra
    ];
  };

  config = (host { }).config;
  services = import ./services.nix {
    pkgs = import nixpkgs {
      inherit system;
      config.allowUnfreePredicate = pkg: lib.getName pkg == "claude-code";
    };
  };
  # A script's text, one line per command, for facts about its order: the
  # roots run's, and the app's builds it shares with scripts/deploy.sh.
  lineIn = lines: needle: lib.lists.findFirstIndex (lib.hasInfix needle) null lines;
  beforeIn = lines: a: b:
    lineIn lines a != null && lineIn lines b != null && lineIn lines a < lineIn lines b;
  roots = lib.splitString "\n" services.rootsScript;
  lineOf = lineIn roots;
  before = beforeIn roots;
  builds = lib.splitString "\n" (builtins.readFile ../scripts/app-builds.sh);
  buildsLineOf = lineIn builds;
  buildsBefore = beforeIn builds;
  # The module's own units: what baseUnit made, whatever the name.
  ours = lib.filterAttrs (_: unit: unit.environment ? SUZATARY_USER_REPO);
  units = ours config.systemd.user.services;
  # Every unit of the module's a host with `extra` declares, `<name>.<kind>`.
  unitsOf = extra:
    let
      c = (host extra).config.systemd;
      named = kind: set: map (name: "${name}.${kind}") (lib.attrNames set);
      prefixed = lib.filterAttrs (name: _: lib.hasPrefix "suzatary" name);
    in
    lib.sort lib.lessThan (lib.concatLists [
      (named "service" (ours c.user.services))
      (named "timer" (prefixed c.user.timers))
      (named "path" (prefixed c.user.paths))
      (named "target" (prefixed c.user.targets))
      (named "service" (prefixed c.services))
      (named "timer" (prefixed c.timers))
    ]);
  # What every host runs: the units whose work is a function of the instance
  # the binary reads at run time (an empty roster or plugin list is no work).
  core = [
    "suzatary-app.target"
    "suzatary-mail-idle.service"
    "suzatary-plugins.service"
    "suzatary-plugins.timer"
    "suzatary-roots.path"
    "suzatary-roots.service"
    "suzatary-triage.service"
    "suzatary-triage.timer"
    "suzatary-web.service"
  ];
  execOf = unit: toString unit.serviceConfig.ExecStart;
  # Every unit of the module's on a host with every option set.
  full = ours (host {
    services.suzatary = {
      protonBridge.enable = true;
      vault.enable = true;
      web.tlsDomain = "box.example.ts.net";
    };
  }).config.systemd.user.services;

  # The units a deploy restarts: those part of suzatary-app.target.
  serving = lib.filterAttrs (_: unit: lib.elem "suzatary-app.target" unit.partOf);
  # The app's service wrappers (nix/services.nix `appService`), by the binary
  # each installs.
  appBins = lib.mapAttrsToList (_: wrapper: "/bin/${wrapper.name}")
    (lib.filterAttrs (_: value: lib.isAttrs value && value.runsTheApp or false) services);
  runsTheApp = unit: lib.any (bin: lib.hasSuffix bin (execOf unit)) appBins;
  # A oneshot reads the lock at each start; any other unit runs the app it
  # started with until it restarts.
  longRunning = unit: (unit.serviceConfig.Type or "simple") != "oneshot";
  # The units that would keep running an old app through a deploy.
  unlisted = set: lib.attrNames (lib.filterAttrs
    (name: unit: runsTheApp unit && longRunning unit && !(serving set ? ${name}))
    set);

  # Does a configuration with `extra` evaluate as far as the user repository?
  evaluates = extra:
    (builtins.tryEval (builtins.deepSeq (host extra).config.services.suzatary.userRepo true)).success;

  facts = {
    "a host with the module evaluates, so each refusal below is the option's" =
      evaluates { };
    "the roots refresh watches the user repository's flake.lock, and nothing else" =
      config.systemd.user.paths.suzatary-roots.pathConfig.PathChanged == [ "${userRepo}/flake.lock" ];
    "the roots refresh runs the one reader of the lock" =
      lib.hasSuffix "/bin/suzatary-roots" (execOf units.suzatary-roots);
    "the roots refresh runs at Nice=10, behind the triage timer" =
      units.suzatary-roots.serviceConfig.Nice == 10;
    "the roots run reads only a committed lock" =
      before "show HEAD:flake.lock" "app-input.nix";
    "the roots run builds only a revision a deploy pinned on the branch this box follows, checked first" =
      before "followed.sh admit" "locked source.outPath";
    "the roots run builds the app's builds under its own names, then links the app last" =
      before "app-builds.sh \"$app\" \"$roots/lock-\"" "\"$roots/app\"";
    "the roots run keeps one generation of the operator's profile" =
      lineOf "wipe-history --profile \"$roots/triage-profile\"" != null;
    "the app's builds root the tree before building from it" =
      buildsBefore "--add-root \"\${at}src\"" "#suzatary\"";
    "the app's builds refuse a tree without the lock interface before building" =
      buildsBefore "/nix/app-input.nix\" ]]" "#suzatary\"";
    "the app's builds are the binary, the bundle, the plugins and the shell" =
      lib.all (needle: buildsLineOf needle != null) [
        "#suzatary\" -o \"\${at}bin\""
        "#suzatary-web\" -o \"\${at}dist\""
        "#suzatary-plugins\" -o \"\${at}plugins\""
        "--profile \"\${at}shell\" --command"
      ];
    "the app's builds keep one generation of the shell, after building it" =
      buildsBefore "--profile \"\${at}shell\" --command" "wipe-history --profile \"\${at}shell\"";
    "a path literal for userRepo never becomes a store copy: refused, or the same string" =
      let
        value = (host { services.suzatary.userRepo = lib.mkForce /home/ada/suzatary-data; })
          .config.services.suzatary.userRepo;
        read = builtins.tryEval (builtins.deepSeq value value);
      in
      !read.success || read.value == userRepo;
    "every unit names the user repository and its instance" =
      lib.all
        (unit: unit.environment.SUZATARY_USER_REPO == userRepo
          && unit.environment.SUZATARY_INSTANCE == "${userRepo}/instance.toml")
        (lib.attrValues units);
    "no unit names a code checkout: each wrapper sets SUZATARY_REPO to the app's store path" =
      lib.all (unit: !(unit.environment ? SUZATARY_REPO)) (lib.attrValues units);
    "every unit has a /tmp of its own and creates its files for its user alone" =
      lib.all (unit: unit.serviceConfig.PrivateTmp == true && unit.serviceConfig.UMask == "0077")
        (lib.attrValues full);
    "every unit links into the units' roots, never roots-dev" =
      lib.all (unit: unit.environment.SUZATARY_ROOTS == "/home/ada/.local/state/suzatary/roots")
        (lib.attrValues units);
    "the web runs its wrapper" =
      lib.hasSuffix "/bin/suzatary-web" (execOf units.suzatary-web);
    "a minimal host (enable, user, userRepo) runs the core units and nothing else" =
      unitsOf { } == core;
    "each option adds exactly its own units to the core" =
      unitsOf {
        services.suzatary = {
          protonBridge.enable = true;
          vault.enable = true;
          web.tlsDomain = "box.example.ts.net";
        };
      } == lib.sort lib.lessThan (core ++ [
        "proton-bridge.service"
        "suzatary-tls.service"
        "suzatary-tls.timer"
        "suzatary-vault.service"
        "suzatary-vault.timer"
      ]);
    "a declared vault snapshots daily, at a minute no triage tick lands on" =
      (host { services.suzatary.vault.enable = true; })
        .config.systemd.user.timers.suzatary-vault.timerConfig.OnCalendar == "*-*-* 04:17:00";
    "a declared bridge restarts always, as every Proton job is downstream of it" =
      (host { services.suzatary.protonBridge.enable = true; })
        .config.systemd.user.services.proton-bridge.serviceConfig.Restart == "always";
    "every unit that runs one of the app's wrappers for longer than one job is part of suzatary-app.target, which a deploy restarts" =
      unlisted full == [ ];
    "a long-running unit that runs the app and is left off suzatary-app.target is found" =
      unlisted (ours (host {
        systemd.user.services.suzatary-mail-idle.partOf = lib.mkForce [ ];
      }).config.systemd.user.services) == [ "suzatary-mail-idle" ];
    "suzatary-app.target refuses a manual start and stop, which would stop every unit on it" =
      let unit = config.systemd.user.targets.suzatary-app.unitConfig;
      in unit.RefuseManualStart && unit.RefuseManualStop;
    "suzatary-app.target holds the IMAP watcher and the web" =
      lib.attrNames (serving full) == [ "suzatary-mail-idle" "suzatary-web" ];
    "each unit of suzatary-app.target runs the wrapper named for it, which roots its app as <unit>-app" =
      lib.all (name: lib.hasSuffix "/bin/${name}" (execOf full.${name})) (lib.attrNames (serving full));
    "the IMAP watcher restarts a broken watch and leaves an empty one down" =
      units.suzatary-mail-idle.serviceConfig.Restart == "on-failure";
    "userRepo refuses a relative path" =
      !evaluates { services.suzatary.userRepo = lib.mkForce "suzatary-data"; };
    "userRepo refuses a store path, which is not a checkout" =
      !evaluates { services.suzatary.userRepo = lib.mkForce "/nix/store/00000000000000000000000000000000-data"; };
    "the live clone's option is gone" =
      !evaluates { services.suzatary.repo = "/home/ada/.local/share/suzatary/live"; };
    "the separate data option is gone: userRepo is the one" =
      !evaluates { services.suzatary.dataDir = userRepo; };
  };

  failed = lib.attrNames (lib.filterAttrs (_: holds: !holds) facts);
in
if failed == [ ] then
  pkgs.writeText "suzatary-module-check" (lib.concatLines (lib.attrNames facts))
else
  throw "the NixOS module, evaluated, breaks: ${lib.concatStringsSep "; " failed}"
