//! The timer's own mail sync, started on demand: the systemd unit the timer starts, and the
//! report its sync step leaves, read back as an interpreter of [`MailSync`].
//!
//! This is the one door to a sync outside the timer. `suzatary mail idle` and the secretary's
//! `sync_mail` both start the same unit, which systemd never runs twice at once, so there is one
//! pipeline however many things ask for it, and nothing reaches a server the timer does not.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use suzatary_core::capability::{Datetime, MailSync, SyncFailure, Synced};
use suzatary_core::mail::verbs::SyncReport;

use crate::present::Present as _;
use crate::run::{Cmd, Ran, RunError, run};

/// The systemd user unit the timer starts: a sync, then triage when there is something new.
pub const TRIAGE_UNIT: &str = "suzatary-triage.service";

/// How long `start` waits for the sync step's report; the pass's triage, if any, carries on.
const WAIT: Duration = Duration::from_secs(150);

/// How often the report is looked at while waiting.
const POLL: Duration = Duration::from_secs(2);

/// The unit and the report its sync step writes.
#[derive(Debug, Clone)]
pub struct Timer {
    program: PathBuf,
    report: PathBuf,
    wait: Duration,
}

impl Timer {
    /// The triage unit, whose sync writes `report` (`Paths::last_sync`, the state directory's
    /// `last-sync.json`).
    #[must_use]
    pub fn new(report: impl Into<PathBuf>) -> Self {
        Self {
            program: PathBuf::from("systemctl"),
            report: report.into(),
            wait: WAIT,
        }
    }

    /// Start the unit and wait for its whole pass, as `mail idle`'s debounce does, so wakes
    /// that arrive during a pass start another after it.
    ///
    /// # Errors
    /// The `systemctl` that could not be run.
    pub fn run_now(&self) -> Result<Ran, RunError> {
        run(&self.systemctl(&[]))
    }

    fn systemctl(&self, flags: &[&str]) -> Cmd {
        Cmd::new(&self.program)
            .arg("--user")
            .arg("start")
            .args(flags)
            .arg(TRIAGE_UNIT)
            // The user manager's bus, when the session names it rather than the runtime dir.
            .pass(&["DBUS_SESSION_BUS_ADDRESS"])
    }
}

impl MailSync for Timer {
    fn last(&self) -> Result<Option<Synced>, SyncFailure> {
        let unread = |failed: std::io::Error| {
            SyncFailure(format!("reading {}: {failed}", self.report.display()))
        };
        let Some((report, written)) = read_report(&self.report).map_err(unread)? else {
            return Ok(None);
        };
        Ok(instant(written).map(|at| Synced { at, report }))
    }

    fn start(&self, since: Datetime) -> Result<Option<Synced>, SyncFailure> {
        let ran = run(&self.systemctl(&["--no-block"]))
            .map_err(|failed| SyncFailure(failed.to_string()))?;
        if !ran.status.success() {
            return Err(SyncFailure(format!(
                "systemctl could not start {TRIAGE_UNIT}: {}",
                String::from_utf8_lossy(&ran.stderr).trim()
            )));
        }
        let until = Instant::now() + self.wait;
        loop {
            if let Some(synced) = self.last()?.filter(|synced| synced.at > since) {
                return Ok(Some(synced));
            }
            if Instant::now() >= until {
                return Ok(None);
            }
            std::thread::sleep(POLL.min(self.wait));
        }
    }
}

/// The sync report at `path` and when it was written: `None` before the first sync, and while
/// one is being written, since a half-written report reads as none yet. A report that is there
/// and cannot be read is an error.
///
/// # Errors
/// Any failure to read the file but its absence.
pub fn read_report(path: &Path) -> std::io::Result<Option<(SyncReport, SystemTime)>> {
    let Some(text) = std::fs::read_to_string(path).present()? else {
        return Ok(None);
    };
    let Ok(report) = serde_json::from_str(&text) else {
        return Ok(None);
    };
    let Some(written) = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .present()?
    else {
        return Ok(None);
    };
    Ok(Some((report, written)))
}

/// A file's time as the box's clock spells an instant: naive and UTC, as every `at` is.
fn instant(at: SystemTime) -> Option<Datetime> {
    let utc: chrono::DateTime<chrono::Utc> = at.into();
    prodrome_core::fpl::datetime_of(utc.naive_utc()).ok()
}

#[cfg(test)]
mod tests {
    use suzatary_core::capability::Clock as _;
    use suzatary_core::mail::verbs::AccountSync;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("suzatary-mail-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    fn report(tail: &str) -> String {
        serde_json::to_string_pretty(&SyncReport {
            unheld: None,
            synced: vec![AccountSync {
                account: suzatary_core::mail::account_email::AccountEmail::new("ada@example.test")
                    .expect("an address"),
                outcome: suzatary_core::health::Outcome::Succeeded,
                tail: tail.to_owned(),
            }],
            notmuch: "No new mail.".to_owned(),
        })
        .expect("a report serialises")
    }

    fn timer(dir: &std::path::Path, program: &str) -> Timer {
        Timer {
            program: PathBuf::from(program),
            report: dir.join("last-sync.json"),
            wait: Duration::from_secs(5),
        }
    }

    #[test]
    fn no_report_and_a_half_written_one_read_as_none() {
        let dir = scratch("none");
        let timer = timer(&dir, "true");
        assert_eq!(timer.last().expect("read"), None);
        std::fs::write(dir.join("last-sync.json"), "{\"synced\": [").expect("written");
        assert_eq!(timer.last().expect("read"), None);
        std::fs::write(dir.join("last-sync.json"), report("done")).expect("written");
        let read = timer.last().expect("read").expect("a report");
        assert_eq!(read.report.synced[0].tail, "done");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A report that is there and cannot be read (a directory where the file belongs) is a
    /// failure, never "no sync yet".
    #[test]
    fn an_unreadable_report_is_a_failure() {
        let dir = scratch("unreadable");
        std::fs::create_dir(dir.join("last-sync.json")).expect("a directory");
        assert!(timer(&dir, "true").last().is_err());
        assert!(read_report(&dir.join("last-sync.json")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_start_answers_the_first_report_written_after_it() {
        let dir = scratch("start");
        let timer = timer(&dir, "true");
        std::fs::write(dir.join("last-sync.json"), report("before")).expect("written");
        let since = crate::clock::SystemClock.now();
        let path = dir.join("last-sync.json");
        let writer = std::thread::spawn(move || {
            // Past the clock's one-second grain, so the new report's time is after `since`.
            std::thread::sleep(Duration::from_millis(1100));
            std::fs::write(path, report("after")).expect("written");
        });
        let synced = timer.start(since).expect("started").expect("a report came");
        writer.join().expect("the writer finished");
        assert_eq!(synced.report.synced[0].tail, "after");
        assert!(synced.at > since);
        let _ = std::fs::remove_dir_all(dir);
    }
}
