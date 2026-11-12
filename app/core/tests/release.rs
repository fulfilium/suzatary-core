//! The version as a reading of the project chain (`suzatary_core::release`): the last completed
//! release's `major.minor`, a function of the set of objects, monotone in the objects and in the
//! instant, and moved by releases alone; and the step from one release to the next always climbs.

use prodrome_core::event::{Actor, TodoId};
use prodrome_core::event::{Hash, mk_cancelled, mk_completed, mk_reopened, mk_spec_revised};
use prodrome_core::fpl::{self, mk_conj, mk_flat, mk_ref};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Policy;
use prodrome_core::policy::Untrusted;
use prodrome_core::term::Term;
use proptest::prelude::*;
use std::collections::BTreeMap;
use suzatary_core::release::{
    Completed, Completes, Counted, Fragment, Lot, Plan, Reading, Release, Released, Version,
};
use suzatary_record::record::TodoEvent;

const OWNER: &str = "owner";
const AGENT: &str = "triage";

fn day(n: i64) -> Datetime {
    let start = Datetime::new(2026, 9, 1, 12, 0, 0, 0).expect("a real instant");
    fpl::datetime_of(fpl::instant_of(start) + chrono::Duration::days(n)).expect("in range")
}

fn untrusted() -> Untrusted {
    Untrusted::of([Actor::new(AGENT).expect("an actor")])
}

fn todo(id: &str) -> TodoId {
    TodoId::new(id).expect("a todo id")
}

fn release(id: &str, major: u32, minor: u32) -> Release {
    Release {
        todo: todo(id),
        version: Counted { major, minor },
    }
}

/// 2.0, 2.1 and 3.0, on the chain `suzatary`, from 1.
fn plan() -> Plan {
    Plan::new(
        "suzatary",
        1,
        vec![
            release("two", 2, 0),
            release("twoone", 2, 1),
            release("three", 3, 0),
        ],
    )
    .expect("a plan")
}

const RELEASES: [&str; 3] = ["two", "twoone", "three"];
const ITEMS: [&str; 6] = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];

fn composes(items: &[&str]) -> prodrome_core::term::Term {
    let mut terms = vec![mk_flat(0.6).expect("a value")];
    terms.extend(
        items
            .iter()
            .map(|i| mk_ref((*i).to_owned()).expect("a ref")),
    );
    mk_conj(terms, -4.0).expect("a conjunction")
}

/// One object of a random history.
#[derive(Debug, Clone)]
enum Op {
    Complete(usize, bool),
    Cancel(usize, bool),
    Reopen(usize),
    Revise(usize, Vec<usize>, bool),
}

/// Todos 0..6 are items, 6..9 the releases.
fn id(n: usize) -> &'static str {
    if n < ITEMS.len() {
        ITEMS[n]
    } else {
        RELEASES[n - ITEMS.len()]
    }
}

fn event(op: &Op, at: Datetime) -> TodoEvent {
    let actor = |trusted: bool| if trusted { OWNER } else { AGENT };
    match op {
        Op::Complete(n, trusted) => mk_completed(id(*n), at, actor(*trusted), ""),
        Op::Cancel(n, trusted) => mk_cancelled(id(*n), at, actor(*trusted), ""),
        Op::Reopen(n) => mk_reopened(id(*n), at, OWNER, ""),
        Op::Revise(r, items, trusted) => mk_spec_revised(
            RELEASES[*r],
            at,
            actor(*trusted),
            composes(&items.iter().map(|i| ITEMS[*i]).collect::<Vec<_>>()),
            "",
        ),
    }
    .expect("an event")
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..9usize, prop::bool::weighted(0.8)).prop_map(|(n, t)| Op::Complete(n, t)),
        (0..9usize, prop::bool::weighted(0.8)).prop_map(|(n, t)| Op::Cancel(n, t)),
        (0..9usize).prop_map(Op::Reopen),
        (
            0..3usize,
            prop::collection::vec(0..6usize, 0..6),
            prop::bool::weighted(0.8)
        )
            .prop_map(|(r, items, t)| Op::Revise(r, items, t)),
    ]
}

/// A history: each object named by its place, stamped on a day of the month.
fn history() -> impl Strategy<Value = Vec<(Hash, TodoEvent)>> {
    prop::collection::vec((op(), 0..30i64), 0..40).prop_map(|ops| {
        ops.iter()
            .enumerate()
            .map(|(n, (op, d))| {
                (
                    Hash::new(format!("{n:064x}")).expect("a name"),
                    event(op, day(*d)),
                )
            })
            .collect()
    })
}

/// The events of `objects` that bind under the test's policy.
fn bound(objects: &[(Hash, TodoEvent)]) -> Vec<&TodoEvent> {
    let policy = untrusted();
    objects
        .iter()
        .filter(|(name, event)| policy.standing(name, event).binds())
        .map(|(_, event)| event)
        .collect()
}

/// Each release's spec in force at `at`: every revision stamped at the latest instant by
/// then, several being a conflict, so the helper is a function of the set as the fold is.
fn current(events: &[&TodoEvent], at: Datetime) -> BTreeMap<TodoId, Vec<Term>> {
    let mut specs: BTreeMap<TodoId, (Datetime, Vec<Term>)> = BTreeMap::new();
    for event in events {
        if let prodrome_core::event::TodoEvent::SpecRevised(e) = event
            && e.at <= at
        {
            let held = specs.entry(e.todo.clone()).or_insert((e.at, Vec::new()));
            if held.0 < e.at {
                *held = (e.at, Vec::new());
            }
            if held.0 == e.at && !held.1.contains(&e.spec) {
                held.1.push(e.spec.clone());
            }
        }
    }
    specs
        .into_iter()
        .map(|(todo, (_, mut candidates))| {
            candidates.sort_by_key(prodrome_core::fpl::print_term);
            (todo, candidates)
        })
        .collect()
}

fn read(objects: &[(Hash, TodoEvent)], at: Datetime) -> Reading {
    let events = bound(objects);
    Reading::of(&plan(), events.iter().copied(), at, &current(&events, at))
}

proptest! {
    /// A function of the SET of objects: the order they are read in moves nothing.
    #[test]
    fn the_reading_is_a_function_of_the_set(objects in history(), seed in any::<u64>()) {
        let mut shuffled = objects.clone();
        // A deterministic permutation from the seed.
        shuffled.sort_by_key(|(h, _)| {
            let mut x = seed ^ u64::from_str_radix(&h.as_str()[48..], 16).unwrap_or(0);
            x ^= x << 13; x ^= x >> 7; x ^= x << 17;
            x
        });
        prop_assert_eq!(read(&objects, day(40)), read(&shuffled, day(40)));
    }

    /// More objects never read a lower version: the law that keeps every tag above the last.
    #[test]
    fn the_version_is_monotone_in_the_objects(objects in history(), cut in 0..40usize) {
        let some = &objects[..cut.min(objects.len())];
        prop_assert!(read(some, day(40)).counted <= read(&objects, day(40)).counted);
    }

    /// A later instant never reads a lower version.
    #[test]
    fn the_version_is_monotone_in_time(objects in history(), a in 0..32i64, b in 0..32i64) {
        let (early, late) = (a.min(b), a.max(b));
        prop_assert!(read(&objects, day(early)).counted <= read(&objects, day(late)).counted);
    }

    /// The version is the greatest version of a completed release, `1.0` before any: a
    /// release's completion moves it, and an item's never does.
    #[test]
    fn the_version_is_the_last_completed_release(objects in history()) {
        let reading = read(&objects, day(40));
        let completed = reading
            .releases
            .iter()
            .filter(|r| r.lot == Lot::Completed)
            .map(|r| r.version)
            .max()
            .unwrap_or(Counted { major: 1, minor: 0 });
        prop_assert_eq!(reading.counted, completed);
        let releases_only: Vec<_> = objects
            .iter()
            .filter(|(_, e)| RELEASES.contains(&e.todo().as_str()))
            .cloned()
            .collect();
        prop_assert_eq!(reading.counted, read(&releases_only, day(40)).counted);
    }

    /// What the agent writes binds nothing until the owner confirms it, so it moves no version.
    #[test]
    fn untrusted_objects_move_nothing(objects in history()) {
        let trusted: Vec<_> = objects.iter().filter(|(_, e)| e.actor().as_str() != AGENT).cloned().collect();
        prop_assert_eq!(read(&objects, day(40)), read(&trusted, day(40)));
    }

    /// The next release is above the last whenever the chain allows one, and refused only when
    /// the chain reads below it.
    #[test]
    fn the_next_version_always_climbs(
        major in 0..5u32, minor in 0..20u32, patch in 0..9u32, rmajor in 0..5u32, rminor in 0..20u32,
    ) {
        let last = Version { major, minor, patch };
        let read = Counted { major: rmajor, minor: rminor };
        match last.next(read) {
            Ok(next) => {
                prop_assert!(next > last);
                prop_assert_eq!(next.counted(), read);
            }
            Err(behind) => prop_assert!(behind.read < last.counted()),
        }
    }

    /// A version prints and parses as itself.
    #[test]
    fn a_version_round_trips(major in any::<u32>(), minor in any::<u32>(), patch in any::<u32>()) {
        let v = Version { major, minor, patch };
        prop_assert_eq!(v.to_string().parse::<Version>(), Ok(v));
    }
}

/// The project's own plan: the tags end at 2.1.2, read before 2.1 completed while items moved
/// the minor. The train that completes `suzatarytwoone` reads 2.1, so it is 2.1.3, whatever its
/// items; completing 2.2 then reads 2.2.0.
#[test]
fn completing_two_one_after_two_one_two_is_two_one_three() {
    let plan = Plan::declared().expect("app/core/releases.toml reads");
    let reading =
        |objects: &[TodoEvent]| Reading::of(&plan, objects, day(60), &BTreeMap::new()).counted;
    let last = Version {
        major: 2,
        minor: 1,
        patch: 2,
    };
    let mut objects = vec![
        mk_spec_revised(
            "suzatarytwoone",
            day(1),
            OWNER,
            composes(&["alpha", "bravo"]),
            "",
        )
        .expect("an event"),
        mk_completed("suzatarytwo", day(2), OWNER, "").expect("an event"),
        mk_completed("alpha", day(3), OWNER, "").expect("an event"),
    ];
    assert_eq!(reading(&objects), Counted { major: 2, minor: 0 });
    objects.push(mk_completed("suzatarytwoone", day(4), OWNER, "").expect("an event"));
    assert_eq!(
        last.next(reading(&objects)),
        Ok(Version {
            major: 2,
            minor: 1,
            patch: 3
        })
    );
    objects.push(mk_completed("suzatarytwotwo", day(5), OWNER, "").expect("an event"));
    assert_eq!(
        last.next(reading(&objects)),
        Ok(Version {
            major: 2,
            minor: 2,
            patch: 0
        })
    );
}

/// The declaration this build carries reads, and its releases are 2.0, 2.1 and 2.2's todos.
#[test]
fn the_declared_plan_reads() {
    let plan = Plan::declared().expect("app/core/releases.toml reads");
    assert_eq!(plan.chain(), "suzatary");
    assert_eq!(
        plan.releases().first().map(|r| r.todo.as_str()),
        Some("suzatarytwo")
    );
}

/// The file a release writes reads back as itself, each todo once and each title one line.
#[test]
fn a_release_prints_and_parses_as_itself() {
    let completes = Completes::new([
        Completed {
            todo: todo("calendarlinks"),
            title: "calendar \"links\"\nand more".to_owned(),
        },
        Completed {
            todo: todo("calendarlinks"),
            title: "again".to_owned(),
        },
        Completed {
            todo: todo("portsuites"),
            title: "ports as traits".to_owned(),
        },
    ]);
    assert_eq!(completes.todos().len(), 2);
    assert_eq!(completes.todos()[0].title, "calendar \"links\"");
    let version = Version {
        major: 2,
        minor: 3,
        patch: 1,
    };
    for todos in [vec![todo("calendarlinks"), todo("portsuites")], Vec::new()] {
        let released = Released::new(version, todos);
        assert_eq!(Released::parse(&released.print_over("")), Ok(released));
    }
    // The file is public: it names ids, never a title.
    let cut = suzatary_core::release::Cut {
        version,
        completes: completes.clone(),
        folds: None,
    };
    let printed = cut.released().print_over("");
    assert!(
        printed.contains("completes = [\"calendarlinks\", \"portsuites\"]")
            && !printed.contains("calendar \"links")
            && !printed.contains("ports as traits"),
        "{printed}"
    );
    assert_eq!(
        completes.changelog("suzatary"),
        vec![
            "- Completes `todo:suzatary/calendarlinks`: calendar \"links\"".to_owned(),
            "- Completes `todo:suzatary/portsuites`: ports as traits".to_owned(),
        ]
    );
    // The fragments fold under the version, after what it completed.
    let fragments = Fragment::parse("### Fixed\n- b\n")
        .expect("a fragment")
        .append(Fragment::parse("### Added\n- a\n  more of a\n").expect("a fragment"));
    let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).expect("a day");
    assert_eq!(
        cut.changelog(
            "# Changelog\n\n## [Unreleased]\n\n## [1.0.0] - 2026-10-01\n",
            &fragments,
            day,
            "suzatary"
        )
        .expect("closes"),
        format!(
            "# Changelog\n\n## [Unreleased]\n\n## [{version}] - 2026-10-14\n\n{}\n{}\n\n### Added\n- a\n  more of a\n\n### Fixed\n- b\n\n## [1.0.0] - 2026-10-01\n",
            "- Completes `todo:suzatary/calendarlinks`: calendar \"links\"",
            "- Completes `todo:suzatary/portsuites`: ports as traits",
        )
    );
}

/// A version set by hand and never released folds into the next release: the first release is
/// what the chain reads, `major.minor.0`, whatever the hand-set number said, and its section is
/// the new release's. After it, each release is the last one's next.
#[test]
fn an_unreleased_version_folds_into_the_first_release() {
    let placeholder = Version {
        major: 1,
        minor: 10,
        patch: 0,
    };
    let read = Counted { major: 1, minor: 9 };
    let version = Version::after(None, read).expect("nothing was released before");
    assert_eq!(
        version,
        Version {
            major: 1,
            minor: 9,
            patch: 0
        }
    );
    assert_eq!(
        Version::after(Some(version), read),
        Ok(Version {
            major: 1,
            minor: 9,
            patch: 1
        })
    );
    assert!(Version::after(Some(version), Counted { major: 1, minor: 8 }).is_err());
    let cut = suzatary_core::release::Cut {
        version,
        completes: Completes::default(),
        folds: Some(placeholder),
    };
    let changelog = "## [Unreleased]\n\n## [1.10.0] - 2026-10-12\n\nWhy.\n\n### Added\n- a\n\n## [0.17.0] - 2026-09-29\n";
    let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 14).expect("a day");
    assert_eq!(
        cut.changelog(changelog, &Fragment::default(), day, "suzatary")
            .expect("closes"),
        "## [Unreleased]\n\n## [1.9.0] - 2026-10-14\n\nWhy.\n\n### Added\n- a\n\n## [0.17.0] - 2026-09-29\n"
    );
    let entries = "## [Unreleased]\n\n### Fixed\n- b\n\n## [1.10.0] - 2026-10-12\n\nWhy.\n";
    assert_eq!(
        cut.changelog(entries, &Fragment::default(), day, "suzatary")
            .expect("closes"),
        "## [Unreleased]\n\n## [1.9.0] - 2026-10-14\n\n### Fixed\n- b\n\nWhy.\n",
        "one blank line where the folded heading stood"
    );
}

/// A cut rewrites the release file's data lines and keeps its leading comment as it is, so a
/// binary whose own words for that comment differ (an older revision cutting a newer tree)
/// leaves the header the tree holds; a file with no comment gets none.
#[test]
fn a_cut_keeps_the_header_and_rewrites_the_data() {
    let old = "# An example header.\n# Its second line.\n\nversion = \"1.2.0\"\ncompletes = [\"oldtodo\"]\n";
    let released = Released::new(
        Version {
            major: 1,
            minor: 3,
            patch: 0,
        },
        [TodoId::new("newtodo").expect("an id")],
    );
    let printed = released.print_over(old);
    assert_eq!(
        printed,
        "# An example header.\n# Its second line.\n\nversion = \"1.3.0\"\ncompletes = [\"newtodo\"]\n"
    );
    assert_eq!(
        released.print_over(&printed),
        printed,
        "a second cut moves nothing"
    );
    assert_eq!(
        released.print_over("version = \"1.2.0\"\n"),
        "version = \"1.3.0\"\ncompletes = [\"newtodo\"]\n"
    );
    assert_eq!(Released::parse(&printed), Ok(released));
}
