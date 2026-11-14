//! `suzatary-dev fixture-live` over a deployment that names a certificate: the copy names none
//! and holds none, so the browser suite's server serves it as a loopback viewer, which `serve`
//! refuses to do beside a certificate.

use std::io::BufRead as _;
use std::path::Path;
use std::process::{Command, Stdio};

fn fixture(root: &Path) -> Command {
    use suzatary_cli::commands::fixture::{AGENT_SEED, BOX_SEED};
    let mut command = crate::binary::fixture();
    command
        .env("XDG_STATE_HOME", root.join("state"))
        .env("SUZATARY_FIXTURE_NO_MAIL", "1")
        .env(suzatary_signatures::signatures::BOX_KEY, BOX_SEED)
        .env(suzatary_signatures::signatures::AGENT_KEY, AGENT_SEED);
    command
}

#[test]
fn a_copy_of_a_deployment_with_a_certificate_names_none_and_serves_its_viewer() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let root = scratch.path();
    std::fs::create_dir_all(root.join("state")).expect("a directory");
    let built = fixture(root)
        .args(["fixture-chain", "--root"])
        .arg(root.join("box"))
        .args([
            "--port",
            &crate::ports::outside_ephemeral().to_string(),
            "--repo",
        ])
        .arg(root)
        .output()
        .expect("the fixture runs");
    assert!(
        built.status.success(),
        "fixture-chain: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    // The deployment's certificate, in its data tree, where `serve` finds one.
    std::fs::create_dir_all(root.join("box/tls")).expect("a directory");
    for pem in ["cert.pem", "key.pem"] {
        std::fs::write(root.join("box/tls").join(pem), "a pem\n").expect("writes");
    }
    let port = crate::ports::outside_ephemeral();
    let copied = fixture(root)
        .args(["fixture-live", "--from"])
        .arg(root.join("box/instance.toml"))
        .arg("--root")
        .arg(root.join("copy"))
        .args(["--port", &port.to_string(), "--repo"])
        .arg(root)
        .output()
        .expect("fixture-live runs");
    assert!(
        copied.status.success(),
        "fixture-live: {}",
        String::from_utf8_lossy(&copied.stderr)
    );
    assert!(
        !root.join("copy/tls").exists(),
        "the certificate is not copied"
    );
    let text =
        std::fs::read_to_string(root.join("copy/instance.toml")).expect("the copy's instance");
    let doc: toml_edit::DocumentMut = text.parse().expect("an instance file");
    let named = doc["web"]["serving"]["tls_dir"]
        .as_str()
        .expect("a certificate directory");
    assert!(
        !Path::new(named).exists(),
        "the copy's certificate directory holds nothing: {named}"
    );

    let mut server = fixture(root)
        .arg("serve")
        .arg("--instance")
        .arg(root.join("copy/instance.toml"))
        .args([
            "--whois-fixture",
            suzatary_cli::commands::fixture_live::LIVE_LOGIN,
        ])
        .env("XDG_STATE_HOME", root.join("copy-state"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let said = server.stdout.take().expect("piped");
    let listened = std::io::BufReader::new(said)
        .lines()
        .map_while(Result::ok)
        .any(|line| line.contains("serve: http"));
    let _ = server.kill();
    let ended = server.wait_with_output().expect("the server ends");
    assert!(
        listened,
        "the copy's server listened as a loopback viewer: {}",
        String::from_utf8_lossy(&ended.stderr)
    );
}
