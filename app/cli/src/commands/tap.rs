//! `suzatary tap proposal:<conversation>/<n>`: the operator's tap on a card, through the route a
//! device's tap takes. The box refuses the operator's tap on every card that sends mail
//! (`suzatary_core::secretary::sending`), because sending stays the owner's tap.

use std::path::Path;

use anyhow::Context;
use suzatary_api::secretary::routes::TapAnswer;
use suzatary_base::reference::ProposalRef;

use super::call::Operator;
use super::serve::route::Method;
use crate::instance::Instance;

/// Which card, and what the tap brings.
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

/// `suzatary tap <card>`: the conversation as the box answers after the tap, on stdout.
///
/// # Errors
/// The instance file's, the keys', stdin's, and the route's refusal.
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

/// A tap the box performed.
#[derive(Debug)]
pub struct Tapped {
    /// The name the box keeps the tap's record under, which names the root key.
    pub request: prodrome_core::event::Hash,
    /// The conversation as the box answers after the tap.
    pub conversation: serde_json::Value,
}

/// Tap `card` as `operator`, bringing `answer`.
///
/// # Errors
/// The route's refusal, a card that sends mail among them.
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
