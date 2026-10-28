//! The triage pass's pulse on disk, as two [`Doc`]s: the heartbeat, when a pass last ended
//! well, at the instance's `mail.heartbeat`, and the outcome of the last pass beside it. This is
//! their one reader and one writer, so the path and the line are spelled once.
//!
//! [`Doc`]: suzatary_core::store::Doc

use std::path::Path;

use suzatary_core::secretary::heartbeat::{Pass, PassOutcome, Recorded, stamp};
use suzatary_core::store::{Doc as _, Gets as _, StoreError};

use crate::docs::{DocCodec, Docs};

/// The outcome's file name, beside the heartbeat.
pub const OUTCOME: &str = "triage-outcome";

/// The two files.
#[derive(Debug, Clone)]
pub struct Pulse {
    heartbeat: Docs<(), String>,
    outcome: Docs<(), Recorded>,
}

impl Pulse {
    /// The heartbeat at `heartbeat`, and the outcome beside it.
    #[must_use]
    pub fn at(heartbeat: &Path) -> Self {
        let beat = heartbeat.to_owned();
        let outcome = heartbeat.with_file_name(OUTCOME);
        Self {
            heartbeat: Docs::new("triage heartbeat", move |()| beat.clone(), &LINE),
            outcome: Docs::new("triage outcome", move |()| outcome.clone(), &OUTCOME_LINE),
        }
    }

    /// When a pass last ended well; `None` before the first, or when the file cannot be read.
    #[must_use]
    pub fn last_pass(&self) -> Option<String> {
        self.heartbeat.get(&()).ok().flatten()
    }

    /// How the last pass ended: absent before the first, and unparseable for a line of
    /// neither shape or a file that cannot be read, which never reads as healthy.
    #[must_use]
    pub fn last_outcome(&self) -> PassOutcome {
        match self.outcome.get(&()) {
            Ok(Some(recorded)) => PassOutcome::Recorded(recorded),
            Ok(None) => PassOutcome::Absent,
            Err(_) => PassOutcome::Unparseable,
        }
    }

    /// Record one more pass, ended as `pass` at `now`: the outcome follows the last one, and an
    /// ok moves the heartbeat too.
    ///
    /// An outcome file that does not read starts the run of failures again, as an absent one
    /// does, so one bad write cannot stop every later pass from being recorded.
    ///
    /// # Errors
    /// A file that cannot be written; a heartbeat already moved stays moved.
    pub fn record(&self, pass: Pass, now: chrono::NaiveDateTime) -> Result<Recorded, StoreError> {
        let at = stamp(now);
        let outcome = self.last_outcome().then(pass, at.clone());
        if pass == Pass::Ok {
            self.heartbeat.set(&(), &at)?;
        }
        self.outcome.set(&(), &outcome)?;
        Ok(outcome)
    }
}

/// The outcome's one line, [`Recorded::line`] and [`Recorded::parse`]; an empty file reads as
/// absent, and a line of neither shape is a failed read.
const OUTCOME_LINE: DocCodec<(), Recorded> = DocCodec {
    print: |(), recorded| Ok(format!("{}\n", recorded.line()).into_bytes()),
    parse: |(), bytes| {
        let text = String::from_utf8(bytes).map_err(|failed| failed.to_string())?;
        let line = text.trim();
        if line.is_empty() {
            return Ok(None);
        }
        Recorded::parse(line)
            .map(Some)
            .ok_or_else(|| format!("{line:?} is neither `ok <at>` nor `failed <at> <stage>`"))
    },
};

/// One line of text, trimmed when read; an empty file reads as absent.
const LINE: DocCodec<(), String> = DocCodec {
    print: |(), line| Ok(format!("{line}\n").into_bytes()),
    parse: |(), bytes| {
        let text = String::from_utf8(bytes).map_err(|failed| failed.to_string())?;
        let line = text.trim();
        Ok((!line.is_empty()).then(|| line.to_owned()))
    },
};

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use suzatary_core::secretary::heartbeat::Stage;

    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("an instant")
    }

    fn pulse(dir: &tempfile::TempDir) -> Pulse {
        Pulse::at(&dir.path().join("state").join("triage-heartbeat"))
    }

    /// What `record` writes is what the brief reads back, from the heartbeat's directory: a
    /// failure leaves the heartbeat where the last good pass put it, and counts the run.
    #[test]
    fn recorded_passes_read_back_beside_the_heartbeat() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let pulse = pulse(&dir);
        pulse
            .record(Pass::Ok, at("2026-10-07T08:00:00"))
            .expect("records");
        assert_eq!(pulse.last_pass().as_deref(), Some("2026-10-07T08:00:00"));
        assert_eq!(
            pulse.last_outcome(),
            PassOutcome::Recorded(Recorded::Ok {
                at: "2026-10-07T08:00:00".to_owned()
            })
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("state").join(OUTCOME)).expect("reads"),
            "ok 2026-10-07T08:00:00\n"
        );

        pulse
            .record(Pass::Failed(Stage::Review), at("2026-10-07T08:05:00"))
            .expect("records");
        let second = pulse
            .record(Pass::Failed(Stage::Sweep), at("2026-10-07T08:10:00"))
            .expect("records");
        assert_eq!(
            second,
            Recorded::Failed {
                at: "2026-10-07T08:10:00".to_owned(),
                stage: Stage::Sweep,
                count: NonZeroU32::new(2).expect("two"),
            }
        );
        assert_eq!(pulse.last_pass().as_deref(), Some("2026-10-07T08:00:00"));
        assert_eq!(
            pulse.last_outcome(),
            PassOutcome::Recorded(second),
            "the file holds the outcome record answered"
        );
        assert!(
            dir.path().join("state").join(OUTCOME).is_file(),
            "the outcome is the heartbeat's sibling"
        );
    }

    /// A line the wrapper wrote before the verb existed, with no count, starts the run at one.
    #[test]
    fn a_line_without_a_count_is_followed_by_the_second_failure() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(
            dir.path().join(OUTCOME),
            "failed 2026-10-07T08:00:00 verify\n",
        )
        .expect("writes");
        let next = Pulse::at(&dir.path().join("triage-heartbeat"))
            .record(Pass::Failed(Stage::Verify), at("2026-10-07T08:05:00"))
            .expect("records");
        assert_eq!(next.line(), "failed 2026-10-07T08:05:00 verify 2");
    }

    /// A line of neither shape reads as unparseable, never as absent, and the next pass writes
    /// over it, starting the run at one.
    #[test]
    fn a_garbled_line_is_unparseable_and_the_next_pass_replaces_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::fs::write(dir.path().join(OUTCOME), "half a line\n").expect("writes");
        let pulse = Pulse::at(&dir.path().join("triage-heartbeat"));
        assert_eq!(pulse.last_outcome(), PassOutcome::Unparseable);
        let next = pulse
            .record(Pass::Failed(Stage::Push), at("2026-10-07T08:05:00"))
            .expect("records over it");
        assert_eq!(next.line(), "failed 2026-10-07T08:05:00 push 1");
        assert_eq!(pulse.last_outcome(), PassOutcome::Recorded(next));
    }
}
