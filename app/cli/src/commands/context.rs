//! `suzatary context calibrate`: the context budget's token ratios counted with Claude's
//! tokenizer, which needs the app's model port. The budget itself is the train's tool's
//! (`suzatary_tooling::context`, `scripts/context.sh`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use suzatary_held::for_model::ForModel;
use suzatary_io::model::{ClaudeCli, Fence};
use suzatary_tooling::context::plan::{FILE, Plan};
use suzatary_tooling::context::tokens::{Kind, Ratios};
use suzatary_tooling::context::tree::Tree;
use suzatary_tooling::context::{as_toml, edit_plan, kind_key, read_plan};

/// What `suzatary context` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The repository's root.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    #[command(subcommand)]
    act: Act,
}

#[derive(Debug, clap::Subcommand)]
enum Act {
    /// Count the sample's tokens with Claude's tokenizer, through the `claude` CLI, and write
    /// the ratios they give.
    Calibrate {
        /// The model whose tokenizer counts.
        #[arg(long)]
        model: String,
    },
}

/// Calibrate the ratios.
///
/// # Errors
/// When the plan or the tree cannot be read, or the model cannot count.
pub fn run(args: &Args) -> anyhow::Result<()> {
    let Act::Calibrate { model } = &args.act;
    let root = &args.repo.canonicalize()?;
    calibrate(root, &read_plan(root)?, model)
}

/// Count each sample's tokens with `model`'s tokenizer, and write the samples, the ratios and the
/// instant.
fn calibrate(root: &Path, plan: &Plan, model: &str) -> anyhow::Result<()> {
    let tree = Tree::checkout(root)?;
    let scratch = suzatary_io::scratch::Scratch::temporary("suzatary-context-")?;
    let cli = ClaudeCli::new(
        model,
        Fence::at(root.join(".claude/secretary-settings.json")),
    )
    .in_directory(scratch.path().to_path_buf());
    let system = ForModel::of("Reply with one word.");
    // The prompt beside the text: the system prompt and the message's framing.
    let framing = cli
        .prompt_tokens(&system, ForModel::of("."))?
        .saturating_sub(1);
    let mut counted = Vec::new();
    for sample in &plan.tokens.sample {
        let text = tree
            .get(&sample.path)
            .with_context(|| format!("the sample {} is not in the tree", sample.path))?;
        let masked = ForModel::of(text);
        let characters = masked.as_str().chars().count();
        let tokens = cli.prompt_tokens(&system, masked)?.saturating_sub(framing);
        println!(
            "context: {} is {characters} characters, {tokens} tokens",
            sample.path
        );
        counted.push((sample.path.as_str(), characters, tokens));
    }
    let mut sums: BTreeMap<Kind, (usize, u64)> = BTreeMap::new();
    for (path, characters, tokens) in &counted {
        let sum = sums.entry(Kind::of(path)).or_default();
        sum.0 += characters;
        sum.1 += tokens;
    }
    let mut ratios: Ratios = plan.tokens.ratio;
    for kind in Kind::ALL {
        let Some(&(characters, tokens)) = sums.get(&kind).filter(|(_, tokens)| *tokens > 0) else {
            bail!("the sample holds no file of kind {kind:?}: add one to [[tokens.sample]]");
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "the sample is far below 2^52 characters"
        )]
        let ratio = characters as f64 / tokens as f64;
        ratios.set(kind, (ratio * 1000.0).round() / 1000.0);
    }
    let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    edit_plan(root, |document| {
        document["tokens"]["calibrated"] = toml_edit::value(at.as_str());
        for kind in Kind::ALL {
            document["tokens"]["ratio"][kind_key(kind)] = toml_edit::value(ratios.of(kind));
        }
        let samples = document["tokens"]["sample"]
            .as_array_of_tables_mut()
            .context("`tokens.sample` is an array of tables")?;
        for (entry, (_, characters, tokens)) in samples.iter_mut().zip(&counted) {
            entry["characters"] = toml_edit::value(as_toml(u64::try_from(*characters)?)?);
            entry["tokens"] = toml_edit::value(as_toml(*tokens)?);
        }
        Ok(())
    })?;
    println!("context: ratios written to {FILE}");
    Ok(())
}
