use std::sync::Arc;

use prodrome_core::sign::{PublicKey, Secret};

use super::Custody;
use suzatary_base::capability;

/// The secretspec name of the box's key, the deployment's root: it signs every box write and key
/// object.
pub const BOX_KEY: &str = "SUZATARY_BOX_KEY";

/// The secretspec name of the agent's key, registered for the agent's actor alone, so no other
/// key of the box's proves a write as the agent.
pub const AGENT_KEY: &str = "SUZATARY_AGENT_KEY";

/// The box's two signing keys, checked against the declared root. `Debug` shows only the public
/// halves.
#[derive(Clone)]
pub struct BoxKeys {
    root: Arc<Secret>,
    agent: Arc<Secret>,
}

/// Seeds that are not this deployment's keys.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotTheKeys {
    /// Names the secret only, never its value.
    #[error("{0} is not a key's seed (64 lowercase hex)")]
    NotASeed(&'static str),
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

    #[must_use]
    pub fn root(&self) -> &Arc<Secret> {
        &self.root
    }

    #[must_use]
    pub fn agent(&self) -> &Arc<Secret> {
        &self.agent
    }

    #[must_use]
    pub fn of_custody(&self, custody: Custody) -> &Arc<Secret> {
        match custody {
            Custody::Box => &self.root,
            Custody::Agent => &self.agent,
        }
    }
}

/// The agent's key alone: the agent's process never resolves the box's key, so nothing it holds
/// can register a key or sign as the box.
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
