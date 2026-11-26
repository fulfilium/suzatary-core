//! The brief of the project's memory: a function of the store and its arguments, within its
//! budget, and every decision in force reachable from it by zooms.

use std::collections::BTreeSet;

use prodrome_data::event::Actor;
use prodrome_data::literal::Datetime;
use proptest::prelude::*;

use suzatary_digest::decisions::about::About;
use suzatary_digest::decisions::brief::{BYTES_PER_TOKEN, brief, zoom};
use suzatary_digest::decisions::decision_log::DecisionLog;
use suzatary_digest::decisions::summarizing::summarize;
use suzatary_digest::joining::Joining;
use suzatary_record::record::MarkupSource;
use suzatary_store::store::Prodrome as _;
use suzatary_stores::stores::InMemory;
use suzatary_todo::decisions::{
    Area, DecisionEvent, DecisionId, Decisions, Statement, decided, governs, withdrawn,
};

/// One decision of a plan: what it says, which area it governs, and whether it was withdrawn.
#[derive(Debug, Clone)]
struct Plan {
    seed: u64,
    area: Option<usize>,
    withdrawn: bool,
}

fn plans() -> impl Strategy<Value = Vec<Plan>> {
    proptest::collection::vec(
        (any::<u64>(), proptest::option::of(0usize..3), any::<bool>()).prop_map(
            |(seed, area, withdrawn)| Plan {
                seed,
                area,
                withdrawn,
            },
        ),
        1..45,
    )
}

fn ada() -> Actor {
    Actor::new("ada").expect("an actor")
}

fn at(n: usize) -> Datetime {
    let n = u32::try_from(n).expect("a small number");
    Datetime::new(2026, 10, 3 + n / 1000, 8 + (n / 60) % 14, n % 60, 0, 0).expect("an instant")
}

fn id(n: usize) -> DecisionId {
    DecisionId::new(format!("decision{n}")).expect("an id")
}

fn area(part: usize) -> Area {
    Area::new(format!("app/part{part}")).expect("an area")
}

/// A store holding the decisions of `plan`, one every minute.
fn written(plan: &[Plan]) -> InMemory<DecisionEvent> {
    let store = InMemory::<DecisionEvent>::default();
    for (n, step) in plan.iter().enumerate() {
        let statement = Statement::new(
            &format!("Decision {}", step.seed),
            MarkupSource::escaped(&format!(
                "It says {} about part {:?}.",
                step.seed, step.area
            )),
            "Because it was so.",
        )
        .expect("a statement");
        store
            .append(decided(id(n), at(n), ada(), statement, BTreeSet::new()).expect("an event"))
            .expect("appended");
        if let Some(part) = step.area {
            let areas = BTreeSet::from([area(part)]);
            store
                .append(governs(id(n), at(n), ada(), areas).expect("an event"))
                .expect("appended");
        }
        if step.withdrawn {
            store
                .append(withdrawn(id(n), at(n), ada(), "No longer held.").expect("an event"))
                .expect("appended");
        }
    }
    store
}

fn abouts(words: &[&str]) -> Vec<About> {
    words
        .iter()
        .map(|word| About::new(word).expect("an about"))
        .collect()
}

fn brief_of(decisions: &Decisions<'_>, about: &[About], tokens: usize) -> String {
    brief(
        &decisions.ledger().expect("a ledger"),
        &decisions.summaries().expect("summaries"),
        about,
        tokens,
    )
    .expect("a brief")
}

fn summarized(decisions: &Decisions<'_>) {
    summarize(decisions, &Joining::default(), at(0), &ada(), |_| {}).expect("summaries");
}

/// The handle that leads a line of the brief's record, if one does.
fn handle_of(line: &str) -> Option<&str> {
    let handle = line.split_whitespace().next()?;
    let (first, count) = handle.split_once('+')?;
    (first.parse::<usize>().is_ok() && count.parse::<usize>().is_ok()).then_some(handle)
}

/// The decisions that zooming into every part the brief shows reaches.
fn reached(decisions: &Decisions<'_>, brief: &str) -> BTreeSet<String> {
    let log = DecisionLog::of(
        &decisions.ledger().expect("a ledger"),
        &decisions.summaries().expect("summaries"),
    )
    .expect("a log");
    let record = brief
        .split("THE REST, oldest first:\n")
        .nth(1)
        .expect("a record");
    let mut waiting: Vec<String> = record
        .lines()
        .filter_map(|line| handle_of(line).map(str::to_owned))
        .collect();
    if let Some(cut) = record
        .split("zoom(")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
    {
        waiting.push(cut.to_owned());
    }
    let mut found = BTreeSet::new();
    let mut opened = BTreeSet::new();
    while let Some(handle) = waiting.pop() {
        if !opened.insert(handle.clone()) {
            continue;
        }
        let beneath = zoom(&log, &handle).expect("a handle the brief or a zoom printed");
        for line in beneath.lines() {
            let Some(child) = handle_of(line) else {
                continue;
            };
            let leaf = child.ends_with("+1");
            let first = line.split_whitespace().nth(1).unwrap_or_default();
            match (leaf, first.strip_suffix(':')) {
                (true, Some(decision)) => {
                    found.insert(decision.to_owned());
                }
                (_, _) => waiting.push(child.to_owned()),
            }
        }
    }
    found.extend(record.lines().filter_map(|line| {
        let (handle, rest) = line.split_once(' ')?;
        handle
            .ends_with("+1")
            .then(|| rest.split(':').next().map(str::to_owned))?
    }));
    found
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The same objects and arguments print the same bytes, within the budget, with and without
    /// summaries; the decisions a task names stand whole, and no other stands whole.
    #[test]
    fn the_brief_is_a_function_of_the_store_and_its_arguments(
        plan in plans(),
        tokens in 600usize..2_500,
        named in proptest::sample::subsequence(vec!["app/part0", "app/part1", "app/part2/deep", "app"], 0..3),
    ) {
        let (one, two) = (written(&plan), written(&plan));
        let (first, second) = (Decisions { store: &one }, Decisions { store: &two });
        let about = abouts(&named);
        let printed = brief_of(&first, &about, tokens);
        prop_assert_eq!(&printed, &brief_of(&second, &about, tokens));
        prop_assert!(printed.len() <= tokens * BYTES_PER_TOKEN, "{} bytes", printed.len());
        let cut = printed.contains("more name them");
        for (n, step) in plan.iter().enumerate().filter(|(_, step)| !step.withdrawn) {
            let whole = printed.contains(&format!("decision{n}: Decision {}\ndecided", step.seed));
            let names_it = step.area.is_some_and(|part| {
                named.iter().any(|path| {
                    let area = format!("app/part{part}");
                    *path == "app" || *path == area || path.starts_with(&format!("{area}/"))
                })
            });
            prop_assert!(!whole || names_it, "decision{n} stands whole and nothing names it");
            prop_assert!(!names_it || cut || whole, "decision{n} is named and not whole");
        }
        summarized(&first);
        summarized(&second);
        let summed = brief_of(&first, &about, tokens);
        prop_assert_eq!(&summed, &brief_of(&second, &about, tokens));
        prop_assert!(summed.len() <= tokens * BYTES_PER_TOKEN, "{} bytes", summed.len());
    }

    /// Every decision in force is reachable from the brief by zooms, and none out of force is.
    #[test]
    fn every_decision_in_force_is_reachable_from_the_brief_by_zooms(
        plan in plans(),
        tokens in 600usize..1_500,
    ) {
        let store = written(&plan);
        let decisions = Decisions { store: &store };
        let in_force: BTreeSet<String> = plan
            .iter()
            .enumerate()
            .filter(|(_, step)| !step.withdrawn)
            .map(|(n, _)| id(n).to_string())
            .collect();
        prop_assert_eq!(reached(&decisions, &brief_of(&decisions, &[], tokens)), in_force.clone());
        summarized(&decisions);
        prop_assert_eq!(reached(&decisions, &brief_of(&decisions, &[], tokens)), in_force);
    }
}
