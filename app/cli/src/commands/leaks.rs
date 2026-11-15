//! `suzatary leaks`: the leak guard's matcher (`suzatary_held::leak`) over files, or over the
//! paths on stdin, printing each hit as `file:line` on stdout and its layer on stderr, and
//! nothing of what it matched; `suzatary leaks scan-copy` writes the copy the secret scanner
//! reads. `scripts/public-tree.sh --check` walks the tree and calls both.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use suzatary_held::leak::{Admitted, Findings, Matcher, Opaque, Scope, Vouch, placeheld};

/// The rules a scan applies, as `--scope` spells them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Rules {
    /// Every rule and the admitted files: the check a public tree passes.
    Whole,
    /// The data repository's term lists and the admitted files: another public repository's.
    Lists,
    /// The data repository's term lists alone: the private repository's own check.
    Terms,
}

/// Where a person admits a file the guard asks about, so a refusal ends with the command.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AdmitBy {
    /// A line of the tree's manifest, published by `scripts/train.sh publish --admit`.
    Train,
    /// `scripts/public-scan.sh --admit`, for that scan.
    Scan,
    /// Never: words a person wrote (a commit message, a pull request's text) are read as words.
    Never,
}

/// What `suzatary leaks` was told: a scan by the matcher, or the scanner's copy.
#[derive(Debug, clap::Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Args {
    #[command(subcommand)]
    copy: Option<Copy>,
    #[command(flatten)]
    scan: Option<Scan>,
}

/// The copy the secret scanner reads, rather than a scan.
#[derive(Debug, clap::Subcommand)]
enum Copy {
    /// Write the copy the secret scanner reads: stdin is NUL-separated paths, each written to
    /// `<dir>/<n>` (a name no path rule of the scanner reads) with its sealed slots a
    /// placeholder, and `<n>\t<path>` printed for each. A symbolic link is left out: the guard
    /// refuses it on its own.
    ScanCopy {
        /// The empty directory the copy is written to.
        dir: PathBuf,
    },
}

/// What the matcher reads, and against what.
#[derive(Debug, clap::Args)]
struct Scan {
    /// The data repository's terms, one per line.
    #[arg(long)]
    terms: PathBuf,
    /// This deployment's names, one per line.
    #[arg(long)]
    words: PathBuf,
    /// The private history's paths, one per line.
    #[arg(long)]
    links: PathBuf,
    /// Which rules read the input.
    #[arg(long, value_enum, default_value_t = Rules::Whole)]
    scope: Rules,
    /// The manifest of admitted files: a file the matcher cannot read as words is a hit, printed
    /// `file:0`, unless this lists its path with the digest of its bytes. Every scope but
    /// `terms` reads files against one.
    #[arg(long, conflicts_with = "paths")]
    admitted: Option<PathBuf>,
    /// The tree's `.gitleaksignore`: a file holding a finding it admits by fingerprint is
    /// admitted by the manifest by its bytes too.
    #[arg(long, requires = "admitted")]
    findings: Option<PathBuf>,
    /// How a refused file is admitted.
    #[arg(long, value_enum, default_value_t = AdmitBy::Train)]
    admit_by: AdmitBy,
    /// Stdin is one path per line, each hit printed `path:0`.
    #[arg(long, conflicts_with = "files")]
    paths: bool,
    /// Print what the manifest and `.gitleaksignore` admit, one admission a line,
    /// `<path>\tadmitted <digest>` or `<path>\tfinding <fingerprint>`, and read no file.
    #[arg(long, requires_all = ["admitted", "findings"], conflicts_with_all = ["paths", "files"])]
    admissions: bool,
    /// Files to read, each hit printed `file:line`.
    files: Vec<PathBuf>,
}

/// Print every hit, or write the scanner's copy.
///
/// # Errors
/// A list, the manifest or a file unreadable, the manifest malformed, or no mode given.
pub fn run(args: &Args) -> anyhow::Result<()> {
    match (&args.copy, &args.scan) {
        (Some(Copy::ScanCopy { dir }), _) => scan_copy(dir),
        (None, Some(scan)) => matched(scan),
        (None, None) => anyhow::bail!("leaks reads files with --terms, --words and --links"),
    }
}

/// Print every hit the matcher finds.
///
/// # Errors
/// A list, the manifest or a file unreadable, or the manifest malformed.
fn matched(args: &Scan) -> anyhow::Result<()> {
    let read = |path: &Path| {
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
    };
    let list = |path: &Path| -> anyhow::Result<Vec<String>> {
        Ok(read(path)?.lines().map(str::to_owned).collect())
    };
    let matcher = Matcher::new(
        &list(&args.terms)?,
        &list(&args.words)?,
        &list(&args.links)?,
        match args.scope {
            Rules::Whole => Scope::Whole,
            Rules::Lists | Rules::Terms => Scope::Lists,
        },
    );
    if args.paths {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes)?;
        for path in String::from_utf8_lossy(&bytes).lines() {
            if let Some(layer) = matcher.hits(path).into_values().next() {
                eprintln!("leaks: the path {path} {layer}");
                println!("{path}:0");
            }
        }
        return Ok(());
    }
    let admitted = match (args.scope, &args.admitted) {
        (Rules::Terms, _) if args.admissions => {
            anyhow::bail!("--admissions reads a manifest, and --scope terms reads none")
        }
        (Rules::Terms, _) => None,
        (_, Some(manifest)) => Some(
            Admitted::parse(&read(manifest)?)
                .with_context(|| format!("reading {}", manifest.display()))?,
        ),
        (Rules::Whole | Rules::Lists, None) => {
            anyhow::bail!("--scope whole and lists read the files against a manifest: --admitted")
        }
    };
    let findings = match &args.findings {
        Some(ignore) => Findings::parse(&read(ignore)?)
            .with_context(|| format!("reading {}", ignore.display()))?,
        None => Findings::default(),
    };
    if args.admissions {
        for (path, digest) in admitted.iter().flat_map(Admitted::admissions) {
            println!("{path}\tadmitted {digest}");
        }
        for (path, fingerprint) in findings.admissions() {
            println!("{path}\tfinding {fingerprint}");
        }
        return Ok(());
    }
    for file in &args.files {
        let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
        let path = file.to_string_lossy();
        let opaque = Opaque::of(&bytes);
        if let (Some(why), Some(admitted)) = (Vouch::of(&path, opaque, &findings), &admitted)
            && !admitted.holds(&path, &bytes)
        {
            let said = match args.admit_by {
                AdmitBy::Train => format!(
                    "{path} {why}; once you have looked at these bytes: sha256sum {path} >> \
                     scripts/public-admitted.sha256, commit it, then at a terminal: \
                     scripts/train.sh publish --admit {path}"
                ),
                AdmitBy::Scan => format!(
                    "{path} {why}; once you have looked at these bytes, at a terminal: \
                     scripts/public-scan.sh --admit {path} …"
                ),
                AdmitBy::Never => format!(
                    "{path} {why}, and a commit message or a pull request's text is never \
                     admitted: write it as words"
                ),
            };
            eprintln!("leaks: {said}");
            println!("{path}:0");
        }
        if opaque != Some(Opaque::Binary) {
            for (at, layer) in matcher.hits(&String::from_utf8_lossy(&bytes)) {
                eprintln!("leaks: {path}:{at} {layer}");
                println!("{path}:{at}");
            }
        }
    }
    Ok(())
}

/// Write the copy the secret scanner reads into `dir`, one numbered file per path on stdin, and
/// print the index.
///
/// # Errors
/// A path that is not UTF-8, or a file unreadable or unwritable.
fn scan_copy(dir: &Path) -> anyhow::Result<()> {
    let mut paths = Vec::new();
    std::io::stdin().read_to_end(&mut paths)?;
    let mut n = 0;
    for path in paths.split(|b| *b == 0).filter(|path| !path.is_empty()) {
        let path = Path::new(std::str::from_utf8(path).context("a path that is not UTF-8")?);
        if std::fs::symlink_metadata(path)
            .with_context(|| format!("reading {}", path.display()))?
            .is_symlink()
        {
            continue;
        }
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let copy = match std::str::from_utf8(&bytes) {
            Ok(text) => placeheld(text).into_bytes(),
            Err(_) => bytes,
        };
        n += 1;
        std::fs::write(dir.join(n.to_string()), copy)
            .with_context(|| format!("writing the copy of {}", path.display()))?;
        println!("{n}\t{}", path.display());
    }
    Ok(())
}
