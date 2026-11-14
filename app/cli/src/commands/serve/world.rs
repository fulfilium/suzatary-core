//! The interpreters `suzatary serve` holds, and the one place they become records.

use suzatary_api::server::Server;
use suzatary_io::devices::Devices;
use suzatary_io::notify::Configured;

use super::interpreters::{Brain, Peers};
use crate::instance::Instance;
use crate::wiring::{Opened, Stores};

/// [`Opened`] plus what only a server holds (peers, push, devices), and one [`Stores`] per
/// chain.
pub struct World {
    opened: Opened,
    peers: Peers,
    push: Configured<Devices>,
    devices: Devices,
    brain: Brain,
    /// Each chain's stores, in the instance file's order.
    pub chains: Vec<Stores>,
    /// What the box signs with; missing keys leave every write refusing and every read as it was.
    pub signers: suzatary_signatures::signatures::Signers,
}

impl World {
    /// Choose an interpreter for every effect, out of what the instance file says.
    #[must_use]
    pub fn assemble(
        instance: &Instance,
        signers: suzatary_signatures::signatures::Signers,
        peers: Peers,
        brain: Brain,
    ) -> Self {
        let devices = Devices::open(instance.paths.devices.clone());
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
    pub fn begin_topics(&self) -> Result<(), suzatary_store::store::StoreError> {
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

    /// The server's record over one chain's stores.
    #[must_use]
    pub fn server<'a>(&'a self, stores: &'a Stores) -> Server<'a> {
        Server {
            secretary: self.opened.secretary(stores, &self.brain),
            whois: &self.peers,
            notify: &self.push,
            answers: &*self.opened.answers,
            devices: &self.devices,
            sessions: &self.brain,
        }
    }
}
