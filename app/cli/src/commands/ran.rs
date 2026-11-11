//! `suzatary ran <file>`: a model run's journal line, its counts and ids read off the CLI's
//! `result` object, never its words, which quote the mail the run read.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use suzatary_io::model::summary::{Outcome, summary_of};

/// What `suzatary ran` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The run's output, as `claude -p --output-format json` or `stream-json --verbose`
    /// printed it.
    output: PathBuf,
    /// The prefix of the tools the run had to call (`mcp__suzatary__`); with it, the run's
    /// verdict is the exit code: 10 the CLI is not signed in, 11 it called none of them, 12 it
    /// printed no result, 13 its result is not a shape this box reads. Above the codes the
    /// shell and a failed verb take for their own.
    #[arg(long)]
    needs: Option<String>,
    /// The model the run asked (`claude-sonnet-5`): a run that failed for a limit joins the limit
    /// into the model health under it, so every caller of the model pauses until it ends, and
    /// exits 14 under `--needs`.
    #[arg(long)]
    model: Option<String>,
}

/// Why a run that had to call its tools did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failed {
    SignedOut = 10,
    CalledNone = 11,
    NoResult = 12,
    Unreadable = 13,
    Limited = 14,
}

/// Print the run's summary line; a file that holds no result object says only how long it is.
///
/// # Errors
/// The file unreadable, or the instance at `path` when the run's limit is recorded.
pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(&args.output)
        .with_context(|| format!("reading {}", args.output.display()))?;
    let summary = summary_of(&text);
    let limited = match (&args.model, &summary) {
        (Some(model), Err(_) | Ok(None)) => limit_of(path, model, &text)?,
        (Some(model), Ok(Some(summary))) if summary.outcome != Outcome::Clean => {
            limit_of(path, model, &text)?
        }
        (Some(_) | None, _) => None,
    };
    match &summary {
        Ok(Some(summary)) => println!("{summary}"),
        Err(unreadable) => println!("{unreadable}"),
        Ok(None) => println!(
            "the run printed no result object ({} bytes, kept at {})",
            text.len(),
            Path::new(&args.output).display()
        ),
    }
    let Some(needs) = &args.needs else {
        return Ok(());
    };
    let failed = match &summary {
        _ if limited.is_some() => Some(Failed::Limited),
        Err(_) => Some(Failed::Unreadable),
        Ok(None) => Some(Failed::NoResult),
        Ok(Some(summary)) if summary.outcome == Outcome::SignedOut => Some(Failed::SignedOut),
        Ok(Some(summary)) if !summary.called.keys().any(|name| name.starts_with(needs)) => {
            Some(Failed::CalledNone)
        }
        Ok(Some(_)) => None,
    };
    if let Some(failed) = failed {
        std::process::exit(failed as i32);
    }
    Ok(())
}

/// The limit a failed run's `text` names, joined into the model health under `model` and
/// printed; none for a run that failed otherwise.
fn limit_of(
    path: &Path,
    model: &str,
    text: &str,
) -> anyhow::Result<Option<suzatary_core::health::Limited>> {
    use suzatary_core::capability::Clock as _;
    use suzatary_core::secretary::model::ModelError;
    let now = prodrome_core::fpl::instant_of(suzatary_io::clock::SystemClock.now());
    let failed = suzatary_io::model::failure::Heard::of(text).failure(String::new(), now);
    let ModelError::Limited(limited) = failed else {
        return Ok(None);
    };
    let instance = crate::instance::Instance::open(path)?;
    crate::commands::serve::interpreters::answered_by(
        &suzatary_io::model_health::beside(&instance.mail.heartbeat),
        &suzatary_io::model::name_of(model),
        Some(&failed),
    );
    println!("{limited}");
    Ok(Some(limited))
}
