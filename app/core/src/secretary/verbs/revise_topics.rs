use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::add_topics::{Aim, On, parsed, revise};
use crate::secretary::topics::revision::Revision;
use crate::secretary::topics::topic::Topic;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// Drop or rename topics the secretary suggested, on any round or note.
pub struct ReviseTopics;

#[derive(Deserialize, JsonSchema)]
pub struct ReviseTopicsArgs {
    #[serde(default = "answering")]
    on: Aim,
    #[serde(default)]
    drop: Vec<String>,
    #[serde(default)]
    rename: Vec<Renaming>,
}

#[derive(Deserialize, JsonSchema)]
struct Renaming {
    from: String,
    to: String,
}

fn answering() -> Aim {
    Aim::Answering(On::Round)
}

impl Verb for ReviseTopics {
    type Surface = SecretarySurface;
    type Args = ReviseTopicsArgs;
    type Answer = Value;
    const NAME: &'static str = "revise_topics";
    const DESCRIPTION: &'static str = "Take off (`drop`) or rename (`rename: [{from, to}]`) topics \
        YOU suggested, on the round you are answering, its note (`on: \"note\"`), or any round or \
        note you name (`on: \"round:<conversation>/<turn>\"`). A rename changes that one target \
        only. A topic the owner set is theirs: you cannot drop or rename it, and you cannot put \
        back one they took off. A `round:` or `note:` reference reads as its topics, split into \
        `owner` and `suggested`, and so does this answer. A new name follows add_topics' rules.";

    fn run(call: &Call<'_>, args: ReviseTopicsArgs) -> Result<Value, Refusal> {
        let drop = parsed(args.drop, 0)?;
        let one = |text: String| Topic::new(text).map_err(|e| Refusal::bad_request(e.to_string()));
        let rename = args
            .rename
            .into_iter()
            .map(|Renaming { from, to }| Ok((one(from)?, one(to)?)))
            .collect::<Result<BTreeMap<Topic, Topic>, Refusal>>()?;
        if drop.is_empty() && rename.is_empty() {
            return Err(Refusal::bad_request("name a topic to drop or to rename"));
        }
        revise(
            call,
            args.on,
            &Revision {
                drop,
                rename,
                ..Revision::default()
            },
        )
    }
}
