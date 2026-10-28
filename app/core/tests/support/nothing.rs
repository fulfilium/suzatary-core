use prodrome_core::event::Hash;

use super::{
    Agent, AllowRule, AnswerError, Answers, BriefingError, Briefings, Clock, Conversation, Corpus,
    CorpusError, Datetime, Delivered, Drive, DriveFailure, Html, Junk, JunkFailure, MailError,
    MailGrants, Mailbox, Manual, Manuals, Model, ModelError, Notify, NotifyError, Peer, Push,
    Query, Render, RenderError, Reply, RepoFailure, Repositories, Repository, RepositoryError,
    Secrets, Sessions, Usage, UsageError, Weather, WeatherError, Web, WebAnswer, WebFailure,
    WebRequest, Whois, WhoisError,
};
use super::{MailSync, Secretary, Server, SyncFailure, Synced};
use crate::escape::Escaped;
use crate::mail::state::Decision;
use crate::mail::verbs::{Marked, NoiseAdded, Show, Summary, SyncReport};
use crate::mail::{AttachmentMeta, MailMessage, MailRow};
use crate::record::{Envelope, Node, TodoEvent};
use crate::refusal::Refusal;
use crate::repos::{CommitId, Patch, RepoName, RepoPath, RepoRow, Rev, Visibility};
use crate::store::{Prodrome, Reads, StoreError, WriteLock};

/// Every effect, inert: reads find nothing, writes are refused, no peer has a login.
/// A test fills a record from it and swaps in the effects it is about.
#[derive(Debug, Clone, Copy, Default)]
pub struct Nothing;

impl Nothing {
    /// The agent's record, every field inert.
    #[must_use]
    pub fn agent() -> Agent<'static> {
        Agent {
            chain: &Nothing,
            corpus: &Nothing,
            mail: &Nothing,
            repository: &Nothing,
            manuals: &Nothing,
            render: &Nothing,
            clock: &Nothing,
            memory: crate::secretary::recollection::Recollection::of(Nothing::memories(), 0),
        }
    }

    /// The memory and the digest, inert: no fact, no line and no summary.
    #[must_use]
    pub fn memories() -> crate::secretary::memories::Memories<'static> {
        crate::secretary::memories::Memories {
            history: &Nothing,
            texts: &Nothing,
            cards: &Nothing,
            entropy: &Nothing,
            digests: crate::secretary::digests::Digests {
                history: &Nothing,
                texts: &Nothing,
                lines: Nothing::conversations(),
                owners: &NO_OWNERS,
                compacting: &crate::secretary::compacting::Compacting::Off,
            },
        }
    }

    /// The conversations, inert: no line and no key.
    #[must_use]
    pub fn conversations() -> crate::secretary::conversations::Conversations<'static> {
        crate::secretary::conversations::Conversations {
            lines: &Nothing,
            keys: &Nothing,
            entropy: &Nothing,
            renders: &Nothing,
            opened: &OPENED,
        }
    }

    /// The secretary's record, every field inert.
    #[must_use]
    pub fn secretary() -> Secretary<'static> {
        Secretary {
            chain: &Nothing,
            corpus: &Nothing,
            mail: &Nothing,
            repository: &Nothing,
            manuals: &Nothing,
            render: &Nothing,
            clock: &Nothing,
            model: &Nothing,
            conversations: Nothing::conversations(),
            inbox: &Nothing,
            memories: Nothing::memories(),
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
            configuration: &Nothing,
            watches: &Nothing,
        }
    }

    /// The server's record, every field inert.
    #[must_use]
    pub fn server() -> Server<'static> {
        Server {
            secretary: Nothing::secretary(),
            whois: &Nothing,
            notify: &Nothing,
            answers: &Nothing,
            devices: &Nothing,
            usage: &Nothing,
            sessions: &Nothing,
        }
    }
}

struct NoLock;
impl WriteLock for NoLock {}

fn absent(kind: &'static str, name: &str) -> MailError {
    MailError::Absent {
        kind,
        name: name.to_owned(),
    }
}

impl Reads<TodoEvent> for Nothing {
    fn objects(&self) -> Result<Vec<(Hash, Envelope)>, StoreError> {
        Ok(Vec::new())
    }
    fn nodes(&self) -> Result<Vec<Node>, StoreError> {
        Ok(Vec::new())
    }
    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        Ok(std::collections::BTreeSet::new())
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        Ok(Vec::new())
    }
    fn verify(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Prodrome<TodoEvent> for Nothing {
    fn append(&self, _event: TodoEvent) -> Result<Hash, StoreError> {
        Err(StoreError::not_appended("chain", "nothing is written"))
    }
    fn accept(&self, _objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        Err(StoreError::not_appended("chain", "nothing is written"))
    }
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        Ok(Box::new(NoLock))
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
    fn noise_add(&self, _sender: &str) -> Result<NoiseAdded, CorpusError> {
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

impl Manuals for Nothing {
    fn document(&self, _which: Manual) -> Option<String> {
        None
    }
}

/// A closed world that only parses: every text typst's parser accepts compiles to an empty
/// document. The real one is `suzatary_io::typeset`; a door's refusals of what parses and still
/// fails to compile are tested there and in `app/cli/tests`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Parses;

impl crate::capability::Typeset for Parses {
    fn closed(&self, source: &str) -> Result<String, crate::capability::Rejected> {
        crate::capability::parsed(source)
            .map(|()| "<!DOCTYPE html><html><body></body></html>".to_owned())
    }
}

impl crate::capability::Typeset for Nothing {
    fn closed(&self, source: &str) -> Result<String, crate::capability::Rejected> {
        crate::capability::parsed(source)
            .map(|()| "<!DOCTYPE html><html><body></body></html>".to_owned())
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
    fn page(&self, page: &crate::reading::Page) -> Result<Html, RenderError> {
        Ok(Html::from_render(page.as_str()))
    }
}

impl Clock for Nothing {
    fn now(&self) -> Datetime {
        Datetime::new(2026, 9, 7, 0, 0, 0, 0).expect("a real date")
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
    fn pick(&self) -> crate::capability::Pick<'_> {
        crate::capability::Pick::Turn(Box::new(self))
    }
}

/// The inbox and the conversations take what is written and keep none of it.
macro_rules! kept_nowhere {
    ($schema:ty) => {
        impl crate::store::Reads<$schema> for Nothing {
            fn objects(
                &self,
            ) -> Result<
                Vec<(Hash, prodrome_core::event::Envelope<$schema>)>,
                crate::store::StoreError,
            > {
                Ok(Vec::new())
            }
            fn excluded(
                &self,
            ) -> Result<std::collections::BTreeSet<Hash>, crate::store::StoreError> {
                Ok(std::collections::BTreeSet::new())
            }

            fn tips(&self) -> Result<Vec<Hash>, crate::store::StoreError> {
                Ok(Vec::new())
            }
            fn verify(&self) -> Vec<String> {
                Vec::new()
            }
        }

        impl crate::store::Prodrome<$schema> for Nothing {
            fn append(&self, event: $schema) -> Result<Hash, crate::store::StoreError> {
                Ok(prodrome_core::event::seal_hash(
                    &prodrome_core::event::mk_sealed(None, event),
                ))
            }
            fn accept(
                &self,
                _objects: &[(Hash, String)],
            ) -> Result<Vec<Hash>, crate::store::StoreError> {
                Ok(Vec::new())
            }
            fn locked(
                &self,
            ) -> Result<Box<dyn crate::store::WriteLock + '_>, crate::store::StoreError> {
                Ok(Box::new(NoLock))
            }
        }
    };
}

kept_nowhere!(crate::secretary::inbox_event::InboxEvent);
kept_nowhere!(crate::secretary::line_event::LineEvent);
kept_nowhere!(crate::secretary::memory_event::MemoryEvent);
kept_nowhere!(crate::secretary::digest_event::DigestEvent);
kept_nowhere!(crate::secretary::topics::reading::MembershipEvent);
kept_nowhere!(crate::secretary::topics::reading::AliasEvent);

/// No owner: nothing here believes a retraction or an erasure.
static NO_OWNERS: std::collections::BTreeSet<prodrome_core::event::Actor> =
    std::collections::BTreeSet::new();
kept_nowhere!(crate::decisions::DecisionEvent);

/// No calendar: every day asked for is empty.
impl crate::capability::Calendar for Nothing {
    fn days(
        &self,
        first: chrono::NaiveDate,
        count: u32,
    ) -> Result<Vec<crate::calendar::Day>, crate::store::StoreError> {
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
impl crate::capability::RenderCache for Nothing {
    fn forget(&self, _: &crate::secretary::answer::Renderable) -> Result<(), String> {
        Ok(())
    }
}

/// Zeros: nothing a salt or a key guards is kept here.
impl crate::capability::Entropy for Nothing {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
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
    fn secret_state(
        &self,
        _settable: &crate::settings::Settable,
    ) -> crate::capability::SecretState {
        crate::capability::SecretState::Missing
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
    fn pushable(&self, repo: &RepoName, _visibility: Visibility) -> Result<(), RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
}

impl Drive for Nothing {
    fn allowed_folders(&self) -> Vec<crate::drive::DrivePath> {
        Vec::new()
    }
    fn port(&self) -> &dyn crate::drive::Port {
        self
    }
    fn read(
        &self,
        file: &crate::drive::DrivePath,
    ) -> Result<crate::file_read::FileRead, DriveFailure> {
        Err(DriveFailure::Missing(file.to_string()))
    }
}

/// No Drive: nothing is in it, and nothing can be put there.
impl crate::drive::Port for Nothing {
    fn list(
        &self,
        folder: &crate::drive::DrivePath,
    ) -> Result<Vec<crate::drive::DriveEntry>, DriveFailure> {
        Err(DriveFailure::Missing(folder.to_string()))
    }
    fn download(&self, file: &crate::drive::DrivePath) -> Result<Vec<u8>, DriveFailure> {
        Err(DriveFailure::Missing(file.to_string()))
    }
}

/// No file was uploaded or kept, no fact's words either, and none is kept here.
impl<K, T> crate::store::Doc<K, T> for Nothing {
    fn get(&self, _key: &K) -> Result<Option<T>, StoreError> {
        Ok(None)
    }
    fn set(&self, _key: &K, _value: &T) -> Result<(), StoreError> {
        Err(StoreError::unwritable("file store", "nothing keeps files"))
    }
    fn remove(&self, _key: &K) -> Result<(), StoreError> {
        Ok(())
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

impl crate::capability::OlderMail for Nothing {
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

impl crate::capability::UploadIndex for Nothing {
    fn refresh(
        &self,
        named: &std::collections::BTreeSet<crate::attachment::AttachmentId>,
    ) -> Result<
        std::collections::BTreeMap<
            crate::attachment::AttachmentId,
            crate::secretary::upload_standing::UploadStanding,
        >,
        crate::capability::IndexError,
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
    ) -> Result<Option<crate::file_read::FileRead>, crate::capability::IndexError> {
        Ok(None)
    }
}

impl crate::health::Register<crate::source_key::SourceKey> for Nothing {
    fn healths(
        &self,
    ) -> Result<crate::health::Healths<crate::source_key::SourceKey>, crate::store::StoreError>
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

impl Whois for Nothing {
    fn peer(&self, _address: std::net::IpAddr) -> Result<Peer, WhoisError> {
        Ok(Peer::Stranger)
    }
}

impl Notify for Nothing {
    fn push(&self, _message: &Push) -> Result<Delivered, NotifyError> {
        Err(NotifyError::Unreachable("nothing is pushed".into()))
    }
}

impl Answers for Nothing {
    fn block(&self, block: &crate::secretary::answer::Renderable) -> Result<Html, AnswerError> {
        Err(AnswerError(format!("nothing renders {block:?}")))
    }
}

/// No phone is registered, and a registration is refused.
impl crate::store::Log<String, crate::devices::Device> for Nothing {
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
        _decide: &mut crate::store::Decide<'_, crate::devices::Device>,
    ) -> Result<crate::store::Edit<crate::devices::Device>, Refusal> {
        Err(StoreError::unwritable("device store", "nothing is registered").into())
    }
}

/// No watch is set, and setting one is refused.
impl crate::store::Log<crate::mail::watch::WatchId, crate::mail::watch::Line> for Nothing {
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
        _decide: &mut crate::store::Decide<'_, crate::mail::watch::Line>,
    ) -> Result<crate::store::Edit<crate::mail::watch::Line>, Refusal> {
        Err(StoreError::unwritable("watch store", "nothing is watched").into())
    }
}

impl Usage for Nothing {
    fn requests(&self) -> Result<Vec<crate::usage::Request>, UsageError> {
        Ok(Vec::new())
    }
    fn refusals(&self) -> Result<Vec<crate::usage::Refused>, UsageError> {
        Ok(Vec::new())
    }
    fn allowance(&self) -> Vec<(crate::usage::Window, crate::usage::Allowance)> {
        Vec::new()
    }
}

/// Nothing opened: [`Nothing::conversations`] holds no line to open.
static OPENED: std::sync::LazyLock<crate::secretary::opened::Opened> =
    std::sync::LazyLock::new(crate::secretary::opened::Opened::default);

impl crate::secretary::google_sign_in::Consents for Nothing {
    fn begin(
        &self,
        _: &crate::secretary::inbox::ProposalId,
        _: &crate::mail::address::EmailAddress,
        _: &crate::google::Scopes,
        _: &prodrome_core::event::Actor,
        _: crate::capability::Datetime,
    ) -> crate::secretary::responded::Performed {
        crate::secretary::responded::Performed::Tried(crate::secretary::responded::Tried::NotSent {
            reason: "nothing opens a sign-in with Google here".to_owned(),
        })
    }
}
