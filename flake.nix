{
  description = "Suzatary: a self-hosted secretary over the Prodrome";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    # THE DATABASE, AS AN INPUT (docs/ARCHITECTURE.md §1, §8.8). It was
    # `prodrome/` in this tree until 2026-09-09; it is
    # https://github.com/fulfilium/prodrome now, public under MIT OR Apache-2.0,
    # and this repository takes it the way it takes any other dependency.
    #
    # PINNED TO A COMMIT, and to the SAME commit `app/Cargo.toml` names. The
    # binary links the crate through cargo and the browser fetches the wasm
    # through this input, and if the two shas ever parted the page and the
    # server would be two readings of one spec — which is the seam §1 exists to
    # forbid. Moving the pin is THREE places in one commit: this url (with
    # `flake.lock`), the `rev` in `app/Cargo.toml`, and `app/Cargo.lock`'s
    # `source` (and the crate's version with it). It was four until crane
    # (2026-09-13): the `outputHashes` entry that vendored the checkout by hash
    # is gone, because crane fetches a git dependency at the lock's own rev.
    prodrome.url = "github:fulfilium/prodrome/186ac61694186ccc2e9168ad886a41f38d6710cf";
    # One nixpkgs in the closure. The Prodrome's own
    # flake pins its own — that is what its CI builds against — but the two
    # packages that matter here (`cargo`/`rustc` and `wasm-bindgen-cli_0_2_127`,
    # which must stay in lockstep with the `=0.2.127` crate) are present in this
    # repository's pin, which already builds the same derivation.
    prodrome.inputs.nixpkgs.follows = "nixpkgs";
    prodrome.inputs.flake-utils.follows = "flake-utils";
    prodrome.inputs.crane.follows = "crane";
    # Its Typst package's look, the same design system as ours (below).
    prodrome.inputs.typst-design.follows = "typst-design";

    # THE RUST BUILD IS CRANE (2026-09-13; the owner: "there are tools to enable
    # nix caching in github CI … this is important to fix"). Before it, six
    # derivations here each compiled the whole workspace — every dependency,
    # from scratch, inside their own sandbox — so a CI run was about eighty
    # minutes of building tokio, axum and rustls over and over, and the store
    # cache never held what the next run needed because any edit under app/
    # changed all six hashes. crane splits the dependencies into one
    # derivation keyed on Cargo.lock alone (`cargoArtifacts` below), which the
    # store cache carries across runs, and every build after it compiles the
    # four workspace crates and nothing else.
    crane.url = "github:ipetkov/crane";

    # THE LOOK, AS AN INPUT: the owner's design system, the one source of
    # every colour, mark and template Suzatary sets in Typst. It is a Typst
    # package, `@local/typst-design:<version>`, found on `TYPST_PACKAGE_PATH`
    # wherever typst runs here, and bundled beside the tab's kit
    # (`typstPackages` below), so the box and the tab compile against one
    # package. Pinned by commit like the Prodrome.
    typst-design.url = "github:bmabsout/typst-design/79d9dbafcd6457f269c3cd8b6f88c0ac27e35851";
    typst-design.inputs.nixpkgs.follows = "nixpkgs";

    # THE JUNK CHURN SEED'S TWO SOURCES (the owner, 2026-09-27: "the right way
    # is to use one of those lists of spam websites that are available on
    # GitHub as the seed + ours"). `flake = false`: neither is a flake, so
    # each resolves to a plain fetched tree, locked by hash in `flake.lock`
    # exactly like any other input, and bumped by the same
    # `update-flake-lock.yml` (`nix flake update` touches every input
    # uniformly). `suzatary-junk-seed` below is the one reader of both; see
    # its own comment for the files it reads and why.
    disposable-email-domains = {
      url = "github:disposable-email-domains/disposable-email-domains";
      flake = false;
    };
    blocklist-project = {
      url = "github:blocklistproject/Lists";
      flake = false;
    };

    # THE PLUGINS' STANDARD LIBRARY. A plugin is built for wasm32-wasip2, whose std
    # nixpkgs' rustc does not carry (it carries wasm32-unknown-unknown alone), so the
    # plugin derivation alone takes a rustup toolchain from this overlay with that target
    # added. Nothing else the flake builds uses it.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, prodrome, crane, typst-design, disposable-email-domains, blocklist-project, rust-overlay }:
    let
      inherit (nixpkgs) lib;

      # THE APP'S VERSION IS DERIVED FROM THE TREE, NEVER TYPED TWICE.
      #
      # versionName is the RELEASE's version (`app/release.toml`, below) plus the
      # commit it was built from, so a phone's About screen names a commit
      # somebody can check out.
      #
      # versionCode is the COMMIT COUNT: Android's only requirement is that it
      # increase, and the count does that on master for free, with no bump to
      # forget. A dirty tree has no count and gets 0, which is below every
      # release — so a hand-built APK can never present itself to the installer
      # as newer than one that came out of CI.
      #
      # Both are inputs to the derivation, so its hash moves exactly when they
      # do, which is what makes "rebuild only on a change" mean something.
      #
      # THE RELEASE IS `app/release.toml`, which `suzatary release cut` writes and no crate
      # compiles: `suzatary-app` copies the binary and installs the file beside it, where the
      # binary reads it at run time (`app/cli/src/stamp.rs`), so a train rebuilds no binary.
      appRelease = builtins.readFile ./app/release.toml;
      appVersion =
        let
          line = lib.findFirst (lib.hasPrefix "version = ")
            (throw "flake.nix: no version line in app/release.toml")
            (lib.splitString "\n" appRelease);
        in
        lib.removeSuffix "\"" (lib.removePrefix "version = \"" line);
      appVersionName = "${appVersion}+${self.shortRev or self.dirtyShortRev or "dirty"}";
      appVersionCode = toString (self.revCount or 0);
    in
    # System-independent outputs live outside eachDefaultSystem. The NixOS
    # module is consumed by ~/Documents/nixos, which takes this repo as a
    # `path:` flake input; it replaces the four hand-written unit files that
    # used to sit untracked in ~/.config/systemd/user/, plus the imperative
    # `systemd-run` bring-up. See nix/nixos-module.nix for why that mattered.
    {
      nixosModules.default = import ./nix/nixos-module.nix;
      nixosModules.suzatary = self.nixosModules.default;

      # What a new user's private repository starts from
      # (docs/USERREPO-PLAN-2026-10-01.md stage 5): `nix flake init -t <this>#user`.
      templates.user = {
        path = ./templates/user;
        description = "A secretary's user repository: its data, and the app it pins";
      };
      templates.default = self.templates.user;
    }
    //
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          # claude-code is unfree. Named explicitly rather than a blanket
          # allowUnfree, so this stays a decision about one package: the
          # scheduled jobs cannot run without it, and until now that permission
          # was granted only by the host's NixOS config — which is precisely why
          # `nix build .#triage` worked on this box and nowhere else.
          config.allowUnfreePredicate = pkg:
            builtins.elem (nixpkgs.lib.getName pkg) [ "claude-code" ];
        };

        # What the background jobs need in order to run, expressed without
        # reference to systemd — so `nix run .#web` on any Linux with nix and
        # the NixOS unit invoke the same store path. See nix/services.nix.
        services = import ./nix/services.nix { inherit pkgs; };

        # The Android portal. It takes `nixpkgs` and not `pkgs` on purpose: the
        # SDK is unfree and licence-gated, and nix/apk.nix imports its own pkgs
        # so that permission stays inside the one file that needs it instead of
        # widening the posture the comment above just narrowed.
        apk = import ./nix/apk.nix {
          inherit nixpkgs system;
          src = ./android;
          icon = ./web/static/icon-512.png;
          signScript = ./scripts/sign-apk.sh;
          versionCode = appVersionCode;
          versionName = appVersionName;
        };
        # An APK is a function of its PROFILE (nix/apk-profile.nix). The
        # generic one is empty and names no deployment; a person's names their
        # box, and an impure build reads its path from `SUZATARY_APK_PROFILE`.
        # Unset, the build fails saying so, at build time rather than
        # evaluation, so `nix flake check` still evaluates every package.
        apkProfile = import ./nix/apk-profile.nix { inherit (nixpkgs) lib; };
        apkNamed =
          build:
          let
            path = builtins.getEnv "SUZATARY_APK_PROFILE";
          in
          if path == "" then
            pkgs.runCommand "suzatary-apk-unnamed" { } ''
              echo "apk: name a profile's TOML file in SUZATARY_APK_PROFILE and build with --impure" >&2
              exit 1
            ''
          else
            build (apkProfile path);


        # =====================================================================
        # THE APPLICATION (docs/ARCHITECTURE.md). Everything above it is either
        # this repository's own scaffolding or the ONE line that takes the
        # database off the input, because `app/` is where Suzatary moves and the
        # Prodrome is the database it is built on — one direction (§1, §2).
        # =====================================================================

        # THE SOURCE IS ONE TREE NOW, AND STILL NOT THE REPOSITORY. It was two
        # until the split (§8.8): `app/core` reached `../prodrome/core` by path,
        # so the sandbox had to hold that directory too. The database arrives
        # through cargo's git fetch and this vendoring below, so the fileset is
        # `app/` and the three files its tests read — and nothing else.
        # Narrowed with `fileset` rather than handed `./.`, so a briefing, a
        # commit or a rendered document does not change this derivation's hash,
        # and so `target/` never reaches the store.
        appSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./app/Cargo.toml
            ./app/Cargo.lock
            ./app/bench
            ./app/cli
            ./app/core
            ./app/io
            ./app/plugin
            ./app/plugin-fence
            ./app/plugin-ics
            ./app/testkit
            ./app/wasm
            # The plugin worlds the host binds at compile time.
            ./app/wit
            # The site vouches for the Android app, and a test pins the file
            # that says so; the sandbox has to hold it to run that test.
            ./web/static/.well-known/assetlinks.json
            # THE SAMPLE, NOT THE DEPLOYMENT (docs/DATASPLIT-PLAN-2026-09-14.md
            # §3/§7 commit 3): `instance.toml` and `mbsyncrc` are this box's
            # data now, so a build sandbox is handed neither. `tests/init.rs`
            # pins `instance.sample.toml` as `suzatary init`'s own output, and
            # the tests that used to read the real `instance.toml`/`mbsyncrc`
            # for the `Patterns`↔`inbox_folders` invariant now locate this
            # box's own deployment through `Instance::locate` and skip when
            # it is absent — which it is, here.
            ./instance.sample.toml
          ];
        };

        # THE BUILD STAMP TRACKS `appSrc`, NOT THE WHOLE TREE
        # (`todo:suzatary/resolvecopiestree`, 2026-09-21). The comment above
        # is true of `appSrc`'s OWN hash, but `suzatary-app` below used to
        # stamp `SUZATARY_BUILD` with `self.shortRev`/`self.dirtyShortRev` —
        # the WHOLE repository's HEAD — so a triage pass's own commit (one
        # object under `events-*/`, `HEAD`) or a docs edit changed that
        # stamp, and with it `.#suzatary`'s drvPath: a chain write forced a
        # full release recompile of three crates (measured over 10 minutes),
        # of a binary whose OWN source had not moved a byte. `appSrc`'s store
        # path is already a pure hash of exactly the files this binary reads
        # (`lib.fileset.toSource` above); its hash prefix is the stamp
        # instead, so `build` on `GET /api/about` changes exactly when the
        # binary's own inputs do — never on a chain write, a briefing or a
        # doc. This RETIRES the promise `appVersionName` still makes below
        # ("the same short rev the APK's version name carries, so a phone
        # and a box can be compared by eye"): an APK's version and this
        # field no longer name the same git commit. That comparison cost a
        # full release rebuild on every chain write, which nobody asked for
        # at that price. If it is needed again, `appVersionName` would have
        # to move to this same scheme rather than this field moving back to
        # git's.
        appSrcBuild = builtins.substring 0 8 (builtins.baseNameOf (toString appSrc));

        # `cargoRoot` is where the lockfile lives and `buildAndTestSubdir` is
        # where cargo runs; both are `app`, because the source root is the
        # repository and the workspace is one level in.
        #
        # ONE DEPENDENCY BUILD, SHARED BY EVERY DERIVATION BELOW (crane,
        # 2026-09-13). `cargoArtifacts` is the workspace's dependencies compiled
        # once against a dummy copy of the crates, so its hash moves only when
        # `app/Cargo.lock` or the toolchain does — never when a line of this
        # application changes. The release, the fixture, the two checks and the
        # two generators all start from it and compile the three workspace
        # crates and nothing else. On a fresh store that is one dependency build
        # instead of six; on CI's restored store it is none.
        #
        # THE ONE GIT DEPENDENCY IS VENDORED FROM THE LOCK. crane fetches a
        # `git+…` dependency at the rev the lockfile names, so moving the
        # Prodrome pin is now THREE places — `app/Cargo.toml`'s `rev`,
        # `app/Cargo.lock`, and `inputs.prodrome.url` above (with `flake.lock`)
        # — where it used to be four with an `outputHashes` entry here.
        #
        # THE WORKSPACE IS ONE LEVEL IN, so cargo is told where the manifest is
        # and where to put `target/`: crane restores the dependency artifacts
        # into `./target` at the source root, and a cargo that wrote to
        # `app/target` instead would rebuild everything beside them.
        craneLib = crane.mkLib pkgs;
        # The plugins' toolchain: rustup's 1.98.1, nixpkgs' version at this lock, with the
        # wasm32-wasip2 std, whose linker writes the component itself.
        craneWasi = craneLib.overrideToolchain
          ((rust-overlay.lib.mkRustBin { } pkgs).stable."1.98.1".minimal.override {
            targets = [ "wasm32-wasip2" ];
          });
        # Every phase runs INSIDE `app/` (`sourceRoot`), the workspace's own
        # directory, rather than teaching each of crane's hooks a
        # `--manifest-path`: the install hook runs `cargo metadata` from the
        # working directory and knows no such flag. The tree above `app/` is
        # still unpacked, so the three files the tests read by `../../` stay
        # where they read them.
        commonArgs = {
          pname = "suzatary";
          src = appSrc;
          cargoToml = ./app/Cargo.toml;
          cargoLock = ./app/Cargo.lock;
          sourceRoot = "source/app";
          CARGO_TARGET_DIR = "target";
          strictDeps = true;
        };
        # THE DEPENDENCY BUILDS DO NOT READ THE RELEASE: the workspace's version is fixed
        # (`app/Cargo.toml`), and the release is a stamp on the binary alone.
        depsOf = craneLib.buildDepsOnly;
        cargoArtifacts = depsOf commonArgs;
        # The vendored lock, for the wasm builds that start from no dependency build.
        cargoVendorDir = craneLib.vendorCargoDeps { cargoLock = ./app/Cargo.lock; };
        appCrate = args: craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; } // args);

        # THE CORE'S LIBRARY ALONE: the manifests, the library's sources and what
        # they compile in (the release plan, the calendar's schema, the two test
        # supports its unit tests include), and the examples. What the wire's
        # generator and the conformance vectors compile, so an edit to the
        # server, the CLI, a live pin or the core's own integration tests moves
        # neither.
        coreSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./app/Cargo.toml
            ./app/Cargo.lock
            ./app/bench/Cargo.toml
            ./app/cli/Cargo.toml
            ./app/io/Cargo.toml
            ./app/plugin/Cargo.toml
            ./app/plugin-fence/Cargo.toml
            ./app/plugin-ics/Cargo.toml
            ./app/plugin-ics/ics.schema
            ./app/testkit/Cargo.toml
            ./app/wasm/Cargo.toml
            ./app/core/Cargo.toml
            ./app/core/src
            ./app/core/releases.toml
            ./app/core/examples
            ./app/core/tests/support
          ];
        };
        # In cargo's own profiles (`CARGO_PROFILE` empty: no `--release`), since
        # the generators are a `cargo test` and a `cargo run`, and with the core's
        # own feature set (`-p suzatary-core`): built against the release,
        # whole-workspace `cargoArtifacts`, they compiled every dependency again.
        coreArgs = commonArgs // {
          src = coreSrc;
          CARGO_PROFILE = "";
        };
        # `.#suzatary-core-deps`: the dependencies of the core's unit tests and of its
        # examples, built once per lock and rooted by the merge gate.
        suzatary-core-deps = depsOf (coreArgs // {
          pname = "suzatary-core";
          buildPhaseCargoCommand = "cargo build --locked -p suzatary-core --examples";
          checkPhaseCargoCommand = "cargo test --locked -p suzatary-core --lib --no-run";
        });
        coreCrate = args: craneLib.buildPackage (coreArgs // { cargoArtifacts = suzatary-core-deps; } // args);

        # `nix build .#suzatary-bin` — the one binary (§2). Pure: the dependency
        # tree is vendored from app/Cargo.lock, nothing here reaches the
        # network, and there is no OpenSSL to link against (rustls, §2).
        # `doCheck` is OFF here since 2026-09-13: the laws are
        # `checks.suzatary-core`, which `nix flake check` and CI build from the
        # same `cargoArtifacts`, and running them a second time inside the
        # release was a third of CI's hour for a verdict already taken.
        #
        # IT DOES NOT READ THE RELEASE: no attribute here names `app/release.toml`, and
        # `appSrc` does not hold it, so a train's release commit leaves this derivation
        # as it was and nothing recompiles (scripts/release-free.sh is the gate's proof).
        suzatary-bin = appCrate {
          pname = "suzatary";
          doCheck = false;
          # THE REVISION THE ABOUT PAGE SHOWS (`GET /api/about`): `appSrcBuild`
          # (above `appSrc`) — a pure hash of exactly this binary's own
          # sources, NOT `self.shortRev`/`dirtyShortRev` any more (see that
          # comment for why: the whole-repo git rev made every chain write a
          # full release rebuild). A binary cargo built outside the flake has
          # no stamp and says `dev`.
          SUZATARY_BUILD = appSrcBuild;
          meta = {
            description = "Suzatary's binary, without its release; `.#suzatary` installs both";
            mainProgram = "suzatary";
          };
        };

        # `nix build .#suzatary` — the application (docs/ARCHITECTURE.md): the
        # binary above, copied, with `app/release.toml` installed beside it at
        # `share/suzatary/release.toml`, where the binary reads its release at run
        # time (`GET /api/about`, `--version`). A COPY, not a link: the binary finds
        # the file from its own resolved path, and a link would resolve to
        # `suzatary-bin`, which holds no release. A release commit rebuilds this copy
        # and nothing else. It runs `--version` before it ships, so a release file the
        # binary does not read never ships.
        suzatary-app = pkgs.runCommand "suzatary-${appVersion}" {
          release = ./app/release.toml;
          meta = {
            description = "Suzatary: FPL-driven todos and mail triage over the Prodrome";
            longDescription = "See docs/ARCHITECTURE.md in the Suzatary repository.";
            mainProgram = "suzatary";
          };
        } ''
          mkdir -p $out/bin $out/share/suzatary
          cp ${suzatary-bin}/bin/* $out/bin/
          cp $release $out/share/suzatary/release.toml
          [ "$($out/bin/suzatary --version)" = "suzatary ${appVersion}" ] \
            || { echo "the binary does not read its release file as ${appVersion}" >&2; exit 1; }
        '';

        # `suzatary-dev`: EVERY VERB OF THE BINARY, PLUS THE END-TO-END FIXTURE
        # AND THE TRIAGE BENCHMARK, and it is a
        # SECOND DERIVATION rather than a flag on the first for the reason
        # CLAUDE.md's Containment section gives: a flag in the shipped program
        # is a write path reachable by anything that can set an environment
        # variable on this box. `--whois-fixture` makes every peer one login,
        # which is what puts the write surface inside a browser test's reach —
        # a headless Chromium here arrives from 127.0.0.1 whatever address it
        # dialled, which no daemon names, so without it every request is refused.
        #
        # `.#suzatary` does NOT enable the feature, so the deployed program
        # does not contain the code at all. Removal is checkable; a blocklist
        # is not. `scripts/web-e2e.sh` reaches for this one by store path.
        suzatary-fixture = appCrate {
          pname = "suzatary-fixture";
          doCheck = false;
          # `suzatary-dev` alone: the one binary built with the feature.
          cargoExtraArgs = "--features fixture --bin suzatary-dev";
          meta = {
            description = "suzatary-dev: the end-to-end fixture and the triage benchmark — never deployed";
            mainProgram = "suzatary-dev";
          };
        };

        # THE CORE, FOR THE BROWSER (`nix build .#suzatary-wasm`): `app/wasm`,
        # the Prodrome's generic exports at this application's schemas,
        # compiled to WebAssembly in the derivation shape the Prodrome builds its
        # reference module with. Its dependencies are built once against the
        # lock (`buildDepsOnly`), so a `.rs` edit compiles the workspace's own
        # crates and nothing else. The wasm32 std comes with nixpkgs' rustc, which
        # links with `lld` from PATH; the `wasm-bindgen` CLI is the exact patch
        # the crate pins, since the two negotiate over a schema version compiled
        # into both. The output is the two glues, `web/` for the app and
        # `nodejs/` for scripts/web-test.sh, beside one `.wasm` each.
        #
        # The source is the manifests and the two crates the module links, so an
        # edit to the server or the CLI does not rebuild it. Panic locations are
        # remapped off the vendor directory, so no store path is in the bytes.
        wasmArgs = commonArgs // {
          pname = "suzatary-wasm";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./app/Cargo.toml
              ./app/Cargo.lock
              ./app/bench/Cargo.toml
              ./app/cli/Cargo.toml
              ./app/io/Cargo.toml
              ./app/plugin/Cargo.toml
              ./app/plugin-fence/Cargo.toml
              ./app/plugin-ics/Cargo.toml
              ./app/testkit/Cargo.toml
              ./app/core/Cargo.toml
              ./app/core/src
              # The release plan, which the core compiles in.
              ./app/core/releases.toml
              ./app/wasm
            ];
          };
          inherit cargoVendorDir;
          preBuild = ''
            for registry in "$cargoVendorDir"/*/; do
              export CARGO_BUILD_RUSTFLAGS="--remap-path-prefix=''${registry%/}=/build/cargo-vendor-dir"
            done
          '';
          nativeBuildInputs = [ pkgs.lld ];
          doCheck = false;
          buildPhaseCargoCommand = ''
            cargo build --offline --frozen \
              --profile wasm-release --target wasm32-unknown-unknown -p suzatary-wasm
          '';
        };
        # Its dependency build, keyed on the lock like `cargoArtifacts`, and exposed as
        # `.#suzatary-wasm-deps` so the merge gate can root it: a build-time input no output
        # holds, which a collection of the store took, five minutes of wasm32 dependencies each time.
        suzatary-wasm-deps = depsOf wasmArgs;
        suzatary-wasm = craneLib.mkCargoDerivation (wasmArgs // {
          cargoArtifacts = suzatary-wasm-deps;
          nativeBuildInputs = [ pkgs.lld pkgs.wasm-bindgen-cli_0_2_127 pkgs.binaryen ];
          doInstallCargoArtifacts = false;
          # The glue keeps the name `prodrome` the loader and the tests have always
          # fetched. `wasm-opt -Os` over what bindgen emits, with the proposals
          # rustc's wasm32 baseline already uses named, since wasm-opt's older
          # default refuses them rather than dropping them. `SIZES` is the module
          # before and after it, which is what a first visit pays.
          installPhaseCommand = ''
            wasm=target/wasm32-unknown-unknown/wasm-release/suzatary_wasm.wasm
            features="--enable-bulk-memory --enable-bulk-memory-opt --enable-sign-ext"
            features="$features --enable-nontrapping-float-to-int --enable-mutable-globals"
            features="$features --enable-multivalue --enable-reference-types"
            for target in web nodejs; do
              wasm-bindgen --target "$target" --out-dir "$out/$target" --out-name prodrome "$wasm"
              bound=$(stat -c %s "$out/$target/prodrome_bg.wasm")
              # shellcheck disable=SC2086
              wasm-opt -Os $features -o "$out/$target/prodrome_bg.wasm" "$out/$target/prodrome_bg.wasm"
            done
            printf 'cargo\t%s bytes\nbindgen\t%s bytes\nwasm-opt\t%s bytes\n' \
              "$(stat -c %s "$wasm")" "$bound" "$(stat -c %s "$out/web/prodrome_bg.wasm")" | tee "$out/SIZES"
          '';
          meta.description = "Suzatary's browser module: the Prodrome's exports at Suzatary's schemas, with the JS glue";
        });

        # THE PLUGIN HOST'S ADVERSARY (`nix build .#suzatary-plugin-fence`):
        # `app/plugin-fence`, a plugin whose every mode tries to break one of the
        # host's fences, as the two components `app/plugin/tests/fence.rs` loads.
        # `fence.wasm` targets the source world; `escapee.wasm` is the same
        # plugin built against a world that also imports a filesystem and a
        # network, which the host must refuse to load. Built for
        # wasm32-unknown-unknown and wrapped by `wasm-tools component new`, so
        # nothing of WASI is linked in and the components import exactly what
        # their worlds name. A test fixture, never loaded by the app; its
        # dependencies are wit-bindgen's few, so it shares no dependency build.
        suzatary-plugin-fence = craneLib.mkCargoDerivation (commonArgs // {
          pname = "suzatary-plugin-fence";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./app/Cargo.toml
              ./app/Cargo.lock
              ./app/bench/Cargo.toml
              ./app/cli/Cargo.toml
              ./app/io/Cargo.toml
              ./app/core/Cargo.toml
              ./app/plugin/Cargo.toml
              ./app/plugin-ics/Cargo.toml
              ./app/testkit/Cargo.toml
              ./app/wasm/Cargo.toml
              ./app/plugin-fence
              ./app/wit
            ];
          };
          cargoArtifacts = null;
          inherit cargoVendorDir;
          nativeBuildInputs = [ pkgs.lld pkgs.wasm-tools ];
          doCheck = false;
          doInstallCargoArtifacts = false;
          buildPhaseCargoCommand = ''
            cargo rustc --offline --frozen --release --target wasm32-unknown-unknown \
              -p suzatary-plugin-fence --crate-type cdylib
            cargo rustc --offline --frozen --release --target wasm32-unknown-unknown \
              -p suzatary-plugin-fence --crate-type cdylib --features escapee --target-dir target/escapee
          '';
          installPhaseCommand = ''
            mkdir -p "$out"
            wasm-tools component new target/wasm32-unknown-unknown/release/suzatary_plugin_fence.wasm -o "$out/fence.wasm"
            wasm-tools component new target/escapee/wasm32-unknown-unknown/release/suzatary_plugin_fence.wasm -o "$out/escapee.wasm"
          '';
          meta.description = "The plugin host's fence tests' adversary: two components, never loaded by the app";
        });

        # THE APP'S REFERENCE PLUGINS (`nix build .#suzatary-plugins`): `ics.wasm`,
        # the calendar's source (`app/plugin-ics`), beside its schema. Built for
        # wasm32-wasip2 with the plugins' toolchain, whose linker writes the
        # component; it imports `fetch` and the WASI clocks, random numbers and
        # standard streams the host links. The box's plugin timer roots this output
        # and resolves a manifest's `component` against it (`scripts/plugin-sync.sh`);
        # the fence tests load it too, so a reference plugin is held to the same
        # fences as the adversary.
        suzatary-plugins = craneWasi.mkCargoDerivation (commonArgs // {
          pname = "suzatary-plugins";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./app/Cargo.toml
              ./app/Cargo.lock
              ./app/bench/Cargo.toml
              ./app/cli/Cargo.toml
              ./app/io/Cargo.toml
              ./app/core/Cargo.toml
              ./app/plugin/Cargo.toml
              ./app/plugin-fence/Cargo.toml
              ./app/testkit/Cargo.toml
              ./app/wasm/Cargo.toml
              ./app/plugin-ics
              ./app/wit
            ];
          };
          cargoArtifacts = null;
          inherit cargoVendorDir;
          doCheck = false;
          doInstallCargoArtifacts = false;
          buildPhaseCargoCommand = ''
            cargo rustc --offline --frozen --release --target wasm32-wasip2 \
              -p suzatary-plugin-ics --crate-type cdylib
          '';
          installPhaseCommand = ''
            mkdir -p "$out"
            cp target/wasm32-wasip2/release/suzatary_plugin_ics.wasm "$out/ics.wasm"
            # Its declared schema ships beside it; the host admits it at load.
            cp "$src/app/plugin-ics/ics.schema" "$out/ics.schema"
          '';
          meta.description = "Suzatary's reference plugin as a component: ics.wasm, the calendar, beside ics.schema";
        });
        # TYPST FOR THE TAB: the Prodrome's `typst-wasm` (`compile_html`,
        # `add_font`), the build its flake makes at the revision `inputs.prodrome`
        # pins. The web build copies it beside `prodrome_bg.wasm`; the tab loads it
        # only when a Typst view is first drawn. `SIZES` prints raw and brotli bytes.
        suzatary-typst-wasm = prodrome.packages.${system}.prodrome-typst-wasm;

        # THE FRONTEND'S WIRE TYPES, GENERATED (§2, last bullet; §6.6).
        #
        # `cargo test -p suzatary-core` writes `wire.ts` from the Rust
        # declarations when `TS_RS_EXPORT_DIR` is set — ts-rs's export-on-test,
        # aimed at $out — and `payload.ts`, the decoders folded from it. A
        # DERIVATION rather than a step inside scripts/build-web.sh for the
        # reason suzatary-wasm is a derivation: that script is a bundler, Nix
        # is the build, and a frontend build that shelled out to cargo would
        # need a toolchain and a crates.io fetch in the one shell that is meant
        # to need neither.
        #
        # IT AND THE VECTORS BELOW ARE BUILT FROM THE CORE ALONE (2026-10-10). On
        # `appSrc` with the release `cargoArtifacts`, each compiled every dependency
        # again in cargo's own profiles (nine minutes on the gate's 4 cores), twice a
        # gate: the private tree's and the public one's, whose `appSrc` differs by
        # the live pins it leaves out. `coreSrc` is the manifests, `app/core` and the
        # one file its tests include from beside it, so both trees give one
        # derivation, rebuilt only when the core moves; `suzatary-core-deps` is their
        # dependencies in cargo's own dev and test profiles, keyed on the lock.
        suzatary-wire-ts = coreCrate {
          pname = "suzatary-wire-ts";
          doCheck = false;
          # This build phase is a `cargo test`, not a `cargo build`: there is
          # no build log and no binary for crane's post-build hook to install.
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            TS_RS_EXPORT_DIR=$out cargo test --offline -p suzatary-core --lib -- wire::tests::generates
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            test -s $out/wire.ts
            test -s $out/payload.ts
            echo "wire.ts: $(wc -l < $out/wire.ts) lines, payload.ts: $(wc -l < $out/payload.ts)"
            runHook postInstall
          '';
          meta.description = "The TypeScript for Suzatary's wire, generated from suzatary-core";
        };

        # THE FRONTEND BUNDLE, AS A BUILD (`todo:suzatary/servedisabuild`,
        # 2026-09-20). Before this, `instance.toml`'s `dist` named `web/dist` —
        # a gitignored DIRECTORY in the dev checkout, which is how a rebuild
        # during code review overwrote the bundle a running server was serving
        # and broke every page for two days: the binary cannot be clobbered
        # because it is content-addressed (`.#suzatary`, resolved by
        # `scripts/app-bin.sh`) and the bundle could be, because it was a
        # folder. Same doctrine as the binary and the document, applied to the
        # one build artefact that had not received it yet.
        #
        # THIS RUNS THE SAME `scripts/build-web.sh` A DEV LOOP RUNS — not a
        # second bundler invocation with different flags. That script already
        # takes its every impure input as an env var (`SUZATARY_PRODROME_WASM`,
        # `SUZATARY_WIRE_TS`) and already copies out of the read-only store and
        # `chmod u+w`s what it copies, because `serve-web.sh` was already
        # handing it `SUZATARY_WIRE_TS` from a store path before this — so
        # nothing in the script changed to make it buildable in a sandbox; a
        # derivation is one more caller supplying the same two variables and
        # reading `web/dist` back out. The innerHTML
        # doors and the `as` cast ceiling run unconditionally inside that
        # script, with no `--dev`-only skip, so they gate THIS build exactly as
        # they gate a dev one: a violation fails `nix build .#suzatary-web`,
        # not only `scripts/web-test.sh`.
        #
        # A PLAIN `stdenv.mkDerivation`, not `appCrate`: this compiles no Rust
        # at all (the wire's TypeScript arrives already generated, from
        # `suzatary-wire-ts`), so it needs esbuild/tsc/node on `PATH` and
        # nothing from `cargoArtifacts`.
        suzatary-web = pkgs.stdenv.mkDerivation {
          pname = "suzatary-web";
          version = "0";
          # web/ plus the one script that builds it — not `./.`, so an edit to
          # a todo or a briefing does not move this derivation's hash any more
          # than it moves `suzatary-app`'s.
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./web
              ./scripts/build-web.sh
              ./scripts/web-inputs.sh
              ./scripts/nix-out.sh
              ./scripts/wire.sh
              ./scripts/typst-packages.mjs

              ./scripts/web-static.sh
            ];
          };
          nativeBuildInputs = [ pkgs.esbuild pkgs.typescript pkgs.nodejs_24 pkgs.brotli ];
          SUZATARY_PRODROME_WASM = "${suzatary-wasm}";
          SUZATARY_TYPST_WASM = "${suzatary-typst-wasm}";
          SUZATARY_WIRE_TS = "${suzatary-wire-ts}";
          TYPST_PACKAGE_PATH = "${typstPackages}";
          dontConfigure = true;
          buildPhase = ''
            runHook preBuild
            bash scripts/build-web.sh
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            cp -r web/dist $out
            runHook postInstall
          '';
          meta.description = "Suzatary's web frontend bundle — scripts/build-web.sh, as a derivation";
        };

        # THE WEB TEST'S FPL/SERIES VECTORS, REGENERATED FROM THE CRATE'S OWN
        # GRAMMAR. Prodrome 0.4 (2b71293e10ced78b57ad6540ed5f8a6a9379a951, "the
        # evidence is a literal") replaced `conformance/fpl.json`/`series.json`
        # with `conformance/fpl.py`/`series.py`: the same evidence, printed in
        # the crate's own §2 grammar instead of a second JSON encoding of it.
        # `web/test/wasm.test.ts` has no parser for that grammar and should not
        # grow one — a second reading of somebody else's format is exactly
        # what this repository refuses elsewhere (typst's HTML, the store's
        # own objects) — so `app/core/examples/vector_json.rs` reads the two
        # files with the crate's OWN parser (`literal::parse_literal` under
        # `literal::Open`, the same door `app/cli/tests/live.rs` reads
        # `events/objects/` through) and re-emits the JSON shape the test has
        # always consumed. `env`/`json` are copied through from
        # `prodrome-wasm`'s own frozen `wasm/conformance/term-json.json`
        # (the JSON codec's evidence, unchanged since 0.4 moved it) rather than
        # re-derived, so that half has one reading too; only `samples` (fpl)
        # and the whole of `series` — neither ever touched by the JSON codec —
        # are converted here. CI's tests.yml points
        # `SUZATARY_PRODROME_CONFORMANCE` HERE; scripts/web-test.sh without it
        # runs the same two `cargo run`s in the working tree.
        suzatary-vector-json = coreCrate {
          pname = "suzatary-vector-json";
          doCheck = false;
          # A `cargo run` of an example: nothing for the post-build hook.
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            cargo run --offline -p suzatary-core --example vector_json -- \
              fpl ${prodrome}/conformance/fpl.py \
              ${prodrome}/wasm/conformance/term-json.json \
              > $out/fpl.json
            cargo run --offline -p suzatary-core --example vector_json -- \
              series ${prodrome}/conformance/series.py \
              > $out/series.json
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            test -s $out/fpl.json
            test -s $out/series.json
            echo "fpl.json: $(wc -c < $out/fpl.json) bytes; series.json: $(wc -c < $out/series.json) bytes"
            runHook postInstall
          '';
          meta.description = "fpl/series conformance vectors, regenerated from prodrome's own literal grammar for web/test/wasm.test.ts";
        };

        # `nix build .#suzatary-junk-seed` — THE MONTHLY JUNK SWEEP'S CHURN
        # SEED (the CLAUDE history's grep point: 2026-09-27, "the right way
        # is to use one of those lists of spam websites that are available on
        # GitHub as the seed + ours"). Normalises two maintained public lists —
        # `disposable-email-domains`'s single file, and four of
        # `blocklistproject/Lists`' `alt-version/*-nl.txt` files, each `#`
        # comments then domains, one per line (checked against the fetched
        # source, 2026-09-27) — into ONE sorted, deduplicated `domains.txt`
        # plus a `sources.txt` naming each source's rev and the count it
        # contributed. `SUZATARY_JUNK_SEED` (`nix/nixos-module.nix`'s
        # `junkSeed` option) points a deployment at this derivation's
        # `domains.txt`; the binary reads it fresh each time a digest runs
        # (`suzatary_io::junk::OutlookJunk::churn`), never resolving or
        # holding it here. NO NETWORK AT RUNTIME: both sources are fetched by
        # the flake input mechanism at evaluation time, locked by hash in
        # `flake.lock`, and this build reads them from the store like any
        # other input.
        #
        # `cargo run --example junk_seed`, the same shape as
        # `suzatary-vector-json` above: a pure Rust reader
        # (`app/core/examples/junk_seed.rs`, over
        # `suzatary_core::mail::junk::normalize_seed_file`) rather than a shell
        # pipeline, so the ONE normaliser a digest's own runtime types trust is
        # exactly the one this build ran, and it is unit-tested in
        # `app/core/src/mail/junk.rs` like anything else in that crate.
        suzatary-junk-seed = appCrate {
          pname = "suzatary-junk-seed";
          doCheck = false;
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            cargo run --offline -p suzatary-core --example junk_seed -- $out \
              disposable-email-domains ${disposable-email-domains.rev} \
                ${disposable-email-domains}/disposable_email_blocklist.conf \
              blocklistproject-phishing ${blocklist-project.rev} \
                ${blocklist-project}/alt-version/phishing-nl.txt \
              blocklistproject-scam ${blocklist-project.rev} \
                ${blocklist-project}/alt-version/scam-nl.txt \
              blocklistproject-fraud ${blocklist-project.rev} \
                ${blocklist-project}/alt-version/fraud-nl.txt \
              blocklistproject-abuse ${blocklist-project.rev} \
                ${blocklist-project}/alt-version/abuse-nl.txt
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            test -s $out/domains.txt
            test -s $out/sources.txt
            echo "domains.txt: $(wc -l < $out/domains.txt) domains"
            cat $out/sources.txt
            runHook postInstall
          '';
          meta.description = "Suzatary's Junk churn seed: disposable-email-domains + blocklistproject/Lists, normalised into one domains.txt";
        };

        # The Typst packages every compile here may import, as a package path
        # (`<path>/local/typst-design/<version>`): the shells export it as
        # `TYPST_PACKAGE_PATH`, and `scripts/build-web.sh` bundles the same
        # files for the tab.
        typstPackages = typst-design.lib.packagePath pkgs;

        # A CHECK IS A VERDICT, NOT AN ARTEFACT — `touch $out`, exactly as the
        # Prodrome's two above.
        fontPaths = [
          "${pkgs.libertinus}/share/fonts/opentype"
          # the working document's body face (typst-design): the CV's and the site's.
          "${pkgs.eb-garamond}/share/fonts"
          "${pkgs.font-awesome_5}/share/fonts/truetype"
          "${pkgs.font-awesome_5}/share/fonts/opentype"
          "${pkgs.merriweather}/share/fonts"
          "${pkgs.crimson-pro}/share/fonts"
          "${pkgs.source-serif}/share/fonts"
          "${pkgs.jost}/share/fonts"
          "${pkgs.libre-baskerville}/share/fonts"
          "${pkgs.lato}/share/fonts"
        ];

        # THE RUST TOOLCHAIN app/ BUILDS WITH, SHARED. Defined once so `.#rust`
        # and `.#triage` cannot drift by a hand-edit to just one of them — see
        # devShells.triage below for why `.#triage` needs this now. `.#rust`
        # adds rust-analyzer, the editor's, and takes nothing else; `.#triage`
        # takes just this subset.
        rustToolchain = {
          # `lld` is what the wasm32 target links with: nixpkgs' rustc carries
          # the wasm32-unknown-unknown std (checked with `rustc --print
          # target-list` and `ls $(rustc --print sysroot)/lib/rustlib/`, 2026-09-06),
          # so no rust-overlay and no fenix — but it expects the linker on PATH
          # rather than bundling rust-lld, and without it the build stops at
          # "linker `lld` not found".
          packages = with pkgs; [ cargo rustc rustfmt clippy lld mold ];
          RUST_BACKTRACE = "1";
          # THE SHELLS' BUILDS LINK WITH MOLD. A test build links a dozen executables
          # of the whole application each, and GNU ld spent 322 of the 907 seconds
          # of rustc in a warm gate's test build linking them (one of 73 s); the
          # code linked is the same. Target-specific rustflags, so a `--target
          # wasm32` build is untouched; the Nix derivations never read a shell.
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
        };

        # WHAT THE WORKSPACE'S TESTS RUN BESIDE THE TOOLCHAIN, in every shell
        # that runs them: typst renders the site's pages and the answer blocks,
        # and poppler reads a PDF's text layer. Without them in `.#rust`, CI's
        # test job fell back to `nix develop` from a scratch directory and
        # failed where the box's gate, in `.#triage`, passed.
        testTools = {
          packages = with pkgs; [ typst poppler-utils ];
          TYPST_FONT_PATHS = pkgs.lib.concatStringsSep ":" fontPaths;
          TYPST_PACKAGE_PATH = "${typstPackages}";
        };
      in
      {
        packages = {
          # `nix run .#roots` — build the app `$SUZATARY_USER_REPO/flake.lock`
          # names and link it, which every other service runs from.
          roots = services.roots;
          # `nix run .#web` — the read-only chain browser on the tailnet.
          web = services.web;
          # `nix build .#apk-unsigned` — the generic portal, which asks for its
          # box at first launch, with no network and no Gradle. Unsigned because a key in the store is a key
          # published; `nix run .#apk-sign -- <in> <out>` is the impure half.
          apk-unsigned = apk.unsigned (apkProfile ./android/profile/generic.toml);
          # `SUZATARY_APK_PROFILE=<file> nix build --impure
          # .#apk-profiled-unsigned` — the portal one profile names.
          apk-profiled-unsigned = apkNamed apk.unsigned;
          # `SUZATARY_APK_PROFILE=<file> nix build --impure
          # .#apk-tsnet-unsigned` — a SPIKE: the same portal joining
          # the tailnet itself through an embedded tsnet node, installed beside
          # the portal as com.bmabsout.suzatary.tsnet
          # (the `tsnetportal-android-spike-x2b41w` report).
          apk-tsnet-unsigned = apkNamed apk.tsnet;
          apk-sign = apk.sign;
          # `nix build .#suzatary` — the application (docs/ARCHITECTURE.md).
          suzatary = suzatary-app;
          # `nix build .#suzatary-bin` — its binary alone, which no release moves.
          inherit suzatary-bin;
          # `nix build -o <root> .#suzatary-deps` — the dependency build every
          # derivation above starts from, exposed so it can be ROOTED
          # (2026-09-13). It is a build-time input, which no `result` link of
          # a package keeps alive, and the box's daemon collects below 3 GB
          # free: the remote builder's first warm run spent 1007 s
          # re-vendoring because the previous run's artifacts had already
          # been collected. Root it per architecture — this box's own, and
          # the builder's x86_64 — and a lock change is the only thing that
          # rebuilds it.
          suzatary-deps = cargoArtifacts;
          # The other two dependency builds, the browser module's and the core's
          # alone (the wire's and the vectors'), exposed to be rooted the same way.
          inherit suzatary-wasm-deps suzatary-core-deps;
          # `nix build .#suzatary-wire-ts` — the frontend's wire declarations,
          # generated from suzatary-core rather than mirrored by hand.
          inherit suzatary-wire-ts;
          # `nix build .#suzatary-web` — the frontend bundle itself, the same
          # `scripts/build-web.sh` run produces, as a derivation. Resolved and
          # rooted by `scripts/web-dist.sh`, the way `.#suzatary` is by
          # `scripts/app-bin.sh`.
          inherit suzatary-web;
          # `nix build .#suzatary-vector-json` — the fpl/series conformance
          # vectors `web/test/wasm.test.ts` reads, regenerated from prodrome's
          # own literal grammar (see the derivation's own comment).
          inherit suzatary-vector-json;
          # `nix build .#suzatary-wasm` — the browser's module, exposed so
          # scripts/build-web.sh and scripts/web-test.sh can resolve it by name.
          inherit suzatary-wasm;
          # `nix build .#suzatary-plugin-fence` — the components the plugin
          # host's fence tests load; resolved by scripts/merge-gate.sh.
          inherit suzatary-plugin-fence;
          # `nix build .#suzatary-plugins` — the app's reference plugins
          # (`ics.wasm`), rooted and loaded by scripts/plugin-sync.sh.
          inherit suzatary-plugins;
          # `nix build .#suzatary-typst-wasm` — Typst for the tab, resolved by
          # scripts/build-web.sh the same way.
          inherit suzatary-typst-wasm;
          # `nix build .#suzatary-fixture` — `suzatary-dev`, the binary with
          # the end-to-end fixture and the benchmark. NEVER DEPLOYED; the
          # comment on the derivation says why it is a second one.
          inherit suzatary-fixture;
          # `nix build .#suzatary-junk-seed` — the monthly Junk sweep's churn
          # seed, normalised from two maintained public lists. See the
          # derivation's own comment.
          inherit suzatary-junk-seed;
          # `nix build .#docs` — the guide (docs/guide): its HTML compiled in the
          # app's closed world and its PDF by typst with the e2e suite's pictures,
          # around the cast's deployment as `init` writes it. Reads no network.
          docs = pkgs.runCommand "suzatary-docs" {
            nativeBuildInputs = [ pkgs.typst ];
            TYPST_FONT_PATHS = lib.concatStringsSep ":" fontPaths;
            TYPST_PACKAGE_PATH = "${typstPackages}";
            guide = ./docs/guide;
            images = ./docs/images;
          } ''
            mkdir -p $out
            ${suzatary-fixture}/bin/suzatary-dev guide --from $guide --out $out
            cp $images/*.png $out/
            typst compile --root $out $out/guide.typ $out/guide.pdf
          '';
        };

        # `nix flake check` runs THE APPLICATION'S suites and nothing else.
        #
        # It used to run the database's too — SPEC §9 against
        # `prodrome/conformance/*.json`, and clippy over that workspace. Both
        # are gone, and not because they stopped mattering: they are
        # https://github.com/fulfilium/prodrome's own CI now, and that CI ran on
        # the commit `inputs.prodrome` pins before this repository could pin it.
        # Re-running them here would compile the same crate a second time to
        # re-state a verdict already taken, which is the duplication the split
        # was for — and the seeded vectors they read are not even in this tree
        # any more. What did NOT go upstream is the LIVE chain's pins, and those
        # are `app/cli/tests/live.rs`, run by the merge gate's live half because the
        # sandbox has no `events/` to run them against.
        checks = {
          # The NixOS module, evaluated into a host's configuration, and the
          # facts a deploy rests on read off it (nix/module-check.nix).
          suzatary-module = import ./nix/module-check.nix {
            inherit nixpkgs system;
            module = self.nixosModules.default;
          };
          # `cargo test` over the workspace (docs/ARCHITECTURE.md §2): the
          # escape's laws, the capability bundles' shapes, the wire's
          # declarations — and clippy at `-D warnings` over `--all-targets`.
          # Both start from `cargoArtifacts` (crane, 2026-09-13): the tests
          # compile the four workspace crates in the test profile, clippy
          # lints them, and neither builds a dependency.
          suzatary-core = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
            pname = "suzatary-core";
          });
          suzatary-clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            pname = "suzatary-clippy";
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });
          # The plugin host's fences (`app/plugin/tests/fence.rs`), a `test =
          # false` target run by name with the components it attacks with.
          # Loopback only: the test's own server on 127.0.0.1, which the build
          # sandbox has.
          suzatary-plugin-fence = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
            pname = "suzatary-plugin-fence";
            cargoTestExtraArgs = "-p suzatary-plugin --test fence";
            SUZATARY_FENCE_PLUGINS = suzatary-plugin-fence;
            SUZATARY_PLUGINS = suzatary-plugins;
          });
          # `suzatary plugin sync` over a loopback feed, against the same
          # reference components (`app/cli/tests/plugin.rs`), run by name.
          suzatary-plugin-sync = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
            pname = "suzatary-plugin-sync";
            cargoTestExtraArgs = "-p suzatary --test plugin";
            SUZATARY_PLUGINS = suzatary-plugins;
          });
        };

        apps.apk-sign = {
          type = "app";
          program = "${apk.sign}/bin/suzatary-sign-apk";
          meta.description = "Sign an unsigned Suzatary APK from $SUZATARY_KEYSTORE";
        };
        # `nix run <this app at a revision> -- <verb>`, which a user repository's
        # `./suzatary` runs: the binary with this source as its code root, the
        # same store path a unit's wrapper gives it, so its scripts and
        # secretspec.toml are the ones it was built with.
        apps.default = {
          type = "app";
          program = toString (pkgs.writeShellScript "suzatary" ''
            export SUZATARY_REPO=${self}
            exec ${suzatary-app}/bin/suzatary "$@"
          '');
          meta.description = "Suzatary, with this source as its code root";
        };

        # Single operator env for the triage code + suzatary CLI: stdlib Python,
        # notmuch (read the store), lieer (Gmail sync), isync/mbsync (IMAP sync,
        # e.g. a university's Exchange), plus openssl/curl for diagnostics.
        # Enter with: nix develop .#triage
        #
        # `.#rust` is the EDITOR's shell for `app/`: the same toolchain the
        # checks above build with, plus a language server. It was written when
        # `prodrome/` was a second workspace here needing a cargo of its own;
        # that workspace is its own repository now (§8.8, and it ships a
        # `devShells.default` of its own for anyone working ON it), so what is
        # left is one workspace and one shell to edit it in.
        devShells.rust = pkgs.mkShell {
          # `rust-analyzer` is the editor's. Nothing else — no pkg-config, no
          # OpenSSL, no interpreter: rustls, never OpenSSL (ARCHITECTURE §2),
          # and there is no build script here that asks a Python about itself.
          packages = rustToolchain.packages ++ testTools.packages ++ [ pkgs.rust-analyzer ];
          inherit (rustToolchain) RUST_BACKTRACE CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS
            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS;
          inherit (testTools) TYPST_FONT_PATHS TYPST_PACKAGE_PATH;
        };

        devShells.triage = let
          # The variables this shell sets that a gate's steps read (typst's
          # fonts and packages, the Rust flags): scripts/train.sh refuses a
          # gate where such a variable came from anywhere else, and reads
          # which are the shell's from SUZATARY_SHELL_SETS.
          sets = {
            # typst and the fonts: the pages, the site and the guide compile in
            # this shell.
            inherit (testTools) TYPST_FONT_PATHS TYPST_PACKAGE_PATH;
            inherit (rustToolchain) RUST_BACKTRACE CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS
              CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS;
          };
        in pkgs.mkShell (sets // {
          # scripts/train.sh gates only inside this shell.
          SUZATARY_SHELL = "triage";
          SUZATARY_SHELL_SETS = builtins.concatStringsSep " " (builtins.attrNames sets);
          # The browser's fonts are the shell's, not the machine's: a Codespace
          # has none, and chromium dies in Skia's font manager with nothing to
          # fall back to. DejaVu Sans alone is the one font this box holds, so
          # the e2e suite measures the same text on both.
          FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts.minimal ]; };
          # A SHELL IS TOOLS, NOT BUILDS (2026-09-29). Everything here comes
          # from nixpkgs, so entering it downloads from the binary cache and
          # builds nothing of this repository's; it used to export the
          # Prodrome's wasm and the conformance vectors as JSON, which made a
          # fresh session compile the whole dependency tree before `true`
          # could run. The scripts that read those (scripts/build-web.sh,
          # scripts/web-test.sh) resolve them when they run, and a caller
          # that has them already (the `suzatary-web` derivation, CI's
          # tests.yml, scripts/merge-gate.sh) passes them in.
          # Every command in this repo runs through this shell, so this is
          # where the commit-msg hook gets wired — not a README step and not
          # a value some tool can clobber for a day (memory/, decision hookspath:
          # the worktree harness kept setting it absolute). Relative resolves
          # against the worktree root, so it is right in every worktree.
          shellHook = ''
            if git rev-parse --is-inside-work-tree >/dev/null 2>&1 \
               && [ -d scripts/hooks ] \
               && [ "$(git config core.hooksPath 2>/dev/null)" != scripts/hooks ]; then
              git config core.hooksPath scripts/hooks
            fi
          '';
          packages = with pkgs; [
            # esbuild/typescript/nodejs: the TypeScript frontend (web/) — one
            # bundle, no npm, tsc as the type gate, node for the unit tests.
            #
            # ⚠️ NO INTERPRETER HERE ANY MORE (stage 6). This shell carried
            # `python3.withPackages (msal, httpx, prodrome-py)` for as long as
            # there was a Python application to run; there is not. `node` also
            # reads the two JSON documents `scripts/hourly-triage.sh` used to
            # read with `python3 -c`, and the end-to-end fixture's chain is a
            # verb of `suzatary-dev`, behind the `fixture` feature.
            # ⚠️ `nodejs_24` BY NAME, not the unversioned alias. Three places
            # assert node 24 out loud — scripts/web-test.sh (`--test` semantics
            # and `crypto.subtle`), scripts/web-e2e.sh (a global `WebSocket` is
            # the whole CDP client) and the browser comment below — and an
            # alias moves when nixpkgs moves, with only a test failure to say
            # so. The Rust toolchain is explicit; this was the one tool in the
            # shell whose version was a coincidence.
            esbuild typescript nodejs_24
            # brotli: scripts/build-web.sh precompresses the frontend at
            # quality 11, so the server never compresses a file per request.
            brotli
            # THE BROWSER IS PART OF THE TOOLCHAIN (2026-09-04). The frontend's
            # unit tests run its pure modules under node; until now nothing
            # rendered a page, so the screenshots an agent took by eye were the
            # UI's only test suite. `scripts/web-e2e.sh` drives this headless
            # over CDP — node 24's global WebSocket is the whole client, so the
            # browser is a pinned package and not an npm dependency tree.
            chromium
            # typst and poppler-utils come with testTools, below.
            # The type and lint gates are `cargo clippy` and `cargo fmt` now;
            # `ty` and `ruff` checked a package that no longer exists.
            notmuch lieer isync openssl curl
            muchsync  # replicas: sync the notmuch store (messages + tags) to other machines over SSH; the fetcher stays the only one
            protonmail-bridge pass gnupg  # Proton: Bridge serves localhost IMAP; pass/gnupg back its keyring on this headless box
            # Resolves the static secrets declared in secretspec.toml out of the
            # `pass` store, so no password sits in plaintext in a config file.
            # NOT the `keyring` provider: it needs a DBus secret-service and
            # dies headless with "Unable to autolaunch a dbus-daemon without a
            # $DISPLAY". `pass` works here because Proton Bridge already
            # required a passphraseless GPG key to back its own keyring.
            secretspec
            # Plugins are WebAssembly components (docs/PLUGINS-PLAN-2026-10-02.md):
            # `wasm-tools component new` wraps a guest, `component wit` reads
            # back what one imports.
            wasm-tools
            # util-linux: scripts/train.sh's flock, and the `script` that gives
            # the remote gate's keep-alive session its pty.
            util-linux
            # The leak guard's secret scanner (scripts/public-tree.sh): a
            # maintained rule set, run offline over a directory, never
            # verifying a finding against its issuer.
            gitleaks
            # actionlint: the guards lint .github/workflows/gate.yml, the workflow core runs a
            # train's tree half with (app/cli/tests/guards/gate_yml.rs).
            actionlint
            # cargo-sweep: a Codespace's merge gate drops the target's units no
            # gate has read lately, where it used to clear the whole target.
            cargo-sweep
          ] ++ rustToolchain.packages ++ testTools.packages;
        });
      }
    );
}
