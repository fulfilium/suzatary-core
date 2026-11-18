//! A generated deployment and what its stores observed, read as the overview: test support
//! for the overview's laws here and in core's integration tests, never production.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    Asking, Build, Declared, ModelJob, Observed, Overview, Registers, Storage, Store, overview,
    wire,
};
use suzatary_connect::deployment::Deployment;
use suzatary_connect::secrets::{MailGrants, SecretState};
use suzatary_connect::settings::remote::Pushed;
use suzatary_health::health::plugin::PluginName;
use suzatary_health::health::{Healths, SourceKey};
use suzatary_services::plugin::Manifest;

use suzatary_mail::mail::account::{MailAccount, MsImap, Proton, Sync};
use suzatary_mail::mail::sync_report::{AccountSync, SyncReport};

/// One generated deployment and what its stores observed, every name it may hold drawn from
/// small alphabets so tables collide and secrets repeat.
#[derive(Debug, Clone)]
pub struct World {
    pub jobs: Vec<ModelJob>,
    pub channels: Vec<suzatary_connect::connect::Channel>,
    pub accounts: Vec<MailAccount>,
    pub plugins: Vec<Manifest>,
    pub coding: bool,
    pub drive: Vec<suzatary_base::drive_path::DrivePath>,
    pub secrets: BTreeMap<String, SecretState>,
    pub grants: BTreeMap<String, MailGrants>,
    pub failing: Vec<(String, String)>,
    pub seen: BTreeMap<PluginName, Store>,
    pub asking: Vec<Asking>,
    pub pushed: Pushed,
    pub links: Healths<SourceKey>,
    pub pushes: Option<suzatary_health::health::Health>,
}

impl World {
    /// A deployment with nothing declared, its data repository pushed nowhere.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            jobs: Vec::new(),
            channels: Vec::new(),
            accounts: Vec::new(),
            plugins: Vec::new(),
            coding: false,
            drive: Vec::new(),
            secrets: BTreeMap::new(),
            grants: BTreeMap::new(),
            failing: Vec::new(),
            seen: BTreeMap::new(),
            asking: Vec::new(),
            pushed: Pushed::Nowhere,
            links: Healths::default(),
            pushes: None,
        }
    }

    #[must_use]
    pub fn deployment(&self) -> Deployment {
        let actor = |name: &str| prodrome_core::event::Actor::new(name).expect("an actor");
        let chain = suzatary_base::chains::ChainPolicy {
            name: suzatary_base::chains::ChainName::new("life").expect("a slug"),
            untrusted: prodrome_core::policy::Untrusted::of([actor("triage")]),
            roster: [actor("ada"), actor("triage")].into_iter().collect(),
            root: suzatary_connect::cast::keys::root(),
            naming: suzatary_base::chains::Naming::AsActs,
        };
        Deployment {
            viewers: BTreeSet::new(),
            actors: BTreeMap::new(),
            chains: suzatary_base::chains::Chains::of(vec![chain]).expect("one chain"),
            categories: BTreeMap::new(),
            app_url: "https://example.test".to_owned(),
            coding: self.coding(),
            upkeep: None,
            zone: suzatary_base::zone::Zone::UTC,
        }
    }

    #[must_use]
    pub fn coding(&self) -> Option<suzatary_connect::connect::Coding> {
        self.coding.then(|| suzatary_connect::connect::Coding {
            routine: "trig01fixture".parse().expect("a routine id"),
            repo: "example/repo".parse().expect("a repository"),
        })
    }

    #[must_use]
    pub fn settables(&self) -> suzatary_connect::settings::Settables {
        suzatary_connect::settings::Settables::derive(
            suzatary_connect::settings::MODEL_TOKEN,
            self.channels.iter().copied(),
            self.coding().as_ref(),
            &self.plugins,
        )
    }

    #[must_use]
    pub fn mail(&self) -> Healths<SourceKey> {
        let report = SyncReport {
            unheld: None,
            synced: self
                .failing
                .iter()
                .map(|(account, tail)| AccountSync {
                    account: suzatary_mail::mail::account_email::AccountEmail::new(account)
                        .expect("a generated address"),
                    outcome: suzatary_mail::outcome::Outcome::Failed(
                        suzatary_mail::mail::fault::Fault::Unclassified,
                    ),
                    tail: tail.clone(),
                })
                .collect(),
            notmuch: String::new(),
        };
        Healths::of_sync(
            &report,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
                .and_then(|day| day.and_hms_opt(9, 0, 0))
                .expect("an instant"),
        )
    }

    /// The overview, and every name the deployment declares.
    #[must_use]
    pub fn read(&self) -> (Overview, Vec<String>) {
        let deployment = self.deployment();
        let settables = self.settables();
        let build = Build {
            version: "0.0.0".to_owned(),
            revision: "dev".to_owned(),
            completed: Vec::new(),
        };
        let declared = Declared {
            deployment: &deployment,
            settables: &settables,
            channels: &self.channels,
            accounts: &self.accounts,
            jobs: &self.jobs,
            plugins: &self.plugins,
            drive: &self.drive,
            storage: &Storage {
                root: "/srv/data".to_owned(),
                pushed: self.pushed.clone(),
            },
            build: &build,
            reach: &suzatary_connect::connect::PLAIN,
        };
        let health = self.mail().join(&self.links);
        let observed = Observed {
            secrets: &self.secrets,
            grants: &self.grants,
            registers: Registers {
                health: &health,
                answered: &Healths::default(),
                plugins: &self.seen,
                pushes: self.pushes.as_ref(),
            },
        };
        let names = settables.names().into_iter().map(str::to_owned).collect();
        (overview(&declared, &observed), names)
    }

    #[must_use]
    pub fn wire(&self) -> (wire::Overview, Vec<String>) {
        let (overview, names) = self.read();
        let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
        (
            overview.wire(&borrowed, &self.asking, "setup".to_owned(), Vec::new()),
            names,
        )
    }
}

/// An account of address `email` and kind `kind`, out of four.
#[must_use]
pub fn account(email: &str, kind: u8) -> MailAccount {
    let sync = match kind {
        0 => Sync::Gmail,
        1 => Sync::MsImap(MsImap {
            authority: suzatary_mail::mail::account::Authority::Organizations,
            inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
            sent_folder: "Sent Items".to_owned(),
        }),
        3 => Sync::Google,
        _ => Sync::Proton(Proton {
            channel: "proton-all".to_owned(),
            smtp_user: email.to_owned(),
            send_addresses: vec![email.to_owned()],
            inbox_folders: suzatary_mail::mail::folders::Folders::inbox(),
        }),
    };
    MailAccount {
        subdir: email.replace(['@', '.', '+'], "-"),
        email: suzatary_mail::mail::account_email::AccountEmail::new(email)
            .expect("a lower-case address"),
        sync,
    }
}
