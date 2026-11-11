//! The topics as a page reads and writes them: the two stores' replicas, the
//! `suzatary_core::secretary::topics::Reading` over what they hold, and its planners, which answer
//! events for the replicas' `append`. The box plans with the same functions, so a tab writing
//! offline writes what the box would write holding the same objects.
//!
//! The replicas are the Prodrome's `schema!` exports at the two stores' schemas. A page reads
//! the topics through [`TopicReading`].

use prodrome_core::event::{Actor, canonical};
use prodrome_core::fpl::datetime_of;
use prodrome_core::literal::Datetime;
use prodrome_wasm_exports::wire::parse_instant;
use suzatary_base::topic_target::Target;
use suzatary_core::secretary::topics::provenance::Owner;
use suzatary_core::secretary::topics::reading::AliasEvent;
use suzatary_core::secretary::topics::reading::MembershipEvent;
use suzatary_core::secretary::topics::reading::Reading;
use suzatary_core::secretary::topics::topic::Topic;
use wasm_bindgen::prelude::*;

use crate::{json, refused};

crate::object_rows::objects_store!(Memberships = MembershipEvent);
crate::object_rows::objects_store!(TopicAliases = AliasEvent);

/// The topics both classes' objects read as, read once.
#[wasm_bindgen]
pub struct TopicReading(pub(crate) Reading);

#[wasm_bindgen]
impl TopicReading {
    /// The memberships and the aliases read together.
    #[wasm_bindgen(constructor)]
    pub fn new(memberships: &Memberships, aliases: &TopicAliases) -> Result<TopicReading, JsError> {
        let memberships = memberships.0.reading()?.read().map_err(refused)?;
        let aliases = aliases.0.reading()?.read().map_err(refused)?;
        suzatary_core::secretary::topics::reading::of(memberships.nodes, aliases.nodes)
            .map(Self)
            .map_err(|several| refused(several.to_string()))
    }

    /// The topics in use, the most rounds first, each with the names in dispute for it.
    pub fn bar(&self) -> Result<String, JsError> {
        json(&suzatary_core::secretary::topics::reading::bar(&self.0))
    }

    /// The owner's `Added` event that puts `topic` on `target` or keeps the secretary's
    /// suggestion of it, printed, or `undefined` where the owner set it already.
    pub fn add(
        &self,
        target: &str,
        topic: &str,
        at: &str,
        actor: &str,
    ) -> Result<Option<String>, JsError> {
        let (target, topic, at, actor) = (
            target_of(target)?,
            topic_of(topic)?,
            at_of(at)?,
            owner_of(actor)?,
        );
        Ok(self
            .0
            .add(&target, &topic, at, &actor)
            .map(|event| canonical(&event)))
    }

    /// The `Removed` events that take `topic` off `target`, printed, as a JSON array.
    pub fn remove(
        &self,
        target: &str,
        topic: &str,
        at: &str,
        actor: &str,
    ) -> Result<String, JsError> {
        let (target, topic, at, actor) = (
            target_of(target)?,
            topic_of(topic)?,
            at_of(at)?,
            owner_of(actor)?,
        );
        json(
            &self
                .0
                .remove(&target, &topic, at, &actor)
                .iter()
                .map(canonical)
                .collect::<Vec<_>>(),
        )
    }

    /// The alias events that rename `from` to `to`, printed, as a JSON array.
    pub fn rename(&self, from: &str, to: &str, at: &str, actor: &str) -> Result<String, JsError> {
        let (from, to, at, actor) = (topic_of(from)?, topic_of(to)?, at_of(at)?, owner_of(actor)?);
        json(
            &self
                .0
                .rename(&from, &to, at, &actor)
                .iter()
                .map(canonical)
                .collect::<Vec<_>>(),
        )
    }
}

/// Why `text` is not a topic, in the rule it breaks, or `undefined` for a topic.
#[wasm_bindgen]
#[must_use]
pub fn topic_refusal(text: &str) -> Option<String> {
    Topic::new(text).err().map(|e| e.to_string())
}

/// `text`, a JSON array of topics, each through [`Topic::new`].
pub(crate) fn topics_of(text: &str) -> Result<std::collections::BTreeSet<Topic>, JsError> {
    let named: Vec<String> =
        serde_json::from_str(text).map_err(|e| refused(format!("topics: {e}")))?;
    named.into_iter().map(|name| topic_of(&name)).collect()
}

fn target_of(text: &str) -> Result<Target, JsError> {
    text.parse()
        .map_err(|e: suzatary_base::topic_target::NotATarget| refused(e.to_string()))
}

fn topic_of(text: &str) -> Result<Topic, JsError> {
    Topic::new(text).map_err(|e| refused(e.to_string()))
}

fn at_of(text: &str) -> Result<Datetime, JsError> {
    let at = parse_instant("at", text).map_err(refused)?;
    datetime_of(at).map_err(|e| refused(format!("at: {e}")))
}

fn owner_of(text: &str) -> Result<Owner, JsError> {
    let actor = Actor::new(text).map_err(|e| refused(format!("actor: {e}")))?;
    Owner::new(actor).map_err(|e| refused(e.to_string()))
}
