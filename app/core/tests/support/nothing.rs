use crate::agent::Agent;
use crate::drive::Drive;
use crate::mail::corpus::{Corpus, CorpusError};
use crate::mail::junk_folder::{Junk, JunkFailure};
use crate::mail::mail_sync::{MailSync, SyncFailure, Synced};
use crate::mail::mailbox::{Mailbox, Query};
use crate::mail::state::Decision;
use crate::mail::sync_report::SyncReport;
use crate::mail::verbs::{Marked, Show, Summary};
use crate::mail::{AttachmentMeta, MailMessage, MailRow};
use crate::mail_error::MailError;
use crate::notify::{Delivered, Notify, NotifyError, Push};
use crate::render::{Html, Render};
use crate::repos::{Patch, RepoRow, Visibility};
use crate::repositories::{RepoFailure, Repositories};
use crate::repository::{Repository, RepositoryError};
use crate::secretary::Secretary;
use crate::secretary::answers::{AnswerError, Answers};
use crate::secretary::briefings::{BriefingError, Briefings};
use crate::secretary::model::{Conversation, Model, ModelError, Reply, Sessions};
use crate::secrets::{MailGrants, Secrets};
use crate::weather::{Weather, WeatherError};
use crate::web::{AllowRule, Web, WebAnswer, WebFailure, WebRequest};
use suzatary_base::capability::RenderError;
use suzatary_base::capability::{Datetime, Nothing};
use suzatary_base::drive_path::DriveFailure;
use suzatary_base::escape::Escaped;
use suzatary_base::refusal::Refusal;
use suzatary_base::repo_ids::{CommitId, RepoName, RepoPath, Rev};
use suzatary_store::store::StoreError;

impl Agent<'static> {
    /// The agent's record, every field inert.
    #[must_use]
    pub fn nothing() -> Self {
        Agent {
            chain: &Nothing,
            corpus: &Nothing,
            mail: &Nothing,
            repository: &Nothing,
            manuals: &Nothing,
            render: &Nothing,
            clock: &Nothing,
            memory: crate::secretary::recollection::Recollection::of(
                crate::secretary::memories::Memories::nothing().reads(),
                0,
            ),
        }
    }
}

impl crate::secretary::memories::Memories<'static> {
    /// The memory and the digest, inert: no fact, no line and no summary.
    #[must_use]
    pub fn nothing() -> Self {
        crate::secretary::memories::Memories {
            history: &Nothing,
            texts: &Nothing,
            cards: &Nothing,
            entropy: &Nothing,
            digests: crate::secretary::digests::Digests {
                history: &Nothing,
                texts: &Nothing,
                lines: crate::secretary::conversations::Conversations::nothing(),
                owners: &NO_OWNERS,
                compacting: &crate::secretary::compacting::Compacting::Off,
            },
        }
    }
}

impl crate::secretary::conversations::Conversations<'static> {
    /// The conversations, inert: no line and no key.
    #[must_use]
    pub fn nothing() -> Self {
        crate::secretary::conversations::Conversations {
            lines: &Nothing,
            keys: &Nothing,
            entropy: &Nothing,
            renders: &Nothing,
            sessions: &Nothing,
            opened: &OPENED,
        }
    }
}

impl Secretary<'static> {
    /// The secretary's record, every field inert.
    #[must_use]
    pub fn nothing() -> Self {
        Secretary {
            chain: &Nothing,
            corpus: &Nothing,
            mail: &Nothing,
            repository: &Nothing,
            manuals: &Nothing,
            render: &Nothing,
            clock: &Nothing,
            model: &Nothing,
            conversations: crate::secretary::conversations::Conversations::nothing(),
            inbox: &Nothing,
            memories: crate::secretary::memories::Memories::nothing(),
            decisions: crate::decisions::Decisions { store: &Nothing },
            topics: crate::secretary::topics::stores::TopicStores {
                memberships: &Nothing,
                aliases: &Nothing,
            },
            briefings: &Nothing,
            weather: &Nothing,
            calendar: &Nothing,
            secrets: &Nothing,
            web: &Nothing,
            repositories: &Nothing,
            drive: &Nothing,
            junk: &Nothing,
            sync: &Nothing,
            health: &Nothing,
            older: &Nothing,
            uploads: &Nothing,
            upload_index: &Nothing,
            kept: &Nothing,
            pdf: &Nothing,
            tapped: &Nothing,
            configuration: &Nothing,
            watches: &Nothing,
        }
    }
}

fn absent(kind: &'static str, name: &str) -> MailError {
    MailError::Absent {
        kind,
        name: name.to_owned(),
    }
}

impl crate::settings::Configuration for Nothing {
    fn declared(&self) -> Result<crate::connect::Declared, crate::settings::EditError> {
        Ok(crate::connect::Declared::default())
    }
}

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
    ) -> Result<crate::file_read::FileRead, MailError> {
        Err(absent("attachment", name))
    }
}

impl Repository for Nothing {
    fn diff_summary(&self) -> Result<String, RepositoryError> {
        Ok(String::new())
    }
}

impl Render for Nothing {
    fn html(&self, markup: &Escaped) -> Result<Html, RenderError> {
        Ok(Html::from_render(markup.as_str()))
    }
    fn cached(&self, markup: &Escaped) -> Option<Html> {
        Some(Html::from_render(markup.as_str()))
    }
    fn warm(&self, markup: &[Escaped]) -> usize {
        markup.len()
    }
    fn page(&self, page: &crate::template::Called) -> Result<Html, RenderError> {
        Ok(Html::from_render(page.job().state.clone()))
    }
}

impl Model for Nothing {
    fn name(&self) -> String {
        "nothing".to_owned()
    }
    fn turn(
        &self,
        _conversation: &Conversation,
        _tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        Ok(Reply::default())
    }
}

impl Sessions for Nothing {
    fn pick(&self) -> crate::secretary::model::Pick<'_> {
        crate::secretary::model::Pick::Turn(Box::new(self))
    }
}

/// No owner: nothing here believes a retraction or an erasure.
static NO_OWNERS: std::collections::BTreeSet<prodrome_core::event::Actor> =
    std::collections::BTreeSet::new();

/// No calendar: every day asked for is empty.
impl crate::calendar::effect::Calendar for Nothing {
    fn days(
        &self,
        first: chrono::NaiveDate,
        count: u32,
    ) -> Result<Vec<crate::calendar::Day>, suzatary_store::store::StoreError> {
        Ok(first
            .iter_days()
            .take(count as usize)
            .map(|date| crate::calendar::Day {
                date,
                entries: Vec::new(),
            })
            .collect())
    }
}

/// Nothing is rendered, so nothing is kept to forget.
impl crate::render_cache::RenderCache for Nothing {
    fn forget(&self, _: &suzatary_base::answer::Renderable) -> Result<(), String> {
        Ok(())
    }
}

/// No session is kept, so none is forgotten.
impl crate::secretary::model_sessions::ModelSessions for Nothing {
    fn forget(&self, _: &crate::secretary::model::SessionId, _: &str) -> Result<(), String> {
        Ok(())
    }
}

impl Briefings for Nothing {
    fn all(&self) -> Result<Vec<(chrono::NaiveDate, String)>, BriefingError> {
        Ok(Vec::new())
    }
    fn read(&self, day: chrono::NaiveDate) -> Result<String, BriefingError> {
        Err(BriefingError::Absent(day.to_string()))
    }
}

impl Weather for Nothing {
    fn today(&self) -> Result<String, WeatherError> {
        Ok("clear all day, no umbrella".to_owned())
    }
}

impl Secrets for Nothing {
    fn secret_state(&self, _settable: &crate::settings::Settable) -> crate::secrets::SecretState {
        crate::secrets::SecretState::Missing
    }
    fn mail_grants(&self, _account: &crate::mail::account::MailAccount) -> MailGrants {
        MailGrants::default()
    }

    fn google_grant(&self, _: &crate::mail::address::EmailAddress) -> crate::google::Held {
        crate::google::Held::NotSignedIn
    }
}

impl Web for Nothing {
    fn allow_rules(&self) -> Vec<AllowRule> {
        Vec::new()
    }
    fn perform(&self, request: &WebRequest) -> Result<WebAnswer, WebFailure> {
        Err(WebFailure::Unreachable(request.sentence()))
    }
}

impl Repositories for Nothing {
    fn repos(&self) -> Result<Vec<RepoRow>, RepoFailure> {
        Ok(Vec::new())
    }
    fn resolve(&self, repo: &RepoName, _at: Option<&Rev>) -> Result<CommitId, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn tree(
        &self,
        repo: &RepoName,
        _at: &CommitId,
        _dir: &RepoPath,
    ) -> Result<Vec<String>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn blob(
        &self,
        repo: &RepoName,
        _at: &CommitId,
        _path: &RepoPath,
    ) -> Result<Vec<u8>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn grep(
        &self,
        repo: &RepoName,
        _at: &CommitId,
        _needle: &str,
    ) -> Result<Vec<String>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn applies(
        &self,
        repo: &RepoName,
        _base: &CommitId,
        _patch: &Patch,
    ) -> Result<(), RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn merged(
        &self,
        repo: &RepoName,
        _patch: &Patch,
    ) -> Result<Option<crate::repositories::InBranch>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn pushable(&self, repo: &RepoName, _visibility: Visibility) -> Result<(), RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
}

impl Drive for Nothing {
    fn allowed_folders(&self) -> Vec<suzatary_base::drive_path::DrivePath> {
        Vec::new()
    }
    fn port(&self) -> &dyn crate::drive::Port {
        self
    }
    fn read(
        &self,
        file: &suzatary_base::drive_path::DrivePath,
    ) -> Result<crate::file_read::FileRead, DriveFailure> {
        Err(DriveFailure::Missing(file.to_string()))
    }
}

/// No Drive: nothing is in it, and nothing can be put there.
impl crate::drive::Port for Nothing {
    fn list(
        &self,
        folder: &suzatary_base::drive_path::DrivePath,
    ) -> Result<Vec<crate::drive::DriveEntry>, DriveFailure> {
        Err(DriveFailure::Missing(folder.to_string()))
    }
    fn download(
        &self,
        file: &suzatary_base::drive_path::DrivePath,
    ) -> Result<Vec<u8>, DriveFailure> {
        Err(DriveFailure::Missing(file.to_string()))
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

impl crate::upload_index::UploadIndex for Nothing {
    fn refresh(
        &self,
        named: &std::collections::BTreeSet<crate::attachment::AttachmentId>,
    ) -> Result<
        std::collections::BTreeMap<
            crate::attachment::AttachmentId,
            crate::secretary::upload_standing::UploadStanding,
        >,
        crate::upload_index::IndexError,
    > {
        Ok(named
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    crate::secretary::upload_standing::UploadStanding::Erased,
                )
            })
            .collect())
    }
    fn open(
        &self,
        _id: &crate::attachment::AttachmentId,
    ) -> Result<Option<crate::file_read::FileRead>, crate::upload_index::IndexError> {
        Ok(None)
    }
}

impl crate::health::Register<crate::health::SourceKey> for Nothing {
    fn healths(
        &self,
    ) -> Result<crate::health::Healths<crate::health::SourceKey>, suzatary_store::store::StoreError>
    {
        Ok(crate::health::Healths::default())
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

impl Notify for Nothing {
    fn push(&self, _message: &Push) -> Result<Delivered, NotifyError> {
        Err(NotifyError::Unreachable("nothing is pushed".into()))
    }
}

impl Answers for Nothing {
    fn block(&self, block: &suzatary_base::answer::Renderable) -> Result<Html, AnswerError> {
        Err(AnswerError(format!("nothing renders {block:?}")))
    }
}

/// No phone is registered, and a registration is refused.
impl suzatary_store::store::Log<String, crate::devices::Device> for Nothing {
    fn keys(&self) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }
    fn lines(&self, _actor: &String) -> Result<Vec<crate::devices::Device>, StoreError> {
        Ok(Vec::new())
    }
    fn start(&self, _actor: &String, _first: &crate::devices::Device) -> Result<(), StoreError> {
        Err(StoreError::unwritable(
            "device store",
            "nothing is registered",
        ))
    }
    fn push(&self, _actor: &String, _line: &crate::devices::Device) -> Result<(), StoreError> {
        Err(StoreError::unwritable(
            "device store",
            "nothing is registered",
        ))
    }
    fn edit(
        &self,
        _actor: &String,
        _decide: &mut suzatary_store::store::Decide<'_, crate::devices::Device>,
    ) -> Result<suzatary_store::store::Edit<crate::devices::Device>, Refusal> {
        Err(StoreError::unwritable("device store", "nothing is registered").into())
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

/// Nothing opened: [`Nothing::conversations`] holds no line to open.
static OPENED: std::sync::LazyLock<crate::secretary::opened::Opened> =
    std::sync::LazyLock::new(crate::secretary::opened::Opened::default);

impl crate::secretary::google_sign_in::Consents for Nothing {
    fn redirect(&self) -> Option<&crate::google::RedirectUri> {
        None
    }

    fn begin(
        &self,
        _: &crate::secretary::inbox::ProposalId,
        _: &crate::mail::address::EmailAddress,
        _: &crate::google::Scopes,
        _: &prodrome_core::event::Actor,
        _: suzatary_base::capability::Datetime,
        _: crate::secretary::google_sign_in::Then,
    ) -> crate::secretary::responded::Performed {
        crate::secretary::responded::Performed::Tried(crate::secretary::responded::Tried::NotSent {
            reason: "nothing opens a sign-in with Google here".to_owned(),
        })
    }
}

impl crate::pdf_text::PdfText for Nothing {
    fn layer(&self, _: &[u8]) -> Result<crate::file_read::Layer, String> {
        Err("this test reads no PDF".to_owned())
    }
}
