//! The inert mailbox and mail sync: test support, never production.

use suzatary_base::capability::{Datetime, Nothing};
use suzatary_held::mail_error::MailError;

use crate::mail::mail_sync::{MailSync, SyncFailure, Synced};
use crate::mail::mailbox::{Mailbox, Query};
use crate::mail::{AttachmentMeta, MailMessage, MailRow};

fn absent(kind: &'static str, name: &str) -> MailError {
    MailError::Absent {
        kind,
        name: name.to_owned(),
    }
}

impl Mailbox for Nothing {
    fn accounts(&self) -> Vec<String> {
        Vec::new()
    }
    fn inbox(
        &self,
        _account: Option<&str>,
        _limit: usize,
        _before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        Ok(Vec::new())
    }
    fn search(
        &self,
        _query: &Query,
        _account: Option<&str>,
        _limit: usize,
        _before: Option<Datetime>,
    ) -> Result<Vec<MailRow>, MailError> {
        Ok(Vec::new())
    }
    fn message(&self, id: &str) -> Result<MailMessage, MailError> {
        Err(absent("message", id))
    }
    fn thread(&self, thread: &str, _limit: usize) -> Result<Vec<MailMessage>, MailError> {
        Err(absent("conversation", thread))
    }
    fn attachments(&self, _id: &str) -> Result<Vec<AttachmentMeta>, MailError> {
        Ok(Vec::new())
    }
    fn stored(&self, id: &str, _cap: u64) -> Result<crate::mail::Stored, MailError> {
        Err(absent("message", id))
    }
    fn attachment(&self, _id: &str, name: &str) -> Result<crate::mail::AttachmentFile, MailError> {
        Err(absent("attachment", name))
    }
    fn read_attachment(
        &self,
        _id: &str,
        name: &str,
    ) -> Result<suzatary_held::file_read::FileRead, MailError> {
        Err(absent("attachment", name))
    }
}

impl MailSync for Nothing {
    fn last(&self) -> Result<Option<Synced>, SyncFailure> {
        Ok(None)
    }
    fn start(&self, _since: Datetime) -> Result<Option<Synced>, SyncFailure> {
        Err(SyncFailure("this box runs no mail sync".to_owned()))
    }
}
