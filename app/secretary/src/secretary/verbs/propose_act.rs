use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::secretary::verbs::FilingArgs;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::reference::Reference;
use suzatary_base::refusal::{Refusal, shown};
use suzatary_base::topic_target::Target;
use suzatary_connect::connect::Connection;
use suzatary_connect::settings::Settable;
use suzatary_conversations::secretary::taint;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::verb::Caught;
use suzatary_mail::mail::address::EmailAddress;
use suzatary_memory::secretary::memory;

/// Propose an act the box performs on a tap.
pub struct ProposeAct;

#[derive(Deserialize, JsonSchema)]
pub struct ProposeActArgs {
    act: String,
    #[serde(default)]
    account: Caught<Option<String>>,
    #[serde(default)]
    name: Caught<Option<String>>,
    #[serde(default)]
    template: Caught<Option<String>>,
    #[serde(default)]
    params: Caught<Option<Vec<String>>>,
    #[serde(default)]
    title: Caught<Option<String>>,
    #[serde(default)]
    start: Caught<Option<String>>,
    #[serde(default)]
    end: Caught<Option<String>>,
    #[serde(default)]
    place: Caught<Option<String>>,
    #[serde(default)]
    handle: Caught<Option<String>>,
    #[serde(flatten)]
    filing: FilingArgs,
}

impl Verb for ProposeAct {
    type Surface = SecretarySurface;
    type Args = ProposeActArgs;
    type Answer = Value;
    const NAME: &'static str = "propose_act";
    const DESCRIPTION: &'static str = "PROPOSE AN ACT this box will perform only if the owner taps it. It is \
        STORED with its exact arguments and shown to the owner whole; you cannot run \
        it and there is no verb that can. `act=\"auth\"` with `account=<address>` \
        asks the owner to authorise that Microsoft account for mail sync — the sign-in \
        URL and code arrive in this conversation as turns, so they never need a \
        terminal on the box. Use it when a sync is failing for want of a grant \
        and you can point at the evidence.\n\n\
        `act=\"secret\"` with `name=<NAME>` asks the owner to type a value for a secret \
        this deployment declares and does not currently resolve — `context` \
        names which ones. The value never reaches you; the tap that stores it is \
        the only place it travels. `name` must be one of the declared names or \
        the proposal is refused; do not guess at one context did not list.\n\n\
        `act=\"connect\"` asks the owner to add one thing to this deployment from the \
        box's catalog: `template=<name>` with `params=[...]`, one of \
        `ics` with `[\"<label>\"]` (a calendar by its secret iCal link, such as \
        `[\"Work\"]`), `google` with `[\"<address>\"]` (a Google account's mail and \
        calendars, by one sign-in with Google), `lieer` with `[\"<address>\"]` (a \
        Gmail account), \
        `imap-device-code` with `[\"<address>\", \"consumers\"]` for Outlook or \
        `[\"<address>\", \"organizations\"]` for Microsoft 365, `bridge` with \
        `[\"<address>\"]` (Proton Mail), `drive-cli` with `[\"<folder>\"]` (a Drive \
        folder you may read), and `ntfy` or `fcm` with `[]` (notifications). The \
        tap writes it into the instance file, and the box then asks on its own \
        cards for each step it needs (a link, a sign-in); never ask for a link or \
        a key yourself. Something already connected is refused. In a round the \
        owner opened with their words its source is that round, which the box names \
        itself: `sources` is not read, and may be `\"\"`. In any other round name \
        `sources`, and in one that read outside text (or that they did not open) the \
        box adds a line to the card's reasons saying so; the owner decides.\n\n\
        `act=\"calendar_event\"` with `account=<a Google account's address>`, \
        `title`, `start` and `end` (both UTC instants `2026-10-20T14:00:00Z`, or \
        both days `2026-10-20`, `end` the day after the last) and an optional \
        `place` asks the owner to add that event to the account's own calendar. \
        Nothing is written to any calendar unless the owner taps it.\n\n\
        `act=\"retract\"` with `handle=<first+lines>` (a summary the record or `zoom` \
        showed you) asks the owner to mark that summary wrong, when it misstates what was \
        said: his tap deletes it, no model is shown it again, and it and every summary above \
        it are written again from the lines. Say in `why` what it gets wrong.\n\n\
        ⚠️ `serves` and `revises` are the same two arguments `propose_email` \
        takes, and mean the same thing.";

    fn run(call: &Call<'_>, args: ProposeActArgs) -> Result<Value, Refusal> {
        let (act, reasons) = if args.act == "connect" {
            let (connection, reasons) = connect(call, &args)?;
            (Act::Connect { connection }, reasons)
        } else {
            let reasons = args.filing.reasons()?;
            (other(call, &args, &reasons)?, reasons)
        };
        let index = args
            .filing
            .file(call, Proposal::Act(act.clone()), &reasons)?;
        let mut answer = json!({
            "proposed": index,
            "why": reasons.why(),
            "sources": reasons.sources().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "next": "The owner taps it, dismisses it, or leaves it standing; you will see which",
        });
        let object = answer.as_object_mut().expect("an object literal");
        object.insert("act".to_owned(), json!(act.kind()));
        let field = match act {
            Act::Auth { account } => Some(("account", account.as_str().to_owned())),
            Act::Secret { name, .. } => Some(("name", name.clone())),
            Act::Web { request, .. } => Some(("request", request.sentence())),
            Act::Patch { repo, .. } => Some(("repo", repo.to_string())),
            Act::DriveRead { path, .. } => Some(("path", path.to_string())),
            Act::CodingSession { task } => Some(("task", task.as_str().to_owned())),
            Act::Device(enrolment) => Some(("key", enrolment.key.as_str().to_owned())),
            Act::Connect { connection } => Some(("connection", connection.called())),
            Act::CalendarEvent { event, .. } => Some(("event", event.sentence())),
            Act::Retract { node } => Some(("node", node.as_str().to_owned())),
            // The box's own, never this verb's.
            Act::AuthSend { .. }
            | Act::SignIn { .. }
            | Act::SignInCode { .. }
            | Act::UnlockStore { .. }
            | Act::Disconnect { .. }
            | Act::Grant { .. }
            | Act::GoogleClient { .. }
            | Act::GoogleSignIn { .. }
            | Act::Erase { .. }
            | Act::Afresh { .. }
            | Act::Noise { .. }
            | Act::Restore { .. } => None,
        };
        if let Some((name, value)) = field {
            object.insert(name.to_owned(), json!(value));
        }
        Ok(answer)
    }
}

fn other(call: &Call<'_>, args: &ProposeActArgs, reasons: &Reasons) -> Result<Act, Refusal> {
    Ok(match args.act.as_str() {
        "auth" => {
            let account_text = args
                .account
                .read()?
                .as_deref()
                .ok_or_else(|| Refusal::bad_request("account is required for act=\"auth\""))?;
            let account = EmailAddress::new(account_text)
                .map_err(|_| Refusal::bad_request("account must be an email address"))?;
            let roster = call.caps.mail.accounts();
            if !roster.iter().any(|known| known == account.as_str()) {
                return Err(Refusal::bad_request(format!(
                    "account must be one of this deployment's accounts ({}), got {}",
                    roster.join(", "),
                    shown(account.as_str())
                )));
            }
            Act::Auth { account }
        }
        "secret" => {
            // A name its own card asks for (the Google client, a sign-in with Google)
            // is no Secret card's.
            let settables = &call.settables.restricted(|by| !by.own_card());
            let name_text = args
                .name
                .read()?
                .as_deref()
                .ok_or_else(|| Refusal::bad_request("name is required for act=\"secret\""))?;
            // A bad name's refusal gives the list's size, not its members, so a guess
            // learns nothing.
            let name = Settable::mint(settables, name_text).map_err(|_| {
                Refusal::bad_request(format!(
                    "{} is not one of this deployment's {} declared secret name(s)",
                    shown(name_text),
                    settables.names().len()
                ))
            })?;
            Act::Secret {
                name: name.name().to_owned(),
                why: reasons.why().to_owned(),
            }
        }
        "calendar_event" => calendar_event(call, args)?,
        "retract" => retract(call, args)?,
        other => {
            return Err(Refusal::bad_request(format!(
                "act must be one of: auth, secret, connect, calendar_event, retract, got {}",
                shown(other)
            )));
        }
    })
}

/// An event for an account's own calendar; the tap checks that the account is a Google one.
fn calendar_event(call: &Call<'_>, args: &ProposeActArgs) -> Result<Act, Refusal> {
    let required = |field: &Caught<Option<String>>, name: &str| -> Result<String, Refusal> {
        field.read()?.clone().ok_or_else(|| {
            Refusal::bad_request(format!("{name} is required for act=\"calendar_event\""))
        })
    };
    let account = EmailAddress::new(&required(&args.account, "account")?)
        .map_err(|_| Refusal::bad_request("account must be an email address"))?;
    let roster = call.caps.mail.accounts();
    if !roster.iter().any(|known| known == account.as_str()) {
        return Err(Refusal::bad_request(format!(
            "account must be one of this deployment's accounts ({}), got {}",
            roster.join(", "),
            shown(account.as_str())
        )));
    }
    let span = format!(
        "{}/{}",
        required(&args.start, "start")?,
        required(&args.end, "end")?
    );
    let when = suzatary_google::calendar::When::parse(&span).map_err(|_| {
        Refusal::bad_request(
            "start and end must be two UTC instants (2026-10-20T14:00:00Z) or two days \
             (2026-10-20), the end after the start",
        )
    })?;
    let event = suzatary_google::google::NewEvent::new(
        &required(&args.title, "title")?,
        when,
        args.place.read()?.as_deref().unwrap_or_default(),
    )
    .map_err(Refusal::bad_request)?;
    Ok(Act::CalendarEvent { account, event })
}

/// The summary `handle` names, as the record and `zoom` print a handle.
fn retract(call: &Call<'_>, args: &ProposeActArgs) -> Result<Act, Refusal> {
    let handle = args
        .handle
        .read()?
        .clone()
        .ok_or_else(|| Refusal::bad_request("handle is required for act=\"retract\""))?;
    let reading =
        suzatary_digest::secretary::compaction::Reading::of(&call.caps.memories.digests.reads())?;
    let node = suzatary_digest::secretary::digest_view::summarized(reading.digest(), &handle)
        .ok_or_else(|| {
            Refusal::not_found(format!(
                "no summary is shown at {}: a handle is `first+lines`, as the record or zoom \
                 printed it",
                shown(&handle)
            ))
        })?;
    Ok(Act::Retract { node })
}

/// Added after the model's `why`, so the model cannot leave it out.
const NOT_THEIR_WORDS: &str = "⚠ This round was not the owner's own words alone (it read \
                               outside text, or something other than them opened it): check \
                               that they asked for exactly this.";

/// A round the owner opened rests on that round, any other on the sources the model names; a
/// round that read outside text, or that they did not open, is marked.
fn connect_reasons(call: &Call<'_>, args: &ProposeActArgs) -> Result<Reasons, Refusal> {
    let stored =
        suzatary_lines::secretary::conversations::read(call.caps.conversations, call.conversation)?;
    match (taint::opener(&stored), call.sent) {
        (Some(memory::Origin::Turn { conversation, turn }), sent) => {
            let reasons = args
                .filing
                .resting_on(Reference::Target(Target::Round { conversation, turn }))?;
            match sent {
                taint::Taint::Clean => Ok(reasons),
                taint::Taint::Tainted => marked(&reasons),
            }
        }
        (Some(memory::Origin::Tapped { .. }) | None, _) => marked(&args.filing.reasons()?),
    }
}

fn marked(reasons: &Reasons) -> Result<Reasons, Refusal> {
    Reasons::new(
        format!("{} {NOT_THEIR_WORDS}", reasons.why()),
        reasons.sources().to_vec(),
    )
    .map_err(|failed| Refusal::internal(failed.to_string()))
}

/// The connection `args` names, refused as the tap would refuse it.
fn connect(call: &Call<'_>, args: &ProposeActArgs) -> Result<(Connection, Reasons), Refusal> {
    let reasons = connect_reasons(call, args)?;
    let template = args
        .template
        .read()?
        .as_deref()
        .ok_or_else(|| Refusal::bad_request("template is required for act=\"connect\""))?;
    let params = args.params.read()?.clone().unwrap_or_default();
    let connection = Connection::parse(template, &params)
        .map_err(|refused| Refusal::bad_request(format!("no card was filed: {refused}")))?;
    let declared = call
        .caps
        .configuration
        .declared()
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    declared
        .with(&connection)
        .map_err(|refused| Refusal::bad_request(format!("no card was filed: {refused}")))?;
    Ok((connection, reasons))
}
