//! The triage trigger as a rule: mail in the queue is owed a triage pass.
//!
//! The queue is hash-gated by the dedup authority (`state.json`), so the pass's outcome, a
//! disposition per message, takes each message out of the queue and discharges the rule; a pass
//! that recorded nothing leaves the same reading, owed again by the next pass. A failed account
//! is not the model's work: the health register shows it.

use crate::mail::verbs::{Summary, SyncReport};
use crate::rule::{Named, ReadingName, Rule};

/// What triage reads: the queued messages' canonical hashes, and the last sync's report as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    hashes: Vec<String>,
    report: Option<SyncReport>,
}

impl Queue {
    /// The queue's rows beside the report.
    #[must_use]
    pub fn of(rows: &[Summary], report: Option<SyncReport>) -> Self {
        Self {
            hashes: rows.iter().map(|row| row.hash.clone()).collect(),
            report,
        }
    }
}

/// Named by its messages, and by whether the report read, which is part of what P reads.
impl Named for Queue {
    fn name(&self) -> ReadingName {
        let unread = self
            .report
            .is_none()
            .then_some("the sync report is unreadable");
        ReadingName::of(self.hashes.iter().map(String::as_str).chain(unread))
    }
}

impl std::fmt::Display for Queue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !Triage.holds(self) {
            return f.write_str("nothing new");
        }
        write!(f, "{} new message(s)", self.hashes.len())?;
        if self.report.is_none() {
            f.write_str(", and the sync report is unreadable")?;
        }
        Ok(())
    }
}

/// The one mail queue this box triages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mailbox;

impl std::fmt::Display for Mailbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the mail queue")
    }
}

/// New mail, or a sync that left no report to say there is none, is owed a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triage;

impl Rule for Triage {
    type Key = Mailbox;
    type Reading = Queue;
    type Witness = ();

    fn name(&self) -> &'static str {
        "triage"
    }

    fn holds(&self, queue: &Queue) -> bool {
        !queue.hashes.is_empty() || queue.report.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::verbs::AccountSync;
    use crate::rule::Engine;

    fn report(oks: &[bool]) -> SyncReport {
        SyncReport {
            unheld: None,
            synced: oks
                .iter()
                .map(|ok| AccountSync {
                    account: crate::mail::account_email::AccountEmail::new("a@example.com")
                        .expect("an address"),
                    outcome: if *ok {
                        crate::health::Outcome::Succeeded
                    } else {
                        crate::health::Outcome::Failed(crate::connect::Failure::Unclassified)
                    },
                    tail: String::new(),
                })
                .collect(),
            notmuch: String::new(),
        }
    }

    fn rows(hashes: &[&str]) -> Vec<Summary> {
        hashes
            .iter()
            .map(|hash| Summary {
                hash: (*hash).to_owned(),
                id: format!("{hash}@example.test"),
                sender: "ada@example.test".to_owned(),
                title: "Hello".to_owned(),
                timestamp: None,
                container: "inbox".to_owned(),
                sources: 1,
            })
            .collect()
    }

    #[test]
    fn the_rule_is_quiet_only_with_no_mail_and_a_report() {
        let queue = |hashes: &[&str], report| Queue::of(&rows(hashes), report);
        assert!(!Triage.holds(&queue(&[], Some(report(&[true, true])))));
        assert_eq!(queue(&[], Some(report(&[true]))).to_string(), "nothing new");
        assert_eq!(
            queue(&["a1", "b2"], Some(report(&[true]))).to_string(),
            "2 new message(s)"
        );
        assert!(
            !Triage.holds(&queue(&[], Some(report(&[true, false])))),
            "a failed account is the health register's, not a model run's"
        );
        assert_eq!(
            queue(&[], None).to_string(),
            "0 new message(s), and the sync report is unreadable",
            "a missing report is not a quiet pass"
        );
    }

    /// A pass fires once per queue; its dispositions empty the queue, so the next pass, a fresh
    /// process with a fresh engine, fires nothing, while a pass that recorded nothing is owed
    /// again.
    #[test]
    fn dispositions_discharge_the_rule() {
        let owed = Queue::of(&rows(&["a1", "b2"]), Some(report(&[true])));
        let mut engine = Engine::new(vec![Triage]);
        assert!(engine.due(&Mailbox, &owed).is_some());
        assert!(engine.due(&Mailbox, &owed).is_none());
        assert!(Engine::new(vec![Triage]).due(&Mailbox, &owed).is_some());
        let triaged = Queue::of(&[], Some(report(&[true])));
        assert!(Engine::new(vec![Triage]).due(&Mailbox, &triaged).is_none());
    }

    /// The queue's name is its set of messages, whatever order the corpus listed them in.
    #[test]
    fn the_queues_name_is_its_set_of_messages() {
        let one = Queue::of(&rows(&["a1", "b2"]), Some(report(&[true])));
        let other = Queue::of(&rows(&["b2", "a1"]), Some(report(&[false])));
        assert_eq!(one.name(), other.name());
        assert_ne!(one.name(), Queue::of(&rows(&["a1", "b2"]), None).name());
    }
}
