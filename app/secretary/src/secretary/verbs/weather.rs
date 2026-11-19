use serde_json::{Value, json};

use crate::secretary::{Call, SecretarySurface};
use crate::verb::{NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// Today's forecast, one line.
pub struct Weather;

impl Verb for Weather {
    type Surface = SecretarySurface;
    type Args = NoArgs;
    type Answer = Value;
    const NAME: &'static str = "weather";
    const DESCRIPTION: &'static str = "Today's forecast for the city this deployment serves, as ONE LINE with \
        the umbrella verdict on it. It is the first line of the morning brief \
        and nothing else — a public feed, sometimes wrong, never something to \
        reason from beyond what it says.";

    fn run(call: &Call<'_>, _: NoArgs) -> Result<Value, Refusal> {
        let line = call
            .caps
            .weather
            .today()
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        Ok(json!({
            "line": line,
            "note": "One sentence at the top of a brief, umbrella verdict included. It is a \
                     public feed and it is sometimes wrong; say what it says and move on.",
        }))
    }
}
