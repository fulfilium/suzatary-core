//! The automated triage wrapper's own pulse: written by `suzatary heartbeat` at the end of each
//! pass of `scripts/hourly-triage.sh`, and read here.
//!
//! The chain is silent exactly when the wrapper cannot commit, so this answers what the chain
//! cannot: when a pass last succeeded, and where the last one failed. [`alarm`] is the decision
//! the brief appends itself, not an instruction to the model, since a model that never ran never
//! reads one.
//!
//! A heartbeat alone cannot tell healthy from failing every run (only a pass reaching the push
//! moves it), so each pass also records its verdict in `triage-outcome` ([`PassOutcome`]); only
//! a fresh heartbeat with an `ok` or absent outcome is silent. The outcome after a run of passes
//! is a fold of [`PassOutcome::then`], which always answers a [`Recorded`] line; that line is
//! printed by [`Recorded::line`] and read by [`Recorded::parse`], one type for both sides.

use std::num::NonZeroU32;

use serde_json::{Value, json};

/// Where a pass gave up, as the wrapper writes it in a `failed` line; closed, so an unknown word
/// reads as [`PassOutcome::Unparseable`] rather than a silently wrong word on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `scripts/app-bin.sh sync`, and what must hold before it runs (free disk, the auth check).
    Sync,
    /// The triage agent: its auth check, the fail-closed preflight that fences its
    /// tools, and the run itself.
    Triage,
    /// `suzatary secretary review`, reviewing the cards the pass just filed.
    Review,
    /// `suzatary secretary sweep`, the once-a-day pass over the open list.
    Sweep,
    /// `safe-push.sh`'s checks of the data (the append-only walk, `suzatary verify` of every
    /// store, the allowlist) before committing.
    Verify,
    /// `safe-push.sh`'s `git commit`/`git push`, past its checks of the data.
    Push,
    /// Recording the pass itself, after everything it did succeeded.
    Record,
}

impl std::str::FromStr for Stage {
    type Err = String;

    fn from_str(word: &str) -> Result<Self, String> {
        Self::parse(word).ok_or_else(|| {
            format!("{word:?} is not a stage: sync, triage, review, sweep, verify, push or record")
        })
    }
}

impl Stage {
    /// Parse the closed word a `failed` line holds; `None` for anything else.
    fn parse(word: &str) -> Option<Self> {
        match word {
            "sync" => Some(Self::Sync),
            "triage" => Some(Self::Triage),
            "review" => Some(Self::Review),
            "sweep" => Some(Self::Sweep),
            "verify" => Some(Self::Verify),
            "push" => Some(Self::Push),
            "record" => Some(Self::Record),
            _ => None,
        }
    }

    /// The one spelling this stage parses back from, so the wrapper and this enum cannot drift.
    fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::Triage => "triage",
            Self::Review => "review",
            Self::Sweep => "sweep",
            Self::Verify => "verify",
            Self::Push => "push",
            Self::Record => "record",
        }
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How one pass ended, as the wrapper reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// Every stage did its work.
    Ok,
    /// The first stage that failed.
    Failed(Stage),
}

/// How the last pass ended, as `triage-outcome`'s one line says it: what a pass writes, and so
/// the only thing the file is printed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// `ok <at>`: the pass reached `safe-push.sh`'s successful end.
    Ok {
        /// When, in [`stamp`]'s text, unparsed since nothing here needs the number.
        at: String,
    },
    /// `failed <at> <stage> [<n>]`: the pass gave up at `stage`, the `n`th failure running.
    Failed {
        /// When the pass gave up, in [`stamp`]'s text.
        at: String,
        /// Where it gave up.
        stage: Stage,
        /// The run of consecutive failures ending with this one; a line without it is the first.
        count: NonZeroU32,
    },
}

impl Recorded {
    /// The one line [`parse`](Self::parse) reads back as this outcome.
    #[must_use]
    pub fn line(&self) -> String {
        match self {
            Self::Ok { at } => format!("ok {at}"),
            Self::Failed { at, stage, count } => format!("failed {at} {stage} {count}"),
        }
    }

    /// Read one line, surrounding whitespace ignored; `None` for a line of neither shape.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let mut words = line.split_whitespace();
        match words.next()? {
            "ok" => match (words.next(), words.next()) {
                (Some(at), None) => Some(Self::Ok { at: at.to_owned() }),
                _ => None,
            },
            "failed" => {
                let at = words.next()?.to_owned();
                let stage = Stage::parse(words.next()?)?;
                let count = match words.next() {
                    None => NonZeroU32::MIN,
                    Some(n) => n.parse::<NonZeroU32>().ok()?,
                };
                words
                    .next()
                    .is_none()
                    .then_some(Self::Failed { at, stage, count })
            }
            _ => None,
        }
    }
}

/// The verdict on the last pass, as the file reads; "no line" and "a line that will not parse"
/// are different facts, and neither may read as healthy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassOutcome {
    /// The line a pass wrote.
    Recorded(Recorded),
    /// No file, or an empty one; unlike `Ok`, it defers to the heartbeat rather than claim success.
    Absent,
    /// A line of neither recognised shape, or a file that cannot be read; never read as healthy.
    Unparseable,
}

impl PassOutcome {
    /// The outcome once one more pass, ended as `pass` at `at`, follows this one: a failure
    /// extends a run of failures, at whatever stage, and an ok ends it.
    #[must_use]
    pub fn then(&self, pass: Pass, at: String) -> Recorded {
        match pass {
            Pass::Ok => Recorded::Ok { at },
            Pass::Failed(stage) => Recorded::Failed {
                at,
                stage,
                count: match self {
                    Self::Recorded(Recorded::Failed { count, .. }) => count.saturating_add(1),
                    Self::Recorded(Recorded::Ok { .. }) | Self::Absent | Self::Unparseable => {
                        NonZeroU32::MIN
                    }
                },
            },
        }
    }

    /// Read `triage-outcome`'s text, `None` when the file is absent.
    #[must_use]
    pub fn parse(recorded: Option<&str>) -> Self {
        match recorded.map(str::trim) {
            None | Some("") => Self::Absent,
            Some(line) => Recorded::parse(line).map_or(Self::Unparseable, Self::Recorded),
        }
    }
}

/// The instant text both files hold: naive and UTC, as `SystemClock` reads.
const FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// `now` as both files spell an instant, which [`hours_since`] reads back.
#[must_use]
pub fn stamp(now: chrono::NaiveDateTime) -> String {
    now.format(FORMAT).to_string()
}

/// Hours between `now` and a recorded heartbeat, or `None` when it will not parse.
///
/// A gap between two instants on this box is far inside the range where the `f64` cast rounds.
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn hours_since(now: chrono::NaiveDateTime, recorded: &str) -> Option<f64> {
    let at = chrono::NaiveDateTime::parse_from_str(recorded, FORMAT).ok()?;
    Some((now - at).num_minutes() as f64 / 60.0)
}

/// How long ago the wrapper last reached its push step, as the JSON a model reads: never run, an
/// age, or unparseable. Built on [`hours_since`] so it and [`alarm`] never disagree on an age.
#[must_use]
pub fn freshness(now: chrono::NaiveDateTime, recorded: Option<&str>) -> Value {
    let Some(text) = recorded else {
        return json!({
            "at": Value::Null,
            "hours_ago": Value::Null,
            "note": "the automated triage wrapper has never run to completion on this \
                     deployment yet — new, not broken.",
        });
    };
    let Some(hours) = hours_since(now, text) else {
        return json!({
            "at": text,
            "hours_ago": Value::Null,
            "note": "the recorded heartbeat did not parse as a timestamp — say so rather \
                     than inventing an age.",
        });
    };
    json!({
        "at": text,
        "hours_ago": (hours * 10.0).round() / 10.0,
    })
}

/// Whether the mechanical alarm speaks: `Some(line)` to append as a
/// `Turn::Act`, `None` to append nothing.
///
/// A failed or unreadable pass outcome speaks however fresh the heartbeat
/// is, because a pass dying at the same stage every five minutes never
/// writes one. `Ok` and `Absent` defer to the heartbeat (`Absent` so a
/// wrapper older than the outcome file reads as it did), and only a fresh
/// heartbeat is silent: missing, unparseable and stale each say something,
/// since the owner cannot tell "new" from "broken" unless told.
#[must_use]
pub fn alarm(
    now: chrono::NaiveDateTime,
    recorded: Option<&str>,
    stale_hours: f64,
    outcome: &PassOutcome,
) -> Option<String> {
    match outcome {
        PassOutcome::Unparseable => {
            return Some(
                "The automated triage wrapper's outcome file does not read as a recognised \
                 line (neither `ok <at>` nor `failed <at> <stage>`). Something wrote it wrong, \
                 or something else is writing to that path — this does not mean the last pass \
                 succeeded."
                    .to_owned(),
            );
        }
        PassOutcome::Recorded(Recorded::Failed { at, stage, count }) => {
            let age = hours_since(now, at).map_or_else(
                || format!("at {at}, which does not read as a timestamp"),
                |hours| format!("{hours:.1} hours ago"),
            );
            let streak = if count.get() > 1 {
                format!(" — {count} consecutive failures, the last at this stage")
            } else {
                String::new()
            };
            return Some(format!(
                "The automated triage wrapper's last pass FAILED at the {stage} stage, {age}\
                 {streak}. Check hourly-triage.sh's own log on the box."
            ));
        }
        PassOutcome::Recorded(Recorded::Ok { .. }) | PassOutcome::Absent => {}
    }
    let Some(text) = recorded else {
        return Some(
            "The automated triage wrapper has no recorded pass at all. Either it has never \
             completed one since this deployment was set up, or `mail.heartbeat`'s path is \
             wrong — check hourly-triage.sh's own log on the box."
                .to_owned(),
        );
    };
    let Some(hours_ago) = hours_since(now, text) else {
        return Some(format!(
            "The automated triage wrapper's heartbeat file does not read as a timestamp \
             (recorded value: {text:?}). Something wrote it wrong, or something else is \
             writing to that path — this does not mean triage is healthy."
        ));
    };
    if hours_ago > stale_hours {
        return Some(format!(
            "The automated triage wrapper has not completed a pass in {hours_ago:.1} hours \
             (last recorded at {text}). Something is likely blocking it — check \
             hourly-triage.sh's own log on the box."
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Pass, PassOutcome, Recorded, Stage, alarm, stamp};
    use chrono::NaiveDateTime;
    use proptest::prelude::*;
    use std::num::NonZeroU32;

    fn n(count: u32) -> NonZeroU32 {
        NonZeroU32::new(count).expect("a count is at least one")
    }

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("a well-formed instant")
    }

    // --- alarm: all four heartbeat arms ------------------------------------

    /// Fresh is the only silent heartbeat arm, with an outcome that defers: an absent one, so a
    /// wrapper that never writes `triage-outcome` does not page, or an ok one; and a pass exactly
    /// at the threshold is not yet stale (`>`, not `>=`).
    #[test]
    fn a_fresh_pass_with_an_outcome_that_defers_appends_nothing() {
        let ok = PassOutcome::Recorded(Recorded::Ok {
            at: "2026-09-20T07:00:00".to_owned(),
        });
        for (now, outcome) in [
            ("2026-09-20T07:05:00", &PassOutcome::Absent),
            ("2026-09-20T08:00:00", &PassOutcome::Absent),
            ("2026-09-20T07:05:00", &ok),
        ] {
            assert_eq!(
                alarm(at(now), Some("2026-09-20T07:00:00"), 1.0, outcome),
                None,
                "{outcome:?} at {now}"
            );
        }
    }

    /// Stale says the age and the last recording.
    #[test]
    fn a_stale_pass_names_the_age() {
        let line = alarm(
            at("2026-09-20T07:00:14"),
            Some("2026-09-17T07:00:14"),
            1.0,
            &PassOutcome::Absent,
        )
        .expect("past the threshold, this arm must say something");
        assert!(line.contains("72.0 hours"), "{line}");
        assert!(line.contains("2026-09-17T07:00:14"), "{line}");
    }

    /// Never run speaks here, unlike `freshness`'s note: a wiped state directory or a wrong
    /// `mail.heartbeat` path must not read as a quiet day.
    #[test]
    fn never_run_says_so_rather_than_staying_quiet() {
        let line = alarm(at("2026-09-20T00:00:00"), None, 1.0, &PassOutcome::Absent)
            .expect("absence must say something");
        assert!(line.contains("no recorded pass"), "{line}");
    }

    /// Corrupt content must not read as healthy.
    #[test]
    fn unparseable_content_says_so_rather_than_reading_as_healthy() {
        let line = alarm(
            at("2026-09-20T00:00:00"),
            Some("not a timestamp"),
            1.0,
            &PassOutcome::Absent,
        )
        .expect("a heartbeat that cannot be trusted must not be silent");
        assert!(line.contains("does not read as a timestamp"), "{line}");
        assert!(line.contains("not a timestamp"), "{line}");
    }

    // --- PassOutcome::parse ------------------------------------------------

    /// Each shape the outcome file holds reads as its outcome: nothing or a blank line is absent,
    /// a failed line with no count is the first failure, and an unknown stage, a trailing field,
    /// a zero count and a line starting with neither word are each unparseable.
    #[test]
    fn an_outcome_line_reads_as_its_shape() {
        let failed = |stage, count| {
            PassOutcome::Recorded(Recorded::Failed {
                at: "2026-09-20T07:00:00".to_owned(),
                stage,
                count: n(count),
            })
        };
        for (line, outcome) in [
            (None, PassOutcome::Absent),
            (Some("   "), PassOutcome::Absent),
            (
                Some("ok 2026-09-20T07:00:00"),
                PassOutcome::Recorded(Recorded::Ok {
                    at: "2026-09-20T07:00:00".to_owned(),
                }),
            ),
            (
                Some("failed 2026-09-20T07:00:00 verify"),
                failed(Stage::Verify, 1),
            ),
            (
                Some("failed 2026-09-20T07:00:00 push 12"),
                failed(Stage::Push, 12),
            ),
            (
                Some("failed 2026-09-20T07:00:00 wobble"),
                PassOutcome::Unparseable,
            ),
            (
                Some("failed 2026-09-20T07:00:00 push 12 extra"),
                PassOutcome::Unparseable,
            ),
            (
                Some("ok 2026-09-20T07:00:00 extra"),
                PassOutcome::Unparseable,
            ),
            (
                Some("failed 2026-09-20T07:00:00 push 0"),
                PassOutcome::Unparseable,
            ),
            (Some("garbage"), PassOutcome::Unparseable),
        ] {
            assert_eq!(PassOutcome::parse(line), outcome, "{line:?}");
        }
    }

    // --- alarm's outcome arms ----------------------------------------------

    /// Failed speaks even with a fresh heartbeat: a pass dying at the same stage every five minutes
    /// never reaches the heartbeat write.
    #[test]
    fn failed_and_fresh_speaks_naming_the_stage() {
        let line = alarm(
            at("2026-09-20T07:05:00"),
            Some("2026-09-20T07:00:00"),
            1.0,
            &PassOutcome::Recorded(Recorded::Failed {
                at: "2026-09-20T07:04:00".to_owned(),
                stage: Stage::Verify,
                count: n(1),
            }),
        )
        .expect("a failed outcome must speak even with a fresh heartbeat");
        assert!(line.contains("verify"), "{line}");
        assert!(line.contains("FAILED"), "{line}");
    }

    /// A recorded streak is named, so the owner reads "this has been failing".
    #[test]
    fn failed_with_twelve_consecutive_names_the_streak() {
        let line = alarm(
            at("2026-09-20T07:05:00"),
            Some("2026-09-17T07:00:00"),
            1.0,
            &PassOutcome::Recorded(Recorded::Failed {
                at: "2026-09-20T07:04:00".to_owned(),
                stage: Stage::Sync,
                count: n(12),
            }),
        )
        .expect("a failed outcome must speak");
        assert!(line.contains("12"), "{line}");
    }

    /// An unparseable outcome speaks, as an unparseable heartbeat does.
    #[test]
    fn unparseable_outcome_speaks() {
        let line = alarm(
            at("2026-09-20T07:05:00"),
            Some("2026-09-20T07:00:00"),
            1.0,
            &PassOutcome::Unparseable,
        )
        .expect("an outcome file that will not parse must not read as healthy");
        assert!(line.contains("outcome file"), "{line}");
    }

    // --- the line and the fold: laws ---------------------------------------

    fn a_stage() -> impl Strategy<Value = Stage> {
        prop_oneof![
            Just(Stage::Sync),
            Just(Stage::Triage),
            Just(Stage::Review),
            Just(Stage::Sweep),
            Just(Stage::Verify),
            Just(Stage::Push),
            Just(Stage::Record),
        ]
    }

    fn a_pass() -> impl Strategy<Value = Pass> {
        prop_oneof![Just(Pass::Ok), a_stage().prop_map(Pass::Failed)]
    }

    fn an_instant() -> impl Strategy<Value = String> {
        (0i64..4_000_000_000).prop_map(|seconds| {
            stamp(
                chrono::DateTime::from_timestamp(seconds, 0)
                    .expect("in range")
                    .naive_utc(),
            )
        })
    }

    proptest! {
        /// What is written reads back as itself, so the writer and the reader are one format.
        #[test]
        fn a_written_line_parses_back_as_the_outcome_it_printed(
            before in proptest::collection::vec((a_pass(), an_instant()), 0..11),
            (pass, at) in (a_pass(), an_instant()),
        ) {
            let recorded = before
                .into_iter()
                .fold(PassOutcome::Absent, |outcome, (pass, at)| {
                    PassOutcome::Recorded(outcome.then(pass, at))
                })
                .then(pass, at);
            prop_assert_eq!(
                PassOutcome::parse(Some(&recorded.line())),
                PassOutcome::Recorded(recorded)
            );
        }

        /// The fold of passes is a function of the run since the last ok: the last stage, and
        /// how many failed since.
        #[test]
        fn the_outcome_is_the_run_of_failures_since_the_last_ok(
            start in prop_oneof![
                Just(PassOutcome::Absent),
                Just(PassOutcome::Unparseable),
                an_instant().prop_map(|at| PassOutcome::Recorded(Recorded::Ok { at })),
            ],
            passes in proptest::collection::vec(a_pass(), 1..20),
        ) {
            let outcome = passes
                .iter()
                .fold(start, |outcome, pass| {
                    PassOutcome::Recorded(outcome.then(*pass, "t".to_owned()))
                });
            let failing = passes.iter().rev().take_while(|pass| **pass != Pass::Ok).count();
            match (passes.last(), outcome) {
                (Some(Pass::Ok), PassOutcome::Recorded(Recorded::Ok { .. })) => prop_assert_eq!(failing, 0),
                (Some(Pass::Failed(last)), PassOutcome::Recorded(Recorded::Failed { stage, count, .. })) => {
                    prop_assert_eq!(*last, stage);
                    prop_assert_eq!(usize::try_from(count.get()).ok(), Some(failing));
                }
                (last, outcome) => prop_assert!(false, "{last:?} folded to {outcome:?}"),
            }
        }
    }
}
