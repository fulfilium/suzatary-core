//! The derived figures: numbers that are a function of the merged tree alone. The tree records each
//! as the train's build last measured it, and a pull request moves none. A growth past the allowance
//! is declared on a trailer, and the gate judges every figure by one rule (`judge`).

use std::collections::BTreeMap;
use std::fmt;

use super::measure::Report;
use super::plan::{FILE, Plan, Ratchet};
use super::wasm::BUDGET;

/// A family of figures: its trailer, its unit and its allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Family {
    Reading,
    Use,
    Wasm,
}

pub(crate) const FAMILIES: [Family; 3] = [Family::Reading, Family::Use, Family::Wasm];

impl Family {
    /// The trailer a pull request declares a growth on.
    #[must_use]
    pub(crate) const fn trailer(self) -> &'static str {
        match self {
            Self::Reading => "Reading-growth",
            Self::Use => "Use-growth",
            Self::Wasm => "Wasm-growth",
        }
    }

    const fn unit(self) -> &'static str {
        match self {
            Self::Reading | Self::Use => "tokens",
            Self::Wasm => "bytes",
        }
    }

    const fn form(self) -> &'static str {
        match self {
            Self::Reading => "+<tokens>: <reason>",
            Self::Use => "<unit> +<tokens>: <reason>",
            Self::Wasm => "<module> +<bytes>: <reason>",
        }
    }

    /// The file that records it.
    const fn file(self) -> &'static str {
        match self {
            Self::Reading | Self::Use => FILE,
            Self::Wasm => BUDGET,
        }
    }
}

/// One figure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Figure {
    /// The repository's reading set, in tokens.
    Reading,
    /// The USE of a unit over `budget.use`, in tokens.
    Use(String),
    /// A shipped wasm module's bytes after wasm-opt, as `system` builds it.
    Wasm { module: String, system: String },
}

impl Figure {
    #[must_use]
    pub(crate) const fn family(&self) -> Family {
        match self {
            Self::Reading => Family::Reading,
            Self::Use(_) => Family::Use,
            Self::Wasm { .. } => Family::Wasm,
        }
    }

    /// The name a declaration gives it: a module's growth holds on every system.
    fn named(&self) -> Option<&str> {
        match self {
            Self::Reading => None,
            Self::Use(unit) => Some(unit),
            Self::Wasm { module, .. } => Some(module),
        }
    }
}

impl fmt::Display for Figure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reading => write!(f, "the reading set"),
            Self::Use(unit) => write!(f, "{unit}'s USE"),
            Self::Wasm { module, system } => write!(f, "{module} on {system}"),
        }
    }
}

/// Figures and their numbers.
pub(crate) type Ledger = BTreeMap<Figure, u64>;

/// The rule's terms over one range of commits.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rule {
    /// What a tokens figure may grow by for each merged pull request, undeclared.
    pub(crate) tokens: u64,
    /// What a module may grow by for each merged pull request, undeclared.
    pub(crate) bytes: u64,
    /// `budget.use`: where a unit the ratchet does not hold starts.
    pub(crate) use_budget: u64,
    /// The pull requests merged in the range: a train's `Merge #` commits.
    pub(crate) merges: u64,
}

impl Rule {
    /// The most `figure` may be: `recorded` (the base's), the allowance for each merge (one at
    /// least) and what `grants` declare for it. A unit's USE the base does not record starts at the
    /// budget with no allowance, so crossing it is declared; another figure starts at 0.
    #[must_use]
    pub(crate) fn limit(&self, figure: &Figure, recorded: Option<u64>, grants: &Grants) -> u64 {
        let allowance = match figure.family() {
            Family::Reading | Family::Use => self.tokens,
            Family::Wasm => self.bytes,
        }
        .saturating_mul(self.merges.max(1));
        let (start, allowance) = match (figure, recorded) {
            (_, Some(recorded)) => (recorded, allowance),
            (Figure::Use(_), None) => (self.use_budget, 0),
            (Figure::Reading | Figure::Wasm { .. }, None) => (0, allowance),
        };
        start
            .saturating_add(allowance)
            .saturating_add(grants.of(figure))
    }
}

/// What the trailers declare, by family and name.
#[derive(Debug, Default)]
pub(crate) struct Grants(BTreeMap<(Family, Option<String>), u64>);

impl Grants {
    /// The names declared in `family`.
    pub(crate) fn named(&self, family: Family) -> impl Iterator<Item = &str> {
        self.0
            .keys()
            .filter(move |(f, _)| *f == family)
            .filter_map(|(_, name)| name.as_deref())
    }

    fn of(&self, figure: &Figure) -> u64 {
        self.0
            .get(&(figure.family(), figure.named().map(str::to_owned)))
            .copied()
            .unwrap_or(0)
    }
}

/// How the declarations of several commit messages join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gathered {
    /// A train's merges, each holding its pull request's lines once: they add.
    Merges,
    /// A branch's own commits, which repeat a line the description also holds: the largest
    /// counts.
    Commits,
}

/// One refusal of the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A trailer's value not in its family's form: it declares nothing.
    Malformed { family: Family, value: String },
    /// A trailer's value whose number, with the others for its figure, does not fit in 64 bits.
    TooLarge { family: Family, value: String },
    /// A figure grown past what the merge base allows; `by`, the merges that moved its sources.
    Undeclared {
        figure: Figure,
        measured: u64,
        limit: u64,
        by: Vec<String>,
    },
    /// A figure written otherwise than the merge base records, outside the train's commit.
    Written(Moved<String>),
    /// A tokens figure the train's commit records otherwise than the tree measures.
    Misrecorded(Moved<String>),
}

/// A figure two maps hold otherwise; `None` where one does not hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Moved<V> {
    pub(crate) figure: Figure,
    pub(crate) from: Option<V>,
    pub(crate) to: Option<V>,
}

impl<V: fmt::Display> Moved<V> {
    pub(crate) fn shown(self) -> Moved<String> {
        Moved {
            figure: self.figure,
            from: self.from.map(|v| v.to_string()),
            to: self.to.map(|v| v.to_string()),
        }
    }
}

impl<V: fmt::Display> fmt::Display for Moved<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown = |v: &Option<V>| {
            v.as_ref()
                .map_or_else(|| "nothing".to_owned(), ToString::to_string)
        };
        write!(
            f,
            "{}: {}, from {}",
            self.figure,
            shown(&self.to),
            shown(&self.from)
        )
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let named = |figure: &Figure| figure.named().map_or_else(String::new, |n| format!("{n} "));
        match self {
            Self::Malformed { family, value } => write!(
                f,
                "`{trailer}: {value}` is not `{trailer}: {}`",
                family.form(),
                trailer = family.trailer()
            ),
            Self::TooLarge { family, value } => write!(
                f,
                "`{}: {value}` declares more than 64 bits hold",
                family.trailer()
            ),
            Self::Undeclared {
                figure,
                measured,
                limit,
                by,
            } => {
                write!(
                    f,
                    "{figure} is {measured} {unit}, over the {limit} its merge base allows: \
                     delete as much, or declare `{trailer}: {}+{}: <reason>` (AGENTS.md, \
                     \"Derived figures\")",
                    named(figure),
                    measured - limit,
                    unit = figure.family().unit(),
                    trailer = figure.family().trailer()
                )?;
                if by.is_empty() {
                    Ok(())
                } else {
                    write!(f, "; moved by {}", by.join(", "))
                }
            }
            Self::Written(moved) => write!(
                f,
                "{}: {moved} on the merge base, written by hand where the train records it. \
                 Remove the edit, and declare a growth on a `{}: {}` line",
                moved.figure.family().file(),
                moved.figure.family().trailer(),
                moved.figure.family().form()
            ),
            Self::Misrecorded(moved) => write!(
                f,
                "{}: the tree measures {} where the train's commit records {}: the train \
                 recorded with another measure than the gate's",
                moved.figure.family().file(),
                moved.to.as_deref().unwrap_or("nothing"),
                moved.from.as_deref().unwrap_or("nothing")
            ),
        }
    }
}

/// What the commit `messages` declare, each a list of trailers with its family, joined as
/// `gathered` says. A value not in its family's form, or too large, is refused, so a declaration
/// is never read as something it does not say.
#[must_use]
pub(crate) fn grants(
    messages: &[Vec<(Family, String)>],
    gathered: Gathered,
) -> (Grants, Vec<Refusal>) {
    let (mut grants, mut refused) = (Grants::default(), Vec::new());
    for message in messages {
        let mut here: BTreeMap<(Family, Option<String>), (u64, &str)> = BTreeMap::new();
        for (family, value) in message {
            let Some((name, n)) = parse(*family, value) else {
                refused.push(Refusal::Malformed {
                    family: *family,
                    value: value.clone(),
                });
                continue;
            };
            let entry = here.entry((*family, name)).or_insert((0, value));
            match entry.0.checked_add(n) {
                Some(sum) => *entry = (sum, value),
                None => refused.push(Refusal::TooLarge {
                    family: *family,
                    value: value.clone(),
                }),
            }
        }
        for (key, (n, value)) in here {
            let family = key.0;
            let total = grants.0.entry(key).or_insert(0);
            let joined = match gathered {
                Gathered::Merges => total.checked_add(n),
                Gathered::Commits => Some((*total).max(n)),
            };
            match joined {
                Some(joined) => *total = joined,
                None => refused.push(Refusal::TooLarge {
                    family,
                    value: value.to_owned(),
                }),
            }
        }
    }
    (grants, refused)
}

fn parse(family: Family, value: &str) -> Option<(Option<String>, u64)> {
    let (head, why) = value.split_once(':')?;
    if why.trim().is_empty() {
        return None;
    }
    let (name, n) = match family {
        Family::Reading => (None, head.trim().strip_prefix('+')?),
        Family::Use | Family::Wasm => {
            let (name, n) = head.trim().split_once(" +")?;
            let name = name.trim();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return None;
            }
            (Some(name.to_owned()), n)
        }
    };
    Some((name, n.trim().parse().ok()?))
}

/// Each figure of `now` over its limit (`Rule::limit`) from what `base` records.
#[must_use]
pub(crate) fn judge(base: &Ledger, now: &Ledger, grants: &Grants, rule: Rule) -> Vec<Refusal> {
    now.iter()
        .filter_map(|(figure, &measured)| {
            let limit = rule.limit(figure, base.get(figure).copied(), grants);
            (measured > limit).then(|| Refusal::Undeclared {
                figure: figure.clone(),
                measured,
                limit,
                by: Vec::new(),
            })
        })
        .collect()
}

/// Each figure `to` holds otherwise than `from`.
#[must_use]
pub(crate) fn moved<V: PartialEq + Clone>(
    from: &BTreeMap<Figure, V>,
    to: &BTreeMap<Figure, V>,
) -> Vec<Moved<V>> {
    let figures: std::collections::BTreeSet<&Figure> = from.keys().chain(to.keys()).collect();
    figures
        .into_iter()
        .filter(|figure| from.get(figure) != to.get(figure))
        .map(|figure| Moved {
            figure: figure.clone(),
            from: from.get(figure).cloned(),
            to: to.get(figure).cloned(),
        })
        .collect()
}

/// The tokens figures `ratchet` records.
#[must_use]
pub(crate) fn recorded(ratchet: &Ratchet) -> Ledger {
    std::iter::once((Figure::Reading, ratchet.reading))
        .chain(
            ratchet
                .use_
                .iter()
                .map(|(unit, &tokens)| (Figure::Use(unit.clone()), tokens)),
        )
        .collect()
}

/// The tokens figures of `report`: the reading set, and the USE of each unit over the budget.
#[must_use]
pub(crate) fn measured(plan: &Plan, report: &Report) -> Ledger {
    std::iter::once((Figure::Reading, report.reading.total()))
        .chain(
            report
                .units
                .iter()
                .filter(|unit| unit.figures.use_ > plan.budget.use_)
                .map(|unit| (Figure::Use(unit.name.clone()), unit.figures.use_)),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Family, Figure, Gathered, Grants, Ledger, Refusal, Rule, grants, judge, moved};

    fn rule(merges: u64) -> Rule {
        Rule {
            tokens: 2_000,
            bytes: 16_000,
            use_budget: 25_000,
            merges,
        }
    }

    fn views() -> Figure {
        Figure::Wasm {
            module: "views_bg.wasm".to_owned(),
            system: "aarch64-linux".to_owned(),
        }
    }

    fn declared(lines: &[(Family, &str)]) -> Vec<(Family, String)> {
        lines.iter().map(|(f, v)| (*f, (*v).to_owned())).collect()
    }

    fn one(lines: &[(Family, &str)]) -> Grants {
        grants(&[declared(lines)], Gathered::Merges).0
    }

    /// Each family's form, and a value not in it, or too large, refused, so it is never read as
    /// what it does not say.
    #[test]
    fn a_declaration_is_read_in_its_family_s_form_or_refused() {
        let right = [
            (Family::Reading, "+600: the laws"),
            (Family::Use, "lines +700: the rule's view"),
            (Family::Wasm, "views_bg.wasm +19000: the topic bar"),
        ];
        assert!(grants(&[declared(&right)], Gathered::Merges).1.is_empty());
        let wrong = [
            (Family::Reading, "600 tokens"),
            (Family::Reading, "+600:"),
            (Family::Use, "+700: no unit"),
            (Family::Wasm, "views_bg.wasm 19000 bytes: why"),
            (Family::Wasm, "two words +1: why"),
        ];
        for pair in wrong {
            let (_, refused) = grants(&[declared(&[pair])], Gathered::Merges);
            assert!(
                refused.len() == 1 && refused[0].to_string().contains(pair.0.form()),
                "{pair:?}"
            );
        }
        let huge = format!("+{}: x", u64::MAX);
        let (_, refused) = grants(
            &[declared(&[
                (Family::Reading, "+1: x"),
                (Family::Reading, &huge),
            ])],
            Gathered::Merges,
        );
        assert!(
            matches!(&refused[..], [Refusal::TooLarge { .. }]),
            "{refused:?}"
        );
    }

    /// A train's merges each hold their pull request's lines once, so they add; a branch's own
    /// commits repeat the description's line, so the largest counts and a repeat grants nothing.
    #[test]
    fn a_declaration_counts_once_on_a_branch_and_once_per_merge_on_a_train() {
        let line = declared(&[(Family::Wasm, "views_bg.wasm +20000: the sheet")]);
        let twice = [line.clone(), line];
        let limit = |gathered| rule(1).limit(&views(), Some(0), &grants(&twice, gathered).0);
        assert_eq!(limit(Gathered::Commits), 16_000 + 20_000);
        assert_eq!(limit(Gathered::Merges), 16_000 + 40_000);
    }

    /// One rule over every family: a figure grows by the allowance for each merged pull request
    /// and by what is declared for it alone; a fall is within; a module the base does not record
    /// starts at 0; a unit the ratchet does not hold starts at the budget, with no allowance.
    #[test]
    fn a_figure_grows_by_the_allowance_per_merge_or_by_what_is_declared() {
        let lines = Figure::Use("lines".to_owned());
        let base = Ledger::from([
            (Figure::Reading, 1_000),
            (lines.clone(), 30_000),
            (views(), 490_000),
        ]);
        let at = |reading, use_, bytes| {
            Ledger::from([
                (Figure::Reading, reading),
                (lines.clone(), use_),
                (views(), bytes),
            ])
        };
        let none = Grants::default();
        assert!(judge(&base, &at(3_000, 32_000, 506_000), &none, rule(0)).is_empty());
        assert_eq!(
            judge(&base, &at(3_001, 32_001, 506_001), &none, rule(1)).len(),
            3
        );
        assert!(judge(&base, &at(3_001, 32_001, 506_001), &none, rule(2)).is_empty());
        assert!(
            judge(&base, &at(10, 20_000, 1), &none, rule(1)).is_empty(),
            "a fall"
        );
        let grown = at(3_000, 32_000, 525_000);
        let refused = judge(&base, &grown, &none, rule(1));
        assert!(
            refused.len() == 1
                && refused[0]
                    .to_string()
                    .contains("`Wasm-growth: views_bg.wasm +19000: <reason>`"),
            "{refused:?}"
        );
        let wasm = |v| one(&[(Family::Wasm, v)]);
        assert!(
            judge(
                &base,
                &grown,
                &wasm("views_bg.wasm +19000: the bar"),
                rule(1)
            )
            .is_empty()
        );
        assert_eq!(
            judge(&base, &grown, &wasm("prodrome_bg.wasm +19000: x"), rule(1)).len(),
            1
        );
        let new = Ledger::from([(views(), 30_000)]);
        assert!(matches!(
            &judge(&Ledger::new(), &new, &none, rule(1))[..],
            [Refusal::Undeclared {
                measured: 30_000,
                limit: 16_000,
                ..
            }]
        ));
        let crossed = Ledger::from([(Figure::Use("os".to_owned()), 26_000)]);
        let refused = judge(&Ledger::new(), &crossed, &none, rule(3));
        assert!(
            refused.len() == 1
                && refused[0]
                    .to_string()
                    .contains("`Use-growth: os +1000: <reason>`"),
            "a unit crossing the budget is declared, whatever the merges: {refused:?}"
        );
        let said = one(&[(Family::Use, "os +1000: the split")]);
        assert!(judge(&Ledger::new(), &crossed, &said, rule(3)).is_empty());
    }

    /// A figure written, added or removed by hand is refused, naming the trailer to write.
    #[test]
    fn a_figure_written_by_hand_is_refused_naming_the_trailer() {
        let base = Ledger::from([(Figure::Reading, 1_000), (views(), 490_000)]);
        assert!(moved(&base, &base).is_empty());
        let raised = Ledger::from([(Figure::Reading, 1_000), (views(), 500_000)]);
        let refused: Vec<Refusal> = moved(&base, &raised)
            .into_iter()
            .map(|m| Refusal::Written(m.shown()))
            .collect();
        assert!(refused.len() == 1 && refused[0].to_string().contains("`Wasm-growth: <module>"));
        let joined = Ledger::from([(Figure::Reading, 1_000), (Figure::Use("os".to_owned()), 1)]);
        assert_eq!(moved(&base, &joined).len(), 2);
    }
}
