//! `suzatary-dev fixture-live`: a copy of a deployment's data tree that the browser suite's server
//! reads, so a scene draws the owner's real readings while nothing it does reaches the data tree.

use std::path::{Path, PathBuf};

use anyhow::Context as _;

use super::fixture::HOST;
use crate::instance::File;

/// The login the copy's whois fixture answers with: a viewer that acts as nobody.
pub const LIVE_LOGIN: &str = "e2e-live@example.invalid";

/// What `suzatary-dev fixture-live` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The deployment's instance file, whose data root is copied.
    #[arg(long)]
    from: PathBuf,
    /// The empty directory the copy is made in.
    #[arg(long)]
    root: PathBuf,
    /// The port the copy's server will bind.
    #[arg(long)]
    port: u16,
    /// The checkout the copy's code root names, for the frontend it serves.
    #[arg(long)]
    repo: PathBuf,
}

/// Copy the data root and write the copy's instance file; print what the suite needs.
///
/// # Errors
/// The instance unreadable, the root not empty, or a copy or a write failing.
pub fn run(args: &Args) -> anyhow::Result<()> {
    let file = File::read(&args.from)?;
    let here = args.from.parent().unwrap_or_else(|| Path::new("."));
    let data = here.join(&file.data);
    if args.root.exists() && std::fs::read_dir(&args.root)?.next().is_some() {
        anyhow::bail!("{} is not empty", args.root.display());
    }
    // The deployment's certificate, where it lies in the data tree, is not copied: the copy names none.
    let certificate = data.join(&file.web.serving.tls_dir);
    copy_tree(&data, &args.root, &certificate)
        .with_context(|| format!("copying {} to {}", data.display(), args.root.display()))?;
    let text = std::fs::read_to_string(&args.from)
        .with_context(|| format!("reading {}", args.from.display()))?;
    let instance = args.root.join("instance.toml");
    // The copied file may be a link into the data tree, and a write through it would reach it.
    if instance.symlink_metadata().is_ok() {
        std::fs::remove_file(&instance)?;
    }
    std::fs::write(&instance, retarget(&text, args)?)
        .with_context(|| format!("writing {}", instance.display()))?;
    println!(
        "{}",
        serde_json::json!({
            "base": format!("http://{HOST}:{}", args.port),
            "root": args.root.display().to_string(),
            "instance": instance.display().to_string(),
            "login": LIVE_LOGIN,
        })
    );
    Ok(())
}

/// The instance file's text, every byte kept but the data root, the address, the certificate
/// directory and the logins: one viewer, and no login that acts.
fn retarget(text: &str, args: &Args) -> anyhow::Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("parsing the instance file")?;
    let repo = std::path::absolute(&args.repo)?;
    doc["repo"] = toml_edit::value(repo.display().to_string());
    doc["data"] = toml_edit::value(".");
    let web = doc["web"]
        .as_table_mut()
        .context("the instance file has no [web] table")?;
    web["viewers"] = toml_edit::value(toml_edit::Array::from_iter([LIVE_LOGIN]));
    web["app_url"] = toml_edit::value(format!("http://{HOST}:{}", args.port));
    web["act_as"] = toml_edit::table();
    let serving = web["serving"]
        .as_table_mut()
        .context("the instance file has no [web.serving] table")?;
    serving["host"] = toml_edit::value(HOST);
    serving["port"] = toml_edit::value(i64::from(args.port));
    // A loopback viewer serves plain http, and `serve` refuses its whois fixture beside a
    // certificate: so the copy's certificate directory is one that holds none.
    serving["tls_dir"] = toml_edit::value(no_certificate(&args.root)?.display().to_string());
    Ok(doc.to_string())
}

/// The copy's certificate directory: a path under it that nothing holds, absolute, since the
/// deployment's own may be absolute or a directory of the data tree the copy holds.
fn no_certificate(root: &Path) -> anyhow::Result<PathBuf> {
    let dir = std::path::absolute(root)?.join(".no-certificate");
    anyhow::ensure!(!dir.exists(), "{} exists in the copy", dir.display());
    Ok(dir)
}

/// `from`'s tree copied under `to`, its symbolic links as links, without the repository's `.git`
/// and without the directory `except`.
fn copy_tree(from: &Path, to: &Path, except: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&source)?, &target)?;
        } else if kind.is_dir() {
            if entry.file_name() != ".git" && source != except {
                copy_tree(&source, &target, except)?;
            }
        } else {
            std::fs::copy(&source, &target)?;
        }
    }
    Ok(())
}
