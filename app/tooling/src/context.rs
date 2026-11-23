//! The context budget: the tokens an agent reads to use and to develop each unit, against
//! `context.toml`. It reads files and git alone, so the train measures a tree without building it.

pub mod finding;
pub mod interface;
pub mod measure;
mod order;
pub mod plan;
pub mod tokens;
pub mod tree;
mod wasm;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};

use self::finding::{Allowed, Finding, findings};
use self::measure::{Report, measure};
use self::plan::{ExternalShape, FILE, Plan};
use self::tokens::Kind;
use self::tree::{Tree, git};

/// What to do besides printing the table.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Act {
    /// Print the table; refuse on a finding.
    Show,
    /// List the files a unit's USE and DEVELOP read (the reading set's with none), largest first.
    Files(Option<String>),
    Record(Recording),
    Growth,
    /// Measure the external library `name`'s USE from a checkout of its pinned revision.
    External {
        name: String,
        checkout: PathBuf,
    },
    Order {
        unit: String,
        cut: Option<PathBuf>,
    },
}

/// Which figure `record` may write as the ratchet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recording {
    /// Only one at or below it: the ratchet falls.
    Lowered,
    /// The merged tree's reading set alone, whatever it is: the train's build, whose growth the
    /// guard judges against the merge base.
    Merged,
}

/// The tree a change started from, measured by the same plan, or why there is none.
#[derive(Debug)]
pub enum Base {
    /// The merge base with `origin/master`.
    Measured { rev: String, report: Report },
    /// No `origin/master` to compare with.
    Absent { why: String },
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

/// `act` on the repository at `repo`.
///
/// # Errors
/// When the plan or the tree cannot be read, when the guard's findings are not empty, or the
/// act's own.
pub fn run(repo: &Path, act: &Act) -> anyhow::Result<()> {
    let root = &repo
        .canonicalize()
        .with_context(|| format!("finding {}", repo.display()))?;
    let plan = read_plan(root)?;
    match act {
        Act::Show => {
            let (table, found) = check(root)?;
            print!("{table}");
            let grown = growth(root).unwrap_or_else(|e| {
                eprintln!("context: the growth check did not run: {e:#}");
                Vec::new()
            });
            let found: Vec<String> = found.iter().map(ToString::to_string).chain(grown).collect();
            for finding in &found {
                eprintln!("context: {finding}");
            }
            if !found.is_empty() {
                bail!("{} finding(s) against {FILE}", found.len());
            }
            Ok(())
        }
        Act::Growth => {
            let refused = growth(root)?;
            for growth in &refused {
                eprintln!("growth: {growth}");
            }
            if !refused.is_empty() {
                bail!(
                    "{} growth(s) the commits since the merge base do not declare",
                    refused.len()
                );
            }
            println!(
                "growth: the reading set and the wasm budget are within what the commits declare"
            );
            Ok(())
        }
        Act::Files(unit) => {
            let report = measure(&plan, &Tree::checkout(root)?);
            print!("{}", files(&report, unit.as_deref())?);
            Ok(())
        }
        Act::Record(recording) => record(root, &plan, *recording),
        Act::External { name, checkout } => external(root, &plan, name, checkout),
        Act::Order { unit, cut } => {
            let cut = match cut {
                Some(path) => Some(order::Cut::parse(
                    &std::fs::read_to_string(path)
                        .with_context(|| format!("reading {}", path.display()))?,
                )?),
                None => None,
            };
            let tree = Tree::checkout(root)?;
            let report = measure(&plan, &tree);
            print!(
                "{}",
                order::order(&plan, &tree, &report, unit, cut.as_ref())?
            );
            Ok(())
        }
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
    for (library, external) in plan.externals() {
        let _ = writeln!(
            out,
            "{:<13} {:<20} external, {} at {}, re-exports {:?}",
            external.name,
            cell(external.use_, None, Some(plan.budget.use_)),
            library.source,
            &library.rev[..8.min(library.rev.len())],
            external.reexports
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
    unit: String,
    figure: Budgeted,
    tokens: u64,
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

/// The repository's lines of the table: the goal, the reading set, the ratchets, the plans in
/// force and the uncited plans.
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
    let _ = writeln!(
        out,
        "plans in force, outside the reading set: {}, {} tokens",
        report.in_force.0.len(),
        short(report.in_force.total())
    );
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
/// ratchet, or a unit is over budget with no ratchet. `Recording::Merged` writes the reading set
/// alone, whatever it is.
fn record(root: &Path, plan: &Plan, recording: Recording) -> anyhow::Result<()> {
    let report = measure(plan, &Tree::checkout(root)?);
    let reading = report.reading.total();
    if recording == Recording::Merged {
        // The one line the train's build writes; the units' ratchets fall only by hand.
        edit_plan(root, |document| {
            document["ratchet"]["reading"] = toml_edit::value(as_toml(reading)?);
            Ok(())
        })?;
        println!("context: the reading set of the merged tree is {reading} tokens");
        return Ok(());
    }
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
    let (library, part) = plan
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
        if part.path.is_empty() {
            file.to_owned()
        } else {
            format!("{}/{file}", part.path)
        }
    };
    let ratio = &plan.tokens.ratio;
    let readme = tree
        .get(&inside("README.md"))
        .map_or(0, |text| ratio.tokens(Kind::Markdown, text));
    let src = inside("src");
    let interface: u64 = match part.shape {
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
        let libraries = document["library"]
            .as_array_of_tables_mut()
            .context("`library` is an array of tables")?;
        let entry = libraries
            .iter_mut()
            .filter_map(|library| library.get_mut("external")?.as_array_of_tables_mut())
            .flat_map(toml_edit::ArrayOfTables::iter_mut)
            .find(|entry| entry.get("name").and_then(toml_edit::Item::as_str) == Some(name))
            .with_context(|| format!("no [[library.external]] named {name}"))?;
        entry["use"] = toml_edit::value(as_toml(tokens)?);
        Ok(())
    })?;
    println!(
        "context: {name} at {} is {tokens} tokens to use",
        library.rev
    );
    Ok(())
}

/// The key of `kind` in `[tokens.ratio]`.
#[must_use]
pub const fn kind_key(kind: Kind) -> &'static str {
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
///
/// # Errors
/// When it does not fit.
pub fn as_toml(count: u64) -> anyhow::Result<i64> {
    Ok(i64::try_from(count)?)
}

/// Change `context.toml` at `root` by `change`, keeping its comments and layout, and check that
/// it still parses.
///
/// # Errors
/// When it cannot be read, changed or written, or no longer parses.
pub fn edit_plan(
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

/// Each growth of the reading set or a wasm line the commits since the merge base do not declare,
/// and each declaration not in its form.
///
/// # Errors
/// When the plan, the tree or git cannot be read, or there is no merge base with `origin/master`:
/// a check that compared nothing is never a pass.
pub(crate) fn growth(root: &Path) -> anyhow::Result<Vec<String>> {
    let plan = read_plan(root)?;
    let report = measure(&plan, &Tree::checkout(root)?);
    let base = git(root, &["merge-base", "HEAD", "origin/master"])
        .context("no merge base with origin/master to compare the growth with")?;
    let base = base.trim();
    let was = plan::ratchet(&git(root, &["show", &format!("{base}:{FILE}")])?)
        .with_context(|| format!("reading {FILE} at the merge base"))?;
    let range = format!("{base}..HEAD");
    let declared =
        crate::git::declared(root, &range, "Reading-growth").map_err(|e| anyhow::anyhow!("{e}"))?;
    let merges = git(root, &["log", "--first-parent", "--format=%s", &range])?
        .lines()
        .filter(|s| s.starts_with("Merge #"))
        .count();
    let allowed = Allowed::of(was.reading, plan.budget.allowance, merges as u64, &declared);
    let mut refused = finding::malformed(&declared);
    refused.extend(
        finding::reading(plan.ratchet.reading, report.reading.total(), &allowed)
            .iter()
            .map(ToString::to_string),
    );
    refused.extend(wasm::refused(root, base)?);
    Ok(refused)
}
