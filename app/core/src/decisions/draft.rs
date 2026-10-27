use std::collections::BTreeSet;

use prodrome_core::literal::{ProdromeError, Value};
use serde::{Deserialize, Serialize};

use prodrome_core::literal::codec::{Constructor, Fields as Read, Literal, record};

use super::{DecisionId, NotADecision};

/// A decision as a writer asks for it, before its body has been through the markup door: the
/// fields a `Decided` or a `Revised` takes, trimmed, the id and links already parsed. What the
/// CLI reads from its flags, the secretary's proposal stores and the owner's tap performs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Fields", into = "Fields")]
pub struct Draft {
    id: DecisionId,
    title: String,
    body: String,
    rationale: String,
    supersedes: BTreeSet<DecisionId>,
}

/// The stored shape of a [`Draft`]: plain strings, parsed back through [`Draft::new`].
#[derive(Serialize, Deserialize)]
struct Fields {
    id: String,
    title: String,
    body: String,
    rationale: String,
    supersedes: Vec<String>,
}

impl Draft {
    /// Parse one: the id and each link checked, the title, body and rationale trimmed.
    ///
    /// # Errors
    /// [`NotADecision::Id`] for an id or link of the wrong shape, and [`NotADecision::Itself`]
    /// for a decision naming itself.
    pub fn new<'s>(
        id: &str,
        title: &str,
        body: &str,
        rationale: &str,
        supersedes: impl IntoIterator<Item = &'s str>,
    ) -> Result<Self, NotADecision> {
        let id = DecisionId::new(id.trim())?;
        let supersedes = supersedes
            .into_iter()
            .map(str::trim)
            .filter(|link| !link.is_empty())
            .map(DecisionId::new)
            .collect::<Result<BTreeSet<_>, _>>()?;
        if supersedes.contains(&id) {
            return Err(NotADecision::Itself(id.to_string()));
        }
        Ok(Self {
            id,
            title: title.trim().to_owned(),
            body: body.trim().to_owned(),
            rationale: rationale.trim().to_owned(),
            supersedes,
        })
    }

    /// The decision.
    #[must_use]
    pub fn id(&self) -> &DecisionId {
        &self.id
    }

    /// The title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The body as written, Typst markup not yet admitted.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Why.
    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    /// What it supersedes.
    #[must_use]
    pub fn supersedes(&self) -> &BTreeSet<DecisionId> {
        &self.supersedes
    }
}

impl TryFrom<Fields> for Draft {
    type Error = NotADecision;

    fn try_from(fields: Fields) -> Result<Self, NotADecision> {
        Draft::new(
            &fields.id,
            &fields.title,
            &fields.body,
            &fields.rationale,
            fields.supersedes.iter().map(String::as_str),
        )
    }
}

impl From<Draft> for Fields {
    fn from(draft: Draft) -> Self {
        Fields {
            id: draft.id.to_string(),
            title: draft.title,
            body: draft.body,
            rationale: draft.rationale,
            supersedes: draft.supersedes.iter().map(ToString::to_string).collect(),
        }
    }
}

/// `Decide(id, title, body, rationale, supersedes)`, a decision proposed in the inbox.
pub const DECIDE: Constructor = (
    "Decide",
    &["id", "title", "body", "rationale", "supersedes"],
);

impl Literal for Draft {
    fn to_value(&self) -> Value {
        record(
            DECIDE,
            vec![
                self.id.to_string().to_value(),
                self.title.to_value(),
                self.body.to_value(),
                self.rationale.to_value(),
                self.supersedes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .to_value(),
            ],
        )
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Read::of(value, DECIDE)?;
        let supersedes: Vec<String> = fields.get("supersedes")?;
        Draft::new(
            &fields.get::<String>("id")?,
            &fields.get::<String>("title")?,
            &fields.get::<String>("body")?,
            &fields.get::<String>("rationale")?,
            supersedes.iter().map(String::as_str),
        )
        .map_err(|failed| ProdromeError::invalid(failed.to_string()))
    }
}
