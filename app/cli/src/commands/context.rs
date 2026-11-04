//! `suzatary context`: how many tokens an agent reads to use and to develop each unit of the
//! repository, against the budgets of `context.toml`; `record` lowers the ratchet, `external`
//! measures a pinned library, and `calibrate` measures the token ratios with Claude's tokenizer.
//! `scripts/context.sh` runs it, and `app/cli/tests/guards/context.rs` enforces it.

pub mod finding;
pub mod interface;
pub mod measure;
pub mod plan;
pub mod tokens;
pub mod tree;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use suzatary_core::for_model::ForModel;
use suzatary_io::model::{ClaudeCli, Fence};

use self::finding::{Finding, findings};
use self::measure::{Report, measure};
use self::plan::{ExternalShape, FILE, Plan};
use self::tokens::{Kind, Ratios};
use self::tree::{Tree, git};

/// What `suzatary context` was told.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The repository's root.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    #[command(subcommand)]
    act: Option<Act>,
}

/// What to do besides printing the table.
#[derive(Debug, clap::Subcommand)]
enum Act {
    /// List the files a unit's USE and DEVELOP read, or the reading set's, largest first.
    Files {
        /// The unit; none lists the repository's reading set.
        unit: Option<String>,
    },
    /// Write the reading set's tokens as the new ratchet; refused when they are above it.
    Record,
    /// Measure an external library's USE from a checkout of its pinned revision.
    External {
        /// The library's name in `context.toml`.
        name: String,
        /// A checkout of its repository at the pinned revision.
        checkout: PathBuf,
    },
    /// Count the sample's tokens with Claude's tokenizer, through the `claude` CLI, and write
    /// the ratios they give.
    Calibrate {
        /// The model whose tokenizer counts.
        #[arg(long)]
        model: String,
    },
}

/// The tree a change started from, measured by the same plan, or why there is none.
#[derive(Debug)]
pub enum Base {
    /// The merge base with `origin/master`.
    Measured {
        /// Its commit.
        rev: String,
        /// Its measures.
        report: Report,
    },
    /// No `origin/master` to compare with.
    Absent {
        /// Why.
        why: String,
    },
}

impl Base {
    /// Its measures, when it was measured.
    #[must_use]
    pub const fn report(&self) -> Option<&Report> {
        match self {
            Self::Measured { report, .. } => Some(report),
            Self::Absent { .. } => None,
        }
    }
}

/// Print the table; or record, measure or calibrate.
///
/// # Errors
/// When the plan or the tree cannot be read, when the guard's findings are not empty, or the
/// act's own.
pub fn run(args: &Args) -> anyhow::Result<()> {
    let root = &args
        .repo
        .canonicalize()
        .with_context(|| format!("finding {}", args.repo.display()))?;
    let plan = read_plan(root)?;
    match &args.act {
        None => {
            let tree = Tree::checkout(root)?;
            let report = measure(&plan, &tree);
            print!("{}", table(&plan, &report, &base(root, &plan)));
            let found = findings(&plan, &tree, &report);
            for finding in &found {
                eprintln!("context: {finding}");
            }
            if !found.is_empty() {
                bail!("{} finding(s) against {FILE}", found.len());
            }
            Ok(())
        }
        Some(Act::Files { unit }) => {
            let report = measure(&plan, &Tree::checkout(root)?);
            print!("{}", files(&report, unit.as_deref())?);
            Ok(())
        }
        Some(Act::Record) => record(root, &plan),
        Some(Act::External { name, checkout }) => external(root, &plan, name, checkout),
        Some(Act::Calibrate { model }) => calibrate(root, &plan, model),
    }
}

/// `context.toml` at `root`, parsed.
///
/// # Errors
/// When it cannot be read or parsed.
pub fn read_plan(root: &Path) -> anyhow::Result<Plan> {
    let text = std::fs::read_to_string(root.join(FILE))
        .with_context(|| format!("reading {}", root.join(FILE).display()))?;
    Plan::parse(&text)
}

/// The merge base of `HEAD` and `origin/master` at `root`, measured by `plan`.
#[must_use]
pub fn base(root: &Path, plan: &Plan) -> Base {
    let measured = git(root, &["merge-base", "HEAD", "origin/master"]).and_then(|rev| {
        let rev = rev.trim().to_owned();
        let tree = Tree::commit(root, &rev)?;
        Ok(Base::Measured {
            report: measure(plan, &tree),
            rev,
        })
    });
    measured.unwrap_or_else(|failed| Base::Absent {
        why: format!("{failed:#}"),
    })
}

/// Every unit's USE, DEVELOP, tests and fixtures, and the reading set, with the change against
/// `base`.
#[must_use]
pub fn table(plan: &Plan, report: &Report, base: &Base) -> String {
    let was = |name: &str| {
        base.report()
            .and_then(|report| report.unit(name))
            .map(|unit| unit.figures)
    };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "context: tokens an agent reads, by ratios measured at {}; budgets: use {}, develop {}, tests {}",
        plan.tokens.calibrated,
        short(plan.budget.use_),
        short(plan.budget.develop),
        short(plan.budget.tests)
    );
    match base {
        Base::Measured { rev, .. } => {
            let _ = writeln!(
                out,
                "change against {}, the merge base with origin/master",
                &rev[..8.min(rev.len())]
            );
        }
        Base::Absent { why } => {
            let _ = writeln!(out, "no change shown: {why}");
        }
    }
    let _ = writeln!(
        out,
        "{:<13} {:<20} {:<24} {:<22} {:<16} depends on",
        "unit", "use", "develop", "tests", "fixtures"
    );
    for (unit, measured) in plan.units.iter().zip(&report.units) {
        let now = measured.figures;
        let then = was(&unit.name);
        let depends: Vec<&str> = unit.depends.iter().map(String::as_str).collect();
        let _ = writeln!(
            out,
            "{:<13} {:<20} {:<24} {:<22} {:<16} {}",
            unit.name,
            cell(now.use_, then.map(|t| t.use_), Some(plan.budget.use_)),
            cell(
                now.develop,
                then.map(|t| t.develop),
                Some(plan.budget.develop)
            ),
            cell(now.tests, then.map(|t| t.tests), Some(plan.budget.tests)),
            cell(now.fixtures, then.map(|t| t.fixtures), None),
            if depends.is_empty() {
                "-".to_owned()
            } else {
                depends.join(" ")
            }
        );
    }
    for external in &plan.externals {
        let _ = writeln!(
            out,
            "{:<13} {:<20} external, {} at {}",
            external.name,
            cell(external.use_, None, Some(plan.budget.use_)),
            external.source,
            &external.rev[..8.min(external.rev.len())]
        );
    }
    out.push_str(&footer(plan, report, base));
    out
}

/// A figure that the goal holds every unit to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Budgeted {
    /// Its USE, against `budget.use`.
    Use,
    /// Its DEVELOP, against `budget.develop`.
    Develop,
}

/// A unit's figure that is over its budget.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Over {
    /// The unit.
    unit: String,
    /// The figure.
    figure: Budgeted,
    /// The figure's tokens.
    tokens: u64,
    /// The budget it is over.
    budget: u64,
}

/// Every unit's USE and DEVELOP that is over its budget, in the plan's order: what stands
/// between the tree and the goal.
#[must_use]
fn over_budget(plan: &Plan, report: &Report) -> Vec<Over> {
    report
        .units
        .iter()
        .flat_map(|unit| {
            [
                (Budgeted::Use, unit.figures.use_, plan.budget.use_),
                (Budgeted::Develop, unit.figures.develop, plan.budget.develop),
            ]
            .into_iter()
            .filter(|(_, tokens, budget)| tokens > budget)
            .map(|(figure, tokens, budget)| Over {
                unit: unit.name.clone(),
                figure,
                tokens,
                budget,
            })
        })
        .collect()
}

/// The repository's lines of the table: the goal, the reading set, the ratchets and the uncited
/// plans.
fn footer(plan: &Plan, report: &Report, base: &Base) -> String {
    let mut out = String::new();
    let over = over_budget(plan, report);
    let goal = format!(
        "goal: no unit over use {} or develop {}",
        short(plan.budget.use_),
        short(plan.budget.develop)
    );
    if over.is_empty() {
        let _ = writeln!(out, "{goal}: reached");
    } else {
        let rows: Vec<String> = over
            .iter()
            .map(|over| {
                format!(
                    "{} {} {} by {}",
                    over.unit,
                    match over.figure {
                        Budgeted::Use => "use",
                        Budgeted::Develop => "develop",
                    },
                    short(over.tokens),
                    short(over.tokens - over.budget)
                )
            })
            .collect();
        let _ = writeln!(out, "{goal}; {} over: {}", over.len(), rows.join(", "));
    }
    let then = base.report().map(|report| report.reading.total());
    let _ = writeln!(
        out,
        "repository    reading set {}; ratchet {} plus {}",
        cell(report.reading.total(), then, None),
        short(plan.ratchet.reading),
        short(plan.budget.allowance)
    );
    if !plan.ratchet.use_.is_empty() {
        let held: Vec<String> = plan
            .ratchet
            .use_
            .iter()
            .map(|(name, tokens)| format!("{name} {}", short(*tokens)))
            .collect();
        let _ = writeln!(
            out,
            "use ratchet (over budget today, may only fall): {}",
            held.join(", ")
        );
    }
    if !report.uncited_plans.is_empty() {
        let _ = writeln!(
            out,
            "plans nothing in the reading set cites ({}): {}",
            report.uncited_plans.len(),
            report.uncited_plans.join(" ")
        );
    }
    out
}

/// The files of `unit`'s USE and DEVELOP, or of the reading set, each with its tokens, largest
/// first.
fn files(report: &Report, unit: Option<&str>) -> anyhow::Result<String> {
    let listed = |title: &str, reading: &measure::Reading| {
        let mut rows: Vec<(&String, &u64)> = reading.0.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let mut out = format!("{title}: {} tokens\n", reading.total());
        for (path, tokens) in rows {
            let _ = writeln!(out, "{tokens:>9}  {path}");
        }
        out
    };
    Ok(match unit {
        None => listed("the reading set", &report.reading),
        Some(name) => {
            let measured = report
                .unit(name)
                .with_context(|| format!("{FILE} names no unit {name}"))?;
            format!(
                "{}{}",
                listed(&format!("{name}: use"), &measured.use_files),
                listed(&format!("{name}: develop"), &measured.develop_files)
            )
        }
    })
}

/// One figure: its tokens, its change since `then`, and `OVER` when it is over `budget`.
fn cell(now: u64, then: Option<u64>, budget: Option<u64>) -> String {
    let over = budget.is_some_and(|budget| now > budget);
    let change = then.map_or_else(String::new, |then| format!(" {}", signed(now, then)));
    format!("{}{change}{}", short(now), if over { " OVER" } else { "" })
}

/// `tokens` in thousands or millions, to three figures.
fn short(tokens: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count of tokens is far below 2^52"
    )]
    let value = tokens as f64;
    if tokens >= 1_000_000 {
        format!("{:.2}M", value / 1e6)
    } else if tokens >= 1_000 {
        format!("{:.1}k", value / 1e3)
    } else {
        tokens.to_string()
    }
}

/// The change from `then` to `now`, signed, in parentheses.
fn signed(now: u64, then: u64) -> String {
    if now >= then {
        format!("(+{})", now - then)
    } else {
        format!("(-{})", then - now)
    }
}

/// Lower the ratchet to the figures now: the reading set, and the USE of each unit still over
/// budget; a unit back within budget leaves the ratchet. Refused when a figure is above its
/// ratchet, or a unit is over budget with no ratchet.
fn record(root: &Path, plan: &Plan) -> anyhow::Result<()> {
    let report = measure(plan, &Tree::checkout(root)?);
    let reading = report.reading.total();
    if reading > plan.ratchet.reading {
        bail!(
            "the reading set is {reading} tokens, above the ratchet of {}: the ratchet only falls",
            plan.ratchet.reading
        );
    }
    let mut uses = BTreeMap::new();
    for unit in report
        .units
        .iter()
        .filter(|unit| unit.figures.use_ > plan.budget.use_)
    {
        match plan.ratchet.use_.get(&unit.name) {
            Some(&recorded) if unit.figures.use_ <= recorded => {
                uses.insert(unit.name.clone(), unit.figures.use_);
            }
            Some(&recorded) => bail!(
                "{}: its USE is {} tokens, above its ratchet of {recorded}: the ratchet only falls",
                unit.name,
                unit.figures.use_
            ),
            None => bail!(
                "{}: its USE is {} tokens, over the budget of {}, and a unit joins no ratchet",
                unit.name,
                unit.figures.use_,
                plan.budget.use_
            ),
        }
    }
    edit_plan(root, |document| {
        document["ratchet"]["reading"] = toml_edit::value(as_toml(reading)?);
        let table = document["ratchet"]["use"]
            .or_insert(toml_edit::table())
            .as_table_mut()
            .context("`ratchet.use` is a table")?;
        table.retain(|name, _| uses.contains_key(name));
        for (name, tokens) in &uses {
            table[name.as_str()] = toml_edit::value(as_toml(*tokens)?);
        }
        Ok(())
    })?;
    println!(
        "context: the ratchet is now {reading} tokens for the reading set, and {uses:?} for USE"
    );
    Ok(())
}

/// Measure the external library `name` from `checkout`, and write its USE.
fn external(root: &Path, plan: &Plan, name: &str, checkout: &Path) -> anyhow::Result<()> {
    let library = plan
        .external(name)
        .with_context(|| format!("{FILE} names no external library {name}"))?;
    let head = git(checkout, &["rev-parse", "HEAD"])?;
    if head.trim() != library.rev {
        bail!(
            "{} is at {}, not at the pinned {}",
            checkout.display(),
            head.trim(),
            library.rev
        );
    }
    let tree = Tree::checkout(checkout)?;
    let inside = |file: &str| {
        if library.path.is_empty() {
            file.to_owned()
        } else {
            format!("{}/{file}", library.path)
        }
    };
    let ratio = &plan.tokens.ratio;
    let readme = tree
        .get(&inside("README.md"))
        .map_or(0, |text| ratio.tokens(Kind::Markdown, text));
    let src = inside("src");
    let interface: u64 = match library.shape {
        ExternalShape::Crate => tree
            .under(&src)
            .filter(|(path, _)| Kind::of(path) == Kind::Rust)
            .map(|(_, text)| ratio.tokens(Kind::Rust, &interface::rust(text)))
            .sum(),
        ExternalShape::Typst => tree
            .under(&src)
            .chain(tree.get(&inside("lib.typ")).map(|text| ("lib.typ", text)))
            .filter(|(path, _)| Kind::of(path) == Kind::Typst)
            .map(|(_, text)| ratio.tokens(Kind::Typst, &interface::typst(text)))
            .sum(),
    };
    let tokens = readme + interface;
    edit_plan(root, |document| {
        let libraries = document["external"]
            .as_array_of_tables_mut()
            .context("`external` is an array of tables")?;
        let entry = libraries
            .iter_mut()
            .find(|entry| entry.get("name").and_then(toml_edit::Item::as_str) == Some(name))
            .with_context(|| format!("no [[external]] named {name}"))?;
        entry["use"] = toml_edit::value(as_toml(tokens)?);
        Ok(())
    })?;
    println!(
        "context: {name} at {} is {tokens} tokens to use",
        library.rev
    );
    Ok(())
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

/// The key of `kind` in `[tokens.ratio]`.
const fn kind_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Rust => "rust",
        Kind::TypeScript => "typescript",
        Kind::Markdown => "markdown",
        Kind::Shell => "shell",
        Kind::Nix => "nix",
        Kind::Toml => "toml",
        Kind::Typst => "typst",
        Kind::Other => "other",
    }
}

/// A count as TOML's integer.
fn as_toml(count: u64) -> anyhow::Result<i64> {
    Ok(i64::try_from(count)?)
}

/// Change `context.toml` at `root` by `change`, keeping its comments and layout, and check that
/// it still parses.
fn edit_plan(
    root: &Path,
    change: impl FnOnce(&mut toml_edit::DocumentMut) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let path = root.join(FILE);
    let mut document: toml_edit::DocumentMut = std::fs::read_to_string(&path)?.parse()?;
    change(&mut document)?;
    let text = document.to_string();
    Plan::parse(&text)?;
    std::fs::write(&path, text)?;
    Ok(())
}

/// Every finding of the checkout at `root`, with its table: what the guard asserts and prints.
///
/// # Errors
/// When the plan or the tree cannot be read.
pub fn check(root: &Path) -> anyhow::Result<(String, Vec<Finding>)> {
    let plan = read_plan(root)?;
    let tree = Tree::checkout(root)?;
    let report = measure(&plan, &tree);
    Ok((
        table(&plan, &report, &base(root, &plan)),
        findings(&plan, &tree, &report),
    ))
}
