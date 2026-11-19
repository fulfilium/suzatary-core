{
  description = "Suzatary: a self-hosted secretary over the Prodrome";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    # THE DATABASE, AS AN INPUT (docs/ARCHITECTURE.md §2), public under MIT OR Apache-2.0.
    #
    # PINNED TO A COMMIT, the SAME one `app/Cargo.toml` names: the binary links the crate and
    # the browser fetches the wasm from this input, and two shas would be two readings of one
    # spec. Moving the pin is three places in one commit: this url (with `flake.lock`), the
    # `rev`s in `app/Cargo.toml`, and `app/Cargo.lock`.
    prodrome.url = "github:fulfilium/prodrome/42f526fb7d9d274d0a9668999f9c5506d024ddee";
    # One nixpkgs in the closure: this pin carries what the Prodrome's build needs, `cargo`/`rustc`
    # and `wasm-bindgen-cli_0_2_127` (in lockstep with the `=0.2.127` crate).
    prodrome.inputs.nixpkgs.follows = "nixpkgs";
    prodrome.inputs.flake-utils.follows = "flake-utils";
    prodrome.inputs.crane.follows = "crane";
    # Its Typst package's look, the same design system as ours (below).
    prodrome.inputs.typst-design.follows = "typst-design";

    # THE RUST BUILD IS CRANE: the dependencies are one derivation keyed on Cargo.lock alone
    # (`cargoArtifacts` below), which the store cache carries across runs, so every build after
    # it compiles the workspace crates and nothing else.
    crane.url = "github:ipetkov/crane";

    # THE LOOK, AS AN INPUT: the owner's design system, the one source of every colour, mark and
    # template Suzatary sets in Typst, as `@local/typst-design:<version>` on `TYPST_PACKAGE_PATH`
    # and beside the tab's kit (`typstPackages` below), so the box and the tab compile against one
    # package. Pinned by commit.
    typst-design.url = "github:bmabsout/typst-design/79d9dbafcd6457f269c3cd8b6f88c0ac27e35851";
    typst-design.inputs.nixpkgs.follows = "nixpkgs";

    # THE JUNK CHURN SEED'S TWO SOURCES: public spam-domain lists. `flake = false`: each is a
    # plain tree, locked by hash in `flake.lock`.
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
    # added.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, prodrome, crane, typst-design, disposable-email-domains, blocklist-project, rust-overlay }:
    let
      inherit (nixpkgs) lib;

      # THE APP'S VERSION IS DERIVED FROM THE TREE, NEVER TYPED TWICE. versionName is
      # `app/release.toml`'s version plus the commit, so a phone's About screen names one;
      # versionCode is the commit count, which only grows on master, and 0 for a dirty tree, so
      # a hand-built APK never installs over CI's. No crate compiles the release file:
      # `suzatary-app` installs it beside the binary, which reads it at run time, so a train
      # rebuilds no binary.
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
    # System-independent outputs live outside eachDefaultSystem. The NixOS module is consumed by
    # the host's configuration, which takes this repo as a `path:` flake input.
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
          # claude-code is unfree. Named explicitly rather than a blanket allowUnfree, so this
          # stays a decision about one package the scheduled jobs need.
          config.allowUnfreePredicate = pkg:
            builtins.elem (nixpkgs.lib.getName pkg) [ "claude-code" ];
        };

        # What the background jobs run, without systemd, so `nix run .#web` and the NixOS unit
        # invoke one store path.
        services = import ./nix/services.nix { inherit pkgs; };

        # The Android portal. It takes `nixpkgs`, not `pkgs`: the SDK is unfree, and nix/apk.nix
        # imports its own pkgs so that permission stays in the one file that needs it.
        apk = import ./nix/apk.nix {
          inherit nixpkgs system;
          src = ./android;
          icon = ./web/static/icon-512.png;
          signScript = ./scripts/sign-apk.sh;
          versionCode = appVersionCode;
          versionName = appVersionName;
        };
        # An APK is a function of its PROFILE: the generic one names no deployment; an impure build
        # reads a person's from `SUZATARY_APK_PROFILE`, and fails without it at build time, not
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


        # ===== THE APPLICATION (docs/ARCHITECTURE.md); above it, scaffolding and the ONE line
        # that takes the database, the Prodrome, off the input — one direction (§1, §2).

        # The model crates, the application's library (`merge_gate.rs`'s `LIBRARY_CRATES`): each
        # fileset below names the part of each it reads, so a new crate is one word here.
        library = part: map (crate: ./app + "/${crate}${part}") [ "base" "record" "release" "store"
          "signatures" "todo" "held" "memory" "mail" "health" "triage" "services" "google" "connect"
          "inbox" "conversations" "lines" "topics" "digest" "cards" "secretary" "api" ];
        # The test supports their libraries include, of the crates that have one.
        supports = lib.filter builtins.pathExists (library "/tests/support");

        # THE SOURCE IS `app/` AND THE FILES ITS TESTS READ: a `fileset`, so a briefing, a commit
        # or a document does not change this hash, and `target/` never reaches the store.
        appSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions ([
            ./app/Cargo.toml
            ./app/Cargo.lock
            ./app/cli
            ./app/io
            ./app/server
            ./app/plugin
            ./app/plugin-fence
            ./app/plugin-ics
            ./app/testkit
            ./app/script-guards
            ./app/viewtree
            ./app/pages
            ./app/wasm
            # The plugin worlds the host binds at compile time.
            ./app/wit
            # The site vouches for the Android app, and a test pins the file
            # that says so; the sandbox has to hold it to run that test.
            ./web/static/.well-known/assetlinks.json
            # The app's own stylesheets, which `prodrome pages` writes beside a store's pages.
            ./web/static/tokens.css
            ./web/static/style.css
            # THE SAMPLE, NOT THE DEPLOYMENT: `tests/init.rs` pins it as `suzatary init`'s output;
            # the tests that read this box's deployment skip when it is absent, as it is here.
            ./instance.sample.toml
            # The colour roles a picture of a public store is drawn in (`prodrome draw`).
            ./tokens.typ
          ] ++ library "");
        };

        # THE BUILD STAMP IS `appSrc`'s hash, so `GET /api/about`'s `build` moves with the
        # binary's inputs, never on a chain write or a doc.
        appSrcBuild = builtins.substring 0 8 (builtins.baseNameOf (toString appSrc));

        # ONE DEPENDENCY BUILD FOR EVERY DERIVATION BELOW, against a dummy copy of the crates, so
        # its hash moves only with `app/Cargo.lock` or the toolchain. Crane restores it into
        # `./target`, inside `app/`, where cargo writes, so nothing rebuilds.
        craneLib = crane.mkLib pkgs;
        # The plugins' toolchain: rustup's 1.98.1, nixpkgs' version at this lock, with the
        # wasm32-wasip2 std, whose linker writes the component itself.
        craneWasi = craneLib.overrideToolchain
          ((rust-overlay.lib.mkRustBin { } pkgs).stable."1.98.1".minimal.override {
            targets = [ "wasm32-wasip2" ];
          });
        # Every phase runs INSIDE `app/` (`sourceRoot`): crane's install hook runs `cargo
        # metadata` from the working directory and takes no `--manifest-path`. The tree above is
        # still unpacked, so the files the tests read by `../../` stay put.
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

        # THE MODEL CRATES ALONE: the manifests, the library's sources and what
        # they compile in (the calendar's schema, the test supports their unit
        # tests include), and the examples. What the wire's generator and the
        # conformance vectors compile, so an edit to the server, the CLI, a live
        # pin or a crate's integration tests moves neither.
        modelFiles = lib.fileset.unions ([
          ./app/Cargo.toml
          ./app/Cargo.lock
          ./app/cli/Cargo.toml
          ./app/io/Cargo.toml
          ./app/server/Cargo.toml
          ./app/plugin/Cargo.toml
          ./app/plugin-fence/Cargo.toml
          ./app/plugin-ics/Cargo.toml
          ./app/plugin-ics/ics.schema
          ./app/testkit/Cargo.toml
          ./app/script-guards/Cargo.toml
          ./app/viewtree/Cargo.toml
          ./app/pages/Cargo.toml
          ./app/wasm/Cargo.toml
          ./app/todo/examples
        ] ++ library "/Cargo.toml" ++ library "/src" ++ supports);
        modelSrc = lib.fileset.toSource { root = ./.; fileset = modelFiles; };
        # In cargo's own profiles (`CARGO_PROFILE` empty: no `--release`), since
        # the generators are a `cargo test` and a `cargo run`, and with the
        # generators' own feature sets (`-p suzatary-api`, `-p suzatary-todo`):
        # built against the release, whole-workspace `cargoArtifacts`, they
        # compiled every dependency again.
        modelArgs = commonArgs // {
          src = modelSrc;
          CARGO_PROFILE = "";
        };
        # `.#suzatary-model-deps`: the dependencies of the wire's generator (the api crate's unit
        # tests) and of the todo crate's examples, built once per lock and rooted by the merge gate.
        suzatary-model-deps = depsOf (modelArgs // {
          pname = "suzatary-model";
          buildPhaseCargoCommand = "cargo build --locked -p suzatary-todo --examples";
          checkPhaseCargoCommand = "cargo test --locked -p suzatary-api --lib --no-run";
        });
        modelCrate = args: craneLib.buildPackage (modelArgs // { cargoArtifacts = suzatary-model-deps; } // args);

        # `nix build .#suzatary-bin`, the one binary: vendored from app/Cargo.lock, offline, no
        # OpenSSL. `doCheck` is off: the laws are the merge gate's `test` step.
        #
        # IT DOES NOT READ THE RELEASE (`appSrc` lacks `app/release.toml`), so a release commit
        # recompiles nothing (scripts/release-free.sh is the gate's proof).
        suzatary-bin = appCrate {
          pname = "suzatary";
          doCheck = false;
          # The revision `GET /api/about` shows: `appSrcBuild`, a hash of this binary's own sources.
          # A binary cargo built outside the flake says `dev`.
          SUZATARY_BUILD = appSrcBuild;
          meta = {
            description = "Suzatary's binary, without its release; `.#suzatary` installs both";
            mainProgram = "suzatary";
          };
        };

        # `nix build .#suzatary` — the application (docs/ARCHITECTURE.md): the binary above,
        # COPIED (a link would resolve to `suzatary-bin`, which holds no release), with
        # `app/release.toml` beside it at `share/suzatary/release.toml`, read at run time. A
        # release commit rebuilds this copy alone, and `--version` runs before it ships. Door 1
        # runs `libexec/suzatary/gitleaks` over `share/suzatary/secret-rules.toml`
        # (`suzatary_cli::stamp`), so no unit's PATH needs gitleaks. `prodrome draw` imports the
        # design system from `share/suzatary/typst-packages`.
        suzatary-app = pkgs.runCommand "suzatary-${appVersion}" {
          release = ./app/release.toml;
          rules = ./scripts/secret-rules.toml;
          meta = {
            description = "Suzatary: FPL-driven todos and mail triage over the Prodrome";
            longDescription = "See docs/ARCHITECTURE.md in the Suzatary repository.";
            mainProgram = "suzatary";
          };
        } ''
          mkdir -p $out/bin $out/share/suzatary $out/libexec/suzatary
          cp ${suzatary-bin}/bin/* $out/bin/
          cp $release $out/share/suzatary/release.toml
          cp $rules $out/share/suzatary/secret-rules.toml
          ln -s ${pkgs.gitleaks}/bin/gitleaks $out/libexec/suzatary/gitleaks
          ln -s ${typstPackages} $out/share/suzatary/typst-packages
          [ "$($out/bin/suzatary --version)" = "suzatary ${appVersion}" ] \
            || { echo "the binary does not read its release file as ${appVersion}" >&2; exit 1; }
        '';

        # `suzatary-dev`: EVERY VERB PLUS THE END-TO-END FIXTURE, as a SECOND DERIVATION, not a
        # flag, which would be a write path for anything that sets an environment variable.
        # `--whois-fixture` makes every peer one login (headless Chromium arrives from 127.0.0.1,
        # which no daemon names); `.#suzatary` does not contain the code. `scripts/web-e2e.sh`
        # reaches for it by store path.
        suzatary-fixture = appCrate {
          pname = "suzatary-fixture";
          doCheck = false;
          # `suzatary-dev` alone: the one binary built with the feature.
          cargoExtraArgs = "--features fixture --bin suzatary-dev";
          meta = {
            description = "suzatary-dev: the end-to-end fixture — never deployed";
            mainProgram = "suzatary-dev";
          };
        };

        # THE BROWSER MODULES: `package` compiled to wasm32 from the manifests, the model crates and
        # `crates` (a server edit rebuilds none), the deploy's build or the gate's (no LTO, no
        # wasm-opt). Dependencies build once per lock; the glues are `web/` and `nodejs/`; panic
        # locations are remapped off the vendor directory. `app/wasm` and the pages' are its
        # instances.
        wasmBuilds = {
          optimised = { profile = "wasm-release"; suffix = ""; wasmOpt = true; };
          development = { profile = "wasm-dev"; suffix = "-dev"; wasmOpt = false; };
        };
        wasmModule = build: { pname, package, name, crates, description }: let
          inherit (wasmBuilds.${build}) profile wasmOpt;
          args = commonArgs // {
            pname = pname + wasmBuilds.${build}.suffix;
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions ([
                ./app/Cargo.toml
                ./app/Cargo.lock
                ./app/cli/Cargo.toml
                ./app/io/Cargo.toml
                ./app/server/Cargo.toml
                ./app/plugin/Cargo.toml
                ./app/plugin-fence/Cargo.toml
                ./app/plugin-ics/Cargo.toml
                ./app/testkit/Cargo.toml
                ./app/script-guards/Cargo.toml
                ./app/viewtree/Cargo.toml
                ./app/pages/Cargo.toml
                ./app/wasm/Cargo.toml
              ] ++ library "/Cargo.toml" ++ library "/src" ++ crates);
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
                --profile ${profile} --target wasm32-unknown-unknown -p ${package}
            '';
          };
          # Its dependency build, keyed on the lock like `cargoArtifacts`, exposed so the merge
          # gate can root it: a build-time input no output holds, which a collection of the
          # store took, five minutes of wasm32 dependencies each time.
          deps = depsOf args;
        in {
          inherit deps;
          module = craneLib.mkCargoDerivation (args // {
            cargoArtifacts = deps;
            nativeBuildInputs = [ pkgs.lld pkgs.wasm-bindgen-cli_0_2_127 pkgs.binaryen ];
            doInstallCargoArtifacts = false;
            # `wasm-opt -Os` over what bindgen emits, naming the proposals rustc's wasm32 baseline
            # uses, which wasm-opt's default refuses. `SIZES` is the module before and after it.
            installPhaseCommand = ''
              wasm=target/wasm32-unknown-unknown/${profile}/${builtins.replaceStrings ["-"] ["_"] package}.wasm
              features="--enable-bulk-memory --enable-bulk-memory-opt --enable-sign-ext"
              features="$features --enable-nontrapping-float-to-int --enable-mutable-globals"
              features="$features --enable-multivalue --enable-reference-types"
              for target in web nodejs; do
                wasm-bindgen --target "$target" --out-dir "$out/$target" --out-name ${name} "$wasm"
                bound=$(stat -c %s "$out/$target/${name}_bg.wasm")
              '' + lib.optionalString wasmOpt ''
                # shellcheck disable=SC2086
                wasm-opt -Os $features -o "$out/$target/${name}_bg.wasm" "$out/$target/${name}_bg.wasm"
              '' + ''
              done
              printf 'cargo\t%s bytes\nbindgen\t%s bytes\n' "$(stat -c %s "$wasm")" "$bound" | tee "$out/SIZES"
              '' + lib.optionalString wasmOpt ''
              printf 'wasm-opt\t%s bytes\n' "$(stat -c %s "$out/web/${name}_bg.wasm")" | tee -a "$out/SIZES"
            '';
            meta.description = description;
          });
        };
        # The Prodrome's module: the glue keeps the name `prodrome` the loader and the tests have always
        # fetched.
        prodromeModule = build: wasmModule build {
          pname = "suzatary-wasm";
          package = "suzatary-wasm";
          name = "prodrome";
          crates = [ ./app/wasm ];
          description = "Suzatary's browser module: the Prodrome's exports at Suzatary's schemas, with the JS glue";
        };
        suzatary-wasm-deps = (prodromeModule "optimised").deps;
        suzatary-wasm = (prodromeModule "optimised").module;
        suzatary-wasm-dev-deps = (prodromeModule "development").deps;
        suzatary-wasm-dev = (prodromeModule "development").module;
        # The pages in the view-tree shape (docs/FRAMEWORK-PLAN-2026-11-02.md), loaded by the page
        # that is drawn from them.
        viewsModule = build: wasmModule build {
          pname = "suzatary-views-wasm";
          package = "suzatary-pages";
          name = "views";
          crates = [ ./app/viewtree ./app/pages ];
          description = "Suzatary's pages as Rust programs, with the JS glue the painter loads";
        };
        suzatary-views-wasm-deps = (viewsModule "optimised").deps;
        suzatary-views-wasm = (viewsModule "optimised").module;
        suzatary-views-wasm-dev-deps = (viewsModule "development").deps;
        suzatary-views-wasm-dev = (viewsModule "development").module;

        # THE PLUGIN HOST'S ADVERSARY: `app/plugin-fence`, each mode trying one of the host's
        # fences (`app/plugin/tests/fence.rs`); `escapee.wasm` also imports a filesystem and a
        # network, which the host must refuse. Built for wasm32-unknown-unknown and wrapped by
        # `wasm-tools component new`, so no WASI is linked. A test fixture, never loaded.
        suzatary-plugin-fence = craneLib.mkCargoDerivation (commonArgs // {
          pname = "suzatary-plugin-fence";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions ([
              ./app/Cargo.toml
              ./app/Cargo.lock
              ./app/cli/Cargo.toml
              ./app/io/Cargo.toml
              ./app/server/Cargo.toml
              ./app/plugin/Cargo.toml
              ./app/plugin-ics/Cargo.toml
              ./app/testkit/Cargo.toml
              ./app/script-guards/Cargo.toml
              ./app/viewtree/Cargo.toml
              ./app/pages/Cargo.toml
              ./app/wasm/Cargo.toml
              ./app/plugin-fence
              ./app/wit
            ] ++ library "/Cargo.toml");
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

        # THE APP'S REFERENCE PLUGINS (`nix build .#suzatary-plugins`): `ics.wasm`, the calendar's
        # source (`app/plugin-ics`), beside its schema, built for wasm32-wasip2; it imports
        # `fetch` and the WASI the host links. The plugin timer roots it (`scripts/plugin-sync.sh`),
        # and the fence tests load it, so it is held to the adversary's fences.
        suzatary-plugins = craneWasi.mkCargoDerivation (commonArgs // {
          pname = "suzatary-plugins";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions ([
              ./app/Cargo.toml
              ./app/Cargo.lock
              ./app/cli/Cargo.toml
              ./app/io/Cargo.toml
              ./app/server/Cargo.toml
              ./app/plugin/Cargo.toml
              ./app/plugin-fence/Cargo.toml
              ./app/testkit/Cargo.toml
              ./app/script-guards/Cargo.toml
              ./app/viewtree/Cargo.toml
              ./app/pages/Cargo.toml
              ./app/wasm/Cargo.toml
              ./app/plugin-ics
              ./app/wit
            ] ++ library "/Cargo.toml");
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
        # TYPST FOR THE TAB: the Prodrome's `typst-wasm` at `inputs.prodrome`'s revision, copied
        # beside `prodrome_bg.wasm` and loaded when a Typst view is first drawn.
        suzatary-typst-wasm = prodrome.packages.${system}.prodrome-typst-wasm;

        # THE FRONTEND'S WIRE TYPES, GENERATED: `cargo test -p suzatary-api` writes `wire.ts` and
        # `payload.ts` when `TS_RS_EXPORT_DIR` is set. A derivation, so the frontend's shell needs no
        # toolchain. It and the vectors below build from `modelSrc` alone (the manifests and the
        # model crates), so the private tree and the public one give one derivation, rebuilt only
        # when those crates move.
        suzatary-wire-ts = modelCrate {
          pname = "suzatary-wire-ts";
          doCheck = false;
          # This build phase is a `cargo test`, not a `cargo build`: there is
          # no build log and no binary for crane's post-build hook to install.
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            TS_RS_EXPORT_DIR=$out cargo test --offline -p suzatary-api --lib -- wire::tests::generates
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            test -s $out/wire.ts
            test -s $out/payload.ts
            echo "wire.ts: $(wc -l < $out/wire.ts) lines, payload.ts: $(wc -l < $out/payload.ts)"
            runHook postInstall
          '';
          meta.description = "The TypeScript for Suzatary's wire, generated from suzatary-api";
        };
        # THE PAGES' BOUNDARY TYPES (`app/pages/tests/boundary.rs`): what a host hands each page
        # of `app/pages` and what the page asks of it, from the model crates' sources and the two
        # crates of the view tree, so the wire's own derivation stays the model crates' alone.
        suzatary-pages-ts = modelCrate {
          pname = "suzatary-pages-ts";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [ modelFiles ./app/viewtree ./app/pages ];
          };
          doCheck = false;
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            TS_RS_EXPORT_DIR=$out cargo test --offline -p suzatary-pages --test boundary -- generates
            runHook postBuild
          '';
          installPhase = ''
            runHook preInstall
            test -s $out/pages.ts
            runHook postInstall
          '';
          meta.description = "The TypeScript of the pages' boundary, generated from suzatary-pages";
        };
        # What `scripts/wire.sh` copies into web/src/wire/generated: the wire's files and the pages'.
        suzatary-web-ts = pkgs.symlinkJoin {
          name = "suzatary-web-ts";
          paths = [ suzatary-wire-ts suzatary-pages-ts ];
        };

        # THE FRONTEND BUNDLE, AS A BUILD: content-addressed like the binary, so a rebuild in a dev
        # checkout cannot overwrite the bundle a running server serves. It runs the same
        # `scripts/build-web.sh` a dev loop runs, whose innerHTML doors and `as` cast ceiling gate
        # this build too. A plain `stdenv.mkDerivation`: it compiles no Rust, so it needs
        # esbuild, tsc and node, and nothing from `cargoArtifacts`.
        suzatary-web = pkgs.stdenv.mkDerivation {
          pname = "suzatary-web";
          version = "0";
          # web/ and the one script that builds it, so a todo or a briefing moves no hash.
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
          SUZATARY_VIEWS_WASM = "${suzatary-views-wasm}";
          SUZATARY_TYPST_WASM = "${suzatary-typst-wasm}";
          SUZATARY_WIRE_TS = "${suzatary-web-ts}";
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

        # THE WEB TEST'S FPL/SERIES VECTORS, read from the Prodrome's `conformance/*.py` with the
        # crate's own parser (`app/todo/examples/vector_json.rs`), so `web/test/wasm.test.ts` needs
        # no second reader of that grammar; `env`/`json` come from `prodrome-wasm`'s frozen
        # `term-json.json` as they are.
        suzatary-vector-json = modelCrate {
          pname = "suzatary-vector-json";
          doCheck = false;
          # A `cargo run` of an example: nothing for the post-build hook.
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            cargo run --offline -p suzatary-todo --example vector_json -- \
              fpl ${prodrome}/conformance/fpl.py \
              ${prodrome}/wasm/conformance/term-json.json \
              > $out/fpl.json
            cargo run --offline -p suzatary-todo --example vector_json -- \
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

        # `nix build .#suzatary-junk-seed`: THE MONTHLY JUNK SWEEP'S CHURN SEED, the two public
        # lists above as one sorted `domains.txt` and a `sources.txt` of revs and counts, read at
        # each digest through `SUZATARY_JUNK_SEED`. Both lists are flake inputs, locked by hash;
        # `suzatary_triage::mail::junk::normalize_seed_file` is the one normaliser.
        suzatary-junk-seed = appCrate {
          pname = "suzatary-junk-seed";
          doCheck = false;
          doNotPostBuildInstallCargoBinaries = true;
          buildPhase = ''
            runHook preBuild
            mkdir -p $out
            cargo run --offline -p suzatary-triage --example junk_seed -- $out \
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

        # A CHECK IS A VERDICT, NOT AN ARTEFACT: `touch $out`.
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

        # The Rust toolchain app/ builds with, defined once so `.#rust` and `.#triage` cannot drift.
        rustToolchain = {
          # nixpkgs' rustc carries the wasm32 std but not its linker, so `lld` is on PATH.
          packages = with pkgs; [ cargo rustc rustfmt clippy lld mold ];
          RUST_BACKTRACE = "1";
          # The shells link with mold, which links the dozen test executables far faster than GNU
          # ld; per target, so a wasm32 build and the Nix derivations are untouched.
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
          CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
        };

        # What the workspace's tests run beside the toolchain, in every shell that runs them: typst
        # renders pages and answer blocks, and poppler reads a PDF's text layer.
        testTools = {
          packages = with pkgs; [ typst poppler-utils ];
          TYPST_FONT_PATHS = pkgs.lib.concatStringsSep ":" fontPaths;
          TYPST_PACKAGE_PATH = "${typstPackages}";
        };
      in
      {
        packages = {
          roots = services.roots;
          web = services.web;
          # Unsigned: a key in the store is a key published; `nix run .#apk-sign` is the impure half.
          apk-unsigned = apk.unsigned (apkProfile ./android/profile/generic.toml);
          apk-profiled-unsigned = apkNamed apk.unsigned;
          # A spike: the portal joining the tailnet itself through an embedded tsnet node.
          apk-tsnet-unsigned = apkNamed apk.tsnet;
          apk-sign = apk.sign;
          suzatary = suzatary-app;
          inherit suzatary-bin;
          # The dependency builds, exposed so a gate can root them: no output keeps them alive.
          suzatary-deps = cargoArtifacts;
          inherit suzatary-wasm-deps suzatary-views-wasm-deps suzatary-model-deps;
          inherit suzatary-wasm-dev-deps suzatary-views-wasm-dev-deps;
          inherit suzatary-wire-ts;
          inherit suzatary-pages-ts suzatary-web-ts;
          inherit suzatary-web;
          inherit suzatary-vector-json;
          inherit suzatary-wasm;
          inherit suzatary-views-wasm;
          inherit suzatary-wasm-dev suzatary-views-wasm-dev;
          inherit suzatary-plugin-fence;
          inherit suzatary-plugins;
          inherit suzatary-typst-wasm;
          inherit suzatary-fixture;
          inherit suzatary-junk-seed;
          # The guide, its HTML compiled in the app's closed world and its PDF by typst; no network.
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

        # The NixOS module, evaluated into a host's configuration, and the facts a deploy rests on
        # read off it (nix/module-check.nix); the merge gate's `nix-module` step builds it. The
        # application's suites are the gate's cargo steps, which run in the shell they need.
        checks = {
          suzatary-module = import ./nix/module-check.nix {
            inherit nixpkgs system;
            module = self.nixosModules.default;
          };
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

        # The operator's shell (`nix develop .#triage`): notmuch, lieer, isync and the tools below.
        #
        # `.#rust` is the EDITOR's shell for `app/`: the same toolchain the checks above build
        # with, plus a language server.
        devShells.rust = pkgs.mkShell {
          # `rust-analyzer` is the editor's. No pkg-config, no OpenSSL, no interpreter.
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
          # The browser's fonts are the shell's, not the machine's: a runner
          # has none, and chromium dies in Skia's font manager with nothing to
          # fall back to. DejaVu Sans alone is the one font this box holds, so
          # the e2e suite measures the same text on both.
          FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts.minimal ]; };
          # A SHELL IS TOOLS, NOT BUILDS: everything comes from nixpkgs, so entering it builds
          # nothing of this repository's; the scripts resolve the wasm and vectors they need.
          # Every command runs through this shell, so the commit-msg hook is wired here, relative
          # so it is right in every worktree (→ `hookspath`).
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
            # ⚠️ NO INTERPRETER HERE: decisions over JSON are verbs of the binary.
            # ⚠️ `nodejs_24` BY NAME, not the alias, which moves with nixpkgs: web-test.sh
            # (`--test`, `crypto.subtle`) and web-e2e.sh (a global `WebSocket`) need node 24.
            esbuild typescript nodejs_24
            # brotli: scripts/build-web.sh precompresses the frontend at
            # quality 11, so the server never compresses a file per request.
            brotli
            # THE BROWSER IS PART OF THE TOOLCHAIN: `scripts/web-e2e.sh` drives it headless over
            # CDP, with node 24's global WebSocket as the client.
            chromium
            # typst and poppler-utils come with testTools, below.
            notmuch lieer isync openssl curl
            protonmail-bridge pass gnupg  # Proton: Bridge serves localhost IMAP; pass/gnupg back its keyring on this headless box
            # Resolves secretspec.toml's secrets from `pass`; the `keyring` provider needs a DBus
            # secret service, which this headless box does not run.
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
            # The nightly unit has no login profile, so every tool train.sh and the gate call is
            # here, and both scripts check theirs at their start.
            gh git openssh systemdMinimal
          ] ++ rustToolchain.packages ++ testTools.packages;
        });
      }
    );
}
