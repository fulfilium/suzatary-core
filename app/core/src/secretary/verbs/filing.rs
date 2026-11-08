use schemars::JsonSchema;
use serde::Deserialize;

use super::{propose, references};
use crate::reference::TodoRef;
use crate::refusal::Refusal;
use crate::secretary::Call;
use crate::secretary::proposal::{Proposal, Reasons};
use crate::verb::Caught;

/// The reasons every proposal carries: why, and what it rests on.
#[derive(Deserialize, JsonSchema)]
pub struct Reasoned {
    why: String,
    sources: String,
}

impl Reasoned {
    /// The reasons, refused when either is empty.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] for an empty `why`, no source, or a source that is no reference.
    pub fn reasons(&self) -> Result<Reasons, Refusal> {
        Reasons::new(&self.why, references(&self.sources, "sources")?)
            .map_err(|failed| Refusal::bad_request(failed.to_string()))
    }
}

/// The reasons a binding verb gives for the card its round may make it, required only then.
#[derive(Deserialize, JsonSchema)]
pub struct IfCarded {
    #[serde(default)]
    why: Caught<Option<String>>,
    #[serde(default)]
    sources: Caught<Option<String>>,
}

impl IfCarded {
    /// The reasons of the card `what` became.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] for a mistyped, empty or missing `why` or `sources`.
    pub fn reasons(&self, what: &str) -> Result<Reasons, Refusal> {
        let (why, sources) = (self.why.read()?, self.sources.read()?);
        Reasons::new(
            why.as_deref().unwrap_or_default(),
            references(sources.as_deref().unwrap_or_default(), "sources")?,
        )
        .map_err(|failed| {
            Refusal::bad_request(format!(
                "this round has read something that is not the owner's own words, so {what} \
                 becomes a card they tap, and a card needs its reasons: {failed}"
            ))
        })
    }
}

/// The arguments every `propose_*` verb takes beside its act.
#[derive(Deserialize, JsonSchema)]
pub(crate) struct FilingArgs {
    #[serde(flatten)]
    reasoned: Reasoned,
    serves: Option<String>,
    revises: Option<usize>,
}

impl FilingArgs {
    /// The reasons, read before anything is built, so a card without them is refused first.
    ///
    /// # Errors
    /// As [`Reasoned::reasons`].
    pub fn reasons(&self) -> Result<Reasons, Refusal> {
        self.reasoned.reasons()
    }

    /// File `proposal` in the inbox with these reasons, and answer its card's index.
    ///
    /// # Errors
    /// A `serves` that is no todo reference, and whatever filing the proposal refuses.
    pub fn file(
        &self,
        call: &Call<'_>,
        proposal: Proposal,
        reasons: &Reasons,
    ) -> Result<usize, Refusal> {
        let serves = self
            .serves
            .as_deref()
            .map(|text| {
                text.trim()
                    .parse::<TodoRef>()
                    .map_err(|failed| Refusal::bad_request(format!("serves: {failed}")))
            })
            .transpose()?;
        propose(
            call.caps,
            call.chains,
            call.conversation,
            proposal,
            reasons,
            serves,
            self.revises,
        )
    }
}
