//! The verdict table's laws: its reading (law 1), the land's order over readings, its stored
//! format (law 7), and the tool's independence of every tree it judges (law 9).

use proptest::prelude::*;
use std::fmt::Write;
use std::process::Command;
use suzatary_tooling::gate::Gate;
use suzatary_tooling::instant::Instant;
use suzatary_tooling::oid::Oid;
use suzatary_tooling::{Half, LandOrder, Outcome, Reading, Row, Scope, Seen, Table};

/// train.sh's `verdict` before this tool, kept as the oracle of law 1: the sorted unique first
/// words of the rows read, `unknown` for none, the word itself for one, `finding` for both.
fn shell_verdict(lines: &[&str]) -> String {
    let mut seen: Vec<&str> = lines
        .iter()
        .map(|line| line.split(' ').next().unwrap_or(""))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    match seen.join(" ").as_str() {
        "" => "unknown".into(),
        word @ ("ok" | "failed") => word.into(),
        _ => "finding".into(),
    }
}

/// train.sh's `verdict_all` and `rank` before this tool, the oracle of the land's order.
fn shell_verdict_all(halves: [&str; 2]) -> String {
    let rank = |word: &str| {
        ["ok", "unknown", "failed", "finding"]
            .iter()
            .position(|w| *w == word)
    };
    halves
        .into_iter()
        .fold(
            "ok",
            |all, word| if rank(word) <= rank(all) { all } else { word },
        )
        .into()
}

const READINGS: [Reading; 4] = [
    Reading::Unjudged,
    Reading::Agreed(Outcome::Green),
    Reading::Agreed(Outcome::Red),
    Reading::Finding,
];

fn outcome() -> impl Strategy<Value = Outcome> {
    prop_oneof![Just(Outcome::Green), Just(Outcome::Red)]
}

fn oid() -> impl Strategy<Value = Oid> {
    "[0-9a-f]{40}".prop_map(|text| Oid::parse(&text).expect("40 hexadecimal digits"))
}

/// Instants up to the year 2514, past any row this table will hold.
fn instant() -> impl Strategy<Value = Instant> {
    (0u64..1 << 34).prop_map(Instant)
}

fn row() -> impl Strategy<Value = Row> {
    (outcome(), oid(), instant()).prop_map(|(outcome, commit, at)| Row {
        outcome,
        commit,
        at,
    })
}

/// A file as train.sh's `record` wrote it: `printf '%s %s %s\n'`, one append a line.
fn as_the_shell_wrote(rows: &[Row]) -> String {
    rows.iter().fold(String::new(), |mut file, row| {
        let _ = writeln!(file, "{} {} {}", row.outcome.word(), row.commit, row.at);
        file
    })
}

/// A file as the tool prints its rows.
fn printed<'a>(rows: impl IntoIterator<Item = &'a Row>) -> String {
    rows.into_iter().fold(String::new(), |mut file, row| {
        let _ = writeln!(file, "{row}");
        file
    })
}

/// A scratch directory of the test's own, under the target directory, emptied first.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Law 1: the reading of a key is the join of its rows' outcomes in the diamond; no row reads
/// unjudged, the join is a semilattice, and it gives what train.sh's `verdict` gave.
#[test]
fn law_1_a_reading_is_the_join_of_its_rows() {
    for a in READINGS {
        assert_eq!(a.join(a), a, "idempotent");
        assert_eq!(a.join(Reading::Unjudged), a, "unjudged is the bottom");
        assert_eq!(
            a.join(Reading::Finding),
            Reading::Finding,
            "finding absorbs"
        );
        for b in READINGS {
            assert_eq!(a.join(b), b.join(a), "commutative");
            for c in READINGS {
                assert_eq!(a.join(b).join(c), a.join(b.join(c)), "associative");
            }
        }
    }
    proptest!(|(rows in prop::collection::vec(row(), 0..6), more in prop::collection::vec(row(), 0..6))| {
        let both: Vec<Row> = rows.iter().chain(&more).cloned().collect();
        prop_assert_eq!(
            Reading::of_rows(&both),
            Reading::of_rows(&rows).join(Reading::of_rows(&more))
        );
        let printed = as_the_shell_wrote(&both);
        let lines: Vec<&str> = printed.lines().collect();
        prop_assert_eq!(Reading::of_rows(&both).to_string(), shell_verdict(&lines));
    });
}

/// The land's order: a total order with green least and finding greatest, and a train's verdict is
/// what train.sh's `verdict_all` gave, so only two greens land.
#[test]
fn a_trains_verdict_is_the_greatest_half_in_the_land_order() {
    let order: Vec<LandOrder> = READINGS.iter().map(|r| LandOrder::of(*r)).collect();
    assert_eq!(
        order,
        [
            LandOrder::Unjudged,
            LandOrder::Green,
            LandOrder::Red,
            LandOrder::Finding
        ]
    );
    assert!(LandOrder::Green < LandOrder::Unjudged && LandOrder::Unjudged < LandOrder::Red);
    for tree in READINGS {
        for live in READINGS {
            let verdict = LandOrder::verdict([tree, live]);
            assert_eq!(
                verdict.to_string(),
                shell_verdict_all([&tree.to_string(), &live.to_string()]),
                "{tree} beside {live}"
            );
            assert_eq!(
                verdict == Reading::Agreed(Outcome::Green),
                tree == Reading::Agreed(Outcome::Green) && live == Reading::Agreed(Outcome::Green),
                "{tree} beside {live}: green only when both are"
            );
        }
    }
}

/// Law 7: a file of rows train.sh wrote parses and prints to the same bytes, an instant prints as
/// `date -u +%FT%TZ` prints it, and the tool's table reads back what it wrote, in the files
/// train.sh read.
#[test]
fn law_7_the_stored_rows_read_and_print_as_train_sh_wrote_them() {
    proptest!(|(rows in prop::collection::vec(row(), 0..8))| {
        let bytes = as_the_shell_wrote(&rows);
        let parsed = Row::parse_all(&bytes).expect("the shell's rows parse");
        prop_assert_eq!(&parsed, &rows);
        prop_assert_eq!(printed(&parsed), bytes);
    });
    proptest!(ProptestConfig::with_cases(32), |(at in instant())| {
        let date = Command::new("date")
            .args(["-u", "-d", &format!("@{}", at.0), "+%FT%TZ"])
            .output()
            .expect("date runs");
        let printed = String::from_utf8_lossy(&date.stdout).trim().to_owned();
        prop_assert_eq!(printed, at.to_string());
        prop_assert_eq!(Instant::parse(&at.to_string()).expect("its own print"), at);
    });
    for refused in [
        "ok",
        "ok x y",
        "passed 0 2026-11-19T10:03:00Z",
        "ok 1234 2026-11-19T10:03:00Z",
    ] {
        assert!(Row::parse(refused).is_err(), "{refused}");
    }
    for refused in [
        "2026-02-30T00:00:00Z",
        "2026-11-19T24:00:00Z",
        "2026-11-19 10:03:00Z",
    ] {
        assert!(Instant::parse(refused).is_err(), "{refused}");
    }
    assert!(Row::parse_all("ok").is_err(), "a torn last line");

    let gates = scratch("law-7");
    let table = Table::at(&gates, &Gate::merge_gate());
    proptest!(ProptestConfig::with_cases(32), |(tree in oid(), rows in prop::collection::vec((row(), prop_oneof![Just(Scope::Selective), Just(Scope::Full)]), 0..6))| {
        for (row, scope) in &rows {
            table.write(Half::Live, *scope, &tree, row).expect("the row is written");
        }
        let of = |scope: Scope| rows.iter().filter(move |(_, s)| *s == scope).map(|(row, _)| row);
        prop_assert_eq!(
            std::fs::read_to_string(gates.join(format!("merge-gate/live-full/{tree}"))).unwrap_or_default(),
            printed(of(Scope::Full))
        );
        prop_assert_eq!(
            std::fs::read_to_string(gates.join(format!("merge-gate/live/{tree}"))).unwrap_or_default(),
            printed(of(Scope::Selective))
        );
        let all: Vec<Row> = rows.iter().map(|(row, _)| row.clone()).collect();
        let full_rows: Vec<Row> = of(Scope::Full).cloned().collect();
        prop_assert_eq!(table.read(Half::Live, Seen::Any, &tree).expect("it reads"), Reading::of_rows(&all));
        prop_assert_eq!(table.read(Half::Live, Seen::Full, &tree).expect("it reads"), Reading::of_rows(&full_rows));
        prop_assert_eq!(table.read(Half::Tree, Seen::Any, &tree).expect("it reads"), Reading::Unjudged);
    });
}

/// Law 9: the tool depends on no member of the workspace, of any kind, so the train never links a leak
/// matcher, or any code, from a tree it judges.
#[test]
fn law_9_the_tool_depends_on_no_workspace_member() {
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
            "--manifest-path",
        ])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).expect("cargo's JSON");
    let packages = metadata["packages"].as_array().expect("its packages");
    let members: Vec<&str> = packages.iter().filter_map(|p| p["name"].as_str()).collect();
    let tooling = packages
        .iter()
        .find(|p| p["name"] == env!("CARGO_PKG_NAME"))
        .expect("the tool is a member");
    let named: Vec<&str> = tooling["dependencies"]
        .as_array()
        .expect("its dependencies")
        .iter()
        .filter_map(|d| d["name"].as_str())
        .filter(|name| members.contains(name))
        .collect();
    assert!(named.is_empty(), "the tool links the members {named:?}");
}
