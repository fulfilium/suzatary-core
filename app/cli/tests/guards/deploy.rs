//! The deploy (`scripts/deploy.sh`), run as the real script against a throwaway user
//! repository, its origin, and an app origin with real revisions, with stub `systemctl`,
//! `curl`, `nix`, `nix-store` and `sleep` whose answers are a small machine on disk (a build
//! links `<tree>-<output>`, never touching a real store or its roots): the roots unit's last
//! run (when it started, whether it is still running, how it ended), what a `start` of it and
//! the path unit on the lock do, what the web serves after a restart. Every call is logged in
//! order, so a test reads what the script did and when.

use super::{Scratch, repo};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The roots unit as `systemctl show` prints it, and its acts. A run reads only a COMMITTED
/// lock, as the real one does: one begun while the working tree's lock differs from HEAD's
/// ends `failed` and links nothing. `busy` counts the reads still to report a run under way;
/// `next-result`, when present, is how a run ends, and a successful one builds through the
/// real `scripts/app-builds.sh` under `lock-`, as the real run does (with `drift` present, its
/// binary comes out another path; with `extra`, it roots an output the deploy's builds do not
/// hold), then links the app. A `start --no-block` returns before the run begins, so the first
/// read after it still reports the PREVIOUS run: the stale `Result=success` a deploy must not
/// trust. With `path-unit` present, a read that finds the lock changed since the last one
/// begins a run, as the path unit on the lock does; a start while a run is under way joins
/// it. `long-run`, holding a lock, makes the first run of that lock a long build. With
/// `term-at-move` present (holding the lock it was set at), the first read of a run of another
/// lock under way sends the deploy TERM, as a stop or a dropped session would mid-wait; with
/// `term-at-restart`, the web's restart does. `consists`, when present, is what
/// `suzatary-app.target`'s `ConsistsOf` names (else the web and the watcher); `state-<unit>`,
/// when present, is that unit's `ActiveState` (else `active`); a unit named in `wont-restart`
/// fails its restart, and any other unit's restart roots the linked app as `<unit>-app`, as its
/// wrapper does.
const SYSTEMCTL: &str = r#"#!/usr/bin/env bash
s="$DEPLOY_STATE"
echo "systemctl $*" >>"$s/log"
lock="$(cat "$s/user/flake.lock")"
begin() {
  [ -f "$s/next-result" ] || return 0
  date +%s.%6N >"$s/started"
  if [ -f "$s/long-run" ] && [ "$lock" = "$(cat "$s/long-run")" ]; then
    rm "$s/long-run"
    echo 100000 >"$s/busy"
  else
    echo 2 >"$s/busy"
  fi
  if [ "$lock" != "$(git -C "$s/user" show HEAD:flake.lock)" ]; then
    echo "roots refuses an uncommitted lock" >>"$s/log"
    echo failed >"$s/result"
    return 0
  fi
  cp "$s/next-result" "$s/result"
  if [ "$(cat "$s/next-result")" = success ]; then
    app="$s/store/app-$lock"
    [ -f "$s/links" ] && { app="$(cat "$s/links")"; rm "$s/links"; }
    echo "$app" >"$s/pending"
  fi
}
# A run links what it built when it ends, never before.
finish() {
  echo "roots run ends" >>"$s/log"
  [ -f "$s/pending" ] || return 0
  app="$(cat "$s/pending")"
  rm "$s/pending"
  bash "$DEPLOY_APP_BUILDS" "$app" "$s/roots/lock-" >/dev/null
  [ -f "$s/extra" ] && ln -sfn "$app-extra" "$s/roots/lock-extra"
  [ -f "$s/drift" ] && ln -sfn "$app-drifted-bin" "$s/roots/lock-bin"
  ln -sfn "$app" "$s/roots/app"
}
# The deploy itself: the outermost ancestor running deploy.sh, past any subshell.
term_deploy() {
  pid=$PPID
  while parent="$(sed 's/.*) //' "/proc/$pid/stat" | cut -d' ' -f2)" \
    && tr '\0' ' ' <"/proc/$parent/cmdline" 2>/dev/null | grep -q deploy.sh; do
    pid=$parent
  done
  echo "deploy sent TERM" >>"$s/log"
  kill -TERM "$pid"
}
case "$*" in
  *"-p Environment --value suzatary-web.service")
    echo "SUZATARY_USER_REPO=$s/user SUZATARY_INSTANCE=$s/user/instance.toml SUZATARY_ROOTS=$s/roots" ;;
  "--user show -p ConsistsOf --value suzatary-app.target")
    cat "$s/consists" 2>/dev/null || echo "suzatary-web.service suzatary-mail-idle.service" ;;
  "--user show -p ActiveState --value "*)
    cat "$s/state-$6" 2>/dev/null || echo active ;;
  "--user show suzatary-roots.service"*)
    echo "show lock=$lock" >>"$s/log"
    if [ -f "$s/starting" ]; then
      if [ "$(cat "$s/starting")" -gt 0 ]; then echo 0 >"$s/starting"; else rm "$s/starting"; begin; fi
    elif [ -f "$s/path-unit" ] && [ "$lock" != "$(cat "$s/seen")" ] && [ "$(cat "$s/busy")" -eq 0 ]; then
      echo "path unit fires" >>"$s/log"
      begin
    fi
    echo "$lock" >"$s/seen"
    if [ -f "$s/term-at-move" ] && [ "$lock" != "$(cat "$s/term-at-move")" ] \
      && [ "$(cat "$s/busy")" -gt 0 ]; then
      rm "$s/term-at-move"
      term_deploy
    fi
    busy="$(cat "$s/busy")"
    if [ "$busy" -gt 0 ]; then
      echo $((busy - 1)) >"$s/busy"
      active=activating
      [ "$busy" -eq 1 ] && finish
    else
      active=inactive
    fi
    echo "LoadState=loaded"
    echo "ActiveState=$active"
    echo "Result=$(cat "$s/result")"
    echo "ExecMainStartTimestamp=$(date -u -d "@$(cat "$s/started")" '+%a %Y-%m-%d %H:%M:%S.%6N UTC')" ;;
  "--user stop suzatary-roots.service")
    [ -f "$s/stop-fails" ] && exit 1
    echo "roots run stopped" >>"$s/log"
    echo 0 >"$s/busy"
    rm -f "$s/pending" "$s/starting" ;;
  "--user start --no-block suzatary-roots.service")
    [ "$(cat "$s/busy")" -gt 0 ] || echo 1 >"$s/starting" ;;
  "--user try-restart suzatary-web.service")
    if [ -f "$s/web-app" ]; then app="$(cat "$s/web-app")"; else app="$(readlink -f "$s/roots/app")"; fi
    ln -sfn "$app" "$s/roots/suzatary-web-app"
    ln -sfn "$(readlink -f "$s/roots/lock-bin")" "$s/roots/app-bin-serving"
    ln -sfn "$(readlink -f "$s/roots/lock-dist")" "$s/roots/web-dist-serving"
    [ -f "$s/term-at-restart" ] && { rm "$s/term-at-restart"; term_deploy; }
    exit 0 ;;
  "--user try-restart "*)
    grep -qx "$3" "$s/wont-restart" 2>/dev/null && exit 1
    ln -sfn "$(readlink -f "$s/roots/app")" "$s/roots/${3%.service}-app" ;;
  *) exit 64 ;;
esac
"#;

/// `nix`: `path-info` answers whether the store holds the builds (`in-store`);
/// `flake lock --override-input suzatary <url> path:<dir>` writes the url's rev as
/// `<dir>`'s lock (or refuses, as Nix does, when `lock-refuses` exists); `eval` answers what
/// `nix/app-input.nix` does (the origin the flake declares and its branch, the pin there, the
/// locked tree) and the system; `build <tree>#<attr> -o <link>` links the tree's output (or fails, when
/// `build-fails` exists; with `save-during-build`, the old web's settings save lands in the
/// instance and is committed meanwhile, once; `rm-during-build` names a file the first build
/// removes) and `develop --profile <p>` links a profile's generation, as Nix does.
const NIX: &str = r#"#!/usr/bin/env bash
s="$DEPLOY_STATE"
echo "nix $*" >>"$s/log"
args=("$@")
last="${args[-1]}"
arg() { for ((i = 0; i < ${#args[@]}; i++)); do [ "${args[i]}" = "$1" ] && echo "${args[i + 1]}"; done; }
if [ "$1 $2" = "flake lock" ]; then
  [ -f "$s/lock-refuses" ] && { echo "error: Cannot find Git revision in ref 'master'" >&2; exit 1; }
  url="$(arg --override-input suzatary 2>/dev/null; arg suzatary)"
  echo "${url##*rev=}" >"${last#path:}/flake.lock"
  exit 0
fi
if [ "$1" = build ]; then
  [ -f "$s/build-fails" ] && { echo "error: builder for '$2' failed" >&2; exit 1; }
  [ -f "$s/rm-during-build" ] && { rm -f "$(cat "$s/rm-during-build")"; rm "$s/rm-during-build"; }
  if [ -f "$s/save-during-build" ]; then
    rm "$s/save-during-build"
    printf '[secretary]\nmodel = "saved while it built"\n' >>"$s/user/instance.toml"
    git -C "$s/user" commit -q -am "a settings save" >/dev/null
  fi
  case "${2##*#}" in
    suzatary) out="${2%#*}-bin" ;;
    suzatary-web) out="${2%#*}-dist" ;;
    suzatary-plugins) out="${2%#*}-plugins" ;;
    *) exit 64 ;;
  esac
  ln -sfn "$out" "$(arg -o)"
  echo "$out"
  exit 0
fi
if [ "$1" = path-info ]; then
  [ -f "$s/in-store" ]
  exit
fi
if [ "$1" = develop ]; then
  profile="$(arg --profile)"
  ln -sfn "${2%%#*}-shell" "$profile-1-link"
  ln -sfn "$(basename "$profile")-1-link" "$profile"
  exit 0
fi
[ "$1 $2" = "profile wipe-history" ] && exit 0
case "$last" in
  builtins.currentSystem) printf test-system ;;
  remote) sed -n 's#.*"git+file://\([^?"]*\).*#\1#p' "$(arg flake)" ;;
  branch) printf '%s' "$(arg ref 2>/dev/null || true)" | grep . || printf master ;;
  pinned) printf '%s' "git+file://$(sed -n 's#.*"git+file://\([^?"]*\).*#\1#p' "$(arg flake)")?ref=$({ arg ref; echo master; } | head -1)&rev=$(arg rev)" ;;
  lockedRef) printf master ;;
  source.outPath) printf '%s' "$s/store/app-$(cat "$(arg lock)")" ;;
  revision) printf '%s' "$(cat "$(arg lock)")" ;;
  *) exit 64 ;;
esac
"#;

/// `git`, the real one beside an observer of the user repository's lock: whenever the lock
/// holds other bytes than the last time a `git` saw it, before or after the command, those bytes
/// must be HEAD's, as `suzatary-roots.path` would start a run on them; any other is logged as
/// [`UNCOMMITTED`].
const GIT: &str = r#"#!/usr/bin/env bash
s="$DEPLOY_STATE"
observe() {
  [ -f "$s/user/flake.lock" ] || return 0
  lock="$(cat "$s/user/flake.lock")"
  [ "$lock" = "$(cat "$s/git-seen" 2>/dev/null)" ] && return 0
  echo "$lock" >"$s/git-seen"
  [ "$lock" = "$("$DEPLOY_GIT" -C "$s/user" show HEAD:flake.lock 2>/dev/null)" ] \
    || echo "the lock holds a state no commit holds" >>"$s/log"
}
observe
"$DEPLOY_GIT" "$@"
status=$?
observe
exit $status
"#;

/// `gh`: `run list …` answers the first line of `runs` as `<status> <conclusion>`, dropping it
/// while another follows, so the last line stands; nothing where there is no `runs`, as GitHub
/// answers for a commit no run built; and fails as an unreachable GitHub with `gh-down`.
const GH: &str = r#"#!/usr/bin/env bash
s="$DEPLOY_STATE"
echo "gh $*" >>"$s/log"
[ ! -f "$s/gh-down" ] || { echo "error connecting to api.github.com" >&2; exit 1; }
[ "$1 $2" = "run list" ] || exit 64
[ -s "$s/runs" ] || exit 0
head -n 1 "$s/runs"
[ "$(wc -l <"$s/runs")" -le 1 ] || sed -i 1d "$s/runs"
"#;

/// What [`GIT`] logs when the lock the path unit watches changed to a state no commit holds.
const UNCOMMITTED: &str = "the lock holds a state no commit holds";

/// The new app's binary: `instance get web.app_url` answers the file's, and `--instance <file>
/// verify` reads the instance as the real one does
/// and refuses, in its own words, one naming a plugin world this host is past; with
/// `term-at-verify`, it sends the deploy TERM while the carried copy stands beside the live
/// instance.
const SUZATARY: &str = r#"#!/usr/bin/env bash
echo "suzatary $* from $SUZATARY_REPO" >>"$DEPLOY_STATE/log"
if [ "$1 $3 $4 $5" = "--instance instance get web.app_url" ]; then
  sed -n 's/^app_url = "\(.*\)"$/\1/p' "$2"
  exit 0
fi
[ "$1 $3" = "--instance verify" ] || exit 64
if [ -f "$DEPLOY_STATE/term-at-verify" ]; then
  rm "$DEPLOY_STATE/term-at-verify"
  pid=$PPID
  while parent="$(sed 's/.*) //' "/proc/$pid/stat" | cut -d' ' -f2)" \
    && tr '\0' ' ' <"/proc/$parent/cmdline" 2>/dev/null | grep -q deploy.sh; do
    pid=$parent
  done
  kill -TERM "$pid"
  sleep 1
fi
if grep -q 'world = "source@0.2.0"' "$2"; then
  echo "suzatary: reading $2: the world is at version 0.2.0, and this host has 0.3.0" >&2
  exit 1
fi
echo "chain OK"
"#;

/// A user repository whose flake declares the publication `core.git`, locked at the
/// publication of the app's `old` revision and pushed to its origin, whose roots and web run
/// that app. The app's origin's master is `pre` (before the lock interface), `old`, `new`, then
/// the merge of a pull request whose series holds `side`, an ancestor of master that no gate
/// ran; `core.git`'s master publishes each commit of master's first-parent chain, one commit
/// each naming its source in a `Source:` trailer, so `side` is never published.
struct Box_ {
    scratch: Scratch,
    pre: String,
    old: String,
    new: String,
    side: String,
}

/// A user repository's flake declaring the app at `origin`.
fn declaring(origin: &Path) -> String {
    format!(
        "{{\n  inputs.suzatary.url = \"git+file://{}?ref=master\";\n  outputs = _: {{ }};\n}}\n",
        origin.display()
    )
}

impl Box_ {
    fn new(name: &str) -> Self {
        Self::declaring(name, true)
    }

    /// The box before its input moved to the publication: its flake declares the app's own
    /// origin and its lock names `old` itself.
    fn before_the_move(name: &str) -> Self {
        Self::declaring(name, false)
    }

    fn declaring(name: &str, published: bool) -> Self {
        let scratch = Scratch::new(&format!("deploy-{name}"));
        let s: &Path = &scratch;

        run_git(s, &["init", "-q", "--bare", "-b", "master", "app.git"]);
        run_git(s, &["init", "-q", "-b", "master", "app"]);
        let app = s.join("app");
        let commit = |file: &str, content: &str| {
            write(&app, file, content);
            run_git(&app, &["add", "-A"]);
            run_git(&app, &["commit", "-q", "-m", file]);
            run_git(&app, &["rev-parse", "HEAD"])
        };
        let pre = commit("README", "the app\n");
        let old = commit("nix/app-input.nix", "{ lock }: { }\n");
        // A pull request's series, merged: its commits are master's ancestors, and only the
        // merge is on master's first-parent chain.
        run_git(&app, &["checkout", "-q", "-b", "pr"]);
        let side = commit("series", "a pull request's middle commit\n");
        run_git(&app, &["checkout", "-q", "master"]);
        let new = commit("scripts/deploy.sh", "the next\n");
        run_git(
            &app,
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                "Merge #1: a pull request",
                "pr",
            ],
        );
        let merged = run_git(&app, &["rev-parse", "HEAD"]);
        run_git(
            &app,
            &[
                "push",
                "-q",
                "../app.git",
                &format!("{merged}:refs/heads/master"),
            ],
        );
        publish(&app, &s.join("core.git"), &[&pre, &old, &new, &merged]);
        let running = if published {
            publication_of(&s.join("core.git"), &old)
        } else {
            old.clone()
        };

        run_git(s, &["init", "-q", "--bare", "-b", "master", "origin.git"]);
        run_git(s, &["init", "-q", "-b", "master", "user"]);
        let user = s.join("user");
        write(&user, "flake.lock", &format!("{running}\n"));
        let origin = s.join(if published { "core.git" } else { "app.git" });
        write(&user, "flake.nix", &declaring(&origin));
        write(&user, "state.json", "{}\n");
        write(
            &user,
            "instance.toml",
            "[web]\napp_url = \"https://box.invalid/\"\n",
        );
        run_git(&user, &["add", "-A"]);
        run_git(&user, &["commit", "-q", "-m", "seed"]);
        run_git(&user, &["remote", "add", "origin", "../origin.git"]);
        run_git(&user, &["push", "-q", "-u", "origin", "master"]);

        let core = s.join("core.git");
        let publication = |source: &String| publication_of(&core, source);
        fill_store(
            &s.join("store"),
            &[pre.clone(), publication(&pre)],
            &[&old, &new, &merged]
                .map(publication)
                .into_iter()
                .chain([&old, &new, &side, &merged].map(String::clone))
                .collect::<Vec<_>>(),
        );
        std::fs::create_dir_all(s.join("roots")).expect("the roots");
        let running = s.join("store").join(format!("app-{running}"));
        let suffixed = |suffix: &str| PathBuf::from(format!("{}{suffix}", running.display()));
        for (link, target) in [
            ("app", running.clone()),
            ("lock-src", running.clone()),
            ("lock-bin", suffixed("-bin")),
            ("lock-dist", suffixed("-dist")),
            ("suzatary-web-app", running.clone()),
            ("app-bin-serving", suffixed("-bin")),
            ("web-dist-serving", suffixed("-dist")),
        ] {
            std::os::unix::fs::symlink(target, s.join("roots").join(link)).expect("a link");
        }

        stubs(&s.join("bin"));
        let deployed = Box_ {
            scratch,
            pre,
            old,
            new,
            side,
        };
        deployed.set("started", "1700000000.000000");
        deployed.set("busy", "0");
        deployed.set("result", "success");
        let lock = deployed.lock_holds();
        deployed.set("seen", &lock);
        deployed.set("git-seen", &lock);
        deployed
    }

    /// The publication of the source `rev`: what a lock holds once `rev` is deployed.
    fn core(&self, rev: &str) -> String {
        publication_of(&self.scratch.join("core.git"), rev)
    }

    /// What the lock holds, as written.
    fn lock_holds(&self) -> String {
        std::fs::read_to_string(self.user().join("flake.lock"))
            .expect("the lock")
            .trim()
            .to_owned()
    }

    fn set(&self, name: &str, value: &str) {
        std::fs::write(self.scratch.join(name), format!("{value}\n")).expect("a state file");
    }

    fn user(&self) -> PathBuf {
        self.scratch.join("user")
    }

    fn deploy(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let path = format!(
            "{}:{}",
            self.scratch.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = isolated(Command::new(repo().join("scripts").join("deploy.sh")));
        let out = cmd
            .args(args)
            .env("PATH", path)
            .env("DEPLOY_STATE", &*self.scratch)
            .env(
                "DEPLOY_APP_BUILDS",
                repo().join("scripts").join("app-builds.sh"),
            )
            .env("XDG_RUNTIME_DIR", self.scratch.join("run"))
            .env("XDG_CACHE_HOME", self.scratch.join("cache"))
            .env("DEPLOY_GIT", real_git())
            .env_remove("SUZATARY_APP_URL")
            .envs(env.iter().copied())
            .output()
            .expect("deploy.sh runs");
        let log = self.log();
        assert!(
            !log.contains(UNCOMMITTED),
            "the path unit would have started a run on an uncommitted lock:\n{log}"
        );
        out
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.scratch.join("log")).unwrap_or_default()
    }

    fn restarted(&self) -> bool {
        self.log().contains("restart suzatary-web")
    }

    /// The source the lock runs: the source its publication names, or, for a lock from before
    /// the move to the publication, the commit it holds.
    fn lock(&self) -> String {
        let held = self.lock_holds();
        let mut cmd = isolated(Command::new("git"));
        let out = cmd
            .args([
                "log",
                "-1",
                "--format=%(trailers:key=Source,valueonly,separator=%x2C)",
                &held,
            ])
            .current_dir(self.scratch.join("core.git"))
            .output()
            .expect("git runs");
        let source = text(&out.stdout).trim().to_owned();
        if out.status.success() && !source.is_empty() {
            source
        } else {
            held
        }
    }

    /// The tree the source `rev`'s publication is in the store.
    fn store(&self, rev: &str) -> PathBuf {
        self.store_of(&self.core(rev))
    }

    /// The tree `held` is in the store, as a lock holds it.
    fn store_of(&self, held: &str) -> PathBuf {
        self.scratch.join("store").join(format!("app-{held}"))
    }

    fn app(&self) -> PathBuf {
        std::fs::canonicalize(self.scratch.join("roots").join("app")).expect("roots/app")
    }

    fn origin_subject(&self) -> String {
        subject(&self.scratch.join("origin.git"))
    }
}

/// The happy path, with the path unit firing on the write: one commit of the lock alone
/// (a staged change of the data tree stays staged), a fresh run's success, the web restarted
/// after it, the record pushed.
#[test]
fn deploy_commits_the_lock_alone_and_restarts_only_on_a_fresh_success() {
    let deployed = Box_::new("happy");
    deployed.set("next-result", "success");
    deployed.set("path-unit", "");
    let user = deployed.user();
    write(&user, "state.json", "{\"a\": 1}\n");
    run_git(&user, &["add", "state.json"]);

    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(subject(&user), format!("Deploy {}", deployed.new));
    assert_eq!(
        run_git(&user, &["show", "--name-only", "--format=", "HEAD"]),
        "flake.lock",
        "the deploy commit holds the lock alone"
    );
    assert_eq!(
        run_git(&user, &["diff", "--cached", "--name-only"]),
        "state.json",
        "a staged change of the data tree stays staged"
    );
    assert_eq!(
        deployed.origin_subject(),
        format!("Deploy {}", deployed.new)
    );
    let log = deployed.log();
    assert!(!log.contains("roots refuses"), "{log}");
    let ended = log.find("roots run ends").expect("the run ends");
    let restarted = log.find("restart suzatary-web").expect("the web restarts");
    assert!(ended < restarted, "{log}");

    // What the roots run will admit (`scripts/followed.sh admit`): the revision this deploy
    // pinned, and no other commit of the chain.
    let mirror = run_git(&user, &["rev-parse", "--absolute-git-dir"]) + "/suzatary-followed.git";
    let remote = run_git(Path::new(&mirror), &["config", "suzatary.remote"]);
    let admits = |rev: &str| {
        isolated(Command::new(repo().join("scripts").join("followed.sh")))
            .args(["admit", &mirror, &remote, "master", rev])
            .output()
            .expect("runs")
            .status
            .success()
    };
    assert!(admits(&deployed.core(&deployed.new)), "what it pinned");
    // The box's lock named `old` without a deploy here choosing it, as a lock committed by hand
    // or pulled would: replacing it does not make it the way back.
    assert!(
        !admits(&deployed.core(&deployed.old)),
        "a lock no deploy pinned, replaced"
    );
    assert!(
        !admits(&deployed.core(&deployed.pre)),
        "a commit of the chain no deploy chose"
    );
}

/// The deploy builds what the lock will name before it moves the lock, under roots of its
/// own; the run after the move links those very paths, the deploy says so, and its own roots
/// are gone.
#[test]
fn deploy_builds_before_it_moves_the_lock_and_the_run_links_the_same_paths() {
    let deployed = Box_::new("builds-first");
    deployed.set("next-result", "success");
    deployed.set("path-unit", "");
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let log = deployed.log();
    let built = log
        .find("/roots/deploy-bin")
        .expect("the deploy builds the binary");
    let shell = log
        .find("/roots/deploy-shell")
        .expect("the deploy builds the shell");
    let moved = log
        .find(&format!("show lock={}", deployed.core(&deployed.new)))
        .expect("the lock moves");
    assert!(built < moved && shell < moved, "{log}");
    let checked = log
        .find(&format!(
            "suzatary --instance {} verify from {}",
            deployed.user().join("instance.toml").display(),
            deployed.store(&deployed.new).display()
        ))
        .expect("the new binary reads the live instance");
    assert!(shell < checked && checked < moved, "{log}");
    assert!(
        text(&out.stderr).contains("exactly the builds the deploy made"),
        "{}",
        text(&out.stderr)
    );
    let roots = deployed.scratch.join("roots");
    let store = deployed.store(&deployed.new);
    for output in ["bin", "plugins", "shell"] {
        assert_eq!(
            std::fs::canonicalize(roots.join(format!("lock-{output}"))).expect("a link"),
            PathBuf::from(format!("{}-{output}", store.display())),
        );
    }
    let left: Vec<_> = std::fs::read_dir(&roots)
        .expect("the roots")
        .map(|entry| entry.expect("an entry").file_name())
        .filter(|name| {
            let name = name.to_string_lossy();
            name.starts_with("deploy-") || name.starts_with("previous-")
        })
        .collect();
    assert!(left.is_empty(), "the deploy's roots stay: {left:?}");
}

/// A build that fails stops the deploy before anything moves: no commit, no run, no revert.
#[test]
fn deploy_whose_build_fails_moves_nothing() {
    let deployed = Box_::new("build-fails");
    deployed.set("next-result", "success");
    deployed.set("build-fails", "");
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("the build of"), "{stderr}");
    assert!(stderr.contains("Nothing moved"), "{stderr}");
    assert_eq!(subject(&deployed.user()), "seed");
    assert_eq!(deployed.origin_subject(), "seed");
    assert_eq!(deployed.lock(), deployed.old);
    assert!(!deployed.log().contains("start --no-block"));
}

/// An instance file the new binary refuses stops the deploy before the lock moves, with the
/// binary's own words and what to do about it.
#[test]
fn deploy_refuses_an_instance_the_new_app_refuses() {
    let deployed = Box_::new("instance-refused");
    deployed.set("next-result", "success");
    write(
        &deployed.user(),
        "instance.toml",
        "[web]\napp_url = \"https://box.invalid/\"\n\n[[plugins]]\nworld = \"source@0.2.0\"\n",
    );
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("the world is at version 0.2.0, and this host has 0.3.0"),
        "{stderr}"
    );
    assert!(
        stderr.contains("must change in the same deploy"),
        "{stderr}"
    );
    assert!(stderr.contains("Nothing moved"), "{stderr}");
    assert_eq!(deployed.lock(), deployed.old);
    assert_eq!(subject(&deployed.user()), "seed");
    assert!(!deployed.log().contains("start --no-block"));
}

/// An instance the new binary reads and the old does not is carried by the deploy: the new
/// binary checks the carried file (not the live one, which it refuses), and the one "Deploy"
/// commit writes it together with the lock.
#[test]
fn deploy_carries_its_instance_file_in_the_deploy_commit() {
    let deployed = Box_::new("carried");
    deployed.set("next-result", "success");
    let old =
        "[web]\napp_url = \"https://box.invalid/\"\n\n[[plugins]]\nworld = \"source@0.2.0\"\n";
    write(&deployed.user(), "instance.toml", old);
    run_git(
        &deployed.user(),
        &["commit", "-q", "-am", "the instance the old app reads"],
    );
    let new =
        "[web]\napp_url = \"https://box.invalid/\"\n\n[[plugins]]\nworld = \"source@0.3.0\"\n";
    write(&deployed.scratch, "next-instance.toml", new);
    let carried = deployed.scratch.join("next-instance.toml");
    let out = deployed.deploy(
        &[
            &deployed.new,
            "--instance",
            carried.to_str().expect("utf-8"),
        ],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        subject(&deployed.user()),
        format!("Deploy {}", deployed.new)
    );
    let changed = run_git(
        &deployed.user(),
        &["show", "--name-only", "--format=", "HEAD"],
    );
    assert_eq!(
        changed, "flake.lock\ninstance.toml",
        "one commit moves both"
    );
    assert_eq!(
        std::fs::read_to_string(deployed.user().join("instance.toml")).expect("the instance"),
        new
    );
    assert_eq!(deployed.lock(), deployed.new);
    assert!(deployed.restarted());
    assert!(
        !std::fs::read_dir(deployed.user())
            .expect("the user repository")
            .filter_map(Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".deploy-instance")),
        "the checked copy is gone"
    );
    assert_eq!(
        deployed.origin_subject(),
        format!("Deploy {}", deployed.new)
    );
}

/// A deploy that carried its instance and never ran is reverted whole: the revert puts the
/// lock and the instance back in one commit, so the old units keep reading the file they read.
#[test]
fn deploy_reverted_puts_its_carried_instance_back_with_the_lock() {
    let deployed = Box_::new("carried-revert");
    deployed.set("next-result", "failed");
    let before =
        std::fs::read_to_string(deployed.user().join("instance.toml")).expect("the instance");
    write(
        &deployed.scratch,
        "next-instance.toml",
        "[web]\napp_url = \"https://box.invalid/\"\n# the new key\n",
    );
    let carried = deployed.scratch.join("next-instance.toml");
    let out = deployed.deploy(
        &[
            &deployed.new,
            "--instance",
            carried.to_str().expect("utf-8"),
        ],
        &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "5")],
    );
    assert!(!out.status.success());
    assert_eq!(
        subject(&deployed.user()),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    let reverted = run_git(
        &deployed.user(),
        &["show", "--name-only", "--format=", "HEAD"],
    );
    assert_eq!(reverted, "flake.lock\ninstance.toml");
    assert_eq!(
        std::fs::read_to_string(deployed.user().join("instance.toml")).expect("the instance"),
        before
    );
    assert_eq!(deployed.lock(), deployed.old);
    assert!(!deployed.restarted());
}

/// A carried instance the new binary refuses moves nothing, and the live one is untouched.
#[test]
fn deploy_refuses_a_carried_instance_the_new_app_refuses() {
    let deployed = Box_::new("carried-refused");
    let before =
        std::fs::read_to_string(deployed.user().join("instance.toml")).expect("the instance");
    write(
        &deployed.scratch,
        "next-instance.toml",
        "[web]\napp_url = \"https://box.invalid/\"\n\n[[plugins]]\nworld = \"source@0.2.0\"\n",
    );
    let carried = deployed.scratch.join("next-instance.toml");
    let out = deployed.deploy(
        &[
            &deployed.new,
            "--instance",
            carried.to_str().expect("utf-8"),
        ],
        &[],
    );
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(stderr.contains("refuses the carried instance"), "{stderr}");
    assert!(stderr.contains("Nothing moved"), "{stderr}");
    assert_eq!(subject(&deployed.user()), "seed");
    assert_eq!(
        std::fs::read_to_string(deployed.user().join("instance.toml")).expect("the instance"),
        before
    );
}

/// A run that links a build other than the one the deploy made is a finding, not a deploy:
/// the web is not restarted and the lock is reverted.
#[test]
fn deploy_refuses_a_run_whose_builds_are_not_its_own() {
    let deployed = Box_::new("drift");
    deployed.set("next-result", "success");
    deployed.set("drift", "");
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "5")]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("which the deploy built"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.restarted(), "{}", deployed.log());
    assert_eq!(
        subject(&deployed.user()),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    assert_eq!(deployed.lock(), deployed.old);
}

/// The run a `start --no-block` asked for has not begun at the first read, which shows the
/// previous run's success; the run that does begin fails, so the web is never restarted and
/// the deploy is reverted in a commit of its own.
#[test]
fn deploy_never_restarts_onto_a_stale_success_and_reverts_the_lock() {
    let deployed = Box_::new("stale");
    deployed.set("next-result", "failed");
    // A short wait, so a revert whose run never comes fails quickly.
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "5")]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("Result=failed"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.restarted(), "{}", deployed.log());
    let user = deployed.user();
    assert_eq!(
        subject(&user),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    assert_eq!(
        run_git(&user, &["log", "-1", "--format=%s", "HEAD~1"]),
        format!("Deploy {}", deployed.new),
        "history only grows"
    );
    assert_eq!(deployed.lock(), deployed.old);
    assert_eq!(
        deployed.origin_subject(),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    assert_eq!(
        std::fs::canonicalize(deployed.scratch.join("roots").join("previous-bin"))
            .expect("the previous binary stays rooted"),
        PathBuf::from(format!("{}-bin", deployed.store(&deployed.old).display())),
    );
}

#[test]
fn deploy_without_a_run_after_the_move_times_out_reverts_and_restarts_nothing() {
    let deployed = Box_::new("no-run");
    // No run comes for the revert either, so its own wait is short too.
    let out = deployed.deploy(
        &[&deployed.new],
        &[
            ("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "1"),
            ("SUZATARY_DEPLOY_REVERT_TIMEOUT", "1"),
        ],
    );
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("no suzatary-roots run started after the move"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.restarted());
    assert_eq!(deployed.lock(), deployed.old);
}

#[test]
fn deploy_lets_a_run_under_way_finish_before_it_moves_the_lock() {
    let deployed = Box_::new("busy");
    deployed.set("busy", "3");
    deployed.set("next-result", "success");
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let log = deployed.log();
    let first_end = log.find("roots run ends").expect("the earlier run ends");
    let moved = log
        .find(&format!("show lock={}", deployed.core(&deployed.new)))
        .expect("the lock moves");
    assert!(first_end < moved, "{log}");
}

#[test]
fn deploy_of_the_locked_rev_commits_nothing_and_still_redeploys() {
    let deployed = Box_::new("again");
    deployed.set("next-result", "success");
    let out = deployed.deploy(&[&deployed.old], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(subject(&deployed.user()), "seed");
    assert!(deployed.restarted());
}

/// Every refusal comes before anything moves: a malformed rev, a rev never published (which
/// Nix's cache would have answered), a rev before the lock interface, and Nix refusing the
/// pin.
#[test]
fn deploy_refuses_before_moving_anything() {
    let deployed = Box_::new("refused");
    for args in [
        &[][..],
        &["abc1234"][..],
        &[deployed.new.as_str(), deployed.new.as_str()][..],
    ] {
        let out = deployed.deploy(args, &[]);
        assert_eq!(out.status.code(), Some(64), "{args:?}");
    }
    // A short wait, so a refusal that failed to refuse ends quickly in a timeout.
    let short = [("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "1")];
    for (rev, says) in [
        (
            &deployed.side,
            "is neither a commit of the first-parent chain",
        ),
        (&deployed.pre, "predates deploys by lock"),
    ] {
        let out = deployed.deploy(&[rev], &short);
        assert!(!out.status.success());
        assert!(text(&out.stderr).contains(says), "{}", text(&out.stderr));
        assert!(text(&out.stderr).contains("Nothing moved"));
    }
    deployed.set("lock-refuses", "");
    let out = deployed.deploy(&[&deployed.new], &short);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("Nothing moved"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(subject(&deployed.user()), "seed");
    assert_eq!(deployed.lock(), deployed.old);
    assert!(!deployed.log().contains("start --no-block"));

    // A box whose module declares no unit part of suzatary-app.target.
    deployed.set("consists", "");
    let out = deployed.deploy(&[&deployed.new], &short);
    assert!(!out.status.success());
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("no unit is part of suzatary-app.target"),
        "{stderr}"
    );
    assert!(stderr.contains("Nothing moved"), "{stderr}");
    assert_eq!(deployed.lock(), deployed.old);
}

/// Once the roots run links the new app, every running unit of suzatary-app.target is
/// restarted onto it and checked to run it; a stopped one stays down; one that will not
/// restart, listed before the web, fails the deploy by name after the web restarted, and the
/// deploy, live by then, is kept.
#[test]
fn deploy_restarts_every_running_unit_that_runs_the_app() {
    let watcher = "suzatary-mail-idle.service";
    for (case, state, refuses) in [
        ("watching", "active", false),
        ("crash-looping", "activating", false),
        ("no-account", "inactive", false),
        ("stopped-broken", "failed", false),
        ("wont-restart", "active", true),
    ] {
        let deployed = Box_::new(&format!("units-{case}"));
        deployed.set("next-result", "success");
        deployed.set(&format!("state-{watcher}"), state);
        if refuses {
            deployed.set("wont-restart", watcher);
            deployed.set("consists", &format!("{watcher} suzatary-web.service"));
        }
        let out = deployed.deploy(&[&deployed.new], &[]);
        let (stderr, log) = (text(&out.stderr), deployed.log());
        let restart = log.find(&format!("try-restart {watcher}"));
        let rooted = std::fs::canonicalize(
            deployed
                .scratch
                .join("roots")
                .join("suzatary-mail-idle-app"),
        )
        .ok();
        assert!(deployed.restarted(), "{case}: {log}");
        if refuses {
            assert!(!out.status.success(), "{case}");
            assert!(
                stderr.contains(&format!("{watcher} would not restart")),
                "{case}: {stderr}"
            );
            assert!(
                stderr.contains("suzatary-web.service restarted onto"),
                "{case}: the record names what runs the new rev: {stderr}"
            );
            assert_eq!(
                deployed.origin_subject(),
                format!("Deploy {}", deployed.new),
                "{case}: the units run it, so the record is pushed"
            );
        } else if state == "active" || state == "activating" {
            assert!(out.status.success(), "{case}: {stderr}");
            let ended = log.find("roots run ends").expect("the run ends");
            assert!(restart.is_some_and(|at| ended < at), "{case}: {log}");
            assert_eq!(rooted, Some(deployed.store(&deployed.new)), "{case}");
        } else {
            assert!(out.status.success(), "{case}: {stderr}");
            assert!(restart.is_none(), "{case}: {log}");
            assert!(
                stderr.contains(&format!("left {watcher} {state}")),
                "{case}: {stderr}"
            );
            assert_eq!(rooted, None, "{case}: the watcher never started");
        }
    }
}

/// A commit that fails (a hook refuses it) wrote nothing: the working tree's lock is HEAD's as
/// it was, so the path unit has nothing to start a run on.
#[test]
fn deploy_whose_commit_fails_writes_nothing() {
    let deployed = Box_::new("commit-fails");
    deployed.set("next-result", "success");
    let _ = suzatary_testkit::fake_program(
        &deployed.user().join(".git/hooks"),
        "pre-commit",
        "#!/bin/sh\nexit 1\n",
    );
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("would not commit"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(deployed.lock(), deployed.old);
    assert_eq!(subject(&deployed.user()), "seed");
    assert!(!deployed.log().contains("start --no-block"));
}

/// A run that succeeds but links some other tree is no deploy of this rev: the web is not
/// restarted onto it, and the lock is reverted.
#[test]
fn deploy_checks_the_linked_app_before_restarting_the_web() {
    let deployed = Box_::new("links-other");
    deployed.set("next-result", "success");
    let other = deployed
        .scratch
        .join("store")
        .join(format!("app-{}", deployed.side));
    deployed.set("links", &other.display().to_string());
    // A short wait, so a revert whose run never comes fails quickly.
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "5")]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("the app the lock names"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.restarted(), "{}", deployed.log());
    assert_eq!(deployed.lock(), deployed.old);
    assert_eq!(deployed.app(), deployed.store(&deployed.old));
    assert!(
        text(&out.stderr).contains("roots/app is the previous app again"),
        "{}",
        text(&out.stderr)
    );
}

/// A run still building the new rev when the deploy gives up would link it when it ends; the
/// revert stops it, then a run of its own relinks the previous app. Only the forward wait is
/// short: the relink keeps its own bound, so a loaded machine still sees it through.
#[test]
fn deploy_reverted_mid_build_stops_the_run_and_relinks_the_previous_app() {
    let deployed = Box_::new("in-flight");
    deployed.set("next-result", "success");
    // The path unit begins the long build on the commit's first read, whatever the load.
    deployed.set("path-unit", "");
    deployed.set("long-run", &deployed.core(&deployed.new));
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "1")]);
    assert!(!out.status.success());
    assert!(!deployed.restarted(), "{}", deployed.log());
    assert_eq!(
        subject(&deployed.user()),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    assert_eq!(deployed.app(), deployed.store(&deployed.old));
    assert!(
        text(&out.stderr).contains("roots/app is the previous app again"),
        "{}",
        text(&out.stderr)
    );
}

/// A revert whose wait dies (here the run it could not stop is still building) says so and
/// still records the revert: the push is the one thing a failed deploy must not lose.
#[test]
fn deploy_whose_revert_cannot_relink_still_pushes_the_revert() {
    let deployed = Box_::new("revert-dies");
    deployed.set("next-result", "success");
    deployed.set("path-unit", "");
    deployed.set("long-run", &deployed.core(&deployed.new));
    deployed.set("stop-fails", "");
    let out = deployed.deploy(
        &[&deployed.new],
        &[
            ("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "1"),
            ("SUZATARY_DEPLOY_REVERT_TIMEOUT", "1"),
        ],
    );
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("no run relinked the previous app"),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(
        deployed.origin_subject(),
        format!("Revert \"Deploy {}\"", deployed.new),
        "the revert is pushed"
    );
    assert_eq!(deployed.lock(), deployed.old);
}

/// Once the web restarts the units run the new app, so a failure after it is said, not
/// reverted: the log names what runs.
#[test]
fn deploy_fails_when_the_web_runs_another_app() {
    let deployed = Box_::new("other-app");
    deployed.set("next-result", "success");
    let running = deployed
        .scratch
        .join("store")
        .join(format!("app-{}", deployed.old));
    deployed.set("web-app", &running.display().to_string());
    // A short wait for the units, so the web's stale root ends it quickly.
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_UNITS_TIMEOUT", "1")]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("the app the lock names"),
        "{}",
        text(&out.stderr)
    );
    assert!(
        text(&out.stderr).contains(&format!("re-run scripts/deploy.sh {}", deployed.new)),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(
        subject(&deployed.user()),
        format!("Deploy {}", deployed.new)
    );
    assert_eq!(
        deployed.origin_subject(),
        format!("Deploy {}", deployed.new),
        "the units run it, so the record is pushed"
    );
}

/// TERM while the deploy waits for the roots run is a failure like any other: the deploy is
/// reverted, the previous app relinked and the revert pushed.
#[test]
fn deploy_terminated_mid_wait_reverts_and_pushes() {
    let deployed = Box_::new("term-wait");
    deployed.set("next-result", "success");
    deployed.set("long-run", &deployed.core(&deployed.new));
    deployed.set("term-at-move", &deployed.core(&deployed.old));
    // Long enough that the signal, not the wait, ends the deploy.
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "60")]);
    assert_eq!(out.status.code(), Some(143), "{}", text(&out.stderr));
    assert!(
        deployed.log().contains("deploy sent TERM"),
        "{}",
        deployed.log()
    );
    assert!(!deployed.restarted(), "{}", deployed.log());
    assert_eq!(
        deployed.origin_subject(),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    assert_eq!(deployed.lock(), deployed.old);
    assert_eq!(deployed.app(), deployed.store(&deployed.old));
}

/// TERM after the web restarted onto the new app keeps the deploy commit, pushes it, and says
/// how to finish.
#[test]
fn deploy_terminated_after_the_restart_pushes_the_deploy() {
    let deployed = Box_::new("term-live");
    deployed.set("next-result", "success");
    deployed.set("term-at-restart", "");
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert_eq!(out.status.code(), Some(143), "{}", text(&out.stderr));
    assert!(deployed.restarted());
    assert!(
        text(&out.stderr).contains(&format!("re-run scripts/deploy.sh {}", deployed.new)),
        "{}",
        text(&out.stderr)
    );
    assert_eq!(
        deployed.origin_subject(),
        format!("Deploy {}", deployed.new)
    );
    assert_eq!(deployed.lock(), deployed.new);
}

/// An output the roots run builds and the deploy did not (the system module's list ahead of
/// this one) is refused rather than built cold under the short wait.
#[test]
fn deploy_refuses_a_run_with_an_output_it_did_not_build() {
    let deployed = Box_::new("extra");
    deployed.set("next-result", "success");
    deployed.set("extra", "");
    let out = deployed.deploy(&[&deployed.new], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "5")]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("an output only one side builds"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.restarted(), "{}", deployed.log());
    assert_eq!(deployed.lock(), deployed.old);
}

#[test]
fn deploy_refuses_while_another_runs() {
    let deployed = Box_::new("concurrent");
    let held = deployed.user().join(".git").join("suzatary-deploy.lock");
    let mut holder = Command::new("flock")
        .arg(&held)
        .args(["sleep", "30"])
        .spawn()
        .expect("flock holds the deploy lock");
    while !held.exists() {
        std::thread::yield_now();
    }
    // flock creates the file before it takes the lock; give it the lock.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let out = deployed.deploy(&[&deployed.new], &[]);
    let _ = holder.kill();
    let _ = holder.wait();
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("another deploy is running"),
        "{}",
        text(&out.stderr)
    );
}

/// The stub commands the deploy runs, in `bin`.
fn stubs(bin: &Path) {
    for (stub, body) in [
        ("systemctl", SYSTEMCTL),
        ("nix", NIX),
        (
            "nix-store",
            "#!/bin/sh\necho \"nix-store $*\" >>\"$DEPLOY_STATE/log\"\nln -sfn \"$5\" \"$2\"\n",
        ),
        (
            "curl",
            "#!/bin/sh\ncat \"$DEPLOY_STATE/about\" 2>/dev/null || printf 200\n",
        ),
        ("sleep", "#!/bin/sh\nexit 0\n"),
        ("git", GIT),
        ("gh", GH),
    ] {
        let _ = suzatary_testkit::fake_program(bin, stub, body);
    }
}

/// A bare `core` whose master publishes `sources`, in order, each commit of it the source's
/// tree on the one before and naming the source in a `Source:` trailer, as
/// `scripts/train.sh publish` makes it; the objects are made in `app`, the sources' repository.
fn publish(app: &Path, core: &Path, sources: &[&String]) {
    let at = core.display().to_string();
    run_git(app, &["init", "-q", "--bare", "-b", "master", &at]);
    let mut tip = String::new();
    for source in sources {
        let tree = format!("{source}^{{tree}}");
        let trailer = format!("Source: {source}");
        let mut args = vec!["commit-tree", tree.as_str()];
        if !tip.is_empty() {
            args.extend(["-p", tip.as_str()]);
        }
        args.extend(["-m", "Publish", "-m", trailer.as_str()]);
        tip = run_git(app, &args);
    }
    let pushed = format!("{tip}:refs/heads/master");
    run_git(app, &["push", "-q", &at, &pushed]);
}

/// The commit of `core`'s master that publishes the source `rev`.
fn publication_of(core: &Path, rev: &str) -> String {
    run_git(
        core,
        &[
            "log",
            "--format=%H %(trailers:key=Source,valueonly,separator=%x2C)",
            "master",
        ],
    )
    .lines()
    .find_map(|line| line.strip_suffix(&format!(" {rev}")).map(str::to_owned))
    .unwrap_or_else(|| panic!("{rev} is published"))
}

/// The store's trees and their outputs: the `pre` ones before the lock interface, the rest
/// after it, each binary the stub [`SUZATARY`].
fn fill_store(store: &Path, pre: &[String], revs: &[String]) {
    for rev in pre.iter().chain(revs) {
        for suffix in ["", "-bin", "-dist", "-plugins", "-shell"] {
            std::fs::create_dir_all(store.join(format!("app-{rev}{suffix}")))
                .expect("a store path");
        }
        let _ = suzatary_testkit::fake_program(
            &store.join(format!("app-{rev}-bin/bin")),
            "suzatary",
            SUZATARY,
        );
    }
    for rev in revs {
        write(store, &format!("app-{rev}/nix/app-input.nix"), "{ }\n");
    }
}

/// The `git` on this test's own PATH, which the stub [`GIT`] runs.
fn real_git() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("git"))
        .find(|git| git.is_file())
        .expect("git on PATH")
}

/// A command that reads no global or system git configuration and commits as one fixed
/// identity.
fn isolated(mut cmd: Command) -> Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t");
    cmd
}

fn run(cmd: &mut Command, what: &str) -> String {
    let out = cmd.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(out.status.success(), "{what}: {}", text(&out.stderr));
    text(&out.stdout).trim().to_owned()
}

/// The subject of the last commit of the repository at `dir`.
fn subject(dir: &Path) -> String {
    run_git(dir, &["log", "-1", "--format=%s"])
}

fn run_git(cwd: &Path, args: &[&str]) -> String {
    let mut cmd = isolated(Command::new("git"));
    run(cmd.args(args).current_dir(cwd), &format!("git {args:?}"))
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("parent dirs");
    std::fs::write(&path, content).unwrap_or_else(|e| panic!("{rel}: {e}"));
}

fn text(out: &[u8]) -> String {
    String::from_utf8_lossy(out).into_owned()
}

/// The build is unbounded and the old web keeps saving settings into the instance while it
/// runs: a carried file made from the instance before the save would lose it, so the move
/// refuses, naming the change, and nothing moves.
#[test]
fn deploy_refuses_a_carried_instance_when_the_live_one_moved_during_the_build() {
    let deployed = Box_::new("carried-moved");
    deployed.set("next-result", "success");
    deployed.set("save-during-build", "");
    write(
        &deployed.scratch,
        "next-instance.toml",
        "[web]\napp_url = \"https://box.invalid/\"\nzone = \"America/New_York\"\n",
    );
    let carried = deployed.scratch.join("next-instance.toml");
    let out = deployed.deploy(
        &[
            &deployed.new,
            "--instance",
            carried.to_str().expect("utf-8"),
        ],
        &[],
    );
    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("instance.toml moved while"), "{stderr}");
    assert!(
        stderr.contains("saved while it built"),
        "the change is named: {stderr}"
    );
    assert_eq!(deployed.lock(), deployed.old, "the lock did not move");
    assert_eq!(subject(&deployed.user()), "a settings save");
    assert!(
        std::fs::read_to_string(deployed.user().join("instance.toml"))
            .expect("the instance")
            .contains("saved while it built"),
        "the save survives"
    );
    assert!(!deployed.restarted());
}

/// A carried file is read once, at the start: a source that changes or disappears while the
/// deploy runs is checked and committed as it read then, and no copy is left beside the live
/// instance when the deploy is stopped by a signal.
#[test]
fn deploy_reads_its_carried_file_once_and_leaves_no_copy_on_a_signal() {
    let deployed = Box_::new("carried-once");
    deployed.set("next-result", "success");
    let new = "[web]\napp_url = \"https://box.invalid/\"\nzone = \"America/New_York\"\n";
    write(&deployed.scratch, "next-instance.toml", new);
    let carried = deployed.scratch.join("next-instance.toml");
    // Removed by the first build, long before the check and the commit read it.
    deployed.set("rm-during-build", carried.to_str().expect("utf-8"));
    let out = deployed.deploy(
        &[
            &deployed.new,
            "--instance",
            carried.to_str().expect("utf-8"),
        ],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!carried.exists(), "the source went during the build");
    assert_eq!(
        std::fs::read_to_string(deployed.user().join("instance.toml")).expect("the instance"),
        new
    );

    let stopped = Box_::new("carried-term");
    stopped.set("next-result", "success");
    stopped.set("term-at-verify", "");
    write(&stopped.scratch, "next-instance.toml", new);
    let carried = stopped.scratch.join("next-instance.toml");
    let out = stopped.deploy(
        &[&stopped.new, "--instance", carried.to_str().expect("utf-8")],
        &[],
    );
    assert_eq!(out.status.code(), Some(143), "{}", text(&out.stderr));
    assert!(
        !std::fs::read_dir(stopped.user())
            .expect("the user repository")
            .filter_map(Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".deploy-instance")),
        "the checked copy is gone after TERM"
    );
    assert_eq!(stopped.lock(), stopped.old);
}

/// Once the web runs the new app, the way back is a deploy of the previous rev, and when this
/// deploy carried the instance, the previous rev's binary refuses the new file: the hint
/// carries the instance the previous rev read, so it runs as printed.
#[test]
fn deploy_stopped_after_the_restart_says_how_to_go_back_with_the_old_instance() {
    let deployed = Box_::new("carried-back");
    deployed.set("next-result", "success");
    deployed.set("term-at-restart", "");
    write(
        &deployed.scratch,
        "next-instance.toml",
        "[web]\napp_url = \"https://box.invalid/\"\nzone = \"America/New_York\"\n",
    );
    let carried = deployed.scratch.join("next-instance.toml");
    let out = deployed.deploy(
        &[
            &deployed.new,
            "--instance",
            carried.to_str().expect("utf-8"),
        ],
        &[],
    );
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(143), "{stderr}");
    let head = run_git(&deployed.user(), &["rev-parse", "HEAD"]);
    assert!(
        stderr.contains(&format!(
            "git -C {} show {}^:instance.toml >/tmp/instance-before.toml && scripts/deploy.sh {} --instance /tmp/instance-before.toml",
            deployed.user().display(),
            &head[..12],
            deployed.core(&deployed.old)
        )),
        "{stderr}"
    );
}

/// A user repository whose flake still declares the app's own origin moves to the publication
/// in one deploy carrying its flake: `--flake` resolves the source in the input the carried
/// flake declares, pins its publication and commits the flake beside the lock.
#[test]
fn deploy_moves_the_input_to_the_publication_carrying_its_flake() {
    let deployed = Box_::before_the_move("move");
    deployed.set("next-result", "success");
    let before = std::fs::read_to_string(deployed.user().join("flake.nix")).expect("the flake");
    let carried = deployed.scratch.join("next-flake.nix");
    let declared = declaring(&deployed.scratch.join("core.git"));
    std::fs::write(&carried, &declared).expect("the carried flake");
    let out = deployed.deploy(
        &[&deployed.new, "--flake", carried.to_str().expect("utf-8")],
        &[],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        subject(&deployed.user()),
        format!("Deploy {}", deployed.new)
    );
    assert_eq!(
        run_git(
            &deployed.user(),
            &["show", "--name-only", "--format=", "HEAD"]
        ),
        "flake.lock\nflake.nix",
        "one commit moves both"
    );
    assert_eq!(deployed.lock_holds(), deployed.core(&deployed.new));
    assert_eq!(
        std::fs::read_to_string(deployed.user().join("flake.nix")).expect("the flake"),
        declared
    );
    assert_ne!(before, declared);
    assert!(deployed.restarted());
}

/// A deploy that carried its flake and never ran is reverted whole: the flake and the lock
/// go back in one commit.
#[test]
fn deploy_reverted_puts_its_carried_flake_back_with_the_lock() {
    let deployed = Box_::before_the_move("move-revert");
    deployed.set("next-result", "failed");
    let before = std::fs::read_to_string(deployed.user().join("flake.nix")).expect("the flake");
    let carried = deployed.scratch.join("next-flake.nix");
    std::fs::write(&carried, declaring(&deployed.scratch.join("core.git")))
        .expect("the carried flake");
    let out = deployed.deploy(
        &[&deployed.new, "--flake", carried.to_str().expect("utf-8")],
        &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "5")],
    );
    assert!(!out.status.success());
    assert_eq!(
        subject(&deployed.user()),
        format!("Revert \"Deploy {}\"", deployed.new)
    );
    assert_eq!(
        std::fs::read_to_string(deployed.user().join("flake.nix")).expect("the flake"),
        before
    );
    assert_eq!(deployed.lock_holds(), deployed.old);
    assert_eq!(deployed.app(), deployed.store_of(&deployed.old));
}

/// A rev that is a commit of the declared branch's first-parent chain is pinned as it is, the
/// commit a user of the published repository sees, with no trailer read.
#[test]
fn deploy_pins_a_commit_of_the_branch_as_it_is() {
    let deployed = Box_::new("as-is");
    deployed.set("next-result", "success");
    let core = deployed.core(&deployed.new);
    let out = deployed.deploy(&[&core], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(deployed.lock_holds(), core);
    assert_eq!(subject(&deployed.user()), format!("Deploy {core}"));
    assert!(
        !text(&out.stderr).contains("is published as"),
        "{}",
        text(&out.stderr)
    );
}

/// A fix not yet landed is deployed from its own branch with `--ref`: the first-parent check
/// reads that branch, and the pin carries it; without `--ref` the same rev is refused, naming
/// both lookups.
#[test]
fn deploy_pins_a_fix_branch_with_ref() {
    let deployed = Box_::new("ref");
    deployed.set("next-result", "success");
    let core = deployed.scratch.join("core.git");
    let tip = run_git(&core, &["rev-parse", "master"]);
    let tree = format!("{tip}^{{tree}}");
    let fix = run_git(&core, &["commit-tree", &tree, "-p", &tip, "-m", "a fix"]);
    run_git(&core, &["update-ref", "refs/heads/fix/a", &fix]);
    fill_store(
        &deployed.scratch.join("store"),
        &[],
        std::slice::from_ref(&fix),
    );

    let refused = deployed.deploy(&[&fix], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "1")]);
    assert!(!refused.status.success());
    let said = text(&refused.stderr);
    assert!(
        said.contains("is neither a commit of the first-parent chain"),
        "{said}"
    );
    assert!(said.contains("Source: trailer"), "{said}");
    assert!(said.contains("--ref"), "{said}");
    assert_eq!(subject(&deployed.user()), "seed");

    let out = deployed.deploy(&[&fix, "--ref", "fix/a"], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(deployed.lock_holds(), fix);
    assert!(
        deployed.log().contains("ref=fix/a&rev="),
        "{}",
        deployed.log()
    );
}

/// A rev publish passed over (a `Skipped:` trailer names it) is refused by name: its tree never
/// passed the leak guard, so it never goes live.
#[test]
fn deploy_refuses_a_skipped_rev_by_name() {
    let deployed = Box_::new("skipped");
    deployed.set("next-result", "success");
    let core = deployed.scratch.join("core.git");
    let tip = run_git(&core, &["rev-parse", "master"]);
    let tree = format!("{tip}^{{tree}}");
    let trailers = format!("Source: {}\nSkipped: {}", deployed.new, deployed.side);
    let row = run_git(
        &core,
        &[
            "commit-tree",
            &tree,
            "-p",
            &tip,
            "-m",
            "Publish",
            "-m",
            &trailers,
        ],
    );
    run_git(&core, &["update-ref", "refs/heads/master", &row]);

    let out = deployed.deploy(&[&deployed.side], &[("SUZATARY_DEPLOY_ROOTS_TIMEOUT", "1")]);
    assert!(!out.status.success());
    let said = text(&out.stderr);
    assert!(said.contains("was skipped by publish"), "{said}");
    assert!(said.contains("Nothing moved"), "{said}");
    assert_eq!(subject(&deployed.user()), "seed");
}

/// A remote's master with a first-parent chain `zero`, `first`, `merged` (the merge of a series
/// holding `side`), a branch `elsewhere` holding `unfetched`, and a mirror nothing fetched yet.
struct Followed {
    _scratch: Scratch,
    mirror: PathBuf,
    remote: String,
    zero: String,
    first: String,
    side: String,
    merged: String,
    unfetched: String,
}

impl Followed {
    fn new(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let s: &Path = &scratch;
        run_git(s, &["init", "-q", "-b", "master", "app"]);
        let app = s.join("app");
        let commit = |file: &str| {
            write(&app, file, file);
            run_git(&app, &["add", "-A"]);
            run_git(&app, &["commit", "-q", "-m", file]);
            run_git(&app, &["rev-parse", "HEAD"])
        };
        let zero = commit("zero");
        let first = commit("first");
        run_git(&app, &["checkout", "-q", "-b", "pr"]);
        let side = commit("side");
        run_git(&app, &["checkout", "-q", "master"]);
        run_git(&app, &["merge", "-q", "--no-ff", "-m", "merge", "pr"]);
        let merged = run_git(&app, &["rev-parse", "HEAD"]);
        run_git(&app, &["checkout", "-q", "-b", "elsewhere"]);
        let unfetched = commit("unfetched");
        run_git(&app, &["checkout", "-q", "master"]);
        Self {
            mirror: s.join("user.git").join("suzatary-followed.git"),
            remote: format!("file://{}", app.display()),
            _scratch: scratch,
            zero,
            first,
            side,
            merged,
            unfetched,
        }
    }

    /// `scripts/followed.sh <verb> <mirror> <args>…`.
    fn run(&self, verb: &str, args: &[&str]) -> Output {
        isolated(Command::new(repo().join("scripts").join("followed.sh")))
            .arg(verb)
            .arg(&self.mirror)
            .args(args)
            .output()
            .expect("runs")
    }

    fn fetch(&self, branch: &str) {
        let fetched = self.run("fetch", &[&self.remote, branch]);
        assert!(fetched.status.success(), "{}", text(&fetched.stderr));
    }
}

/// The one check the deploy and the roots run make (`scripts/followed.sh check`): a revision of
/// the followed branch's first-parent chain, as this box last fetched it, from the remote it
/// follows; never a merged series' commit, another remote's, a revision expression for a
/// branch, or anything before a fetch.
#[test]
fn followed_takes_only_the_first_parent_chain_of_the_remote_it_follows() {
    let f = Followed::new("followed");
    let check = |remote: &str, branch: &str, rev: &str| f.run("check", &[remote, branch, rev]);
    let before = check(&f.remote, "master", &f.first);
    assert_eq!(before.status.code(), Some(1), "nothing fetched yet");
    assert!(text(&before.stderr).contains("follows no remote yet"));

    f.fetch("master");
    for rev in [&f.first, &f.merged] {
        assert!(check(&f.remote, "master", rev).status.success(), "{rev}");
    }
    for (why, remote, branch, rev) in [
        (
            "a merged series' commit",
            f.remote.as_str(),
            "master",
            &f.side,
        ),
        (
            "a branch never fetched",
            f.remote.as_str(),
            "elsewhere",
            &f.unfetched,
        ),
        ("another remote", "file:///elsewhere", "master", &f.first),
        (
            "a revision expression for a branch",
            f.remote.as_str(),
            "master^2",
            &f.side,
        ),
    ] {
        assert_eq!(check(remote, branch, rev).status.code(), Some(1), "{why}");
    }
}

/// What the roots run builds (`scripts/followed.sh admit`): a revision a deploy pinned from the
/// branch this box now follows, or the pin before (a revert's); never an older commit no deploy
/// chose, nor a branch an earlier `--ref` deploy left behind, and nothing a lock names becomes
/// the way back.
#[test]
fn the_roots_run_builds_only_what_a_deploy_pinned() {
    let f = Followed::new("pinned");
    f.fetch("master");
    let admits =
        |branch: &str, rev: &str| f.run("admit", &[&f.remote, branch, rev]).status.success();
    let pin = |rev: &str, branch: &str| {
        let out = f.run("pin", &[rev, branch]);
        assert!(out.status.success(), "{}", text(&out.stderr));
    };
    let unpinned = f.run("admit", &[&f.remote, "master", &f.merged]);
    assert_eq!(unpinned.status.code(), Some(1), "nothing pinned yet");
    assert!(text(&unpinned.stderr).contains("no deploy has pinned"));

    pin(&f.first, "master");
    pin(&f.merged, "master");
    assert!(admits("master", &f.merged), "what the deploy pinned");
    assert!(admits("master", &f.first), "the pin before, its revert's");
    assert!(
        !admits("master", &f.zero),
        "an older commit no deploy chose"
    );
    assert!(
        !admits("master^2", &f.side),
        "a revision expression for a branch"
    );
    assert!(!admits("master", &f.side), "a merged series' commit");
    pin(&f.merged, "master");
    assert!(admits("master", &f.first), "a redeploy keeps its way back");

    f.fetch("elsewhere");
    pin(&f.unfetched, "elsewhere");
    assert!(admits("elsewhere", &f.unfetched), "a --ref deploy's pin");
    assert!(admits("master", &f.merged), "and its way back");
    pin(&f.merged, "master");
    pin(&f.first, "master");
    assert!(admits("master", &f.merged), "back on master");
    assert!(
        !admits("elsewhere", &f.unfetched),
        "a branch an earlier --ref deploy left behind"
    );
    assert!(
        !admits("master", &f.zero),
        "nothing a lock names becomes the way back"
    );
}

/// Where the publication run that builds the pinned commit succeeded, the deploy waits for it
/// and its build downloads; where that run failed, never came, or GitHub could not be asked, it
/// builds here and says which. Either way it deploys. A deploy whose builds the store holds
/// asks GitHub nothing.
#[test]
fn deploy_waits_for_the_publication_run_and_says_where_its_builds_come_from() {
    let cases = [
        (
            "cache-success",
            Some("queued \nin_progress \ncompleted success"),
            "succeeded: its builds come from the binary cache",
        ),
        (
            "cache-failure",
            Some("in_progress \ncompleted failure"),
            "ended failure: building it here",
        ),
        (
            "cache-silence",
            None,
            "appeared within 30s: building it here",
        ),
        (
            "cache-slow",
            Some("in_progress "),
            "was still in_progress after 40s: building it here",
        ),
    ];
    for (name, runs, said) in cases {
        let deployed = Box_::new(name);
        deployed.set("next-result", "success");
        if let Some(runs) = runs {
            deployed.set("runs", runs);
        }
        let core = deployed.core(&deployed.new);
        let out = deployed.deploy(
            &[&deployed.new],
            &[
                ("SUZATARY_DEPLOY_PUBLICATIONS", "owner/core"),
                ("SUZATARY_DEPLOY_CACHE_APPEAR", "30"),
                ("SUZATARY_DEPLOY_CACHE_TIMEOUT", "40"),
            ],
        );
        let stderr = text(&out.stderr);
        assert!(out.status.success(), "{name}: {stderr}");
        assert!(stderr.contains(said), "{name}: {stderr}");
        assert_eq!(deployed.lock_holds(), core, "{name}");
        let log = deployed.log();
        let asked = log
            .find(
                "gh run list --repo owner/core --workflow publication.yml --event push --limit 50 ",
            )
            .unwrap_or_else(|| panic!("{name}: the run of the publication is asked for:\n{log}"));
        assert!(
            log.contains(&format!("Publication cache {core}")),
            "{name}: the run is found by its run-name, which names the commit:\n{log}"
        );
        let built = log
            .find("nix build")
            .unwrap_or_else(|| panic!("{name}: the deploy builds:\n{log}"));
        assert!(
            asked < built,
            "{name}: the wait comes before the build:\n{log}"
        );
    }

    let deployed = Box_::new("cache-unasked");
    deployed.set("next-result", "success");
    deployed.set("gh-down", "");
    let out = deployed.deploy(
        &[&deployed.new],
        &[
            ("SUZATARY_DEPLOY_PUBLICATIONS", "owner/core"),
            ("SUZATARY_DEPLOY_CACHE_POLL", "0"),
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("could not ask GitHub for the publication run")
            && text(&out.stderr).contains("error connecting to api.github.com"),
        "{}",
        text(&out.stderr)
    );

    let deployed = Box_::new("cache-held");
    deployed.set("next-result", "success");
    deployed.set("in-store", "");
    let out = deployed.deploy(
        &[&deployed.new],
        &[("SUZATARY_DEPLOY_PUBLICATIONS", "owner/core")],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("the store holds the builds"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.log().contains("gh "), "{}", deployed.log());

    // An input off GitHub has no publication run to ask for.
    let deployed = Box_::new("cache-elsewhere");
    deployed.set("next-result", "success");
    let out = deployed.deploy(&[&deployed.new], &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("is not on GitHub"),
        "{}",
        text(&out.stderr)
    );
    assert!(!deployed.log().contains("gh "), "{}", deployed.log());
}
