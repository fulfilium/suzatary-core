# The Android portal, built as a PURE DERIVATION.
#
# NO GRADLE: it resolves its dependencies from the network at build time; the SDK's own tools
# (aapt2, d8, zipalign, and javac) build the one Java file and its resources.
#
# ITS OWN pkgs, so "allowUnfree + accept_license" for the Android SDK stays in the file that
# needs it and the flake's pkgs grants unfree to claude-code alone.
#
# x86_64-linux ON EVERY HOST: Google publishes no aarch64 Linux build-tools (aapt2 there fails
# with "Exec format error"), and one system keeps the store path, and release-apk.yml's cache,
# the same on every machine.
#
# NO SIGNING HERE: a key in a derivation is world-readable in the store and copied to every
# substituter, so this makes an UNSIGNED apk and its sha256, and scripts/sign-apk.sh signs it.
{
  nixpkgs,
  # Used ONLY for the signer, a JVM program.
  system,
  # android/route/ holds the routes (below); everything else is common.
  src,
  icon,
  signScript,
  # Derived from the tree by flake.nix so the derivation's hash changes exactly
  # when the thing it describes changes.
  versionCode,
  versionName,
}:

let
  androidConfig = {
    allowUnfree = true;
    android_sdk.accept_license = true;
  };

  # Checked against the pinned nixpkgs' repo.json; "latest" would make the APK a function of a nixpkgs bump.
  buildToolsVersion = "34.0.0";
  platformVersion = "34";

  # Only what compiles an APK; toolsVersion = null drops the legacy `tools` package build-tools would link.
  sdkFor =
    pkgs:
    pkgs.androidenv.composeAndroidPackages {
      platformVersions = [ platformVersion ];
      buildToolsVersions = [ buildToolsVersion ];
      toolsVersion = null;
      includeEmulator = false;
      includeSystemImages = false;
      includeSources = false;
      includeNDK = false;
      includeCmake = false;
    };

  # androidenv deploys every package under $out/libexec/android-sdk/<path>, and
  # neither of these exposes a bin/ — the tools are addressed by their SDK path.
  toolDirOf = sdk: "${builtins.head sdk.build-tools}/libexec/android-sdk/build-tools/${buildToolsVersion}";

  # Pinned: javac's -source/-target 8 (what d8 reads with no desugaring) is deprecated in newer JDKs.
  jdkOf = pkgs: pkgs.jdk17_headless;

  buildPkgs = import nixpkgs {
    system = "x86_64-linux";
    config = androidConfig;
  };
  buildSdk = sdkFor buildPkgs;
  toolDir = toolDirOf buildSdk;
  androidJar =
    "${builtins.head buildSdk.platforms}/libexec/android-sdk/platforms/android-${platformVersion}/android.jar";

  inherit (buildPkgs) lib;

  # EVERYTHING BUT THE ROUTES, so the portal's derivation does not move when
  # a route it does not use does.
  common = builtins.path {
    name = "android";
    path = src;
    filter = path: _: !(lib.hasPrefix "${toString src}/route" path);
  };

  # A ROUTE is how the WebView reaches the box: `device` is the phone's own network (the
  # Tailscale app's VPN) and adds nothing; `tsnet` is a tailnet node inside the app.
  routes = {
    device = {
      name = "device";
      java = src + "/route/device/java";
      jars = [ ];
      libs = null;
      res = null;
      package = null;
    };
    tsnet = {
      name = "tsnet";
      java = src + "/route/tsnet/java";
      jars = [
        "${webkit}/classes.jar"
        "${node}/classes.jar"
      ];
      libs = "${node}/jni";
      res = src + "/route/tsnet/res";
      # Its own package, so it installs BESIDE the portal rather than over it.
      package = "com.bmabsout.suzatary.tsnet";
    };
  };

  # androidx.webkit, for ProxyController (the one supported way to proxy a WebView); its
  # classes.jar is all the app needs. 1.12.1 is the newest whose bytecode build-tools 34's d8
  # reads (1.14.0 crashes it).
  webkit = buildPkgs.runCommand "androidx-webkit-1.12.1" {
    aar = buildPkgs.fetchurl {
      url = "https://dl.google.com/dl/android/maven2/androidx/webkit/webkit/1.12.1/webkit-1.12.1.aar";
      hash = "sha256-GKClGtEp4MhWPwMdRsin5hFp9WroTlYqDXVoaUEmIRE=";
    };
    nativeBuildInputs = [ buildPkgs.unzip ];
  } "mkdir $out && unzip -q $aar classes.jar -d $out";

  # THE NODE, bound to Java by gomobile for arm64 only. Go 1.27.1 because tailscale.com v1.104.0
  # asks for it; the module cache is a fixed-output derivation (proxyVendor), so the build reads
  # no network.
  ndkVersion = "29.0.14206865";
  nodeSdk = buildPkgs.androidenv.composeAndroidPackages {
    platformVersions = [ platformVersion ];
    buildToolsVersions = [ buildToolsVersion ];
    toolsVersion = null;
    includeEmulator = false;
    includeSystemImages = false;
    includeSources = false;
    includeNDK = true;
    ndkVersions = [ ndkVersion ];
    includeCmake = false;
  };
  node = (buildPkgs.buildGoModule.override { go = buildPkgs.go_1_27; }) {
    pname = "suzatary-node";
    # tsnet's version, not the app's: the node rebuilds when its own source
    # moves, not on every commit.
    version = "1.104.0";
    src = src + "/route/tsnet/node";
    proxyVendor = true;
    # What gomobile bind reads beyond the build: it writes a module of its own
    # that requires every module of `go list -m all`, flat, and tidies it, so
    # the cache holds what that tidy reads (the same module, built here).
    modPostBuild = ''
      here=$PWD
      mkdir -p "$TMPDIR/gobind" && cd "$TMPDIR/gobind"
      {
        echo 'module gobind'
        echo "go $(go env GOVERSION | sed 's/^go//')"
        GOOS=android GOARCH=arm64 go -C "$here" list -mod=mod -m \
          -f '{{if .Main}}replace {{.Path}} => {{.Dir}}{{else}}require {{.Path}} {{.Version}}{{end}}' all
      } > go.mod
      printf 'package main\n\nimport (\n\t_ "golang.org/x/mobile/bind/java"\n\t_ "golang.org/x/mobile/bind/seq"\n\t_ "suzatary/node"\n)\n\nfunc main() {}\n' > main.go
      GOOS=android GOARCH=arm64 go mod tidy
      cd "$here"
    '';
    vendorHash = "sha256-cvnbtM6+/cmU5HpfKXdOvj1JC12owqvIoupme7RFVqk=";
    nativeBuildInputs = [
      (buildPkgs.gomobile.override { withAndroidPkgs = false; })
      (jdkOf buildPkgs)
      buildPkgs.unzip
    ];
    ANDROID_HOME = "${nodeSdk.androidsdk}/libexec/android-sdk";
    ANDROID_NDK_HOME = "${nodeSdk.androidsdk}/libexec/android-sdk/ndk/${ndkVersion}";
    buildPhase = ''
      runHook preBuild
      gomobile bind -target android/arm64 -androidapi 26 \
        -javapkg com.bmabsout.suzatary -trimpath -ldflags '-s -w -buildid=' \
        -o node.aar .
      runHook postBuild
    '';
    checkPhase = ''
      runHook preCheck
      go test ./...
      runHook postCheck
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      unzip -q node.aar classes.jar 'jni/*' -d $out
      runHook postInstall
    '';
  };

  # Profile.java, from a profile nix/apk-profile.nix read: its fields are
  # checked there, so they are spliced here as they are.
  profileJava = profile: buildPkgs.writeText "Profile.java" ''
    package com.bmabsout.suzatary;

    /** Written by nix/apk.nix from the APK's profile: the box it names, or null for the generic APK. */
    final class Profile {
        static final Box BOX = ${
          if profile.box == null then "null"
          else
            let
              sender = if profile.box.sender == null then "null" else ''"${profile.box.sender}"'';
            in
            ''new Box("${profile.box.host}", ${sender})''
        };

        private Profile() {
        }
    }
  '';

  apk = profile: route: buildPkgs.stdenvNoCC.mkDerivation {
    pname = "suzatary-apk-unsigned-${route.name}";
    version = versionName;

    dontUnpack = true;
    nativeBuildInputs = [
      (jdkOf buildPkgs)
      buildPkgs.imagemagick # renders the launcher icons; nothing else needs it
      buildPkgs.zip # the one way to add classes.dex to the linked APK
    ];

    buildPhase = ''
      runHook preBuild
      export PATH=${toolDir}:$PATH

      # THE PROFILE is spelled into a copy of the common sources, and a token left over fails
      # the build. The launcher icons are the web app's own PNG; -strip drops the PNG tIME
      # chunk, which would put a clock in the output.
      cp -r ${common} android
      chmod -R u+w android
      cp ${profileJava profile} android/java/com/bmabsout/suzatary/Profile.java
      ${if profile.box == null then ''
        sed -i '/<!-- profile box:/,/<!-- \/profile box -->/d' android/AndroidManifest.xml
      '' else ''
        sed -i 's|@PROFILE_HOST@|${profile.box.host}|g' android/AndroidManifest.xml
      ''}
      if grep -rq '@PROFILE_' android; then
        echo "apk: a @PROFILE_ token has no value" >&2
        exit 1
      fi

      cp -r android/res res
      chmod -R u+w res
      for spec in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
        density=''${spec%%:*}
        px=''${spec##*:}
        mkdir -p "res/mipmap-$density"
        magick ${icon} -resize "''${px}x''${px}" -strip "res/mipmap-$density/ic_launcher.png"
      done

      aapt2 compile --dir res -o compiled.zip
      ${lib.optionalString (route.res != null) "aapt2 compile --dir ${route.res} -o overlay.zip"}

      # --version-code/--version-name inject what the manifest omits; --java writes R.java in the
      # classes' own package whatever the route installs as.
      aapt2 link \
        -o base.apk \
        -I ${androidJar} \
        --manifest android/AndroidManifest.xml \
        --java gen \
        --custom-package com.bmabsout.suzatary \
        --version-code ${versionCode} \
        --version-name ${versionName} \
        ${lib.optionalString (route.package != null) "--rename-manifest-package ${route.package}"} \
        compiled.zip \
        ${lib.optionalString (route.res != null) "--auto-add-overlay -R overlay.zip"}

      mkdir -p classes
      javac -encoding UTF-8 -nowarn \
        -source 8 -target 8 -bootclasspath ${androidJar} \
        ${lib.optionalString (route.jars != [ ]) "-classpath ${lib.concatStringsSep ":" route.jars}"} \
        -d classes $(find android/java ${route.java} gen -name '*.java')

      mkdir -p dex
      d8 --release --min-api 26 --lib ${androidJar} \
        --output dex $(find classes -name '*.class') ${toString route.jars}

      # d8 cannot append to an archive and aapt2 cannot be told about dex, so they meet here; -X
      # drops the extra attributes and the touch sets the DOS date, otherwise the wall clock, to
      # 1980, the DOS epoch and this repo's SOURCE_DATE_EPOCH.
      cp base.apk unaligned.apk
      touch -d @315532800 dex/classes.dex
      ( cd dex && zip -q -X ../unaligned.apk classes.dex )
      ${lib.optionalString (route.libs != null) ''
        # THE ROUTE'S NATIVE CODE, at lib/<abi>/ where the installer looks,
        # deflated (the installer extracts it), dated like the dex.
        mkdir -p native
        cp -r ${route.libs} native/lib
        chmod -R u+w native
        find native -exec touch -d @315532800 {} +
        ( cd native && zip -q -X -r -D ../unaligned.apk lib )
      ''}

      # Android mmaps uncompressed entries straight out of the APK, so they must be 4-byte
      # aligned; -p page-aligns stored .so files, of which there are none (a route's are deflated).
      zipalign -p -f 4 unaligned.apk suzatary-unsigned.apk
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp suzatary-unsigned.apk $out/
      # The release notes read the version here, never by evaluating the flake again.
      printf '%s\n' ${versionName} > $out/version
      # The release workflow skips the upload when this matches the published one: the UNSIGNED
      # hash, a fact about the inputs alone, never about a signature's determinism.
      ( cd $out && sha256sum suzatary-unsigned.apk > suzatary-unsigned.apk.sha256 )
      runHook postInstall
    '';

    passthru = {
      inherit
        versionCode
        versionName
        buildToolsVersion
        platformVersion
        ;
    };

    meta = {
      description = "Suzatary's Android portal (unsigned): a WebView onto the tailnet address";
      platforms = [ "x86_64-linux" ];
    };
  };

  # THE SIGNER is built for the HOST: apksigner is a script over a jar, and the x86_64 aapt2 and
  # zipalign beside it are never invoked, so an aarch64 box can sign.
  hostPkgs = import nixpkgs {
    inherit system;
    config = androidConfig;
  };
  sign = hostPkgs.writeShellScriptBin "suzatary-sign-apk" ''
    export PATH=${hostPkgs.lib.makeBinPath [ (jdkOf hostPkgs) ]}:${toolDirOf (sdkFor hostPkgs)}:"$PATH"
    exec ${hostPkgs.bash}/bin/bash ${signScript} "$@"
  '';
in
{
  # Each takes a profile, as nix/apk-profile.nix reads one.
  unsigned = profile: apk profile routes.device;
  # The tsnet node needs its box before the app runs, so its profile names one.
  tsnet =
    profile:
    if profile.box == null then
      throw "apk: the tsnet route joins the tailnet to reach one box, so its profile names the box"
    else
      apk profile routes.tsnet;
  inherit sign;
}
