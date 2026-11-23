//! Publication's laws: the rows a publish makes (law 5), the outcomes of the one check before a
//! push, the moves of admitted bytes, and the edges a publish parses. Law 8 is `admitted.rs`'s `compile_fail` doc tests.

use proptest::prelude::*;
use std::collections::BTreeSet;
use suzatary_tooling::core_rows::{CoreRow, Rows};
use suzatary_tooling::fresh::{Fresh, Push};
use suzatary_tooling::gate_branch::GateBranch;
use suzatary_tooling::leak_check::{CheckError, LeakCheck};
use suzatary_tooling::oid::Oid;
use suzatary_tooling::plan::{Plan, Source};
use suzatary_tooling::publication;
use suzatary_tooling::publishable::{Publishable, publishable};
use suzatary_tooling::refusal::Refusal;
use suzatary_tooling::repo_path::RepoPath;
use suzatary_tooling::{Command, Error};

fn oid(n: usize) -> Oid {
    Oid::parse(&format!("{n:040x}")).expect("40 hexadecimal digits")
}

/// Master's first-parent chain of `n` commits, newest first, `oid(0)` the oldest.
fn chain(n: usize) -> Vec<Oid> {
    (0..n).rev().map(oid).collect()
}

/// What core publishes of the chain.
#[derive(Clone, Copy, Debug)]
enum Published {
    Nothing,
    /// `oid(0)` through `oid(p)`, one row each.
    Through(usize),
    /// A landed commit not on the chain, so no rev of the chain descends from it.
    Foreign,
}

const FOREIGN: usize = 900;
const OFF_CHAIN: usize = 800;

/// Core's rows, newest first, above a populated commit that names no source.
fn rows_of(published: Published) -> Rows {
    let row = |n: usize| CoreRow {
        commit: oid(1000 + n),
        sources: vec![oid(n)],
        skipped: Vec::new(),
    };
    let mut rows: Vec<CoreRow> = match published {
        Published::Nothing => Vec::new(),
        Published::Through(p) => (0..=p).rev().map(row).collect(),
        Published::Foreign => vec![row(FOREIGN)],
    };
    rows.push(CoreRow {
        commit: oid(999),
        sources: Vec::new(),
        skipped: Vec::new(),
    });
    Rows(rows)
}

/// Core's rows once `plan`'s rows are pushed on `rows`, each a new commit on top.
fn pushed(rows: &Rows, plan: &[Source]) -> Rows {
    let mut out = rows.0.clone();
    for (at, step) in plan.iter().enumerate() {
        if let Source::Row { source, skipped } = step {
            out.insert(
                0,
                CoreRow {
                    commit: oid(5000 + at),
                    sources: vec![source.clone()],
                    skipped: skipped.clone(),
                },
            );
        }
    }
    Rows(out)
}

/// A chain, what core publishes, the rev asked for (`OFF_CHAIN` off the chain) and the skips.
fn a_publish() -> impl Strategy<Value = (usize, Published, usize, BTreeSet<usize>)> {
    (1usize..24).prop_flat_map(|n| {
        (
            Just(n),
            prop_oneof![
                Just(Published::Nothing),
                (0..n).prop_map(Published::Through),
                Just(Published::Foreign),
            ],
            prop_oneof![8 => 0..n, 1 => Just(OFF_CHAIN)],
            proptest::collection::btree_set(0..n, 0..4),
        )
    })
}

proptest! {
    /// Law 5: only a commit of master's first-parent chain is published, and the table only
    /// climbs; the rows are the commits after the newest `Source:`, oldest first (`rev` alone where
    /// core publishes nothing); a commit already published is published by its row; a skip is
    /// taken only for a commit of those rows that is not the last, and the next row names it.
    #[test]
    fn law_5_the_rows_are_the_first_parent_commits_after_the_newest_source(
        (n, published, rev, skips) in a_publish()
    ) {
        let chain = chain(n);
        let rows = rows_of(published);
        let skips: Vec<Oid> = skips.into_iter().map(oid).collect();
        let planned = Plan::of(&chain, &rows, &oid(rev), &skips);
        let sources: Vec<Oid> = match (rev, published) {
            (OFF_CHAIN, _) => {
                prop_assert_eq!(planned, Err(Refusal::NotLanded(oid(rev))));
                return Ok(());
            }
            (_, Published::Through(p)) if rev <= p => {
                prop_assert_eq!(planned, Ok(Plan::Published(oid(1000 + rev))));
                return Ok(());
            }
            (_, Published::Foreign) => {
                prop_assert_eq!(planned, Err(Refusal::Climbs { newest: oid(FOREIGN), rev: oid(rev) }));
                return Ok(());
            }
            (_, Published::Nothing) => vec![oid(rev)],
            (_, Published::Through(p)) => (p + 1..=rev).map(oid).collect(),
        };
        let refused = skips.iter().find_map(|skip| {
            if !sources.contains(skip) {
                Some(Refusal::SkipNotARow(skip.clone()))
            } else if sources.last() == Some(skip) {
                Some(Refusal::SkipIsLast(skip.clone()))
            } else {
                None
            }
        });
        if let Some(refusal) = refused {
            prop_assert_eq!(planned, Err(refusal));
            return Ok(());
        }
        let Ok(Plan::Rows(plan)) = planned else {
            return Err(TestCaseError::fail(format!("no rows: {planned:?}")));
        };
        let reached: Vec<Oid> = plan
            .iter()
            .map(|step| match step {
                Source::Row { source, .. } | Source::Skip(source) => source.clone(),
            })
            .collect();
        prop_assert_eq!(&reached, &sources);
        let mut waiting: Vec<Oid> = Vec::new();
        for step in &plan {
            match step {
                Source::Skip(skip) => {
                    prop_assert!(skips.contains(skip));
                    waiting.push(skip.clone());
                }
                Source::Row { source, skipped } => {
                    prop_assert!(!skips.contains(source));
                    prop_assert_eq!(skipped, &std::mem::take(&mut waiting));
                }
            }
        }
        prop_assert!(waiting.is_empty(), "every skip is named by a later row");
    }

    /// Law 5: `publish(publish(s)) = publish(s)`. Once a plan's rows are pushed, publishing the
    /// same rev again makes no row.
    #[test]
    fn law_5_publishing_twice_is_publishing_once(
        (n, published, rev, _) in a_publish()
    ) {
        let chain = chain(n);
        let rows = rows_of(published);
        if let Ok(Plan::Rows(plan)) = Plan::of(&chain, &rows, &oid(rev), &[]) {
            let again = Plan::of(&chain, &pushed(&rows, &plan), &oid(rev), &[]);
            prop_assert!(matches!(again, Ok(Plan::Published(_))), "{again:?}");
        }
    }
}

fn could_not_run() -> CheckError {
    CheckError::Unrun {
        source: oid(1),
        status: "exited 2".into(),
    }
}

/// The one check before a push, as a table of (check, push, what is fresh) to status: 0
/// publishable, 1 a leak, 3 could not run, 4 held. A gate branch may hold a new plain-text path
/// and no new admission; a row holds neither until a person admits it. A check that could not
/// run is never a leak or a pass: it is status 3, the status of every error of the tool, which
/// train.sh reads as no verdict.
#[test]
fn the_check_before_a_push_is_a_table_of_its_outcomes() {
    let new = || RepoPath::parse("new.txt").expect("a path");
    let none = Vec::new;
    let path = || vec![Fresh::Path(new())];
    let admission = || vec![Fresh::Admission(new())];
    let clean = || LeakCheck::Clean(());
    let unrun = || LeakCheck::CouldNotRun(could_not_run());
    #[rustfmt::skip]
    let table: [(LeakCheck<()>, Push, Vec<Fresh>, u8); 18] = [
        (clean(), Push::GateBranch, none(), 0),      (clean(), Push::Row, none(), 0),
        (clean(), Push::GateBranch, path(), 0),      (clean(), Push::Row, path(), 4),
        (clean(), Push::GateBranch, admission(), 4), (clean(), Push::Row, admission(), 4),
        (LeakCheck::Hits, Push::GateBranch, none(), 1),      (LeakCheck::Hits, Push::Row, none(), 1),
        (LeakCheck::Hits, Push::GateBranch, path(), 1),      (LeakCheck::Hits, Push::Row, path(), 1),
        (LeakCheck::Hits, Push::GateBranch, admission(), 1), (LeakCheck::Hits, Push::Row, admission(), 1),
        (unrun(), Push::GateBranch, none(), 3),      (unrun(), Push::Row, none(), 3),
        (unrun(), Push::GateBranch, path(), 3),      (unrun(), Push::Row, path(), 3),
        (unrun(), Push::GateBranch, admission(), 3), (unrun(), Push::Row, admission(), 3),
    ];
    assert_eq!(Error::Unrun(String::new()).status(), 3);
    for (check, push, fresh, status) in table {
        let verdict = publishable(check, push, &fresh, &[]);
        assert_eq!(verdict.status(), status, "{push:?} {fresh:?}: {verdict:?}");
        if let Publishable::Held(held) = verdict {
            assert_eq!(held, vec![new()]);
        }
    }
}

/// `--skip` is taken only on a check that ran whole and found hits: a clean tree is refused (it
/// is published instead), and a check that could not run stops the publish as such.
#[test]
fn a_skip_is_never_taken_on_a_check_that_did_not_run() {
    let skip = oid(7);
    assert!(LeakCheck::<()>::Hits.skip(&skip).is_ok());
    let clean = LeakCheck::Clean(()).skip(&skip);
    assert!(
        matches!(clean, Err(Error::Refused(Refusal::SkipPasses(_)))),
        "{clean:?}"
    );
    let unrun = LeakCheck::<()>::CouldNotRun(could_not_run()).skip(&skip);
    assert!(
        matches!(&unrun, Err(e @ Error::Unrun(_)) if e.status() == 3),
        "{unrun:?}"
    );
}

/// The edges a publication parses: an empty `<rev>` is refused, never read as master's tip, and
/// `push-gate` force-pushes only a branch under `gate/`, never core's master.
#[test]
fn a_publication_parses_its_rev_and_its_gate_branch() {
    let words = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
    let empty = Command::parse(&words(&["publish", ""]));
    assert!(
        matches!(empty, Err(Error::Refused(Refusal::EmptyRev))),
        "{empty:?}"
    );
    for refused in [
        "master",
        "refs/heads/master",
        "gate/",
        "gate/x:master",
        "gate/../master",
        "+gate/x",
        "gate/a b",
    ] {
        assert!(GateBranch::parse(refused).is_err(), "{refused}");
        let push = Command::parse(&words(&["push-gate", "c", refused, "s", "t"]));
        assert!(matches!(push, Err(Error::Usage(_))), "{refused}: {push:?}");
    }
    for taken in [
        "gate/0123456789abcdef",
        "gate/pr12-0123456789ab",
        "gate/nightly-0123",
    ] {
        assert_eq!(
            GateBranch::parse(taken)
                .map(|b| b.to_string())
                .ok()
                .as_deref(),
            Some(taken)
        );
    }
}

/// Each gate pushes a commit of its own: the stamp in its subject names the instant to the
/// nanosecond, so two pushes at different instants never make the same commit (and so never
/// share a run on the runners).
#[test]
fn each_gate_push_is_a_commit_of_its_own() {
    // Two instants in one second: a stamp without its nanoseconds would make them one commit.
    proptest!(|(second in 0u64..4_000_000_000, an in 0u32..1_000_000_000, bn in 0u32..1_000_000_000)| {
        let (a, b) = (std::time::Duration::new(second, an), std::time::Duration::new(second, bn));
        prop_assert_eq!(publication::gate_stamp(a).1 == publication::gate_stamp(b).1, an == bn);
    });
    proptest!(|(a in 0u64..4_000_000_000, an in 0u32..1_000_000_000, b in 0u64..4_000_000_000, bn in 0u32..1_000_000_000)| {
        let (a, b) = (std::time::Duration::new(a, an), std::time::Duration::new(b, bn));
        prop_assert_eq!(publication::gate_stamp(a).1 == publication::gate_stamp(b).1, a == b);
    });
}
