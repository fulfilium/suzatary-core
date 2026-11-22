//! The verdict table's laws: its reading (law 1), the land's order over readings, set-asides and
//! the live half's data, its stored format (law 7), and the tool's independence of every tree it
//! judges (law 9).

use proptest::prelude::*;
use std::fmt::Write;
use std::process::Command;
use suzatary_tooling::data::{Beside, Data};
use suzatary_tooling::gate::Gate;
use suzatary_tooling::instant::Instant;
use suzatary_tooling::oid::Oid;
use suzatary_tooling::reading::Judged;
use suzatary_tooling::row::{Line, SetAside};
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

/// A row of today's form: the tree half's, or a live row written before rows named their data.
fn row() -> impl Strategy<Value = Row> {
    (outcome(), oid(), instant()).prop_map(|(outcome, commit, at)| Row {
        outcome,
        commit,
        at,
        beside: Beside::Nothing,
    })
}

/// The data of a live row whose digest is `seed` forty times, read as the table reads it.
fn data_of(seed: &str) -> Data {
    let zero = "0".repeat(40);
    let line = format!("ok {zero} 1970-01-01T00:00:00Z data:{}", seed.repeat(40));
    match Line::parse(&line).expect("a live row") {
        Line::Row(Row {
            beside: Beside::Data(data),
            ..
        }) => data,
        other => panic!("{other} names no data"),
    }
}

/// The set-aside of the row of `commit` at `red_at` and `beside`, read as the table reads it.
fn aside_of(commit: u8, red_at: u64, beside: &Beside) -> SetAside {
    let (red_at, at) = (Instant(red_at), Instant(9));
    let line = format!(
        "set-aside {commit:040x} {red_at} {beside} gateverdicts {at} the runners' cache job failed"
    );
    match Line::parse(&line).expect("a set-aside") {
        Line::SetAside(aside) => aside,
        other @ Line::Row(_) => panic!("{other} is no set-aside"),
    }
}

/// Whether `aside` names `row`, a red one.
fn names(aside: &SetAside, row: &Row) -> bool {
    row.outcome == Outcome::Red
        && row.commit == aside.commit
        && row.at == aside.red_at
        && row.beside == aside.beside
}

/// Two digests, so generated rows share a key often.
fn data() -> impl Strategy<Value = Data> {
    prop_oneof![Just("a"), Just("b")].prop_map(data_of)
}

fn beside() -> impl Strategy<Value = Beside> {
    prop_oneof![Just(Beside::Nothing), data().prop_map(Beside::Data)]
}

/// A row at any key, from few commits and instants, so set-asides name rows that exist.
fn any_row() -> impl Strategy<Value = Row> {
    (outcome(), 0u8..2, 0u64..3, beside()).prop_map(|(outcome, commit, at, beside)| Row {
        outcome,
        commit: Oid::parse(&format!("{commit:040x}")).expect("40 hexadecimal digits"),
        at: Instant(at),
        beside,
    })
}

/// A set-aside of the commit, instant and key of any row `any_row` draws, at any outcome.
fn any_aside() -> impl Strategy<Value = SetAside> {
    (0u8..2, 0u64..3, beside()).prop_map(|(commit, at, beside)| aside_of(commit, at, &beside))
}

fn any_lines() -> impl Strategy<Value = Vec<Line>> {
    prop::collection::vec(
        prop_oneof![
            3 => any_row().prop_map(Line::Row),
            1 => any_aside().prop_map(Line::SetAside),
        ],
        0..8,
    )
}

/// A file as train.sh's `record` wrote it: `printf '%s %s %s\n'`, one append a line.
fn as_the_shell_wrote(rows: &[Row]) -> String {
    rows.iter().fold(String::new(), |mut file, row| {
        let _ = writeln!(file, "{} {} {}", row.outcome.word(), row.commit, row.at);
        file
    })
}

/// A file as the tool prints its lines.
fn printed<T: std::fmt::Display>(lines: impl IntoIterator<Item = T>) -> String {
    lines.into_iter().fold(String::new(), |mut file, line| {
        let _ = writeln!(file, "{line}");
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

/// The live half's key, written apart from `Judged::of`: each file's last row that names data
/// (a file is written in order), and between files the later instant, a tie to the later file.
fn newest_key(files: &[Vec<Line>]) -> Option<Beside> {
    let mut newest: Option<&Row> = None;
    for file in files {
        let last = file
            .iter()
            .filter_map(|line| match line {
                Line::Row(row) if matches!(row.beside, Beside::Data(_)) => Some(row),
                Line::Row(_) | Line::SetAside(_) => None,
            })
            .next_back();
        if let Some(row) = last
            && newest.is_none_or(|seen| seen.at <= row.at)
        {
            newest = Some(row);
        }
    }
    newest.map(|row| row.beside.clone())
}

/// A LAND ACCOUNTS FOR EVERY RED: over generated tables of two files, every red row a land reads
/// (every tree row; every live row that names its data) is exactly one of: a row that stops the
/// land (at the key, and no set-aside names it, so the reading is red or a finding), a red
/// printed as lapsed, or a red printed as set aside, whatever its data. The live key is each
/// file's newest row by its order, and the land's answer is a function of the rows alone: no store
/// is read, so no change of the stores after the newest live row changes it. A red and a green are
/// a finding only at one data; a row of today's form answers no live reading.
#[test]
fn a_land_reads_the_live_half_at_its_newest_row() {
    proptest!(|(selective in any_lines(), full in any_lines())| {
        let files = [selective, full];
        for half in Half::BOTH {
            let judged = Judged::of(half, &files);
            if half == Half::Live {
                prop_assert_eq!(&judged.key, &newest_key(&files));
            }
            let reds = files.iter().flatten().filter_map(|line| match line {
                Line::Row(row) if row.outcome == Outcome::Red
                    && (half == Half::Tree || matches!(row.beside, Beside::Data(_))) => Some(row),
                Line::Row(_) | Line::SetAside(_) => None,
            });
            for red in reds {
                let printed_aside = judged.set_aside.iter().any(|aside| names(aside, red));
                let lapsed = judged.lapsed.contains(red);
                let stops = !printed_aside && Some(&red.beside) == judged.key.as_ref();
                if stops {
                    prop_assert!(matches!(judged.reading, Reading::Agreed(Outcome::Red) | Reading::Finding));
                }
                prop_assert_eq!(
                    usize::from(stops) + usize::from(lapsed) + usize::from(printed_aside),
                    1,
                    "{} is one of: stops the land, lapsed, set aside", red
                );
            }
        }
    });
    let (a, b) = (Beside::Data(data_of("a")), Beside::Data(data_of("b")));
    let row = |outcome, at: u64, beside: &Beside| {
        Line::Row(Row {
            outcome,
            commit: Oid::parse(&"0".repeat(40)).expect("a commit"),
            at: Instant(at),
            beside: beside.clone(),
        })
    };
    let live = |lines: Vec<Line>| Judged::of(Half::Live, &[lines]);
    let moved = live(vec![row(Outcome::Red, 1, &a), row(Outcome::Green, 2, &b)]);
    assert_eq!(
        moved.reading,
        Reading::Agreed(Outcome::Green),
        "a green after the stores moved"
    );
    assert_eq!(moved.lapsed.len(), 1, "and the red is printed");
    assert_eq!(
        live(vec![row(Outcome::Red, 1, &a), row(Outcome::Green, 2, &a)]).reading,
        Reading::Finding
    );
    assert_eq!(
        live(vec![row(Outcome::Green, 2, &a), row(Outcome::Red, 1, &b)]).reading,
        Reading::Agreed(Outcome::Red),
        "a red written after a green stops the land, whatever its instant says"
    );
    let aside = Line::SetAside(aside_of(0, 1, &a));
    let explained = live(vec![
        row(Outcome::Red, 1, &a),
        aside,
        row(Outcome::Green, 2, &b),
    ]);
    assert_eq!(explained.reading, Reading::Agreed(Outcome::Green));
    assert_eq!(
        explained.set_aside.len(),
        1,
        "a set-aside at stores moved past is printed"
    );
    assert_eq!(
        Judged::of(
            Half::Live,
            &[
                vec![row(Outcome::Red, 5, &a)],
                vec![row(Outcome::Green, 3, &b)]
            ]
        )
        .reading,
        Reading::Agreed(Outcome::Red),
        "between files, the later instant"
    );
    assert_eq!(
        live(vec![row(Outcome::Green, 1, &Beside::Nothing)]).reading,
        Reading::Unjudged,
        "a row of today's form answers no live reading"
    );
}

/// The land order with set-asides: a set-aside takes out of its key's reading the one red row it
/// names and nothing else; one of a row that is not red, of another data, or of no row changes
/// nothing; so a red nobody set aside beside a green is still a finding.
#[test]
fn the_land_order_with_set_asides() {
    proptest!(|(lines in any_lines(), aside in any_aside())| {
        for half in Half::BOTH {
            let before = Judged::of(half, std::slice::from_ref(&lines));
            let more: Vec<Line> = lines.iter().cloned().chain([Line::SetAside(aside.clone())]).collect();
            let after = Judged::of(half, &[more]);
            let named = lines.iter().any(|line| {
                matches!(line, Line::Row(row) if Some(&row.beside) == before.key.as_ref() && names(&aside, row))
            });
            if !named {
                prop_assert_eq!(after.reading, before.reading, "a set-aside of no red row of the key");
            }
        }
    });
    let row = |outcome, at: u64| Row {
        outcome,
        commit: Oid::parse(&"0".repeat(40)).expect("a commit"),
        at: Instant(at),
        beside: Beside::Nothing,
    };
    let aside = |at: u64| Line::SetAside(aside_of(0, at, &Beside::Nothing));
    let (red, again, green) = (
        Line::Row(row(Outcome::Red, 1)),
        Line::Row(row(Outcome::Red, 2)),
        Line::Row(row(Outcome::Green, 3)),
    );
    let order = |lines: &[Line]| LandOrder::of(Judged::of(Half::Tree, &[lines.to_vec()]).reading);
    assert_eq!(order(&[red.clone(), green.clone()]), LandOrder::Finding);
    assert_eq!(
        order(&[red.clone(), green.clone(), aside(1)]),
        LandOrder::Green
    );
    assert_eq!(
        order(&[red.clone(), again, green.clone(), aside(1)]),
        LandOrder::Finding,
        "a set-aside names one row, never every red of the tree"
    );
    assert_eq!(
        order(&[red.clone(), green.clone(), aside(3)]),
        LandOrder::Finding,
        "a set-aside of a green row changes nothing"
    );
    assert_eq!(
        order(&[red, aside(1)]),
        LandOrder::Unjudged,
        "nothing left to land on"
    );
}

/// Law 7: a file of rows train.sh wrote parses and prints to the same bytes, an instant prints as
/// `date -u +%FT%TZ` prints it, a row naming its data and a set-aside read and print as written,
/// and the tool's table reads back what it wrote, in the files train.sh read, appending only, and
/// reads it as `Judged::of` its lines and nothing else.
#[test]
fn law_7_the_stored_rows_read_and_print_as_train_sh_wrote_them() {
    proptest!(|(rows in prop::collection::vec(row(), 0..8))| {
        let bytes = as_the_shell_wrote(&rows);
        let parsed = Line::parse_all(&bytes).expect("the shell's rows parse");
        prop_assert_eq!(parsed, rows.iter().cloned().map(Line::Row).collect::<Vec<_>>());
        prop_assert_eq!(printed(&rows), bytes);
    });
    proptest!(|(lines in any_lines())| {
        let bytes = printed(&lines);
        prop_assert_eq!(Line::parse_all(&bytes).expect("its own print"), lines);
    });
    let (digest, commit) = ("c".repeat(40), "0".repeat(40));
    for kept in [
        format!("ok {commit} 2026-11-21T10:03:00Z data:{digest}"),
        format!(
            "set-aside {commit} 2026-11-21T10:03:00Z data:{digest} gateverdicts 2026-11-21T11:00:00Z the runners' cache job failed"
        ),
        format!(
            "set-aside {commit} 2026-11-21T10:03:00Z - gateverdicts 2026-11-21T11:00:00Z a flaky law"
        ),
    ] {
        let line = Line::parse(&kept).expect("it parses");
        assert_eq!(line.to_string(), kept);
    }
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
        "ok".to_owned(),
        "ok x y".to_owned(),
        "passed 0 2026-11-19T10:03:00Z".to_owned(),
        "ok 1234 2026-11-19T10:03:00Z".to_owned(),
        format!("ok {commit} 2026-11-19T10:03:00Z {digest}"),
        format!("ok {commit} 2026-11-19T10:03:00Z data:1234"),
        format!("set-aside {commit} 2026-11-19T10:03:00Z - Gate! 2026-11-19T10:03:00Z words"),
        format!("set-aside {commit} 2026-11-19T10:03:00Z - gateverdicts 2026-11-19T10:03:00Z"),
        format!("set-aside {commit} 2026-11-19T10:03:00Z gateverdicts 2026-11-19T10:03:00Z words"),
    ] {
        assert!(Line::parse(&refused).is_err(), "{refused}");
    }
    for refused in [
        "2026-02-30T00:00:00Z",
        "2026-11-19T24:00:00Z",
        "2026-11-19 10:03:00Z",
    ] {
        assert!(Instant::parse(refused).is_err(), "{refused}");
    }
    assert!(Line::parse_all("ok").is_err(), "a torn last line");

    let gates = scratch("law-7");
    let table = Table::at(&gates, &Gate::merge_gate());
    let scope = || prop_oneof![Just(Scope::Selective), Just(Scope::Full)];
    proptest!(ProptestConfig::with_cases(32), |(tree in oid(), rows in prop::collection::vec((any_row(), scope()), 0..6))| {
        let mut before = String::new();
        for (row, scope) in &rows {
            table.write(Half::Live, *scope, &tree, row).expect("the row is written");
            let now = std::fs::read_to_string(table.file(Half::Live, *scope, &tree)).expect("written");
            if *scope == Scope::Selective {
                prop_assert!(now.starts_with(&before), "appended only");
                before = now;
            }
        }
        let of = |scope: Scope| -> Vec<Line> {
            rows.iter().filter(|(_, s)| *s == scope).map(|(row, _)| Line::Row(row.clone())).collect()
        };
        prop_assert_eq!(
            std::fs::read_to_string(gates.join(format!("merge-gate/live-full/{tree}"))).unwrap_or_default(),
            printed(of(Scope::Full))
        );
        prop_assert_eq!(
            std::fs::read_to_string(gates.join(format!("merge-gate/live/{tree}"))).unwrap_or_default(),
            printed(of(Scope::Selective))
        );
        prop_assert_eq!(table.judged(Half::Live, Seen::Any, &tree).expect("it reads"), Judged::of(Half::Live, &[of(Scope::Selective), of(Scope::Full)]));
        prop_assert_eq!(table.judged(Half::Live, Seen::Full, &tree).expect("it reads"), Judged::of(Half::Live, &[of(Scope::Full)]));
        prop_assert_eq!(table.read(Half::Tree, Seen::Any, &tree).expect("it reads"), Reading::Unjudged);
    });
}

fn write_file(root: &std::path::Path, rel: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(path, "x").expect("a file");
}

/// The live half's data: its digest moves with an object of a store beside the instance file and
/// with nothing else there, and no store at all is the digest of no object.
#[test]
fn the_data_digest_moves_with_the_stores_objects_alone() {
    let root = scratch("data-digest");
    write_file(&root, "instance.toml");
    let instance = root.join("instance.toml");
    let digest = || Data::now(Some(&instance)).expect("the stores read");
    let none = Data::now(None).expect("no stores");
    assert_eq!(digest(), none, "no store is the digest of no object");
    write_file(&root, &format!("events/objects/{}.py", "a".repeat(64)));
    let one = digest();
    assert_ne!(one, none);
    for beside in [
        "events/lock".to_owned(),
        "local/mail.json".to_owned(),
        "events/objects/README.md".to_owned(),
        format!(".git/objects/{}.py", "b".repeat(64)),
        format!("events/{}.py", "c".repeat(64)),
    ] {
        write_file(&root, &beside);
        assert_eq!(digest(), one, "{beside} moves no digest");
    }
    write_file(&root, &format!("inbox/objects/{}.py", "d".repeat(64)));
    assert_ne!(digest(), one, "another store's object moves it");
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
