//! `suzatary doctor` over a fresh `suzatary init` passes its startup checks, through the verb
//! an operator runs next, over a temp directory.

use clap::Parser;
use suzatary_cli::commands::{doctor, init};
use suzatary_cli::instance::Instance;

/// A thin wrapper so a test can build [`init::Args`] the way `main.rs`
/// does — through `clap`'s own parser — without `Args`'s fields needing to be
/// `pub` for an external test crate to reach them.
#[derive(Debug, Parser)]
struct Wrapper {
    #[command(flatten)]
    args: init::Args,
}

fn parse(argv: &[&str]) -> init::Args {
    let mut full = vec!["suzatary-init-test"];
    full.extend_from_slice(argv);
    Wrapper::parse_from(full).args
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "suzatary-cli-init-doctor-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp dir");
    dir
}

/// The repository this test file lives in.
fn repo() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("app/cli/../.. is the repository")
        .to_path_buf()
}

/// Every answer `instance.sample.toml` was generated from: a placeholder deployment with no
/// real address in it, `--repo "."` so the committed file reads exactly as `init` wrote it.
fn sample_args() -> init::Args {
    parse(&[
        "--name",
        "Ada",
        "--city",
        "London",
        "--zone",
        "Europe/London",
        "--box",
        "ada-box.tailnet.ts.net",
        "--login",
        "her@example.test",
        "--email",
        "her@example.test",
        "--mail-provider",
        "gmail",
        "--repo",
        ".",
    ])
}

/// `instance.sample.toml` is `suzatary init`'s own output, byte for byte, so it cannot drift
/// from what the verb writes; the fresh `init` runs over a temp directory so its chain
/// directories stay out of the repository.
#[test]
fn the_committed_sample_equals_a_fresh_init() {
    let dir = temp_dir("sample");
    let instance_path = dir.join("instance.toml");
    init::run_keyed(
        &instance_path,
        sample_args(),
        &env_beside(&instance_path),
        |_| Ok(cast_keys()),
    )
    .expect("a fresh init");

    let fresh = std::fs::read_to_string(&instance_path).expect("the fresh file reads");
    let committed = std::fs::read_to_string(repo().join("instance.sample.toml"))
        .expect("instance.sample.toml is committed at the repository root");
    assert_eq!(
        fresh, committed,
        "instance.sample.toml has drifted from what `suzatary init` writes today \
         — regenerate it with the same answers this test uses"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A fresh `init`, then `doctor`, over a deployment nothing has hand-edited.
#[test]
fn doctor_over_a_fresh_init_passes_its_startup_checks() {
    let dir = temp_dir("doctor");
    let instance_path = dir.join("instance.toml");
    let repo = dir.to_str().expect("a utf-8 temp path");
    let args = parse(&[
        "--name",
        "Ada",
        "--city",
        "London",
        "--zone",
        "Europe/London",
        "--box",
        "ada-box.tailnet.ts.net",
        "--login",
        "her@example.test",
        "--email",
        "her@example.test",
        "--mail-provider",
        "gmail",
        "--repo",
        repo,
    ]);
    init::run_keyed(&instance_path, args, &env_beside(&instance_path), |_| {
        Ok(cast_keys())
    })
    .expect("a fresh init");

    // The startup check `suzatary serve` refuses to start without: no dead writer, no
    // unrostered actor, no reserved or repeated chain name, a roster with no duplicate address.
    let read = Instance::read(&instance_path, &env_beside(&instance_path));
    assert!(read.is_ok(), "{:?}", read.err());

    // And `doctor` over it, the operator-facing half of the same claim.
    doctor::run(&instance_path, &doctor::Args::default()).expect("doctor never fails outright");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The deployed binary runs the code its unit's wrapper rooted: `SUZATARY_REPO` is its code
/// root over the instance's `repo`, which then names only a command line's checkout.
#[test]
fn the_binary_takes_its_code_root_from_the_unit_over_the_instance() {
    let dir = temp_dir("adopted-code");
    let instance_path = dir.join("instance.toml");
    let args = parse(&[
        "--name",
        "Ada",
        "--city",
        "London",
        "--zone",
        "Europe/London",
        "--box",
        "ada-box.tailnet.ts.net",
        "--login",
        "her@example.test",
        "--email",
        "her@example.test",
        "--mail-provider",
        "gmail",
        "--repo",
        dir.to_str().expect("a utf-8 temp path"),
    ]);
    init::run_keyed(&instance_path, args, &env_beside(&instance_path), |_| {
        Ok(cast_keys())
    })
    .expect("a fresh init");
    let doctor = |adopted: Option<&str>| {
        let mut cmd = crate::binary::keyed();
        cmd.arg("--instance").arg(&instance_path).arg("doctor");
        if let Some(root) = adopted {
            cmd.env("SUZATARY_REPO", root);
        }
        let out = cmd.output().expect("the binary runs");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let line = |printed: &str| {
        printed
            .lines()
            .find(|line| line.trim_start().starts_with("repo "))
            .map(|line| {
                line.trim_start()
                    .trim_start_matches("repo")
                    .trim()
                    .to_owned()
            })
    };
    let canonical = std::fs::canonicalize(&dir).expect("the dir exists");
    assert_eq!(line(&doctor(None)), Some(canonical.display().to_string()));
    assert_eq!(
        line(&doctor(Some("/nix/store/0000-app"))),
        Some("/nix/store/0000-app".to_owned())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fresh init with a separate `--data` puts the chain directories there and nothing under
/// the code root: a chain's directory is data, resolved on `DataRoot`. The roots must differ,
/// or a writer that stayed on `CodeRoot` would pass.
#[test]
fn a_separate_data_root_gets_the_chain_directories_and_the_code_root_gets_none() {
    let code_dir = temp_dir("data-root-code");
    let data_dir = temp_dir("data-root-data");
    let instance_path = code_dir.join("instance.toml");
    let args = parse(&[
        "--name",
        "Ada",
        "--city",
        "London",
        "--zone",
        "Europe/London",
        "--box",
        "ada-box.tailnet.ts.net",
        "--login",
        "her@example.test",
        "--email",
        "her@example.test",
        "--mail-provider",
        "gmail",
        "--repo",
        code_dir.to_str().expect("a utf-8 temp path"),
        "--data",
        data_dir.to_str().expect("a utf-8 temp path"),
    ]);
    init::run_keyed(&instance_path, args, &env_beside(&instance_path), |_| {
        Ok(cast_keys())
    })
    .expect("a fresh init with a separate data root");

    let read = Instance::read(&instance_path, &env_beside(&instance_path)).expect("it reads back");
    let expected_data = std::fs::canonicalize(&data_dir).expect("the data dir exists");
    for chain in &read.chains {
        assert!(chain.events.is_dir(), "{:?}", chain.events);
        assert!(
            chain.events.starts_with(&expected_data),
            "{:?} did not land under the data root {:?}",
            chain.events,
            expected_data
        );
    }

    // And nothing leaked into the code root.
    for name in ["events", "events-suzatary"] {
        assert!(
            !code_dir.join(name).exists(),
            "{name} leaked into the code root {code_dir:?}"
        );
    }

    let _ = std::fs::remove_dir_all(&code_dir);
    let _ = std::fs::remove_dir_all(&data_dir);
}

/// The cast's deployment keys, what a test's `init` keeps rather than mints.
fn cast_keys() -> suzatary_core::signatures::BoxKeys {
    use suzatary_core::cast::keys;
    suzatary_core::signatures::BoxKeys::of(
        &keys::root(),
        &suzatary_core::capability::Secret::new(keys::ROOT),
        &suzatary_core::capability::Secret::new(keys::AGENT),
    )
    .expect("the cast's keys")
}

/// A deployment's first run as its units run it (`scripts/deployed-shape.sh`): a cleared
/// environment whose PATH holds no `secretspec`. The keys are minted through the secret store,
/// so that run cannot mint them, and says which and why; handed the seeds in its environment,
/// as a store that already holds them answers first, it begins the deployment under their root.
#[test]
fn init_on_the_units_path_takes_its_keys_from_the_environment_or_names_the_store() {
    let dir = temp_dir("units-path");
    let empty = dir.join("bin");
    std::fs::create_dir_all(&empty).expect("an empty PATH");
    let init = |seeds: &[(&str, &str)]| {
        let instance = dir.join(format!("instance-{}.toml", seeds.len()));
        crate::binary::suzatary()
            .env("PATH", &empty)
            .env("HOME", &dir)
            .env("XDG_STATE_HOME", dir.join("state"))
            .env_remove(suzatary_cli::paths::RUN_ENV)
            .envs(seeds.iter().copied())
            .arg("--instance")
            .arg(&instance)
            .args([
                "init",
                "--name",
                "Ada",
                "--city",
                "London",
                "--zone",
                "Europe/London",
                "--actor",
                "ada",
                "--box",
                "ada-box.tailnet.ts.net",
                "--login",
                "her@example.test",
            ])
            .arg("--repo")
            .arg(repo())
            .stdin(std::process::Stdio::null())
            .output()
            .expect("the binary runs")
    };
    let bare = init(&[]);
    assert!(!bare.status.success(), "minted with no store to mint into");
    let said = String::from_utf8_lossy(&bare.stderr);
    assert!(
        said.contains("SUZATARY_BOX_KEY") && said.contains("could not be started"),
        "{said}"
    );

    let seeded = init(&[
        (
            suzatary_core::signatures::BOX_KEY,
            suzatary_core::cast::keys::ROOT,
        ),
        (
            suzatary_core::signatures::AGENT_KEY,
            suzatary_core::cast::keys::AGENT,
        ),
        (
            suzatary_core::mail::dedup_key::DEDUP_KEY,
            suzatary_core::cast::keys::DEDUP,
        ),
    ]);
    assert!(
        seeded.status.success(),
        "{}",
        String::from_utf8_lossy(&seeded.stderr)
    );
    // Its steps set a secret against the file `suzatary secretspec` names when the step runs,
    // which is under the state directory, never the code root, which a deployment cannot write.
    let steps = String::from_utf8_lossy(&seeded.stdout);
    assert!(
        steps.contains("secretspec set CLAUDE_CODE_OAUTH_TOKEN --file \"$(suzatary secretspec)\""),
        "{steps}"
    );
    let named = crate::binary::suzatary()
        .env("PATH", &empty)
        .env("HOME", &dir)
        .env("XDG_STATE_HOME", dir.join("state"))
        .arg("--instance")
        .arg(dir.join("instance-3.toml"))
        .arg("secretspec")
        .output()
        .expect("the binary runs");
    assert!(
        named.status.success(),
        "{}",
        String::from_utf8_lossy(&named.stderr)
    );
    let file = std::path::PathBuf::from(String::from_utf8_lossy(&named.stdout).trim());
    assert!(
        file.starts_with(dir.join("state").join("suzatary").join("secretspec")),
        "{}",
        file.display()
    );
    let declared = std::fs::read_to_string(&file).expect("the declared file");
    assert!(
        declared.contains(&format!("extends = [\"{}\"]", repo().display())),
        "{declared}"
    );
    let written = suzatary_cli::instance::Instance::read(
        &dir.join("instance-3.toml"),
        &env_beside(&dir.join("instance-3.toml")),
    )
    .expect("the instance reads");
    assert_eq!(*written.root(), suzatary_core::cast::keys::root());
    let _ = std::fs::remove_dir_all(&dir);
}

/// An environment beside the instance file at `path`, so a test reads none of this process's.
fn env_beside(path: &std::path::Path) -> suzatary_cli::paths::Env {
    suzatary_cli::paths::Env::under(path.with_file_name("home"), path.with_file_name("run"))
}

/// Every file under `root` and its bytes.
fn tree(root: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut walking = vec![root.to_path_buf()];
    while let Some(dir) = walking.pop() {
        for entry in std::fs::read_dir(&dir).expect("a readable directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walking.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("a readable file");
                files.insert(path, bytes);
            }
        }
    }
    files
}

/// LAW: resolving the box's keys only reads. A verb that asks for them (`doctor`) leaves the
/// instance's directory byte for byte as it was; adopting is `device init`'s act (and the
/// server's), and it is the one that writes.
#[test]
fn the_signers_leave_the_instance_directory_byte_identical() {
    let dir = temp_dir("signers-read");
    let instance_path = dir.join("instance.toml");
    let args = parse(&[
        "--name",
        "Ada",
        "--city",
        "London",
        "--zone",
        "Europe/London",
        "--box",
        "ada-box.tailnet.ts.net",
        "--login",
        "her@example.test",
        "--repo",
        dir.to_str().expect("a utf-8 temp path"),
    ]);
    init::run_keyed(&instance_path, args, &env_beside(&instance_path), |_| {
        Ok(cast_keys())
    })
    .expect("a fresh init");
    let run = |verb: &[&str]| {
        let out = crate::binary::keyed()
            .arg("--instance")
            .arg(&instance_path)
            .args(verb)
            .output()
            .expect("the binary runs");
        assert!(
            out.status.success(),
            "{verb:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let before = tree(&dir);
    let said = run(&["doctor"]);
    assert!(said.contains("keys resolve"), "{said}");
    assert_eq!(
        tree(&dir),
        before,
        "doctor resolved the keys and wrote nothing"
    );
    run(&["device", "init"]);
    assert_ne!(tree(&dir), before, "adoption is the act that writes");
    let adopted = tree(&dir);
    run(&["doctor"]);
    assert_eq!(tree(&dir), adopted);
    let _ = std::fs::remove_dir_all(&dir);
}

/// LAW: `suzatary notify` pushes through the server's one channel list, so a deployment whose
/// `[notify]` names none pushes nothing from either, and says so rather than resolving a secret.
#[test]
fn notify_reads_the_channel_list_the_server_reads() {
    let dir = temp_dir("notify-channels");
    let instance_path = dir.join("instance.toml");
    init::run_keyed(
        &instance_path,
        sample_args(),
        &env_beside(&instance_path),
        |_| Ok(cast_keys()),
    )
    .expect("a fresh init");
    let read = Instance::read(&instance_path, &env_beside(&instance_path)).expect("reads");
    assert!(read.notify.is_none(), "the sample names no channel");
    let out = crate::binary::keyed()
        .arg("--instance")
        .arg(&instance_path)
        .args(["notify", "no-brief"])
        .output()
        .expect("the binary runs");
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("names no [notify] channel"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
