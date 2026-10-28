use schemars::JsonSchema;
use serde::Deserialize;

use super::{propose, references};
use crate::reference::TodoRef;
use crate::refusal::Refusal;
use crate::secretary::Call;
use crate::secretary::store::{Proposal, Reasons};

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

/// The arguments every `propose_*` verb takes beside its act.
#[derive(Deserialize, JsonSchema)]
pub struct FilingArgs {
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
