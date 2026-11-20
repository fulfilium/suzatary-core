//! The wait reading's law, over generated tasks and waits: a task reads the least price upstream
//! of it by waits, cycles included, so no blocker reads less urgent than a task that waits on it.

use std::collections::BTreeSet;

use proptest::prelude::*;
use suzatary_plan::waits::{Reading, readings};

const PRICES: [f64; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];

/// The least price among `task` and every task that reaches it by waits, by brute force.
fn upstream_least(own: &[Option<f64>], waits: &[(usize, usize)], task: usize) -> Option<f64> {
    let mut reached = BTreeSet::from([task]);
    loop {
        let more: Vec<usize> = waits
            .iter()
            .filter(|(w, b)| reached.contains(b) && !reached.contains(w) && *w < own.len())
            .map(|(w, _)| *w)
            .collect();
        if more.is_empty() {
            break;
        }
        reached.extend(more);
    }
    reached
        .iter()
        .filter_map(|task| own[*task])
        .min_by(f64::total_cmp)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A task reads the least price upstream of it by waits, cycles included; so no blocker reads
    /// less urgent than an open task that waits on it, and a raise names a task of that price.
    /// A wait with an end that is no task moves nothing.
    #[test]
    fn a_task_reads_the_most_urgent_task_upstream_of_it(
        own in proptest::collection::vec(proptest::option::of(0..PRICES.len()), 0..12),
        waits in proptest::collection::vec((0usize..14, 0usize..14), 0..24),
    ) {
        let own: Vec<Option<f64>> = own.into_iter().map(|price| price.map(|p| PRICES[p])).collect();
        let read = readings(&own, &waits);
        prop_assert_eq!(read.len(), own.len());
        let value = |task: usize| read[task].value(own[task]);
        for (task, reading) in read.iter().enumerate() {
            prop_assert_eq!(value(task), upstream_least(&own, &waits, task), "task {}", task);
            if let Reading::Raised { value: raised, by } = *reading {
                prop_assert_eq!(own[by], Some(raised));
            }
        }
        for &(waiter, blocker) in &waits {
            if waiter < own.len() && blocker < own.len()
                && let Some(w) = value(waiter)
            {
                prop_assert!(value(blocker).is_some_and(|b| b <= w));
            }
        }
    }
}
