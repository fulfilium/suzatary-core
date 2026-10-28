use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::decisions::Draft;
use crate::refusal::Refusal;
use crate::secretary::store::Proposal;
use crate::secretary::verbs::FilingArgs;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Propose a decision to record, which the owner's tap records as his.
pub struct ProposeDecision;

#[derive(Deserialize, JsonSchema)]
pub struct ProposeDecisionArgs {
    id: String,
    title: String,
    body: String,
    rationale: String,
    supersedes: Option<String>,
    #[serde(flatten)]
    filing: FilingArgs,
}

impl Verb for ProposeDecision {
    type Surface = SecretarySurface;
    type Args = ProposeDecisionArgs;
    type Answer = Value;
    const NAME: &'static str = "propose_decision";
    const DESCRIPTION: &'static str = "PROPOSE A DECISION to record: a design or plan the owner \
        has settled, as an item beside the others `decisions` lists. `id` is 3-64 lowercase \
        letters and digits starting with a letter, new; `title` one line; `body` is Typst \
        MARKUP, checked as a todo's is; `rationale` plain text, why; `supersedes` the ids, \
        comma-separated, of decisions this is the newer word on, which then stop being in \
        force. The owner reads the whole card and their tap records it as theirs; nothing is \
        recorded before. `why` and `sources` are required as for every proposal; `serves` and \
        `revises` as for `propose_email`.";

    fn run(call: &Call<'_>, args: ProposeDecisionArgs) -> Result<Value, Refusal> {
        let reasons = args.filing.reasons()?;
        let draft = Draft::new(
            &args.id,
            &args.title,
            &args.body,
            &args.rationale,
            args.supersedes.as_deref().unwrap_or_default().split(','),
        )
        .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        call.caps.decisions.admits(&draft, call.caps.render)?;
        let sentence = crate::secretary::decide::sentence(&draft);
        let index = args.filing.file(call, Proposal::Decide(draft), &reasons)?;
        Ok(json!({
            "proposed": index,
            "sentence": sentence,
            "next": "The owner reads the whole decision on a card; their tap records it. You \
                     cannot record one and there is no verb that can.",
        }))
    }
}
