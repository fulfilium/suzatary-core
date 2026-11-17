//! What one mail sync came to, account by account.

use serde::{Deserialize, Serialize};

/// One account's sync outcome. A failure is an outcome, never a stop: one
/// expired token must not cost the other accounts their sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSync {
    /// The account's canonical address.
    pub account: crate::mail::account_email::AccountEmail,
    /// Whether it synced, and the class of its failure where it did not.
    pub outcome: crate::outcome::Outcome<super::fault::Fault>,
    /// The last line of what it said.
    pub tail: String,
}

/// What `sync` answers. The wrapper persists it at the last-sync path, where
/// `suzatary gate` (the triage rule, `mail::triage`) and `sync_mail` read it back, so a renamed field is renamed
/// for both; each account's health over time is `health`'s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    /// Every account, in the roster's order.
    pub synced: Vec<AccountSync>,
    /// The last line `notmuch new` printed.
    pub notmuch: String,
    /// Why the sync ran without its hold, `None` when it took it: a removed account's mail
    /// deleted meanwhile may have been pulled back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unheld: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sync_report_carries_the_two_fields_three_consumers_read() {
        let report = SyncReport {
            unheld: None,
            synced: vec![AccountSync {
                account: crate::mail::account_email::AccountEmail::new("her@example.test")
                    .expect("an address"),
                outcome: crate::outcome::Outcome::Failed(crate::mail::fault::Fault::Unclassified),
                tail: "connection refused".to_owned(),
            }],
            notmuch: "No new mail.".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&report).expect("serialises"),
            r#"{"synced":[{"account":"her@example.test","outcome":{"failed":{"kind":"unclassified"}},"tail":"connection refused"}],"notmuch":"No new mail."}"#
        );
    }
}
