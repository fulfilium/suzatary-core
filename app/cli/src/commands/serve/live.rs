//! What a tapped card or a hand edit can change in the instance file while the server runs,
//! held under one lock that the routes and every brain read.

use std::sync::{Arc, RwLock};

use suzatary_connect::connect::Channel;
use suzatary_connect::settings::Settables;
use suzatary_core::nonempty::NonEmpty;
use suzatary_mail::mail::account::Roster;
use suzatary_services::plugin::Manifest;

use crate::instance::Instance;

/// The instance file's declarations a running server reads. A move of the file replaces them
/// whole, so no reader sees a calendar without the name its link is set under.
#[derive(Debug, Clone)]
pub struct Live {
    pub settables: Settables,
    /// `[notify] channels`, `None` for no push.
    pub channels: Option<NonEmpty<Channel>>,
    pub accounts: Roster,
    /// `[[plugins]]`, each name once.
    pub plugins: Vec<Manifest>,
}

/// The one lock over [`Live`], shared by the routes and the compactor.
pub type Shared = Arc<RwLock<Live>>;

impl Live {
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            settables: instance.settables.clone(),
            channels: instance.notify.clone(),
            accounts: instance.mail.accounts.clone(),
            plugins: instance.plugins.manifests.clone(),
        }
    }

    #[must_use]
    pub fn shared(self) -> Shared {
        Arc::new(RwLock::new(self))
    }
}
