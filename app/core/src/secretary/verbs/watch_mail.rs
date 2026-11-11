use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mail::watch::{self, Days, On, Part, Pattern};
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// Watch for a message the owner expects, and push to their phone when it syncs.
pub struct WatchMail;

#[derive(Deserialize, JsonSchema)]
pub struct WatchMailArgs {
    #[schemars(schema_with = "part")]
    on: Part,
    text: String,
    days: u16,
}

impl Verb for WatchMail {
    type Surface = SecretarySurface;
    type Args = WatchMailArgs;
    type Answer = VerbAnswer;
    const NAME: &'static str = "watch_mail";
    const DESCRIPTION: &'static str = "WATCH FOR A MESSAGE the owner expects, and wake their \
        phone once it syncs. Use it when the owner says they are waiting for mail: a parcel's \
        delivery notice, a reply from a person, a library's note that a hold is ready.\n\n`on` \
        is `sender` or `subject`; `text` is words of the sender's address or name, or of the \
        subject, matched as the mail index matches a phrase, without regard to case. Mail from \
        the owner's own accounts never matches. `days` (1 to 60) is how long the watch stands; past \
        it, the watch closes without a push. The first message that matches rings the watch \
        once and closes it. The push carries no text: it says that the awaited mail arrived \
        and opens the message. The owner sees each open watch on the Mail page and can \
        cancel it there.";

    fn run(call: &Call<'_>, args: WatchMailArgs) -> Result<VerbAnswer, Refusal> {
        let on = On {
            part: args.on,
            text: Pattern::new(&args.text)?,
        };
        let now = prodrome_core::fpl::instant_of(call.caps.clock.now());
        let accounts = call.caps.mail.accounts();
        let own: Vec<&str> = accounts.iter().map(String::as_str).collect();
        let set = watch::set(call.caps.watches, on, Days::new(args.days)?, &own, now)?;
        Ok(answer(&set).into())
    }
}

fn part(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type": "string", "enum": ["sender", "subject"]})
}

fn answer(watch: &watch::Watch) -> Value {
    json!({
        "watch": watch.id.as_str(),
        "until": prodrome_core::fpl::iso(watch.set.until),
        "next": "Tell the owner in one clause what you are watching for and until when; they \
                 can cancel it on the Mail page.",
    })
}
