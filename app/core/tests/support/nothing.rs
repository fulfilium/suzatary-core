use crate::agent::Agent;
use crate::drive::Drive;
use crate::notify::{Delivered, Notify, NotifyError, Push};
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
use suzatary_base::capability::Nothing;
use suzatary_base::drive_path::DriveFailure;
use suzatary_base::repo_ids::{CommitId, RepoName, RepoPath, Rev};
use suzatary_todo::render::Html;

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
            decisions: suzatary_todo::decisions::Decisions { store: &Nothing },
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

impl crate::settings::Configuration for Nothing {
    fn declared(&self) -> Result<crate::connect::Declared, crate::settings::EditError> {
        Ok(crate::connect::Declared::default())
    }
}

impl Repository for Nothing {
    fn diff_summary(&self) -> Result<String, RepositoryError> {
        Ok(String::new())
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
    fn mail_grants(&self, _account: &suzatary_mail::mail::account::MailAccount) -> MailGrants {
        MailGrants::default()
    }

    fn google_grant(&self, _: &suzatary_mail::mail::address::EmailAddress) -> crate::google::Held {
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
    ) -> Result<suzatary_held::file_read::FileRead, DriveFailure> {
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
        _: &suzatary_mail::mail::address::EmailAddress,
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
