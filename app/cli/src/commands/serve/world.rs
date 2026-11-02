//! The interpreters `suzatary serve` holds, and the one place they become records.

use suzatary_core::capability::{Server, Usage, UsageError};
use suzatary_io::devices::Devices;
use suzatary_io::notify::Configured;

use super::interpreters::{Brain, Ledger, Peers, transcripts};
use crate::instance::Instance;
use crate::wiring::{Opened, Stores};

/// [`Opened`], once for every chain, and what only a server holds: who each peer is, push, the
/// devices and the usage page. A second chain is a second [`Stores`].
pub struct World {
    opened: Opened,
    peers: Peers,
    push: Configured<Devices>,
    devices: Devices,
    brain: Brain,
    usage: Metered,
    /// Each chain's stores, in the instance file's order.
    pub chains: Vec<Stores>,
    /// What the box signs with: each chain's writer signs with the root, and a tapped device
    /// card registers keys with it; missing keys leave every write refusing and every read as
    /// it was.
    pub signers: suzatary_core::signatures::Signers,
}

impl World {
    /// Choose an interpreter for every effect, out of what the instance file says.
    #[must_use]
    pub fn assemble(
        instance: &Instance,
        signers: suzatary_core::signatures::Signers,
        peers: Peers,
        brain: Brain,
        ledger: Ledger,
    ) -> Self {
        let devices = Devices::open(instance.paths.devices.clone());
        let transcripts = match ledger {
            Ledger::Box => transcripts(instance),
            Ledger::Nowhere(root) => {
                suzatary_io::usage::Transcripts::new(root, &instance.paths.home, instance.code())
            }
        };
        Self {
            opened: Opened::of(
                instance,
                "the settings page: checking whether a declared secret resolves",
            ),
            peers,
            push: crate::commands::notify::push(
                instance.notify.as_ref(),
                instance.secrets(),
                "the web app's watcher",
                &instance.deployment.app_url,
                devices.clone(),
            ),
            devices,
            brain,
            usage: Metered(suzatary_io::usage::Allowance::new(
                transcripts,
                instance.secrets(),
                "the usage page: reading the allowance endpoint",
                &instance.paths.usage_cache,
            )),
            chains: instance
                .chains
                .iter()
                .map(|chain| Stores::of(chain, &signers))
                .collect(),
            signers,
        }
    }

    /// Both topic stores begun, so a tab's first write has a prodrome to rest on.
    ///
    /// # Errors
    /// Either store unreadable or not begun.
    pub fn begin_topics(&self) -> Result<(), suzatary_core::store::StoreError> {
        self.opened.topics.begin()
    }

    /// The stores a tapped act and the coding poll write through: this world's own handles.
    #[must_use]
    pub fn acting_stores(&'static self) -> crate::commands::secretary::ActingStores {
        crate::commands::secretary::ActingStores {
            conversations: &self.opened.remembering.conversations,
            inbox: &self.opened.remembering.inbox,
        }
    }

    /// The server's record over one chain's stores: the secretary's, and what only a server
    /// holds beside it.
    #[must_use]
    pub fn server<'a>(&'a self, stores: &'a Stores) -> Server<'a> {
        Server {
            secretary: self.opened.secretary(stores, &self.brain),
            whois: &self.peers,
            notify: &self.push,
            answers: &*self.opened.answers,
            devices: &self.devices,
            usage: &self.usage,
            sessions: &self.brain,
        }
    }
}

/// The usage reader, logging when the allowance endpoint starts or stops throttling.
pub struct Metered(suzatary_io::usage::Allowance);

impl Usage for Metered {
    fn requests(&self) -> Result<Vec<suzatary_core::usage::Request>, UsageError> {
        self.0.requests()
    }

    fn refusals(&self) -> Result<Vec<suzatary_core::usage::Refused>, UsageError> {
        self.0.refusals()
    }

    // Logged here, once per state change, because `suzatary-io` never calls `tracing`.
    fn allowance(
        &self,
    ) -> Vec<(
        suzatary_core::usage::Window,
        suzatary_core::usage::Allowance,
    )> {
        let (allowance, transition) = self.0.read_allowance();
        match transition {
            Some(suzatary_io::usage::Transition::StartedThrottling {
                retry_after_seconds,
            }) => {
                tracing::info!(
                    retry_after_seconds,
                    "serve: the allowance endpoint is rate-limiting this account"
                );
            }
            Some(suzatary_io::usage::Transition::StoppedThrottling) => {
                tracing::info!(
                    "serve: the allowance endpoint answered again after being rate-limited"
                );
            }
            None => {}
        }
        allowance
    }

    fn horizon(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.0.horizon()
    }

    fn warm_transcripts(&self) -> Result<usize, UsageError> {
        self.0.warm()
    }

    fn scope(&self) -> Result<suzatary_core::usage::Scope, UsageError> {
        self.0.scope()
    }
}
