use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::decisions::{DecisionId, Ledger};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::{Refusal, shown};

/// The recorded decisions: every one in its lineage, or one whole.
pub struct Decisions;

#[derive(Deserialize, JsonSchema)]
pub struct DecisionsArgs {
    id: Option<String>,
}

impl Verb for Decisions {
    type Surface = SecretarySurface;
    type Args = DecisionsArgs;
    type Answer = Value;
    const NAME: &'static str = "decisions";
    const DESCRIPTION: &'static str = "The project's RECORDED DECISIONS: designs and plans as \
        items, each with what it supersedes and what supersedes it. With no `id`, every \
        decision by lineage, newest first, each above what it supersedes, with whether it is \
        in force (neither withdrawn nor superseded). With `id`, that decision whole: its body \
        (Typst markup), its rationale and its links. Specs and laws are files, not decisions. \
        To record one, `propose_decision`.";

    fn run(call: &Call<'_>, args: DecisionsArgs) -> Result<Value, Refusal> {
        let ledger = call.caps.decisions.ledger()?;
        Ok(match args.id {
            None => json!({
                "lineages": ledger
                    .lineages()
                    .into_iter()
                    .map(|lineage| lineage.into_iter().map(|id| row(&ledger, id)).collect())
                    .collect::<Vec<Vec<Value>>>(),
            }),
            Some(id) => {
                let id = DecisionId::new(id.trim())
                    .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
                let Some(decision) = ledger.get(&id) else {
                    return Err(Refusal::not_found(format!(
                        "no decision is named {}",
                        shown(id.as_str())
                    )));
                };
                let mut whole = row(&ledger, &id);
                whole["body"] = json!(decision.statement().body().as_str());
                whole["rationale"] = json!(decision.statement().rationale());
                whole
            }
        })
    }
}

/// One decision as a line of the ledger.
fn row(ledger: &Ledger, id: &DecisionId) -> Value {
    let Some(decision) = ledger.get(id) else {
        return json!({ "id": id.as_str() });
    };
    json!({
        "id": id.as_str(),
        "title": decision.statement().title(),
        "decided": decision.decided.isoformat(),
        "in_force": ledger.in_force(id),
        "withdrawn": decision.withdrawn.as_ref().map(|withdrawn| withdrawn.reason.as_str()),
        "supersedes": decision.supersedes.iter().map(DecisionId::as_str).collect::<Vec<_>>(),
        "superseded_by": ledger.superseded_by(id).into_iter().map(DecisionId::as_str).collect::<Vec<_>>(),
        "statements_in_conflict": decision.is_conflict().then(|| decision.statements().len()),
    })
}
