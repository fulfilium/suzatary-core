//! The inert corpus, Junk folder, older mail and watch log: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_base::refusal::Refusal;
use suzatary_mail::mail::mailbox::Query;
use suzatary_mail::mail::sync_report::SyncReport;
use suzatary_store::store::StoreError;

use crate::mail::corpus::{Corpus, CorpusError};
use crate::mail::junk_folder::{Junk, JunkFailure};
use crate::mail::state::Decision;
use crate::mail::verbs::{Marked, Show, Summary};

impl Corpus for Nothing {
    fn sync(&self) -> Result<SyncReport, CorpusError> {
        Ok(SyncReport {
            synced: Vec::new(),
            notmuch: String::new(),
            unheld: None,
        })
    }
    fn queue(&self, _limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
        Ok(Vec::new())
    }
    fn search(&self, _query: &Query, _limit: Option<usize>) -> Result<Vec<Summary>, CorpusError> {
        Ok(Vec::new())
    }
    fn show(&self, id: &str) -> Result<Show, CorpusError> {
        Ok(Show::NotFound { id: id.to_owned() })
    }
    fn mark(
        &self,
        _hash: &str,
        _decision: Decision,
        _note: Option<&str>,
        _native: Option<&str>,
    ) -> Result<Marked, CorpusError> {
        Err(CorpusError::Refused("nothing is marked".into()))
    }
    fn last_sync(&self) -> Option<SyncReport> {
        None
    }
    fn last_triage_pass(&self) -> Option<String> {
        None
    }
}

impl Junk for Nothing {
    fn junk_accounts(&self) -> Vec<String> {
        Vec::new()
    }
    fn churn(&self) -> crate::mail::junk::ChurnList {
        crate::mail::junk::ChurnList::default()
    }
    fn junk_headers(
        &self,
        account: &str,
        _since: chrono::NaiveDate,
    ) -> Result<Vec<crate::mail::junk::JunkHeader>, JunkFailure> {
        Err(JunkFailure::Unknown(account.to_owned()))
    }
    fn junk_message(
        &self,
        account: &str,
        _uid: crate::mail::junk::Uid,
    ) -> Result<crate::mail::junk::JunkMessage, JunkFailure> {
        Err(JunkFailure::Unknown(account.to_owned()))
    }
}

impl crate::mail::older_mail::OlderMail for Nothing {
    fn search_older(
        &self,
        accounts: &[String],
        _query: &crate::mail::older::OlderQuery,
        _range: crate::mail::older::Range,
    ) -> Vec<crate::mail::older::AccountOlder> {
        accounts
            .iter()
            .map(|account| crate::mail::older::AccountOlder {
                account: account.clone(),
                outcome: crate::mail::older::Outcome::NotSearched(
                    crate::mail::older::Unsearched::Failed(
                        "this box searches no mail server".to_owned(),
                    ),
                ),
            })
            .collect()
    }
}

/// No watch is set, and setting one is refused.
impl suzatary_store::store::Log<crate::mail::watch::WatchId, crate::mail::watch::Line> for Nothing {
    fn keys(&self) -> Result<Vec<crate::mail::watch::WatchId>, StoreError> {
        Ok(Vec::new())
    }
    fn lines(
        &self,
        _watch: &crate::mail::watch::WatchId,
    ) -> Result<Vec<crate::mail::watch::Line>, StoreError> {
        Ok(Vec::new())
    }
    fn start(
        &self,
        _watch: &crate::mail::watch::WatchId,
        _first: &crate::mail::watch::Line,
    ) -> Result<(), StoreError> {
        Err(StoreError::unwritable("watch store", "nothing is watched"))
    }
    fn push(
        &self,
        _watch: &crate::mail::watch::WatchId,
        _line: &crate::mail::watch::Line,
    ) -> Result<(), StoreError> {
        Err(StoreError::unwritable("watch store", "nothing is watched"))
    }
    fn edit(
        &self,
        _watch: &crate::mail::watch::WatchId,
        _decide: &mut suzatary_store::store::Decide<'_, crate::mail::watch::Line>,
    ) -> Result<suzatary_store::store::Edit<crate::mail::watch::Line>, Refusal> {
        Err(StoreError::unwritable("watch store", "nothing is watched").into())
    }
}
