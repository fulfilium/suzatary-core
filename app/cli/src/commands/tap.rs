//! `suzatary tap`: the operator's tap on a card, through the route a device's tap takes, refused
//! on every card that sends mail (`suzatary_inbox::secretary::sending`), because sending stays
//! the owner's tap.

use std::path::Path;

use anyhow::Context;
use suzatary_base::reference::ProposalRef;
use suzatary_server::secretary::routes::TapAnswer;

use super::call::Operator;
use crate::instance::Instance;
use suzatary_server::serve::route::Method;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// The card, in its reference form: `proposal:<conversation>/<n>`.
    card: ProposalRef,
    /// A line for the conversation.
    #[arg(long, default_value = "")]
    note: String,
    /// The card's wider yes ("Always allow").
    #[arg(long, conflicts_with = "value_from_stdin")]
    always: bool,
    /// Read the value a secret or code card asks for from stdin, never from the command line.
    #[arg(long)]
    value_from_stdin: bool,
}

pub fn run(path: &Path, args: &Args) -> anyhow::Result<()> {
    let answer = match (args.always, args.value_from_stdin) {
        (true, _) => TapAnswer::Always,
        (false, true) => {
            let mut value = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut value)
                .context("reading the value")?;
            TapAnswer::Value(suzatary_base::capability::Secret::new(
                value.trim_end_matches(['\r', '\n']),
            ))
        }
        (false, false) => TapAnswer::Plain,
    };
    let operator = Operator::of(&Instance::open(path)?)?;
    let tapped = tap(&operator, &args.card, &args.note, &answer)?;
    println!("{}", tapped.conversation);
    eprintln!("request: {}", tapped.request.as_str());
    Ok(())
}

#[derive(Debug)]
pub struct Tapped {
    /// The name the box keeps the tap's record under, which names the root key.
    pub request: prodrome_data::event::Hash,
    /// The conversation as the box answers after the tap.
    pub conversation: serde_json::Value,
}

/// The route refuses a card that sends mail.
pub fn tap(
    operator: &Operator,
    card: &ProposalRef,
    note: &str,
    answer: &TapAnswer,
) -> anyhow::Result<Tapped> {
    let mut body = serde_json::json!({ "note": note });
    match answer {
        TapAnswer::Plain => {}
        TapAnswer::Always => body["always"] = true.into(),
        TapAnswer::Value(value) => body["value"] = value.expose().into(),
        TapAnswer::Endorsed(endorsement) => {
            body["endorsement"] = serde_json::json!({
                "key": endorsement.key.as_str(),
                "signature": endorsement.signature.as_str(),
            });
        }
    }
    let answered = operator.request(
        Method::Post,
        &format!(
            "/api/secretary/conversations/{}/proposals/{}/tap",
            card.conversation, card.index
        ),
        &serde_json::to_vec(&body)?,
    )?;
    Ok(Tapped {
        conversation: serde_json::from_slice(&answered.body).context("the tap's answer")?,
        request: answered.request,
    })
}
