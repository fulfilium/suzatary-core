use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `GET /api/mail/watches`, and what a cancel answers: soonest to expire first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WatchesPayload {
    pub watches: Vec<MailWatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailWatch {
    pub id: String,
    pub on: WatchOn,
    /// Lower-cased.
    pub text: String,
    /// ISO 8601.
    pub set: String,
    /// ISO 8601.
    pub until: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum WatchOn {
    Sender,
    Subject,
}
