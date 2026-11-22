//! The account's mailbox as Gmail's API reports it: a read through a new grant that shows the
//! grant reads that account's mail.

use super::Endpoints;
use suzatary_mail::mail::address::EmailAddress;

/// Why an answer of the profile was not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unprofiled {
    #[error("the answer is not the Gmail API's profile")]
    NotJson,
    #[error("the grant reads the mailbox of {0}, not the account it was asked for")]
    Another(String),
}

#[must_use]
pub fn profile_url(endpoints: &Endpoints) -> String {
    format!("{}/users/me/profile", endpoints.mail())
}

/// How many messages the profile in `body` says the mailbox holds, when it is `account`'s.
pub fn read_profile(body: &[u8], account: &EmailAddress) -> Result<u64, Unprofiled> {
    let answer: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| Unprofiled::NotJson)?;
    let address = answer
        .get("emailAddress")
        .and_then(serde_json::Value::as_str)
        .ok_or(Unprofiled::NotJson)?;
    if !address.eq_ignore_ascii_case(account.as_str()) {
        return Err(Unprofiled::Another(address.to_owned()));
    }
    answer
        .get("messagesTotal")
        .and_then(serde_json::Value::as_u64)
        .ok_or(Unprofiled::NotJson)
}
