use std::sync::Arc;

use prodrome_core::sign::{PublicKey, Secret};

use super::Custody;
use crate::capability;

/// The secretspec name of the box's own key, the deployment's ROOT: it signs every write the
/// box makes and every key object, so a key counts where it signed one.
pub const BOX_KEY: &str = "SUZATARY_BOX_KEY";

/// The secretspec name of the agent's key, registered for the agent's actor alone, so no other
/// key of the box's proves a write as the agent; the agent's writes are Inbox cards, so it signs
/// nothing.
pub const AGENT_KEY: &str = "SUZATARY_AGENT_KEY";

/// The box's two signing keys, each a seed held in secretspec by its name, checked once against
/// the root the instance declares. Never printed: `Debug` shows the public halves.
#[derive(Clone)]
pub struct BoxKeys {
    root: Arc<Secret>,
    agent: Arc<Secret>,
}

/// Seeds that are not this deployment's keys.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotTheKeys {
    /// A value that is not a seed, by its name only.
    #[error("{0} is not a key's seed (64 lowercase hex)")]
    NotASeed(&'static str),
    /// The box's key is not the root the instance declares.
    #[error("{BOX_KEY} is the key {found}, and the instance declares the root {declared}")]
    NotTheRoot {
        /// The root `[signatures]` names.
        declared: PublicKey,
        /// What the seed derives.
        found: PublicKey,
    },
    /// One key for both: the agent could then sign as the box.
    #[error("{AGENT_KEY} is {BOX_KEY}; the agent's key must be its own")]
    Shared,
}

impl BoxKeys {
    /// The two keys from their seeds, the box's checked to be `root`.
    ///
    /// # Errors
    /// [`NotTheKeys`].
    pub fn of(
        root: &PublicKey,
        box_seed: &capability::Secret,
        agent_seed: &capability::Secret,
    ) -> Result<BoxKeys, NotTheKeys> {
        let key = |name, seed: &capability::Secret| {
            Secret::new(seed.expose()).map_err(|_| NotTheKeys::NotASeed(name))
        };
        let (own, agent) = (key(BOX_KEY, box_seed)?, key(AGENT_KEY, agent_seed)?);
        if own.public() != *root {
            return Err(NotTheKeys::NotTheRoot {
                declared: root.clone(),
                found: own.public(),
            });
        }
        if agent.public() == own.public() {
            return Err(NotTheKeys::Shared);
        }
        Ok(BoxKeys {
            root: Arc::new(own),
            agent: Arc::new(agent),
        })
    }

    /// The box's key, the root: what signs the box's writes and its key objects.
    #[must_use]
    pub fn root(&self) -> &Arc<Secret> {
        &self.root
    }

    /// The agent's key, the one key registered for its actor.
    #[must_use]
    pub fn agent(&self) -> &Arc<Secret> {
        &self.agent
    }

    /// The key `custody` names.
    #[must_use]
    pub fn of_custody(&self, custody: Custody) -> &Arc<Secret> {
        match custody {
            Custody::Box => &self.root,
            Custody::Agent => &self.agent,
        }
    }
}

/// The agent's key alone, for the agent's own process: it is checked not to be the root, and
/// the box's key is never resolved there, so nothing the agent's process holds can register a
/// key or sign as the box.
///
/// # Errors
/// [`NotTheKeys::NotASeed`] and [`NotTheKeys::Shared`].
pub fn agent_key(root: &PublicKey, seed: &capability::Secret) -> Result<Arc<Secret>, NotTheKeys> {
    let agent = Secret::new(seed.expose()).map_err(|_| NotTheKeys::NotASeed(AGENT_KEY))?;
    if agent.public() == *root {
        return Err(NotTheKeys::Shared);
    }
    Ok(Arc::new(agent))
}

impl std::fmt::Debug for BoxKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoxKeys")
            .field("root", &self.root.public())
            .field("agent", &self.agent.public())
            .finish()
    }
}
